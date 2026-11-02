//! Explicit V2 Execution metadata nonce. Exactly the existing first five
//! physical public/service/Actor hops; no Source/private/Native/World request.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;
fn selected(
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
) -> Result<()> {
    let s = &rpc.http.selector;
    if s.api_version != 2
        || s.session_id != rpc.http.original.session_id
        || s.custody_account_id != state.draft().entry.custody_account_id
        || s.provision_id != state.draft().entry.provision_id
        || state.test_sessions().contains_key(&s.session_id)
        || state.test_session_v2_original(&s.session_id).is_none()
    {
        return Err(rejected());
    }
    Ok(())
}
fn prepare(
    env: &Env,
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
    time: UnixMillis,
) -> Result<oasis7_custody_core::http_pop::PreparedHttpPop> {
    crate::execution_dispatch_rpc_v2::require_enabled(env).map_err(|_| rejected())?;
    for flag in [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
    ] {
        if env.var(flag).ok().map(|v| v.to_string()).as_deref() != Some("true") {
            return Err(rejected());
        }
    }
    let origin = env
        .var("CUSTODY_PUBLIC_ORIGIN")
        .map_err(|_| rejected())?
        .to_string();
    state
        .prepare_test_session_v2_tool_nonce_http(
            &rpc.http.original,
            &SessionClientHttpContext {
                public_origin: &origin,
                custody_account_id: &state.draft().entry.custody_account_id,
                provision_id: &state.draft().entry.provision_id,
                operation: SessionClientOperation::ToolNonce,
                representation:
                    oasis7_custody_core::http_pop::HttpHeaderRepresentation::WorkerMergedSingletons,
            },
            time,
        )
        .map_err(|_| rejected())
}
impl OwnerProvisionApprovalStore {
    async fn execution_nonce_proof_transaction<R, T>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        event: &str,
        mut transition: T,
    ) -> Result<R>
    where
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            UnixMillis,
        ) -> Result<R>,
    {
        let initial = self.snapshot()?;
        selected(&initial.state, rpc)?;
        let p = prepare(env, &initial.state, rpc, UnixMillis::new(now()?))?;
        let mut cache = verify(p.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
            selected(state, rpc)?;
            let p = prepare(env, state, rpc, UnixMillis::new(time))?;
            let proof = verify_http_possession(p, UnixMillis::new(time), &mut cache)
                .map_err(|_| rejected())?;
            transition(state, &proof, UnixMillis::new(time))
        })
    }
    pub(crate) async fn issue_client_execution_nonce_five(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<SessionChallengeV1> {
        self.execution_nonce_proof_transaction(
            env,
            rpc,
            "execution_nonce_five_fee",
            |state, proof, time| {
                state
                    .charge_test_session_v2_execution_nonce_five(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        time,
                    )
                    .map(|_| ())
                    .map_err(|_| rejected())
            },
        )
        .await?;
        // Charge SQL committed before RNG or issuance. No later failure refunds.
        let random =
            Zeroizing::new(crate::approval_crypto::verification_nonce().map_err(|_| rejected())?);
        let bytes = Zeroizing::new(hex::decode(random.as_str()).map_err(|_| rejected())?);
        if bytes.len() != 32 {
            return Err(rejected());
        }
        let nonce = URL_SAFE_NO_PAD.encode(bytes.as_slice());
        self.execution_nonce_proof_transaction(
            env,
            rpc,
            "execution_nonce_five_issued",
            |state, proof, time| {
                state
                    .issue_test_session_v2_execution_nonce_five(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        &nonce,
                        time,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
