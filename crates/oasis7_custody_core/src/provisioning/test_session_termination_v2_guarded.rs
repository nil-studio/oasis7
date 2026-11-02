//! Independent Owner-signed Guarded original. Allocation is not dispatch authority.
use super::*;
use oasis7_agent_api::game::{
    GAME_TASK_STOP_DOMAIN_V2, GameOwnStopIntentV2, GameStopIntentProfileV2, GameTaskStopV2,
    SignedGameTaskStopV2, StopObservationGrantV2,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationGuardedStopAllocationV2 {
    pub(in crate::provisioning) payload: GameTaskStopV2,
    pub(in crate::provisioning) allocated_at: UnixMillis,
    pub(in crate::provisioning) current_observed_at: UnixMillis,
    pub(in crate::provisioning) source_operation_id: String,
    pub(in crate::provisioning) source_record_digest: String,
    pub(in crate::provisioning) source_until: UnixMillis,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
    pub(in crate::provisioning) allocation_record_digest: String,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) signed: Option<SignedGameTaskStopV2>,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) signature_claim: Option<TerminationGuardedVaultClaimV2>,
}
fn required<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
fn native_until(l: &SignedManagedNativeSessionLifetimeV1) -> u64 {
    l.lifetime
        .status
        .expires_at
        .0
        .min(l.lifetime.session_expires_at.0)
        .min(l.lifetime.session_idle_expires_at.0)
}
fn source_until(s: &TestSessionSourceV2) -> Result<u64> {
    let v = &s
        .source
        .verification
        .as_ref()
        .ok_or_else(invalid)?
        .verification;
    Ok(v.observed_at
        .0
        .checked_add(5000)
        .ok_or_else(invalid)?
        .min(s.source.expires_at.0)
        .min(v.proof_expires_at.0)
        .min(v.source_session_expires_at.0)
        .min(v.source_session_idle_expires_at.0))
}
impl TerminationGuardedStopAllocationV2 {
    fn record_digest(&self, input: &str, plan: &str) -> Result<String> {
        digest(&(
            "oasis7.custody.guarded-stop-allocation.v2",
            input,
            plan,
            &self.payload,
            self.allocated_at,
            self.current_observed_at,
            &self.source_operation_id,
            &self.source_record_digest,
            self.source_until,
            &self.native_lifetime,
        ))
    }
    pub(in crate::provisioning) fn validate_session(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let source = s
            .sources
            .get(&self.source_operation_id)
            .ok_or_else(invalid)?;
        let nonce = digest(&(
            "oasis7.custody.test-session-current-lifetime.v2",
            &s.header()?.0.termination.session_id,
            &s.termination_state.approved_input_digest,
            &self.source_operation_id,
            &source.source.verify_nonce,
        ))?;
        if let Some(claim) = &self.signature_claim {
            claim.validate_session(self, s)?;
        }
        let original = s.native_lifetime.as_ref().ok_or_else(invalid)?;
        self.native_lifetime
            .lifetime
            .validate_for(
                &original.lifetime.status.receipt,
                &nonce,
                s.session.native_binding_generation,
                s.input.request.context.service_epoch,
                self.allocated_at.get(),
            )
            .map_err(|_| invalid())?;
        if self.native_lifetime.lifetime.status.session_ref != s.session.session_ref {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_shape(&self) -> Result<()> {
        self.payload.validate().map_err(|_| invalid())?;
        self.native_lifetime.validate().map_err(|_| invalid())?;
        let current_until = self
            .current_observed_at
            .get()
            .checked_add(5000)
            .ok_or_else(invalid)?;
        if self.current_observed_at.get() == 0
            || self.current_observed_at.get() > self.allocated_at.get()
            || self.allocated_at.get() >= current_until
            || self.payload.execute_until_ms.0 > current_until
        {
            return Err(invalid());
        }
        if self.payload.observation_grant.is_none()
            || self.allocated_at.get() == 0
            || self.allocated_at.get() >= self.payload.execute_until_ms.0
            || self.payload.execute_until_ms.0 > self.source_until.get()
            || self.payload.execute_until_ms.0 > native_until(&self.native_lifetime)
            || self.source_operation_id.is_empty()
            || self.source_operation_id.len() > 256
        {
            return Err(invalid());
        }
        if self.signed.is_some() && self.signature_claim.is_none() {
            return Err(invalid());
        }
        if let Some(claim) = &self.signature_claim {
            claim.validate(self)?;
        }
        if let Some(signed) = &self.signed {
            signed.validate_shape().map_err(|_| invalid())?;
            if signed.request != self.payload {
                return Err(invalid());
            }
            ws::private_goal::verify_owner_signature_strict(
                GAME_TASK_STOP_DOMAIN_V2,
                &signed.request,
                &signed.subject_public_key,
                &signed.signature_hex,
            )
            .map_err(|_| invalid())?;
        }
        Ok(())
    }
    pub(super) fn validate_source(
        &self,
        sources: &BTreeMap<String, TestSessionSourceV2>,
        funding: &BTreeMap<String, SessionSourceFundingV1>,
        input: &str,
    ) -> Result<()> {
        let s = sources.get(&self.source_operation_id).ok_or_else(invalid)?;
        let v = &s
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if funding.get(&self.source_operation_id) != Some(&SessionSourceFundingV1::TerminationPlan)
            || s.api_version != 2
            || s.approved_input_digest != input
            || digest(s)? != self.source_record_digest
            || source_until(s)? != self.source_until.get()
            || self.allocated_at.get() < v.observed_at.0
            || s.source
                .verified_at
                .is_none_or(|t| t.0 > self.allocated_at.get())
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_input(
        &self,
        t: &TerminationPlanV2,
        input: &str,
        plan: &str,
    ) -> Result<()> {
        self.validate_shape()?;
        let p = &self.payload;
        let expected = StopObservationGrantV2::known_control_for_exact_target(
            p,
            t.cleanup_until.get(),
            t.cleanup_until.get(),
            t.cleanup_until.get(),
        )
        .map_err(|_| invalid())?;
        if p.client_operation_id != t.stop_operation_id
            || p.custody_account_id != t.custody_account_id
            || p.termination_plan_digest != t.digest().map_err(|_| invalid())?
            || p.initial_fence != t.initial_fence
            || p.target_executor_public_key != t.original_delegation.delegate_public_key
            || p.target_executor_generation.0 != t.original_delegation.generation
            || p.target_executor_nonce.0 != t.original_delegation.nonce
            || p.execute_until_ms.0
                != t.cleanup_until
                    .get()
                    .min(self.source_until.get())
                    .min(native_until(&self.native_lifetime))
                    .min(
                        self.current_observed_at
                            .get()
                            .checked_add(5000)
                            .ok_or_else(invalid)?,
                    )
            || p.observation_grant.as_ref() != Some(&expected)
            || self.record_digest(input, plan)? != self.allocation_record_digest
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl TestSessionTerminationStateV2 {
    pub(super) fn guarded_stop_intent(&self) -> Result<Option<GameOwnStopIntentV2>> {
        // A legacy original cannot be relabelled into the new Owner domain.
        if self.stop.is_some() {
            return Err(invalid());
        }
        self.guarded_stop
            .as_ref()
            .map(|s| {
                Ok(GameOwnStopIntentV2 {
                    profile: GameStopIntentProfileV2::GuardedTaskStopV2,
                    operation_id: s.payload.client_operation_id.clone(),
                    intent_digest: oasis7_wasm_abi::canonical_hash(&(
                        GAME_TASK_STOP_DOMAIN_V2,
                        &s.payload.initial_fence.owner_public_key,
                        &s.payload,
                    ))
                    .map_err(|_| invalid())?,
                })
            })
            .transpose()
    }
}
impl AccountProvisionApprovalState {
    /// Fixed approved target only. No World observation or physical send occurs here.
    pub fn allocate_test_session_guarded_stop_v2(
        &mut self,
        id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<GameTaskStopV2> {
        let checks =
            self.test_session_v2_live_scope(id, SessionPurposeV1::TerminationPlan, current, now)?;
        verifier.verify(&checks)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none()
            || s.source_funding.get(current.source_operation_id)
                != Some(&SessionSourceFundingV1::TerminationPlan)
            || !s
                .native_machine_registration
                .as_ref()
                .is_some_and(|m| m.ready())
        {
            return Err(invalid());
        }
        s.termination_state.validate_input_binding(&s.input)?;
        if let Some(old) = &s.termination_state.guarded_stop {
            return Ok(old.payload.clone());
        }
        if s.termination_state.stop.is_some() || s.termination_state.close.is_some() {
            return Err(invalid());
        }
        let t = &s.header()?.0.termination;
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        let until = source_until(source)?
            .min(native_until(current.lifetime))
            .min(t.cleanup_until.get())
            .min(
                current
                    .observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
            );
        if now.get() >= until {
            return Err(invalid());
        }
        let mut payload = GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: t.stop_operation_id.clone(),
            custody_account_id: t.custody_account_id.clone(),
            termination_plan_digest: t.digest().map_err(|_| invalid())?,
            initial_fence: t.initial_fence.clone(),
            target_executor_public_key: t.original_delegation.delegate_public_key.clone(),
            target_executor_generation: DecimalU64(t.original_delegation.generation),
            target_executor_nonce: DecimalU64(t.original_delegation.nonce),
            execute_until_ms: DecimalU64(until),
            observation_grant: None,
        };
        payload.observation_grant = Some(
            StopObservationGrantV2::known_control_for_exact_target(
                &payload,
                t.cleanup_until.get(),
                t.cleanup_until.get(),
                t.cleanup_until.get(),
            )
            .map_err(|_| invalid())?,
        );
        payload.validate().map_err(|_| invalid())?;
        let mut allocation = TerminationGuardedStopAllocationV2 {
            payload: payload.clone(),
            allocated_at: now,
            current_observed_at: current.observed_at,
            source_operation_id: current.source_operation_id.into(),
            source_record_digest: digest(source)?,
            source_until: UnixMillis::new(source_until(source)?),
            native_lifetime: current.lifetime.clone(),
            allocation_record_digest: String::new(),
            signed: None,
            signature_claim: None,
        };
        allocation.allocation_record_digest = allocation.record_digest(
            &s.termination_state.approved_input_digest,
            &s.termination_state.approved_plan_digest,
        )?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| invalid())?;
        s.termination_state.guarded_stop = Some(allocation);
        next.validate_persistence()?;
        *self = next;
        Ok(payload)
    }
    /// Archive the actual Vault response; this never grants dispatch or Current.
    pub fn record_test_session_guarded_stop_signature_v2(
        &mut self,
        id: &str,
        signed: SignedGameTaskStopV2,
    ) -> Result<()> {
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let slot = s
            .termination_state
            .guarded_stop
            .as_mut()
            .ok_or_else(invalid)?;
        if signed.request != slot.payload || slot.signed.as_ref().is_some_and(|old| old != &signed)
        {
            return Err(invalid());
        }
        slot.signed = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
