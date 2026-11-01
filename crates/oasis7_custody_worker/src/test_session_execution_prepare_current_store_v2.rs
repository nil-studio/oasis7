//! Account-owned durable three-edge reservation followed by real off-TX reads.
//! The fixed factory ingress has already been paid separately. No route is
//! opened here, and this returns no signer permission or private Native body.
use super::*;
use crate::owner_provision::native::test_session::{
    ExecutionPrepareObservationV2, execution_prepare_observe_v2,
};
use crate::owner_test_session_rpc::TestSessionSelectorV1;
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn execution_prepare_current_v2(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        factory_key: &str,
    ) -> Result<Option<ExecutionPrepareObservationV2>> {
        if env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        // All three physical claims and Source ticket5 are committed in the
        // same actual Account SQL callback before the first outbound request.
        let funding = self.local_reservation(
            "execution_prepare_current_ports",
            &request.session_id,
            |state, time| {
                let draft = state.draft();
                if request.api_version != 2
                    || request.custody_account_id != draft.entry.custody_account_id
                    || request.provision_id != draft.entry.provision_id
                {
                    return Err(rejected());
                }
                state
                    .claim_test_session_v2_prepare_current_ports(
                        &request.session_id,
                        factory_key,
                        oasis7_custody_core::time_units::UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )?;
        let Some(funding) = funding else {
            // Includes unknown earlier reply: never perform the reads again.
            return Ok(None);
        };
        let source = funding.read().source_operation_id.clone();
        let saved = self.snapshot()?;
        // Any configuration, transport, response, expiry or later SQL failure
        // leaves the already committed three once claims and charges intact.
        execution_prepare_observe_v2(env, &saved, request, &source, funding)
            .await
            .map(Some)
            .map_err(|_| rejected())
    }
}
