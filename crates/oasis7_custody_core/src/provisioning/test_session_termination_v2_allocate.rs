//! Account-owned original allocation only. No Native signing/send is performed.
use super::*;
use oasis7_agent_api::game::{GameTaskCloseConditionsV2, GameTaskRevokeAndCloseV2};
impl AccountProvisionApprovalState {
    pub fn allocate_test_session_close_v2(
        &mut self,
        id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<GameTaskRevokeAndCloseV2> {
        let checks =
            self.test_session_v2_live_scope(id, SessionPurposeV1::TerminationPlan, current, now)?;
        verifier.verify(&checks)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none()
            || s.source_funding.get(current.source_operation_id)
                != Some(&SessionSourceFundingV1::TerminationPlan)
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        s.termination_state.validate_input_binding(&s.input)?;
        if let Some(old) = &s.termination_state.close {
            return Ok(old.payload.clone());
        }
        let (header, _) = s.header()?;
        let term = &header.termination;
        let wrapper = s
            .sources
            .get(current.source_operation_id)
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        let verification = &wrapper
            .source
            .verification
            .as_ref()
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .verification;
        let source_until = verification
            .observed_at
            .0
            .checked_add(5_000)
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .min(wrapper.source.expires_at.0)
            .min(verification.proof_expires_at.0)
            .min(verification.source_session_expires_at.0)
            .min(verification.source_session_idle_expires_at.0);
        let life = &current.lifetime.lifetime;
        let native_until = life
            .status
            .expires_at
            .0
            .min(life.session_expires_at.0)
            .min(life.session_idle_expires_at.0);
        let until = term
            .cleanup_until
            .get()
            .min(source_until)
            .min(native_until)
            .min(
                current
                    .observed_at
                    .get()
                    .checked_add(5_000)
                    .ok_or(ProvisionApprovalError::BindingMismatch)?,
            );
        if until <= now.get() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let (generation, nonce) = self.allocated_signer_high_water()?;
        let generation = generation
            .max(term.original_delegation.generation)
            .checked_add(1)
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        let nonce = nonce
            .max(term.original_delegation.nonce)
            .checked_add(1)
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        let own_stop_intent = s.termination_state.guarded_stop_intent()?;
        let payload = GameTaskRevokeAndCloseV2 {
            api_revision: 2,
            client_operation_id: term.close_operation_id.clone(),
            custody_account_id: term.custody_account_id.clone(),
            termination_plan_digest: term
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: term.initial_fence.clone(),
                own_stop_intent,
            },
            signer_public_key: term.original_delegation.delegate_public_key.clone(),
            signer_generation: DecimalU64(term.original_delegation.generation),
            signer_nonce: DecimalU64(term.original_delegation.nonce),
            revoke_generation: DecimalU64(generation),
            revoke_nonce: DecimalU64(nonce),
            cleanup_until_ms: DecimalU64(until),
        };
        payload
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let source_record_digest =
            crate::provisioning::types::digest(wrapper).map_err(ProvisionApprovalError::from)?;
        let mut allocation = TerminationCloseAllocationV2 {
            signature_claim: None,
            payload: payload.clone(),
            allocated_at: now,
            allocation_record_digest: String::new(),
            source_operation_id: current.source_operation_id.into(),
            source_record_digest,
            source_until: UnixMillis::new(source_until),
            native_until: UnixMillis::new(native_until),
            native_lifetime: current.lifetime.clone(),
            signed: None,
        };
        allocation.allocation_record_digest = allocation.record_digest(
            &s.termination_state.approved_input_digest,
            &s.termination_state.approved_plan_digest,
        )?;
        let mut next = self.clone();
        let record = next.test_session_v2_record_mut(id)?;
        record
            .roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        record.termination_state.close = Some(allocation);
        next.validate_persistence()?;
        *self = next;
        Ok(payload)
    }
}
