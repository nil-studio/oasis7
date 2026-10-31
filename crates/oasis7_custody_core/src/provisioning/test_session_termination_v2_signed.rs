//! Acceptance of the separately paid fixed Owner signature, never signing I/O.
use super::*;
use oasis7_agent_api::game::GameTaskRevokeAndCloseV2;
use oasis7_client_api::world_service::{
    SignedReadRequest, WorldIdentity, task_cleanup_v2::TaskCleanupSubmitV2,
};
impl AccountProvisionApprovalState {
    pub fn publish_test_session_close_v2_signed(
        &mut self,
        id: &str,
        signed: SignedReadRequest<GameTaskRevokeAndCloseV2>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_v2_live_scope(
            id,
            SessionPurposeV1::TerminationPlan,
            current,
            now,
        )?)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        s.termination_state.validate_input_binding(&s.input)?;
        s.termination_state
            .validate_source_binding(&s.sources, &s.source_funding)?;
        let allocation = s
            .termination_state
            .close
            .as_ref()
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        if allocation.payload != signed.request
            || current.source_operation_id != allocation.source_operation_id
            || now.get() >= allocation.payload.cleanup_until_ms.0
            || signed.subject_public_key
                != allocation
                    .payload
                    .permitted_branches
                    .exact_initial_active
                    .owner_public_key
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if let Some(old) = &allocation.signed {
            return if old == &signed {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .termination_state
            .close
            .as_mut()
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .signed = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// Historical exact bytes only. This getter does not authorize a new HTTP,
    /// mint a paid proof, refresh Source, or extend the original's execution UTC.
    pub fn test_session_close_v2_original(&self, id: &str) -> Result<TaskCleanupSubmitV2> {
        let s = self.test_session_v2_record(id)?;
        s.termination_state.validate_input_binding(&s.input)?;
        let (header, _) = s.header()?;
        let original = s
            .termination_state
            .close
            .as_ref()
            .and_then(|a| a.signed.as_ref())
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .clone();
        Ok(TaskCleanupSubmitV2 {
            api_revision: 2,
            expected_world: WorldIdentity {
                world_id: header.termination.world.world_id.clone(),
                genesis_digest: header.termination.world.genesis_digest.clone(),
            },
            original,
        })
    }
}
