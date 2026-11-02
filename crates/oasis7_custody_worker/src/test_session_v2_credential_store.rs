//! V2-only raw PoP transactions. No V1 authority or caller Current is projected.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http_pop::{HttpPossessionReceipt, verify_http_possession},
    provisioning::*,
    time_units::UnixMillis,
};
use worker::Env;

fn selector_v2(
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
) -> Result<()> {
    let selected = &rpc.http.selector;
    let draft = state.draft();
    if selected.api_version != 2
        || selected.custody_account_id != draft.entry.custody_account_id
        || selected.provision_id != draft.entry.provision_id
        || selected.session_id != rpc.http.original.session_id
    {
        return Err(rejected());
    }
    // Lookup the actual V2 map. A same-ID V1 session is never a substitute.
    state
        .test_session_v2_snapshot(&selected.session_id)
        .map_err(|_| rejected())?;
    Ok(())
}

fn prepare_v2(
    env: &Env,
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
    operation: SessionClientOperation,
    time: UnixMillis,
) -> Result<oasis7_custody_core::http_pop::PreparedHttpPop> {
    if !matches!(
        operation,
        SessionClientOperation::Challenge | SessionClientOperation::CredentialClaim
    ) || env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(rejected());
    }
    let origin = env
        .var("CUSTODY_PUBLIC_ORIGIN")
        .map_err(|_| rejected())?
        .to_string();
    state
        .prepare_test_session_v2_credential_http(
            &rpc.http.original,
            &SessionClientHttpContext {
                public_origin: &origin,
                custody_account_id: &state.draft().entry.custody_account_id,
                provision_id: &state.draft().entry.provision_id,
                operation,
                representation:
                    oasis7_custody_core::http_pop::HttpHeaderRepresentation::WorkerMergedSingletons,
            },
            time,
        )
        .map_err(|_| rejected())
}

