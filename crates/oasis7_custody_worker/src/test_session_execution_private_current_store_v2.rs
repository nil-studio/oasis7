//! The two paid private-history RPCs produce a private observation only in
//! this live handler. Stored replies are facts and cannot recreate the holder.
use super::*;
use crate::{
    owner_provision::native::test_session::ExecutionPrepareObservationV2,
    owner_test_session_rpc::TestSessionSelectorV1,
};
use oasis7_custody_core::{
    execution_private_current_v2::ExecutionPrivateCurrentReplyV2,
    provisioning::ExecutionPrivateCurrentObservationV2, time_units::UnixMillis,
};
use worker::Env;
#[wasm_bindgen::prelude::wasm_bindgen(module = "/platform/test_session_rpc.mjs")]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(catch,js_name=readExecutionPrivateCurrentV2)]
    async fn read_private_current(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
}
impl OwnerProvisionApprovalStore {
    pub(crate) async fn execution_prepare_private_current_v2(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        factory_key: &str,
        observed: &ExecutionPrepareObservationV2,
    ) -> Result<Option<ExecutionPrivateCurrentObservationV2>> {
        if env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        let draft = self.snapshot()?;
        if request.api_version != 2
            || request.custody_account_id != draft.state.draft().entry.custody_account_id
            || request.provision_id != draft.state.draft().entry.provision_id
        {
            return Err(rejected());
        }
        // Native Root/LF checks use the actual paid C3 deployment pin. No
        // private generation or archived LF is represented as fresh Current.
        let funded = self
            .native_verified(
                &|| observed.current(&self.state, env).map_err(|_| rejected()),
                "execution_prepare_private_current_claim",
                &request.session_id,
                |state, _, _, _| {
                    let native = observed.current(&self.state, env)?;
                    let mut checks = state
                        .test_session_v2_native_restore_checks(
                            &request.session_id,
                            &native.scope.as_core(),
                        )
                        .map_err(|_| rejected())?;
                    checks.push(
                        state
                            .test_session_v2_prepare_private_current_checks(
                                &request.session_id,
                                &native.scope.as_core(),
                            )
                            .map_err(|_| rejected())?,
                    );
                    Ok(checks)
                },
                |state, _, _, time, cache| {
                    let native = observed.current(&self.state, env)?;
                    let at = UnixMillis::new(time);
                    let Some(token) = state
                        .claim_test_session_v2_prepare_private_current(
                            &request.session_id,
                            factory_key,
                            &native.scope.as_core(),
                            at,
                            UnixMillis::new(
                                observed
                                    .until()
                                    .get()
                                    .min(native.provision.approval.handoff_until.0)
                                    .min(native.provision.approval.source.session_expires_at.0)
                                    .min(
                                        native.provision.approval.source.session_idle_expires_at.0,
                                    ),
                            ),
                            cache,
                        )
                        .map_err(|_| rejected())?
                    else {
                        return Ok(None);
                    };
                    let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)
                        .map_err(|_| rejected())?;
                    issuer
                        .execution_private_current_recorded(
                            state,
                            &request.session_id,
                            token,
                            observed,
                            at,
                        )
                        .map(Some)
                        .map_err(|_| rejected())
                },
            )
            .await?;
        let Some(funded) = funded else {
            return Ok(None);
        };
        // Both physical fees/once and the exact signed payload are committed
        // before this first outbound request. Failure leaves them occupied.
        let proof = funded.request().map_err(|_| rejected())?;
        let expected_digest = proof.digest().map_err(|_| rejected())?;
        let raw = serde_json::to_string(proof).map_err(|_| rejected())?;
        let reply: ExecutionPrivateCurrentReplyV2 = serde_json::from_str(&private_reply_text(
            read_private_current(env, &raw)
                .await
                .map_err(|_| rejected())?,
        )?)
        .map_err(|_| rejected())?;
        if reply.api_version != 2 || reply.proof_digest != expected_digest {
            return Err(rejected());
        }
        let mut funded = Some(funded);
        self.local_reservation(
            "execution_prepare_private_current_observed",
            &request.session_id,
            |state, time| {
                state
                    .complete_test_session_v2_prepare_private_current(
                        &request.session_id,
                        funded.take().ok_or_else(rejected)?,
                        reply.metadata.clone(),
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
}

fn private_reply_text(value: JsValue) -> Result<String> {
    value.as_string().ok_or_else(rejected)
}
