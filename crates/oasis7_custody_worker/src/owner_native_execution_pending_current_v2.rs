//! First master-paid Actor consumer. It cannot issue an ordinary free C3 or
//! fabricate the later RecoveryFence observation. Its private result remains in
//! the same Account handler and carries the original conservative read window.
use super::*;
use crate::directory_rpc::ProvisionGateView;
use oasis7_custody_core::{
    provisioning::{AccountProvisionApprovalState, ExecutionPendingDirectoryDispatchV2},
    time_units::UnixMillis,
};
pub(crate) struct PendingDirectoryObservationV2 {
    pub(super) view: ProvisionGateView,
    pub(super) funding: ExecutionPendingDirectoryDispatchV2,
    pub(super) until: UnixMillis,
}
pub(crate) async fn observe_pending_directory_v2(
    env: &Env,
    state: &AccountProvisionApprovalState,
    funding: ExecutionPendingDirectoryDispatchV2,
) -> Result<PendingDirectoryObservationV2> {
    let original = state
        .test_session_v2_original(funding.session_id())
        .ok_or_else(denied)?;
    let input = original.input();
    if v2::profile(env, input)? != v2::Profile::TestSession
        || input.operation_id != funding.session_id()
        || input
            .approved_test_session_v2_digest()
            .map_err(|_| denied())?
            != funding.proof().claim.approved_input_digest
        || input.request.context.environment != funding.scope().context.identity.environment
        || input.request.context.custody_account_id != funding.scope().context.identity.account_id
        || state.draft().entry.provision_id != funding.scope().provision_id
        || clock()? >= funding.expires_at().get()
    {
        return Err(denied());
    }
    let body = serde_json::to_string(funding.proof())?;
    if body.len() > 32768 {
        return Err(denied());
    }
    let raw = crate::owner_directory::call_pending_directory(env, funding.proof()).await?;
    let reply: crate::owner_directory::PendingDirectoryReplyV2 =
        serde_json::from_str(&raw).map_err(|_| denied())?;
    if reply.api_version != 2
        || reply.claim_digest != funding.proof().digest().map_err(|_| denied())?
    {
        return Err(denied());
    }
    let view = reply.view;
    check_gate(&view, state.draft(), clock()?)?;
    let until = UnixMillis::new(
        funding
            .expires_at()
            .get()
            .min(view.observed_at.0.checked_add(5000).ok_or_else(denied)?),
    );
    if clock()? >= until.get() {
        return Err(denied());
    }
    Ok(PendingDirectoryObservationV2 {
        view,
        funding,
        until,
    })
}

/// Only Directory plus independent RF observations. This is deliberately not
/// a Native Source, Full Current, signing authority or a captured context.
pub(crate) struct PendingDirectoryFenceObservationV2 {
    // Original non-wire Core continuation, retained only by the same handler.
    first: Option<oasis7_custody_core::provisioning::PendingFenceDispatchV2>,
    pub(super) view: ProvisionGateView,
    pub(super) fence: RecoveryFence,
    pub(super) fence_observed_at: DecimalU64,
    pub(super) until: UnixMillis,
}
impl PendingDirectoryFenceObservationV2 {
    // Only this module constructs the private first field after actual replies.
    pub(crate) fn source_session_id(&self) -> Result<String> {
        Ok(self
            .first
            .as_ref()
            .ok_or_else(denied)?
            .proof()
            .binding
            .session_id
            .clone())
    }
    pub(crate) fn source_until(&self) -> UnixMillis {
        self.until
    }
    pub(crate) fn check_source_current(
        &self,
        env: &Env,
        state: &AccountProvisionApprovalState,
        now: u64,
    ) -> Result<()> {
        check_gate(&self.view, state.draft(), now)?;
        self.fence
            .check_epoch(Config::load(env)?.service_epoch.0)
            .map_err(|_| denied())?;
        if self.fence_observed_at.0 > now || now >= self.until.get() {
            return Err(denied());
        }
        Ok(())
    }
    pub(crate) fn take_source_first(
        &mut self,
    ) -> Result<oasis7_custody_core::provisioning::PendingFenceDispatchV2> {
        self.first.take().ok_or_else(denied)
    }
}
pub(super) async fn observe_pending_first_fence_v2(
    env: &Env,
    store: &crate::provision_approval_store::OwnerProvisionApprovalStore,
    directory: PendingDirectoryObservationV2,
) -> Result<PendingDirectoryFenceObservationV2> {
    let saved = store.snapshot().map_err(|_| denied())?;
    check_gate(&directory.view, saved.state.draft(), clock()?)?;
    if clock()? >= directory.until.get() {
        return Err(denied());
    }
    let first = store
        .sign_pending_first_fence_v2(env, directory.funding)
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let proof = first.proof();
    let namespace = env
        .durable_object("EXECUTION_FENCE_DISPATCHERS")
        .map_err(|_| denied())?;
    let name = proof.binding.receiver_name().map_err(|_| denied())?;
    if namespace
        .id_from_name(&name)
        .map_err(|_| denied())?
        .to_string()
        != proof.binding.dispatcher_object_id
    {
        return Err(denied());
    }
    let until = directory.until.get().min(proof.binding.expires_at.0);
    let digest = proof.digest().map_err(|_| denied())?;
    let raw = serde_json::to_string(proof).map_err(|_| denied())?;
    let reply = send_pending_fence(env.as_ref(), &name, &raw, until as f64)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    if reply.len() > 32768 {
        return Err(denied());
    }
    let result: oasis7_custody_core::execution_fence_dispatch_v2::ExecutionFenceObservationV2 =
        serde_json::from_str(&reply).map_err(|_| denied())?;
    let now = clock()?;
    let after = store.snapshot().map_err(|_| denied())?;
    check_gate(&directory.view, after.state.draft(), now)?;
    if result.api_version != 2
        || result.dispatch_digest != digest
        || result.observed_at.0 < proof.binding.issued_at.0
        || result.observed_at.0 > now
        || now >= until
    {
        return Err(denied());
    }
    let config = Config::load(env)?;
    result
        .fence
        .check_epoch(config.service_epoch.0)
        .map_err(|_| denied())?;
    Ok(PendingDirectoryFenceObservationV2 {
        first: Some(first),
        view: directory.view,
        fence: result.fence,
        fence_observed_at: result.observed_at,
        until: UnixMillis::new(until),
    })
}
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/execution_fence_transport_v2.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=sendExecutionFenceDispatcher)]
    async fn send_pending_fence(
        env: &JsValue,
        name: &str,
        body: &str,
        until: f64,
    ) -> std::result::Result<JsValue, JsValue>;
}