impl OwnerProvisionApprovalStore {
    async fn credential_v2_proof_transaction<R, T>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        event: &str,
        mut transition: T,
    ) -> Result<R>
    where
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            UnixMillis,
            &mut crate::approval_store::ExactCache,
        ) -> Result<R>,
    {
        let initial = self.snapshot()?;
        selector_v2(&initial.state, rpc)?;
        let prepared = prepare_v2(env, &initial.state, rpc, operation, UnixMillis::new(now()?))?;
        let mut cache = verify(prepared.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
            selector_v2(state, rpc)?;
            let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
            let proof = verify_http_possession(prepared, UnixMillis::new(time), &mut cache)
                .map_err(|_| rejected())?;
            transition(state, &proof, UnixMillis::new(time), &mut cache)
        })
    }

    /// This is its own SQL commit before any Source, private actor or Native I/O.
    pub(crate) async fn charge_credential_v2_http(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<()> {
        self.credential_v2_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_v2_client_http",
            |state, proof, time, _| {
                state
                    .charge_test_session_v2_claim_http_attempt(
                        &rpc.http.selector.session_id,
                        SessionHttpAdmission {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            possession: proof,
                        },
                        time,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    /// History can answer a lost reply before Source or secret access. It never
    /// manufactures fresh credentials or refreshes a challenge expiry.
    pub(crate) async fn credential_v2_history(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<Option<String>> {
        self.credential_v2_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_v2_client_history",
            |state, proof, time, _| {
                let request = SessionProofRequest {
                    ingress_attempt_id: &rpc.ingress_attempt_id,
                    original_bytes: rpc.http.original.body.as_bytes(),
                    possession: proof,
                };
                match operation {
                    SessionClientOperation::Challenge => state
                        .test_session_v2_credential_challenge_history(&request, time)
                        .map_err(|_| rejected())?
                        .map(|v| serde_json::to_string(&v).map_err(|_| rejected()))
                        .transpose(),
                    SessionClientOperation::CredentialClaim => {
                        let old = state
                            .test_session_v2_credential_claim_history(
                                &rpc.http.selector.session_id,
                                &request,
                                time,
                            )
                            .map_err(|_| rejected())?;
                        // The history wire contains no bearer token and no allocation.
                        old.map(|v| {
                            use crate::test_session_client_response::{
                                ClientCredentialReply, HistoricalClientCredential,
                            };
                            if v.newly_claimed {
                                return Err(rejected());
                            }
                            serde_json::to_string(&ClientCredentialReply::Historical {
                                api_version: 2,
                                session_id: rpc.http.selector.session_id.clone(),
                                request_id: proof.request_id().into(),
                                credentials: v
                                    .roots
                                    .into_iter()
                                    .map(|(purpose, generation)| HistoricalClientCredential {
                                        purpose,
                                        generation,
                                    })
                                    .collect(),
                            })
                            .map_err(|_| rejected())
                        })
                        .transpose()
                    }
                    _ => Err(rejected()),
                }
            },
        )
        .await
    }

    async fn credential_v2_live_transaction<F, R, C, T>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        event: &str,
        current: F,
        checks: C,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
        C: Fn(
            &AccountProvisionApprovalState,
            &TestSessionLiveCurrent<'_, '_>,
            UnixMillis,
        ) -> Result<Vec<ExactSignatureChecks>>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            &TestSessionLiveCurrent<'_, '_>,
            UnixMillis,
            &mut NativeExactCache,
        ) -> Result<R>,
    {
        self.native_verified(
            &|| Ok(current()?.native),
            event,
            &rpc.http.selector.session_id,
            |state, _, _, time| {
                selector_v2(state, rpc)?;
                let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
                let owned = current()?;
                let mut jobs = state
                    .test_session_v2_native_restore_checks(
                        &rpc.http.selector.session_id,
                        &owned.native.scope.as_core(),
                    )
                    .map_err(|_| rejected())?;
                jobs.push(prepared.signature_checks().map_err(|_| rejected())?);
                jobs.extend(current()?.with_core(|c| checks(state, c, UnixMillis::new(time)))?);
                Ok(jobs)
            },
            |state, _, _, time, cache| {
                selector_v2(state, rpc)?;
                let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
                let proof = verify_http_possession(prepared, UnixMillis::new(time), cache)
                    .map_err(|_| rejected())?;
                current()?.with_core(|c| transition(state, &proof, c, UnixMillis::new(time), cache))
            },
        )
        .await
    }

    pub(crate) async fn complete_credential_v2_challenge<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        purpose: SessionPurposeV1,
        nonce: &str,
        current: F,
    ) -> Result<SessionChallengeV1>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
    {
        self.credential_v2_live_transaction(
            env,
            rpc,
            SessionClientOperation::Challenge,
            "test_session_v2_challenge",
            current,
            |state, c, time| {
                Ok(vec![
                    state
                        .test_session_v2_live_scope(&rpc.http.selector.session_id, purpose, c, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, proof, c, time, cache| {
                state
                    .issue_test_session_v2_credential_challenge(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        nonce,
                        c,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    pub(crate) async fn complete_credential_v2_claim<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        allocations: &[SessionCredentialAllocation],
        current: F,
    ) -> Result<SessionCredentialClaimOutcome>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
    {
        self.credential_v2_live_transaction(
            env,
            rpc,
            SessionClientOperation::CredentialClaim,
            "test_session_v2_claim",
            current,
            |state, c, time| {
                allocations
                    .iter()
                    .map(|a| {
                        state
                            .test_session_v2_live_scope(
                                &rpc.http.selector.session_id,
                                a.purpose,
                                c,
                                time,
                            )
                            .map_err(|_| rejected())
                    })
                    .collect()
            },
            |state, proof, c, time, cache| {
                state
                    .claim_test_session_v2_credentials(
                        &rpc.http.selector.session_id,
                        SessionCredentialClaim {
                            allocations,
                            proof: SessionProofRequest {
                                ingress_attempt_id: &rpc.ingress_attempt_id,
                                original_bytes: rpc.http.original.body.as_bytes(),
                                possession: proof,
                            },
                        },
                        c,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}

#[path = "test_session_v2_credential_store_source.rs"]
mod source;
