//! Two independent Account commits for fixed N-query/Host-PoP model admission.
//! No Native/provider/Current request is made by either transaction.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http_pop::{HttpHeaderRepresentation, verify_http_possession},
    provisioning::*,
    time_units::UnixMillis,
};
use worker::Env;
fn prepare(
    env: &Env,
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
    at: UnixMillis,
) -> Result<oasis7_custody_core::http_pop::PreparedHttpPop> {
    let selected = &rpc.http.selector;
    let draft = state.draft();
    if selected.api_version != 2
        || selected.custody_account_id != draft.entry.custody_account_id
        || selected.provision_id != draft.entry.provision_id
        || selected.session_id != rpc.http.original.session_id
        || env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(rejected());
    }
    state
        .test_session_v2_snapshot(&selected.session_id)
        .map_err(|_| rejected())?;
    let origin = env
        .var("CUSTODY_PUBLIC_ORIGIN")
        .map_err(|_| rejected())?
        .to_string();
    state
        .prepare_test_session_v2_native_model_reserve_http(
            &rpc.http.original,
            &NativeModelHttpContextV2 {
                public_origin: &origin,
                custody_account_id: &draft.entry.custody_account_id,
                provision_id: &draft.entry.provision_id,
                representation: HttpHeaderRepresentation::WorkerMergedSingletons,
            },
            at,
        )
        .map_err(|_| rejected())
}
impl OwnerProvisionApprovalStore {
    async fn native_model_transaction<R, F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        event: &str,
        mut transition: F,
    ) -> Result<R>
    where
        F: FnMut(
            &mut AccountProvisionApprovalState,
            &oasis7_custody_core::http_pop::HttpPossessionReceipt,
            UnixMillis,
        ) -> Result<R>,
    {
        let initial = self.snapshot()?;
        let checks = prepare(env, &initial.state, rpc, UnixMillis::new(now()?))?;
        let mut cache = verify(checks.signature_checks().map_err(|_| rejected())?).await?;
        let deadline = initial
            .state
            .next_test_session_v2_expiry_at()
            .map_err(|_| rejected())?;
        let mut commit = || {
            self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
                let at = UnixMillis::new(time);
                let prepared = prepare(env, state, rpc, at)?;
                let proof =
                    verify_http_possession(prepared, at, &mut cache).map_err(|_| rejected())?;
                transition(state, &proof, at)
            })
        };
        match deadline {
            Some(d) => self.prearm_test_session_commit(d, &mut commit).await,
            None => commit(),
        }
    }
    /// Called after the fixed public/Identity/Owner routes, before any Current.
    /// A later reserve failure does not roll this durable receipt back.
    pub(crate) async fn settle_native_model_entry_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<()> {
        self.native_model_transaction(env, rpc, "native_model_entry_v2", |state, proof, at| {
            state
                .charge_test_session_v2_native_model_http_attempt(
                    &rpc.http.selector.session_id,
                    SessionProofRequest {
                        ingress_attempt_id: &rpc.ingress_attempt_id,
                        possession: proof,
                        original_bytes: rpc.http.original.body.as_bytes(),
                    },
                    at,
                )
                .map_err(|_| rejected())
        })
        .await
    }
    /// Separate durable commit; returns pending metadata, never a dispatch token.
    pub(crate) async fn reserve_native_model_pending_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<NativeModelReserveOutcomeV2> {
        self.native_model_transaction(env, rpc, "native_model_pending_v2", |state, proof, at| {
            state
                .reserve_test_session_v2_native_model_graph(
                    &rpc.http.selector.session_id,
                    SessionProofRequest {
                        ingress_attempt_id: &rpc.ingress_attempt_id,
                        possession: proof,
                        original_bytes: rpc.http.original.body.as_bytes(),
                    },
                    at,
                )
                .map_err(|_| rejected())
        })
        .await
    }
}
