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
        SessionClientOperation::Challenge | SessionClientOperation::Control(_)
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
        .prepare_test_session_v2_control_http(
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
    async fn control_v2_proof_transaction<R, T>(
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
        let deadline = initial
            .state
            .next_test_session_v2_expiry_at()
            .map_err(|_| rejected())?;
        let mut commit = || {
            self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
                selector_v2(state, rpc)?;
                let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
                let proof = verify_http_possession(prepared, UnixMillis::new(time), &mut cache)
                    .map_err(|_| rejected())?;
                transition(state, &proof, UnixMillis::new(time), &mut cache)
            })
        };
        match deadline {
            Some(d) => self.prearm_test_session_commit(d, &mut commit).await,
            None => commit(),
        }
    }

    /// This is its own SQL commit before any Source, private actor or Native I/O.
    pub(crate) async fn charge_control_v2_http(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<SessionPurposeV1> {
        self.control_v2_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_v2_client_http",
            |state, proof, time, _| {
                state
                    .charge_test_session_v2_control_http_attempt(
                        &rpc.http.selector.session_id,
                        SessionHttpAdmission {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            possession: proof,
                        },
                        time,
                    )
                    .map_err(|_| rejected())?;
                state
                    .test_session_v2_control_purpose(&rpc.http.selector.session_id, proof, time)
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    pub(crate) async fn control_v2_history(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<Option<SessionControlAckV1>> {
        self.control_v2_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_v2_control_history",
            |state, proof, time, _| match operation {
                SessionClientOperation::Challenge => Ok(None),
                SessionClientOperation::Control(_) => state
                    .test_session_v2_control_history(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        time,
                    )
                    .map_err(|_| rejected()),
                _ => Err(rejected()),
            },
        )
        .await
    }
    pub(crate) async fn complete_control_v2_tightening(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        kind: SessionControlKindV1,
    ) -> Result<SessionControlAckV1> {
        if !matches!(
            kind,
            SessionControlKindV1::Finish | SessionControlKindV1::Abort
        ) {
            return Err(rejected());
        }
        self.control_v2_proof_transaction(
            env,
            rpc,
            SessionClientOperation::Control(kind),
            "test_session_v2_control_closing",
            |state, proof, time, cache| {
                state
                    .control_test_session_v2(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        None,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    async fn control_v2_live_transaction<F, R, C, T>(
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
        let saved = self.snapshot()?;
        let write = saved
            .state
            .test_session_v2_header(&rpc.http.selector.session_id)
            .map_err(|_| rejected())?
            .0
            .write_deadline
            .0;
        let proposed = UnixMillis::new(now()?.checked_add(90_000).ok_or_else(rejected)?.min(write));
        let deadline = saved
            .state
            .next_test_session_v2_expiry_at()
            .map_err(|_| rejected())?
            .map_or(proposed, |old| {
                UnixMillis::new(old.get().min(proposed.get()))
            });
        self.native_verified_with_alarm(
            &|| Ok(current()?.native),
            event,
            &rpc.http.selector.session_id,
            deadline,
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
    pub(crate) async fn complete_control_v2_challenge<F>(
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
        self.control_v2_live_transaction(
            env,
            rpc,
            SessionClientOperation::Challenge,
            "test_session_v2_control_challenge",
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
                    .issue_test_session_v2_control_challenge(
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
    pub(crate) async fn complete_control_v2_live<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        purpose: SessionPurposeV1,
        kind: SessionControlKindV1,
        current: F,
    ) -> Result<SessionControlAckV1>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
    {
        if matches!(
            kind,
            SessionControlKindV1::Finish | SessionControlKindV1::Abort
        ) {
            return Err(rejected());
        }
        self.control_v2_live_transaction(
            env,
            rpc,
            SessionClientOperation::Control(kind),
            "test_session_v2_control_ack",
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
                    .control_test_session_v2(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        Some(c),
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
#[path = "test_session_v2_control_store_source.rs"]
mod source;