/// Same-handler first-token chain. This returns only the first Directory/RF
/// observation, never Source, Full Current, Capture or an Actions response.
/// History has no accepted token and therefore cannot repeat either outbound.
pub(super) async fn observe_pending_first_directory_fence_v2(
    env: &Env,
    store: &crate::provision_approval_store::OwnerProvisionApprovalStore,
    request: &crate::test_session_client_rpc::TestSessionClientAdmissionRpc,
) -> Result<Option<PendingDirectoryFenceObservationV2>> {
    let Some(accepted) = store
        .accept_execution_pending_start_v2(env, request)
        .await
        .map_err(|_| denied())?
    else {
        return Ok(None);
    };
    let attempt = store
        .claim_pending_first_directory_v2(env, accepted)
        .await
        .map_err(|_| denied())?;
    let funding = store
        .sign_pending_first_directory_v2(env, attempt)
        .map_err(|_| denied())?
        .ok_or_else(denied)?;
    let before = store.snapshot().map_err(|_| denied())?;
    let directory = observe_pending_directory_v2(env, &before.state, funding).await?;
    observe_pending_first_fence_v2(env, store, directory)
        .await
        .map(Some)
}

/// Advances the original private D/F observation to its one paid Source
/// subtree. A cold/public proof cannot construct the first parent value.
/// The returned history token still requires paid PreArchive C3 before Source
/// admission, the three later Observe slots, LF and Book birth.
pub(super) async fn observe_pending_source_inspect_v2(
    env: &Env,
    store: &crate::provision_approval_store::OwnerProvisionApprovalStore,
    mut current: PendingDirectoryFenceObservationV2,
) -> Result<oasis7_custody_core::provisioning::PendingSourceHistoryV2> {
    let before = store.snapshot().map_err(|_| denied())?;
    let now = clock()?;
    check_gate(&current.view, before.state.draft(), now)?;
    current
        .fence
        .check_epoch(Config::load(env)?.service_epoch.0)
        .map_err(|_| denied())?;
    if now >= current.until.get() {
        return Err(denied());
    }
    let dispatch = store
        .sign_pending_source_inspect_v2(env, &mut current)
        .await
        .map_err(|_| denied())?;
    let q = dispatch.request();
    let name = q.proof.binding.receiver_name().map_err(|_| denied())?;
    let actual = env
        .durable_object("EXECUTION_SOURCE_INSPECT_DISPATCHERS")
        .map_err(|_| denied())?
        .id_from_name(&name)
        .map_err(|_| denied())?
        .to_string();
    if actual != q.proof.binding.dispatcher_object_id {
        return Err(denied());
    }
    let until = current.until.get().min(q.proof.binding.expires_at.0);
    let bytes = q.wire_bytes().map_err(|_| denied())?;
    let body = std::str::from_utf8(&bytes).map_err(|_| denied())?;
    let raw = send_source_inspect(env.as_ref(), &name, body, until as f64)
        .await
        .map_err(|_| denied())?
        .as_string()
        .ok_or_else(denied)?;
    let reply = oasis7_custody_core::execution_pending_source_result_v2::SignedPendingSourceInspectResultV2::decode(raw.as_bytes()).map_err(|_| denied())?;
    let after = store.snapshot().map_err(|_| denied())?;
    let now = clock()?;
    check_gate(&current.view, after.state.draft(), now)?;
    current
        .fence
        .check_epoch(Config::load(env)?.service_epoch.0)
        .map_err(|_| denied())?;
    if now >= until {
        return Err(denied());
    }
    // The Account archive compares the actual permanent Native leaf receipt
    // and both preserved signatures using fixed NativeRoot deployment pins.
    store
        .archive_pending_source_inspect_history_v2(env, dispatch, reply)
        .map_err(|_| denied())
}
#[wasm_bindgen::prelude::wasm_bindgen(
    module = "/platform/execution_source_inspect_transport_v2.mjs"
)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch, js_name=sendPendingSourceInspectDispatcher)]
    async fn send_source_inspect(
        env: &JsValue,
        name: &str,
        body: &str,
        until: f64,
    ) -> std::result::Result<JsValue, JsValue>;
}
