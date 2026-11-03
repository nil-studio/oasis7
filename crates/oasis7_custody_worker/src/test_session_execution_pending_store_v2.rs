//! The one Exec400 Account ledger: persist the five actual ingress hops,
//! then hold the remaining134 in a distinct SQL. No HTTP or Current is hidden
//! in either transaction; a history response never reconstructs a send handle.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http_pop::{HttpHeaderRepresentation, HttpPossessionReceipt, verify_http_possession},
    provisioning::{
        AcceptedExecutionPendingStartV2, ExecutionPendingDirectoryDispatchV2,
        PendingDirectoryAttemptV2, SessionProofRequest,
    },
    time_units::UnixMillis,
};
use worker::Env;
fn selected(
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
) -> Result<()> {
    let s = &rpc.http.selector;
    if s.api_version != 2
        || s.custody_account_id != state.draft().entry.custody_account_id
        || s.provision_id != state.draft().entry.provision_id
        || s.session_id != rpc.http.original.session_id
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
    at: UnixMillis,
) -> Result<oasis7_custody_core::http_pop::PreparedHttpPop> {
    for flag in [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
        "CUSTODY_EXECUTION_PENDING_START_V2_ENABLED",
    ] {
        if env.var(flag).ok().is_none_or(|v| v.to_string() != "true") {
            return Err(rejected());
        }
    }
    selected(state, rpc)?;
    state
        .prepare_test_session_v2_action_http(
            &rpc.http.original,
            HttpHeaderRepresentation::WorkerMergedSingletons,
            at,
        )
        .map_err(|_| rejected())
}
impl OwnerProvisionApprovalStore {
    async fn pending_execution_http_sql_v2<R, F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        event: &str,
        mut step: F,
    ) -> Result<R>
    where
        F: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            UnixMillis,
        ) -> Result<R>,
    {
        let first = self.snapshot()?;
        let p = prepare(env, &first.state, rpc, UnixMillis::new(now()?))?;
        let mut host = verify(p.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
            let p = prepare(env, state, rpc, UnixMillis::new(time))?;
            let pop = verify_http_possession(p, UnixMillis::new(time), &mut host)
                .map_err(|_| rejected())?;
            step(state, &pop, UnixMillis::new(time))
        })
    }
    pub(crate) async fn accept_execution_pending_start_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<Option<AcceptedExecutionPendingStartV2>> {
        self.pending_execution_http_sql_v2(
            env,
            rpc,
            "execution_pending_ingress_five",
            |state, pop, at| {
                state
                    .charge_test_session_v2_execution_pending_ingress(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: pop,
                        },
                        at,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await?;
        // SQL1 is committed. Pin/history/RNG/budget failures below never undo it.
        let first = self.snapshot()?;
        let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, &first.state)
            .map_err(|_| rejected())?;
        first
            .state
            .test_session_v2_dispatch_static_configuration(
                &rpc.http.selector.session_id,
                &fixed.context,
                &fixed.world,
                (&fixed.namespace, fixed.instance_epoch),
            )
            .map_err(|_| rejected())?;
        let mut verified = Vec::new();
        for job in first
            .state
            .test_session_v2_native_restore_checks(
                &rpc.http.selector.session_id,
                &fixed.native.as_core(),
            )
            .map_err(|_| rejected())?
        {
            verified.push(verify(job).await?);
        }
        let mut native = NativeExactCache(verified);
        let salt = zeroize::Zeroizing::new(
            crate::approval_crypto::verification_nonce().map_err(|_| rejected())?,
        );
        self.pending_execution_http_sql_v2(
            env,
            rpc,
            "execution_pending_master134",
            |state, pop, at| {
                let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, state)
                    .map_err(|_| rejected())?;
                state
                    .test_session_v2_dispatch_static_configuration(
                        &rpc.http.selector.session_id,
                        &fixed.context,
                        &fixed.world,
                        (&fixed.namespace, fixed.instance_epoch),
                    )
                    .map_err(|_| rejected())?;
                state
                    .reserve_test_session_v2_execution_pending(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: pop,
                        },
                        &salt,
                        &fixed.native.as_core(),
                        at,
                        &mut native,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    /// The signed leaf leaves the handler only after its actual Account SQL
    /// commit. Neither a signing failure nor storage failure returns a handle;
    /// the already persisted physical claim remains consumed.
    pub(crate) fn sign_pending_first_directory_v2(
        &self,
        env: &Env,
        attempt: PendingDirectoryAttemptV2,
    ) -> Result<Option<ExecutionPendingDirectoryDispatchV2>> {
        let id = attempt.session_id().to_owned();
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let mut attempt = Some(attempt);
        self.local_reservation("execution_pending_directory_signed", &id, |state, time| {
            issuer
                .pending_directory_recorded_v2(
                    state,
                    attempt.take().ok_or_else(rejected)?,
                    UnixMillis::new(time),
                )
                .map_err(|_| rejected())
        })
    }
    pub(crate) async fn claim_pending_first_directory_v2(
        &self,
        env: &Env,
        accepted: AcceptedExecutionPendingStartV2,
    ) -> Result<PendingDirectoryAttemptV2> {
        let id = accepted.session_id().to_owned();
        let first = self.snapshot()?;
        let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, &first.state)
            .map_err(|_| rejected())?;
        let mut verified = Vec::new();
        for job in first
            .state
            .test_session_v2_native_restore_checks(&id, &fixed.native.as_core())
            .map_err(|_| rejected())?
        {
            verified.push(verify(job).await?);
        }
        let mut cache = NativeExactCache(verified);
        let mut accepted = Some(accepted);
        self.local_reservation("execution_pending_first_directory", &id, |state, time| {
            let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, state)
                .map_err(|_| rejected())?;
            state
                .test_session_v2_dispatch_static_configuration(
                    &id,
                    &fixed.context,
                    &fixed.world,
                    (&fixed.namespace, fixed.instance_epoch),
                )
                .map_err(|_| rejected())?;
            state
                .claim_test_session_v2_pending_first_directory(
                    accepted.take().ok_or_else(rejected)?,
                    &fixed.native.as_core(),
                    UnixMillis::new(time),
                    &mut cache,
                )
                .map_err(|_| rejected())
        })
    }
}
