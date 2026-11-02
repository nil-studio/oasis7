//! Original Stop allocation and physically named World sends only.
//! Native authentication/VIEW/Actor ports remain separately payable; no route enabled.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationStopRecoveryV2 {
    pub(in crate::provisioning) source_operation_id: String,
    pub(in crate::provisioning) source_record_digest: String,
    pub(in crate::provisioning) source_until: UnixMillis,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
    pub(in crate::provisioning) native_until: UnixMillis,
    pub(in crate::provisioning) original_until: UnixMillis,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) submit_claim: Option<TerminationStopWorldClaimV2>,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) lookup_claim: Option<TerminationStopWorldClaimV2>,
    pub(in crate::provisioning) observations:
        BTreeMap<OwnerSetupCanonicalSourceV1, OwnerSetupResultV1>,
    #[serde(deserialize_with = "required")]
    pub(in crate::provisioning) admitted: Option<Box<TerminationStopAdmissionV2>>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationStopAdmissionV2 {
    pub(in crate::provisioning) result: Box<OwnerSetupResultV1>,
    pub(in crate::provisioning) world: OwnerSetupWorldEvidenceV1,
    pub(in crate::provisioning) source_operation_id: String,
    pub(in crate::provisioning) source_record_digest: String,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
    pub(in crate::provisioning) admitted_at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationStopWorldClaimV2 {
    pub(in crate::provisioning) attempt_id: String,
    pub(in crate::provisioning) claimed_at: UnixMillis,
    pub(in crate::provisioning) paid_index: u32,
    pub(in crate::provisioning) original_digest: String,
    pub(in crate::provisioning) source_operation_id: String,
    pub(in crate::provisioning) source_record_digest: String,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
}
fn required<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
impl TerminationStopAllocationV2 {
    pub(in crate::provisioning) fn record_digest(&self, input: &str, plan: &str) -> Result<String> {
        digest(&(
            "oasis7.custody.stop-allocation.v2",
            input,
            plan,
            &self.payload,
            self.allocated_at,
            &self.recovery.source_operation_id,
            &self.recovery.source_record_digest,
            self.recovery.source_until,
            &self.recovery.native_lifetime,
            self.recovery.native_until,
            self.recovery.original_until,
        ))
    }
}
impl AccountProvisionApprovalState {
    /// Stored signing metadata only; not a permit to dispatch or a refreshed deadline.
    pub fn test_session_stop_v2_original_until(&self, id: &str) -> Result<UnixMillis> {
        Ok(self
            .test_session_v2_record(id)?
            .termination_state
            .stop
            .as_ref()
            .ok_or_else(invalid)?
            .recovery
            .original_until)
    }
    pub fn allocate_test_session_stop_v2(
        &mut self,
        id: &str,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<GameExecutorControlV1> {
        let checks = self.test_session_v2_stop_current_checks(id, current, now, None)?;
        verifier.verify(&checks)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if let Some(old) = &s.termination_state.stop {
            return Ok(old.payload.clone());
        }
        if s.termination_state.close.is_some() || s.termination_state.guarded_stop.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let (header, _) = s.header()?;
        let term = &header.termination;
        // The actual approved policy requires the independent Guarded domain.
        // Cached legacy originals above remain historical data, never upgraded.
        if term.stop_policy == WorldTaskStopPolicyV2::GuardedCommittedUtcStopV2 {
            return Err(invalid());
        }

        let source = s
            .sources
            .get(current.live.source_operation_id)
            .ok_or_else(invalid)?;
        let v = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        let source_until = v
            .observed_at
            .0
            .checked_add(5000)
            .ok_or_else(invalid)?
            .min(source.source.expires_at.0)
            .min(v.proof_expires_at.0)
            .min(v.source_session_expires_at.0)
            .min(v.source_session_idle_expires_at.0);
        let life = &current.live.lifetime.lifetime;
        let native_until = life
            .status
            .expires_at
            .0
            .min(life.session_expires_at.0)
            .min(life.session_idle_expires_at.0);
        let until = source_until.min(native_until).min(term.cleanup_until.get());
        if until <= now.get() {
            return Err(invalid());
        }
        let payload = GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: term.stop_operation_id.clone(),
            fence: term.initial_fence.clone(),
            operation: oasis7_agent_api::game::GameExecutorOperationV1::Stop,
        };
        let mut slot = TerminationStopAllocationV2 {
            payload: payload.clone(),
            allocated_at: now,
            allocation_record_digest: String::new(),
            signed: None,
            recovery: TerminationStopRecoveryV2 {
                source_operation_id: current.live.source_operation_id.into(),
                source_record_digest: digest(source)?,
                source_until: UnixMillis::new(source_until),
                native_lifetime: current.live.lifetime.clone(),
                native_until: UnixMillis::new(native_until),
                original_until: UnixMillis::new(until),
                submit_claim: None,
                lookup_claim: None,
                observations: BTreeMap::new(),
                admitted: None,
            },
        };
        slot.allocation_record_digest = slot.record_digest(
            &s.termination_state.approved_input_digest,
            &s.termination_state.approved_plan_digest,
        )?;
        let mut next = self.clone();
        let record = next.test_session_v2_record_mut(id)?;
        record
            .roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| invalid())?;
        record.termination_state.stop = Some(slot);
        next.validate_persistence()?;
        *self = next;
        Ok(payload)
    }
    pub fn test_session_v2_termination_read(
        &self,
        id: &str,
        source_id: &str,
    ) -> Result<ws::ReadWorldViewRequest> {
        let s = self.test_session_v2_record(id)?;
        let (header, _) = s.header()?;
        let mut read = s
            .preflight_read
            .as_ref()
            .ok_or_else(invalid)?
            .read
            .request
            .clone();
        read.observation_nonce = Some(digest(&(
            "oasis7.custody.test-session-termination-read.v2",
            &s.termination_state.approved_input_digest,
            source_id,
        ))?);
        read.deadline_unix_ms = Some(header.termination.cleanup_until.get());
        read.fixed_commit = None;
        Ok(read)
    }
    pub(in crate::provisioning) fn test_session_v2_stop_current_checks(
        &self,
        id: &str,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        selected: Option<&OwnerSetupResultV1>,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none()
            || s.source_funding.get(current.live.source_operation_id)
                != Some(&SessionSourceFundingV1::TerminationPlan)
        {
            return Err(invalid());
        }
        let checks = self.test_session_v2_live_scope(
            id,
            SessionPurposeV1::TerminationPlan,
            current.live,
            now,
        )?;
        let (header, _) = s.header()?;
        let term = &header.termination;
        let w = current.world;
        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        if w.request.read.request
            != self.test_session_v2_termination_read(id, current.live.source_operation_id)?
            || w.request.read.subject_public_key != term.initial_fence.owner_public_key
            || w.response.request_digest
                != ws::request_digest(
                    ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                    &w.request,
                )
                .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        let observed = GameTaskFenceV1::from_control(
            w.response
                .payload
                .control
                .view
                .game_task_control
                .as_ref()
                .ok_or_else(invalid)?,
        );
        let mut expected = term.initial_fence.clone();
        let stop = selected.or_else(|| {
            s.termination_state
                .stop
                .as_ref()
                .and_then(|x| x.recovery.admitted.as_ref().map(|a| a.result.as_ref()))
        });
        if let Some(OwnerSetupResultV1::Canonical { response, .. }) = stop {
            if let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome {
                let r: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                    serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                expected.task_revision = r.task_revision;
                expected.executor_epoch = r.executor_epoch;
                expected.executor_binding = r.executor_binding;
                expected.task_status = r.task_status;
            }
        }
        if observed != expected
            || w.response.payload.signer_delegation.as_ref() != Some(&term.original_delegation)
        {
            return Err(invalid());
        }
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::World(w, &term.world.service_public_key),
        )?)
    }
}
