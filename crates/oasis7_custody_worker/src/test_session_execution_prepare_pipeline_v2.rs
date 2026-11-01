//! One Account handler owns the paid observations through RKV publication.
//! No public Actions route or deserialized Current callback is introduced.
use super::*;
use crate::{
    owner_provision::native::test_session::ExecutionPrepareObservationV2,
    test_session_client_rpc::TestSessionClientAdmissionRpc,
};
use oasis7_custody_core::provisioning::{
    ExecutionPrepareAllocationV2, ExecutionPrepareSignedOriginalV2,
    ExecutionPrivateCurrentObservationV2,
};
use worker::Env;
/// Handler-local prefix; never serialized or rebuilt from history.
pub(crate) struct ExecutionPreparePrefixV2 {
    pub(super) factory_key: String,
    pub(super) observed: ExecutionPrepareObservationV2,
    pub(super) private_current: ExecutionPrivateCurrentObservationV2,
    pub(super) original: ExecutionPrepareSignedOriginalV2,
}
impl OwnerProvisionApprovalStore {
    pub(crate) async fn execution_prepare_prefix_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        allocation: &ExecutionPrepareAllocationV2,
    ) -> Result<Option<ExecutionPreparePrefixV2>> {
        let factory_key = self
            .settle_execution_prepare_owner_factory_v2(env, rpc, allocation)
            .await?;
        let request = &rpc.http.selector;
        let Some(observed) = self
            .execution_prepare_current_v2(env, request, &factory_key)
            .await?
        else {
            return Ok(None);
        };
        let Some(private_current) = self
            .execution_prepare_private_current_v2(env, request, &factory_key, &observed)
            .await?
        else {
            return Ok(None);
        };
        let Some(original) = self
            .complete_execution_prepare_permits_v2(
                env,
                request,
                &factory_key,
                &observed,
                &private_current,
            )
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(ExecutionPreparePrefixV2 {
            factory_key,
            observed,
            private_current,
            original,
        }))
    }
}

impl ExecutionPreparePrefixV2 {
    pub(super) fn current(
        &self,
        state: &JsValue,
        env: &Env,
    ) -> Result<super::client::ClientLiveCurrent> {
        if now()? >= self.private_current.expires_at().get() {
            return Err(rejected());
        }
        Ok(super::client::ClientLiveCurrent {
            native: self.observed.current(state, env)?,
            lifetime: self.private_current.lifetime().clone(),
            private_sid_generation: self.private_current.session().private_sid_generation,
            source_operation_id: self.private_current.source_operation_id().into(),
            source_ticket: 5,
            observed_at: self.private_current.observed_at().get(),
        })
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=runExecutionPrepareNativeV2)]
    async fn run_native_prepare(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
impl OwnerProvisionApprovalStore {
    /// Actual fixed internal caller. Public Actions remains closed until the
    /// rest of its complete approved graph is implemented and verified.
    pub(crate) async fn execute_prepare_context_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        allocation: &ExecutionPrepareAllocationV2,
    ) -> Result<
        Option<oasis7_custody_core::native_execution_prepare_v2::ExecutionPreparedPublicResultV2>,
    > {
        use oasis7_custody_core::native_execution_prepare_v2::ExecutionPreparedPublicResultV2;
        let Some(prefix) = self
            .execution_prepare_prefix_v2(env, rpc, allocation)
            .await?
        else {
            return Ok(None);
        };
        // Public deployment pin captured from the actual paid C3 result; this
        // is used below only for immutable fact verification, never fresh admit.
        let native_pin = prefix.current(&self.state, env)?.native.scope;
        let dispatch = self
            .execution_native_prepare_dispatch_v2(env, &rpc.http.selector, &prefix)
            .await?;
        let q = dispatch.into_request();
        let raw =
            String::from_utf8(q.wire_bytes().map_err(|_| rejected())?).map_err(|_| rejected())?;
        // Ten typed physical leaves and their exact original committed before
        // this first service RPC. There is no retry or history-token rebuild.
        let response = run_native_prepare(env, &raw)
            .await
            .map_err(|_| rejected())?
            .as_string()
            .ok_or_else(rejected)?;
        let public: ExecutionPreparedPublicResultV2 =
            serde_json::from_str(&response).map_err(|_| rejected())?;
        public
            .verify_native(
                &q,
                &native_pin.native_root_key_id,
                &native_pin.native_root_public_key,
            )
            .map_err(|_| rejected())?;
        self.local_reservation(
            "execution_prepared_public_archived",
            &rpc.http.selector.session_id,
            |state, time| {
                state
                    .archive_test_session_v2_prepared_public_result(
                        &rpc.http.selector.session_id,
                        public.clone(),
                        &native_pin.as_core(),
                        oasis7_custody_core::time_units::UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )?;
        Ok(Some(public))
    }
}
