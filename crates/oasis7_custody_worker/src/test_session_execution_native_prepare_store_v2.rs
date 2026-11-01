//! Fresh same-DO reentry allocates Native10 from the real RKV/public original.
//! The first token leaves SQL only after all ten fees/once claims commit.
use super::execution_prepare_pipeline_v2::ExecutionPreparePrefixV2;
use super::*;
use crate::owner_test_session_rpc::TestSessionSelectorV1;
use oasis7_custody_core::{provisioning::ExecutionNativeGatedDispatchV2, time_units::UnixMillis};
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn execution_native_prepare_dispatch_v2(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        prefix: &ExecutionPreparePrefixV2,
    ) -> Result<ExecutionNativeGatedDispatchV2> {
        let saved = self.snapshot()?;
        if request.api_version != 2
            || request.custody_account_id != saved.state.draft().entry.custody_account_id
            || request.provision_id != saved.state.draft().entry.provision_id
        {
            return Err(rejected());
        }
        let permits = saved
            .state
            .test_session_v2_prepare_service_permits(&request.session_id)
            .ok_or_else(rejected)?
            .clone();
        if permits.sign.permit.physical_ingress_key != prefix.factory_key {
            return Err(rejected());
        }
        let archived = saved
            .state
            .test_session_v2_prepare_original_history(&request.session_id)
            .map_err(|_| rejected())?
            .ok_or_else(rejected)?;
        if archived != prefix.original {
            return Err(rejected());
        }
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)
            .map_err(|_| rejected())?;
        self.native_verified(
            &|| Ok(prefix.current(&self.state, env)?.native),
            "execution_native_prepare_funded",
            &request.session_id,
            |state, _, _, time| {
                let current = prefix.current(&self.state, env)?;
                current.with_core(|c| {
                    state
                        .test_session_v2_native_prepare_checks(
                            &request.session_id,
                            c,
                            UnixMillis::new(time),
                        )
                        .map_err(|_| rejected())
                })
            },
            |state, _, _, time, cache| {
                let at = UnixMillis::new(time);
                let authority = prefix.current(&self.state, env)?.with_core(|c| {
                    state
                        .prepare_test_session_v2_native_prepare(
                            &request.session_id,
                            c,
                            issuer.execution_native_issuer(),
                            at,
                            cache,
                        )
                        .map_err(|_| rejected())
                })?;
                issuer
                    .execution_native_gated_recorded(
                        state,
                        &request.session_id,
                        authority,
                        &prefix.observed,
                        at,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
