//! Actual fixed Directory1 + RF reader1 + RF object1, no free old callback.
//! A private non-Serde observation is returned only after the owned paid token
//! has been consumed and both real fixed requests succeeded.
use super::*;
use oasis7_custody_core::{
    provisioning::{ExecutionPrepareCurrentPortsV2, ExecutionPrepareSigningAuthorityV2},
    time_units::UnixMillis,
};
pub(crate) struct ExecutionPrepareObservationV2 {
    observed: Observations,
    funding: ExecutionPrepareCurrentPortsV2,
    until: UnixMillis,
}
impl ExecutionPrepareObservationV2 {
    pub(crate) fn current(
        &self,
        state: &JsValue,
        env: &Env,
    ) -> std::result::Result<NativeStageCurrent, JsValue> {
        if clock().map_err(|_| JsValue::from_str("execution current unavailable"))?
            >= self.until.get()
        {
            return Err(JsValue::from_str("execution current unavailable"));
        }
        v2::current_v2(state, env, &self.observed, false)
    }
    /// Metadata for the fixed issuer child, not permission to skip final Core
    /// checks. Actual opaque Core authority must bind this exact paid read.
    pub(crate) fn check_authority(
        &self,
        authority: &ExecutionPrepareSigningAuthorityV2,
        now: UnixMillis,
    ) -> Result<()> {
        let b = authority.bundle();
        if now.get() >= self.until.get()
            || now.get() < authority.admitted_at().get()
            || self.funding.session_id() != b.session_id
            || self.funding.approved_input_digest() != b.approved_input_digest
            || self.funding.factory_key() != authority.paid_key()
            || self.funding.read().source_operation_id != b.source_operation_id
            || self.funding.source_nonce() != b.source_nonce
            || self.funding.read().source_ticket != 5
            || authority.admitted_at().get() < self.funding.read().claimed_at.get()
            || authority.admitted_at().get() < self.observed.gate.observed_at.0
            || authority.admitted_at().get() < self.observed.fence_started.0
        {
            return Err(denied());
        }
        Ok(())
    }
    pub(crate) fn until(&self) -> UnixMillis {
        self.until
    }
    pub(crate) fn directory_observed_at(&self) -> DecimalU64 {
        self.observed.gate.observed_at
    }
    pub(crate) fn fence_started(&self) -> DecimalU64 {
        self.observed.fence_started
    }
}
pub(crate) async fn observe_execution_prepare_v2(
    env: &Env,
    draft: &oasis7_custody_core::provisioning::AccountDraftV1,
    input: ApprovalAdmissionInput,
    signed: &SignedApprovalVerificationV1,
    funding: ExecutionPrepareCurrentPortsV2,
) -> Result<ExecutionPrepareObservationV2> {
    if v2::profile(env, &input)? != v2::Profile::TestSession
        || input.operation_id != funding.session_id()
        || draft.entry.environment != funding.environment()
        || &draft.entry.manager != funding.manager()
        || signed.verification.verify_nonce != funding.source_nonce()
        || funding.read().source_ticket != 5
        || clock()? >= funding.expires_at().get()
        || clock()? < funding.read().claimed_at.get()
    {
        return Err(denied());
    }
    let manager = manager(&input, signed);
    let config = Config::load(env)?;
    if config.directory.principal(&manager)? != draft.entry.manager {
        return Err(denied());
    }
    let rpc = ApproveRpc {
        manager_session: manager.clone(),
        provision_id: draft.entry.provision_id.clone(),
        request: oasis7_custody_core::provisioning::ProvisionApproveRequestV1 {
            api_version: 1,
            input: input.clone(),
        },
    };
    // Exactly one Manager DO request plus the existing fixed RF service RPC
    // and its fixed FENCES object request. Independent readonly pair only;
    // no retry, Native/World call, response-selected path or authority callback.
    let (gate_result, fence_result) = paid_factory_reads(gate(env, &rpc, draft), fence(env)).await;
    let gate = gate_result?;
    let (fence, fence_started) = fence_result?;
    let until = UnixMillis::new(
        [
            funding.expires_at().get(),
            gate.observed_at.0.checked_add(5000).ok_or_else(denied)?,
            fence_started.0.checked_add(5000).ok_or_else(denied)?,
        ]
        .into_iter()
        .min()
        .ok_or_else(denied)?,
    );
    if clock()? >= until.get() {
        return Err(denied());
    }
    Ok(ExecutionPrepareObservationV2 {
        observed: Observations {
            input,
            manager,
            gate,
            fence,
            fence_started,
            paid_started: None,
            custody_v2: Some(v2::Profile::TestSession),
            inspection_only: false,
        },
        funding,
        until,
    })
}
