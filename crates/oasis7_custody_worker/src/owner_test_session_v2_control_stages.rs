use super::*;
use crate::provision_approval_store::native::client::ClientLiveCurrent;
use crate::test_session_v2_control_rpc::*;
use oasis7_custody_core::{
    native_binding::SignedManagedNativeSessionLifetimeV1, time_units::UnixMillis,
};
#[path = "owner_test_session_v2_control_live.rs"]
mod live;
async fn observed(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    source: &str,
    inspection: bool,
) -> Result<authority::Observations> {
    let saved = store.snapshot().map_err(|_| denied())?;
    let result =
        session_authority::runtime_observe_v2(env, &saved, &rpc.http.selector, source, inspection)
            .await?;
    session_authority::current_v2(state, env, &result, !inspection).map_err(|_| denied())?;
    Ok(result)
}
pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionControlStageRpcV2,
    operation: SessionClientOperation,
    role: Role,
) -> Result<String> {
    if role != Role::Identity
        || rpc.api_version != 2
        || rpc.admission.http.selector.api_version != 2
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(denied());
    }
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let admission = &rpc.admission;
    let id = &admission.http.selector.session_id;
    let saved = store.snapshot().map_err(|_| denied())?;
    if saved.state.test_sessions().contains_key(id)
        || saved
            .state
            .test_session_v2_credential_input_digest(id)
            .map_err(|_| denied())?
            != rpc.approved_input_digest
    {
        return Err(denied());
    }
    // All stages repeat exact PoP and the same ingress charge binding. The first
    // Host charge is independently committed; later failures never refund it.
    let purpose = store
        .charge_control_v2_http(env, admission, operation)
        .await
        .map_err(|_| denied())?;
    let source = source_id(admission)?;
    match rpc.action {
        TestSessionControlActionV2::ClaimSource => {
            let current = observed(state, env, &store, admission, &source, true).await?;
            json(
                store
                    .claim_test_session_v2_source(id, &source, || {
                        session_authority::current_v2(state, env, &current, false)
                    })
                    .map_err(|_| denied())?,
            )
        }
        TestSessionControlActionV2::RecordSource { signed } => {
            let saved = store.snapshot().map_err(|_| denied())?;
            let input = saved
                .state
                .test_session_v2_snapshot(id)
                .map_err(|_| denied())?
                .input()
                .clone();
            let current = session_authority::observe_v2(env, &saved, input, &signed, false).await?;
            store
                .complete_test_session_v2_source(id, &source, *signed, || {
                    session_authority::current_v2(state, env, &current, true)
                })
                .await
                .map_err(|_| denied())?;
            json(())
        }
        TestSessionControlActionV2::Observe { index } => {
            let expected = match index {
                TestSessionCredentialObservationV2::PreDecrypt => 1,
                TestSessionCredentialObservationV2::PostDecrypt => 2,
            };
            observe(
                state, env, &store, admission, operation, &source, expected, false,
            )
            .await
        }
        TestSessionControlActionV2::ClaimLifetime => {
            observe(state, env, &store, admission, operation, &source, 3, true).await
        }
        TestSessionControlActionV2::Complete { lifetime } => {
            let current =
                live::current(state, env, &store, admission, purpose, &source, *lifetime).await?;
            match operation {
                SessionClientOperation::Control(kind) => json(
                    store
                        .complete_control_v2_live(env, admission, purpose, kind, || {
                            Ok(current.clone())
                        })
                        .await
                        .map_err(|_| denied())?,
                ),
                SessionClientOperation::Challenge => {
                    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
                    let random = zeroize::Zeroizing::new(
                        crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
                    );
                    let bytes = zeroize::Zeroizing::new(
                        hex::decode(random.as_str()).map_err(|_| denied())?,
                    );
                    if bytes.len() != 32 {
                        return Err(denied());
                    }
                    json(
                        store
                            .complete_control_v2_challenge(
                                env,
                                admission,
                                purpose,
                                &URL_SAFE_NO_PAD.encode(bytes.as_slice()),
                                || Ok(current.clone()),
                            )
                            .await
                            .map_err(|_| denied())?,
                    )
                }
                _ => Err(denied()),
            }
        }
    }
}
async fn observe(
    state: &JsValue,
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    operation: SessionClientOperation,
    source: &str,
    expected: u8,
    claim: bool,
) -> Result<String> {
    let id = &rpc.http.selector.session_id;
    let ticket = store
        .reserve_test_session_v2_source_read(id, source)
        .map_err(|_| denied())?;
    if ticket != expected {
        return Err(denied());
    }
    let started = clock()?;
    let current = observed(state, env, store, rpc, source, false).await?;
    let descriptor = store
        .observe_control_v2_source(env, rpc, operation, ticket, claim, || {
            session_authority::current_v2(state, env, &current, true)
        })
        .await
        .map_err(|_| denied())?;
    let native = session_authority::current_v2(state, env, &current, true).map_err(|_| denied())?;
    json(live::observation(
        store, id, descriptor, ticket, &native, started,
    )?)
}
