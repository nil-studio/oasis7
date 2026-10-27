//! Closed fixed-route client dispatcher; no serialized Current or role reaches Core.
use super::*;
use oasis7_custody_core::client_enrollment::{
    ClientCredentialClaimRequestV1, TestSessionPurposeV1,
};
use zeroize::Zeroizing;

pub(crate) async fn dispatch(
    state: &JsValue,
    env: &Env,
    rpc: TestSessionClientStageRpc,
    operation: SessionClientOperation,
    role: Role,
) -> Result<String> {
    if role != Role::Identity {
        return Err(denied());
    }
    session_authority::enabled(env)?;
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let admission = &rpc.admission;
    let selector = &admission.http.selector;
    let id = &selector.session_id;
    let purpose = store
        .revalidate_client_http(env, admission, operation)
        .await
        .map_err(|_| denied())?;
    let source = client_source_operation_id(admission).map_err(|_| denied())?;
    let local = matches!(
        operation,
        SessionClientOperation::Control(SessionControlKindV1::Finish | SessionControlKindV1::Abort)
    );
    match rpc.action {
        TestSessionClientAction::Admit => {
            let reserved = store
                .admit_client_http(env, admission, operation, purpose)
                .await
                .map_err(|_| denied())?;
            json((reserved.source, reserved.descriptor))
        }
        TestSessionClientAction::Complete { lifetime } if local => {
            if lifetime.is_some() {
                return Err(denied());
            }
            let SessionClientOperation::Control(kind) = operation else {
                return Err(denied());
            };
            json(
                store
                    .control_client_session(env, admission, kind, purpose, || {
                        Err(JsValue::from_str("no live authority"))
                    })
                    .await
                    .map_err(|_| denied())?,
            )
        }
        _ if local => Err(denied()),
        TestSessionClientAction::ClaimSource => {
            let current = observed(state, env, &store, admission, &source, true).await?;
            json(
                store
                    .claim_test_session_source(id, &source, || {
                        session_authority::current(state, env, &current, false)
                    })
                    .map_err(|_| denied())?,
            )
        }
        TestSessionClientAction::RecordSource { signed } => {
            let saved = store.snapshot().map_err(|_| denied())?;
            let input = session_authority::input(&saved, selector)?;
            let current = session_authority::observe(env, &saved, input, &signed, false).await?;
            store
                .complete_test_session_source(id, &source, *signed, || {
                    session_authority::current(state, env, &current, true)
                })
                .await
                .map_err(|_| denied())?;
            json(())
        }
        TestSessionClientAction::Observe { index } => {
            let ticket = store
                .reserve_test_session_source_read(id, &source)
                .map_err(|_| denied())?;
            let expected = match index {
                TestSessionClientObservation::PreDecrypt => 1,
                TestSessionClientObservation::PostDecrypt => 2,
            };
            if ticket != expected {
                return Err(denied());
            }
            let started = clock()?;
            let current = observed(state, env, &store, admission, &source, false).await?;
            let descriptor = store
                .client_source_descriptor(id, &source, ticket, purpose, || {
                    session_authority::current(state, env, &current, true)
                })
                .await
                .map_err(|_| denied())?;
            let native =
                session_authority::current(state, env, &current, true).map_err(|_| denied())?;
            json(observation(
                &store, id, descriptor, ticket, &native, started,
            )?)
        }
        TestSessionClientAction::ClaimLifetime => {
            let ticket = store
                .reserve_test_session_source_read(id, &source)
                .map_err(|_| denied())?;
            if ticket != 3 {
                return Err(denied());
            }
            let started = clock()?;
            let current = observed(state, env, &store, admission, &source, false).await?;
            let descriptor = store
                .claim_client_lifetime(id, &source, purpose, || {
                    session_authority::current(state, env, &current, true)
                })
                .await
                .map_err(|_| denied())?;
            let native =
                session_authority::current(state, env, &current, true).map_err(|_| denied())?;
            json(observation(
                &store, id, descriptor, ticket, &native, started,
            )?)
        }
        TestSessionClientAction::Complete { lifetime } => {
            let current = live(
                state,
                env,
                &store,
                admission,
                &source,
                *lifetime.ok_or_else(denied)?,
            )
            .await?;
            match operation {
                SessionClientOperation::Challenge => {
                    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
                    let random = Zeroizing::new(
                        crate::approval_crypto::verification_nonce().map_err(|_| denied())?,
                    );
                    let bytes = Zeroizing::new(hex::decode(random.as_str()).map_err(|_| denied())?);
                    let nonce = URL_SAFE_NO_PAD.encode(bytes.as_slice());
                    json(
                        store
                            .issue_client_challenge(env, admission, &nonce, purpose, || {
                                Ok(current.clone())
                            })
                            .await
                            .map_err(|_| denied())?,
                    )
                }
                SessionClientOperation::Control(kind) => json(
                    store
                        .control_client_session(env, admission, kind, purpose, || {
                            Ok(current.clone())
                        })
                        .await
                        .map_err(|_| denied())?,
                ),
                SessionClientOperation::CredentialClaim => {
                    credentials(env, &store, admission, current).await
                }
            }
        }
    }
}
fn claim_purpose(purpose: TestSessionPurposeV1) -> Result<SessionPurposeV1> {
    match purpose {
        TestSessionPurposeV1::Execute => Ok(SessionPurposeV1::Execution),
        TestSessionPurposeV1::Read => Ok(SessionPurposeV1::ReconcileRead),
        _ => Err(denied()),
    }
}
async fn credentials(
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    admission: &TestSessionClientAdmissionRpc,
    current: ClientLiveCurrent,
) -> Result<String> {
    use crate::test_session_client_response::*;
    let request: ClientCredentialClaimRequestV1 =
        serde_json::from_str(&admission.http.original.body)?;
    request.validate().map_err(|_| denied())?;
    let mut tokens = Vec::new();
    let mut allocations = Vec::new();
    for selected in &request.purposes {
        let purpose = claim_purpose(*selected)?;
        let token =
            Zeroizing::new(crate::approval_crypto::verification_nonce().map_err(|_| denied())?);
        let digest =
            hex::encode(oasis7_custody_core::http_pop::token_digest(&token).map_err(|_| denied())?);
        allocations.push(SessionCredentialAllocation { purpose, digest });
        tokens.push((purpose, token));
    }
    let outcome = store
        .claim_client_credentials(env, admission, &allocations, || Ok(current.clone()))
        .await
        .map_err(|_| denied())?;
    let id = &admission.http.selector.session_id;
    if !outcome.newly_claimed {
        return json(ClientCredentialReply::Historical {
            api_version: 1,
            session_id: id.clone(),
            request_id: request.request_id,
            credentials: outcome
                .roots
                .into_iter()
                .map(|(purpose, generation)| HistoricalClientCredential {
                    purpose,
                    generation,
                })
                .collect(),
        });
    }
    let saved = store.snapshot().map_err(|_| denied())?;
    let session = saved.state.test_sessions().get(id).ok_or_else(denied)?;
    let mut credentials = Vec::new();
    for (purpose, token) in tokens {
        let generation = outcome
            .roots
            .iter()
            .find(|(p, _)| *p == purpose)
            .map(|(_, g)| *g)
            .ok_or_else(denied)?;
        let stored = session
            .roots
            .get(&purpose)
            .and_then(|r| r.credential.as_ref())
            .ok_or_else(denied)?;
        let allocation = allocations
            .iter()
            .find(|a| a.purpose == purpose)
            .ok_or_else(denied)?;
        if stored.generation != generation
            || stored.digest != allocation.digest
            || stored.claim_request_id != request.request_id
        {
            return Err(denied());
        }
        credentials.push(ClaimedClientCredential {
            purpose,
            generation,
            token: PrivateClientToken::new(token),
        });
    }
    json(ClientCredentialReply::Claimed {
        api_version: 1,
        session_id: id.clone(),
        request_id: request.request_id,
        credentials,
    })
}
