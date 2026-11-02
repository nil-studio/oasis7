//! One fixed Account RPC; fee settlement survives a later graph rejection.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http::Role, native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
    provisioning::NativeModelReserveOutcomeV2,
};
use serde::Serialize;
const DOMAIN: &str = "oasis7.custody.native-model-reservation-pending.v2";
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum PendingStatus {
    PaidPending,
    ExactHistory,
}
#[derive(Serialize)]
struct PendingReply {
    api_version: u8,
    domain: &'static str,
    session_id: String,
    reservation_id: String,
    query_digest: String,
    status: PendingStatus,
}
pub(crate) async fn admit(
    state: &JsValue,
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    role: Role,
) -> Result<String> {
    if role != Role::Identity || admission.http.selector.api_version != 2 {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let initial = store.snapshot().map_err(|_| denied())?;
    let id = &admission.http.selector.session_id;
    if initial.state.test_sessions().contains_key(id)
        || initial.state.test_session_v2_original(id).is_none()
    {
        return Err(denied());
    }
    // Exactly one received Host HTTP, two fixed abuse Actor routes and the two service/Account routes have
    // occurred. This commit precedes any additional source/current/provider IO.
    store
        .settle_native_model_entry_v2(env, admission)
        .await
        .map_err(|_| denied())?;
    // This is intentionally another transaction in the same Account invocation.
    // Failure cannot undo settlement and never issues an Account floor.
    let outcome = store
        .reserve_native_model_pending_v2(env, admission)
        .await
        .map_err(|_| denied())?;
    let query: SignedNativeModelReservationQueryV2 =
        serde_json::from_str(&admission.http.original.body).map_err(|_| denied())?;
    Ok(serde_json::to_string(&PendingReply {
        api_version: 2,
        domain: DOMAIN,
        session_id: id.clone(),
        reservation_id: query.query.reservation_id.clone(),
        query_digest: oasis7_custody_core::approval::bytes_digest(
            admission.http.original.body.as_bytes(),
        ),
        status: match outcome {
            NativeModelReserveOutcomeV2::RecordedPending => PendingStatus::PaidPending,
            NativeModelReserveOutcomeV2::ExactHistory => PendingStatus::ExactHistory,
        },
    })?)
}
