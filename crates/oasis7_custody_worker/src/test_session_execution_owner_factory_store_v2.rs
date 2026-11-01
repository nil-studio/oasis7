//! Fixed Owner-service/Account-Actor settlement before Current3 or signer I/O.
//! Raw client PoP is reverified inside the actual Account transaction; no
//! deserialized receipt, Current, fee count or caller-selected profile is accepted.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http_pop::{HttpHeaderRepresentation, verify_http_possession},
    provisioning::{ExecutionPrepareAllocationV2, SessionProofRequest},
    time_units::UnixMillis,
};
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn settle_execution_prepare_owner_factory_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        allocation: &ExecutionPrepareAllocationV2,
    ) -> Result<String> {
        if env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        let prepare = |state: &AccountProvisionApprovalState, time: u64| {
            let selected = &rpc.http.selector;
            let draft = state.draft();
            if selected.api_version != 2
                || selected.custody_account_id != draft.entry.custody_account_id
                || selected.provision_id != draft.entry.provision_id
                || selected.session_id != rpc.http.original.session_id
                || allocation.factory_ingress_id != rpc.ingress_attempt_id
            {
                return Err(rejected());
            }
            state
                .prepare_test_session_v2_action_http(
                    &rpc.http.original,
                    HttpHeaderRepresentation::WorkerMergedSingletons,
                    UnixMillis::new(time),
                )
                .map_err(|_| rejected())
        };
        let saved = self.snapshot()?;
        let first = prepare(&saved.state, now()?)?;
        let mut cache = verify(first.signature_checks().map_err(|_| rejected())?).await?;
        // These two inbound physical edges have occurred. Commit their fixed
        // typed settlement before returning any opaque/read/transport handle.
        // Insufficient fee or failed PoP sends no subsequent Current/signer I/O.
        self.local_reservation(
            "execution_prepare_owner_factory",
            &rpc.http.selector.session_id,
            |state, time| {
                let checked = prepare(state, time)?;
                let proof = verify_http_possession(checked, UnixMillis::new(time), &mut cache)
                    .map_err(|_| rejected())?;
                state
                    .charge_test_session_v2_execution_prepare_owner_factory_attempt(
                        &rpc.http.selector.session_id,
                        allocation,
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: &proof,
                        },
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
}
