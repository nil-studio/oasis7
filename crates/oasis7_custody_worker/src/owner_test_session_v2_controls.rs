//! Closed Control-only V2 routing. No caller Current and no V1 authority projection.
use super::*;
use crate::{
    test_session_client_rpc::TestSessionClientAdmissionRpc, test_session_v2_control_rpc::*,
};
fn json<T: serde::Serialize>(v: T) -> Result<String> {
    Ok(serde_json::to_string(&v)?)
}
pub(crate) async fn first_admit(
    state: &JsValue,
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    operation: SessionClientOperation,
    role: Role,
) -> Result<String> {
    if role != Role::Identity
        || admission.http.selector.api_version != 1
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(denied());
    }
    let typed = ControlRoutingOperationV2::from_operation(operation).map_err(|_| denied())?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let id = &admission.http.selector.session_id;
    let snapshot = store.snapshot().map_err(|_| denied())?;
    if snapshot.state.test_sessions().contains_key(id)
        || snapshot.state.test_session_v2_original(id).is_none()
    {
        return Err(denied());
    }
    let mut selected = admission.clone();
    selected.http.selector.api_version = 2;
    store
        .charge_control_v2_http(env, &selected, operation)
        .await
        .map_err(|_| denied())?;
    let immediate_reply = match store
        .control_v2_history(env, &selected, operation)
        .await
        .map_err(|_| denied())?
    {
        Some(ack) => Some(ack),
        None => match operation {
            SessionClientOperation::Control(
                k @ (SessionControlKindV1::Finish | SessionControlKindV1::Abort),
            ) => Some(
                store
                    .complete_control_v2_tightening(env, &selected, k)
                    .await
                    .map_err(|_| denied())?,
            ),
            _ => {
                store
                    .reserve_control_v2_source(env, &selected, operation)
                    .await
                    .map_err(|_| denied())?;
                None
            }
        },
    };
    let saved = store.snapshot().map_err(|_| denied())?;
    json(ControlAdmissionRoutingV2 {
        domain: CONTROL_ROUTING_DOMAIN_V2.into(),
        api_version: 2,
        session_id: id.clone(),
        ingress_attempt_id: admission.ingress_attempt_id.clone(),
        operation: typed,
        raw_body_digest: oasis7_custody_core::approval::bytes_digest(
            admission.http.original.body.as_bytes(),
        ),
        approved_input_digest: saved
            .state
            .test_session_v2_credential_input_digest(id)
            .map_err(|_| denied())?,
        immediate_reply,
    })
}

#[path = "owner_test_session_v2_control_stages.rs"]
mod stages;
pub(crate) use stages::dispatch;

fn source_id(rpc: &TestSessionClientAdmissionRpc) -> Result<String> {
    Ok(format!(
        "control-v2-source-{}",
        oasis7_custody_core::approval::bytes_digest(&serde_json::to_vec(&(
            "oasis7.custody.control-ingress-source.v2",
            &rpc.http.selector.session_id,
            &rpc.ingress_attempt_id
        ))?)
    ))
}
