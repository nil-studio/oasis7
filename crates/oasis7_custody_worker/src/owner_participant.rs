//! Fixed first-participant provisioning; no generic signing or arbitrary forwarding.
use super::*;
#[path = "owner_participant_authority.rs"]
mod participant_authority;
#[path = "owner_participant_dispatch.rs"]
mod participant_dispatch;

pub(crate) async fn participant_dispatch(
    state: &JsValue,
    env: &Env,
    rpc: crate::participant_runtime_rpc::ParticipantRuntimeRpc,
    static_role: Role,
) -> Result<String> {
    participant_dispatch::dispatch(state, env, rpc, static_role).await
}

pub(crate) async fn start_participant_workflow(
    state: &JsValue,
    env: &Env,
    request: oasis7_custody_core::provisioning::ParticipantProvisionRequestV1,
) -> Result<()> {
    participant_dispatch::start_workflow(state, env, request).await
}
