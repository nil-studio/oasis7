//! Identity-only fixed V2 credentials. No caller Current or private body is accepted.
use super::*;
use crate::{
    provision_approval_store::native::client::ClientLiveCurrent,
    test_session_client_rpc::TestSessionClientAdmissionRpc, test_session_v2_credential_rpc::*,
};
use oasis7_custody_core::{
    client_enrollment::*, native_binding::SignedManagedNativeSessionLifetimeV1,
    time_units::UnixMillis,
};
#[path = "owner_test_session_v2_credential_live.rs"]
mod live;
#[path = "owner_test_session_v2_credential_reply.rs"]
mod reply;
fn json<T: serde::Serialize>(value: T) -> Result<String> {
    Ok(serde_json::to_string(&value)?)
}
/// Enter only from the existing first Admit after an exact namespace lookup.
/// The routing reply is metadata; every later fixed V2 stage authenticates again.
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
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let id = &admission.http.selector.session_id;
    if saved.state.test_sessions().contains_key(id)
        || saved.state.test_session_v2_original(id).is_none()
    {
        return Err(denied());
    }
    let mut selected = admission.clone();
    selected.http.selector.api_version = 2;
    // Actual raw PoP is verified and Host charged before reserving Source or
    // emitting routing metadata. Failure cannot fall back to the V1 book.
    let historical_reply = store
        .admit_credential_v2_source(env, &selected, operation)
        .await
        .map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let approved_input_digest = saved
        .state
        .test_session_v2_credential_input_digest(id)
        .map_err(|_| denied())?;
    json(CredentialAdmissionRoutingV2 {
        domain: CREDENTIAL_ROUTING_DOMAIN_V2.into(),
        api_version: 2,
        session_id: id.clone(),
        ingress_attempt_id: admission.ingress_attempt_id.clone(),
        operation: CredentialRoutingOperationV2::from_operation(operation).map_err(|_| denied())?,
        raw_body_digest: oasis7_custody_core::approval::bytes_digest(
            admission.http.original.body.as_bytes(),
        ),
        approved_input_digest,
        historical_reply,
    })
}
pub(crate) fn source_id(rpc: &TestSessionClientAdmissionRpc) -> Result<String> {
    Ok(format!(
        "client-v2-source-{}",
        oasis7_custody_core::approval::bytes_digest(&serde_json::to_vec(&(
            "oasis7.custody.client-ingress-source.v2",
            &rpc.http.selector.session_id,
            &rpc.ingress_attempt_id
        ))?)
    ))
}
fn purpose(
    rpc: &TestSessionClientAdmissionRpc,
    operation: SessionClientOperation,
) -> Result<SessionPurposeV1> {
    match operation {
        SessionClientOperation::Challenge => {
            let q: SessionChallengeRequestV1 = serde_json::from_str(&rpc.http.original.body)?;
            if q.kind != SessionChallengeKindV1::CredentialClaim
                || q.purpose == SessionPurposeV1::TerminationPlan
            {
                return Err(denied());
            }
            Ok(q.purpose)
        }
        SessionClientOperation::CredentialClaim => {
            let q: ClientCredentialClaimRequestV1 = serde_json::from_str(&rpc.http.original.body)?;
            q.validate().map_err(|_| denied())?;
            match q.purposes.first().ok_or_else(denied)? {
                TestSessionPurposeV1::Execute => Ok(SessionPurposeV1::Execution),
                TestSessionPurposeV1::Read => Ok(SessionPurposeV1::ReconcileRead),
                _ => Err(denied()),
            }
        }
        _ => Err(denied()),
    }
}
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
    rpc: TestSessionCredentialStageRpcV2,
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
    store
        .charge_credential_v2_http(env, admission, operation)
        .await
        .map_err(|_| denied())?;
    let purpose = purpose(admission, operation)?;
    let source = source_id(admission)?;
    match rpc.action {
        TestSessionCredentialActionV2::Admit => json(
            store
                .admit_credential_v2_source(env, admission, operation)
                .await
                .map_err(|_| denied())?,
        ),
        TestSessionCredentialActionV2::ClaimSource => {
            // Source creation already paid its Native transport. Persist the
            // distinct first oracle credit before any remote observation.
            if store
                .reserve_test_session_v2_source_read(id, &source)
                .map_err(|_| denied())?
                != 1
            {
                return Err(denied());
            }
            let current = observed(state, env, &store, admission, &source, true).await?;
            json(
                store
                    .claim_test_session_v2_source(id, &source, || {
                        session_authority::current_v2(state, env, &current, false)
                    })
                    .map_err(|_| denied())?,
            )
        }
        TestSessionCredentialActionV2::RecordSource { signed } => {
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
        TestSessionCredentialActionV2::Observe { index } => {
            let expected = match index {
                TestSessionCredentialObservationV2::PreDecrypt => 1,
                TestSessionCredentialObservationV2::PostDecrypt => 2,
            };
            observe(
                state, env, &store, admission, operation, purpose, &source, expected, false,
            )
            .await
        }
        TestSessionCredentialActionV2::ClaimLifetime => {
            observe(
                state, env, &store, admission, operation, purpose, &source, 3, true,
            )
            .await
        }
        TestSessionCredentialActionV2::Complete { lifetime } => {
            let current =
                live::current(state, env, &store, admission, purpose, &source, *lifetime).await?;
            match operation {
                SessionClientOperation::CredentialClaim => {
                    reply::claim(env, &store, admission, current).await
                }
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
                            .complete_credential_v2_challenge(
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
    purpose: SessionPurposeV1,
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
        .observe_credential_v2_source(env, rpc, operation, purpose, ticket, claim, || {
            session_authority::current_v2(state, env, &current, true)
        })
        .await
        .map_err(|_| denied())?;
    let native = session_authority::current_v2(state, env, &current, true).map_err(|_| denied())?;
    json(live::observation(
        store, id, descriptor, ticket, &native, started,
    )?)
}
