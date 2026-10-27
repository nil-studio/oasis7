//! Fixed public HTTP context. No caller JSON supplies a role, Current or verified receipt.
use oasis7_custody_core::{
    http_pop::{CanonicalHttpTarget, HttpHeaderRepresentation, HttpPopRoute, PreparedHttpPop},
    provisioning::{
        AccountProvisionApprovalState, SessionClientHttpContext, SessionClientHttpRequestV1,
        SessionClientOperation,
    },
    time_units::UnixMillis,
};
use wasm_bindgen::JsValue;
use worker::Env;
fn rejected() -> JsValue {
    JsValue::from_str("test session client authority unavailable")
}
/// Public deployment metadata only. There is no signing seed or private session access here.
pub(crate) struct ClientHttpConfiguration {
    origin: String,
}
impl ClientHttpConfiguration {
    pub(crate) fn load(env: &Env) -> Result<Self, JsValue> {
        if env
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
        CanonicalHttpTarget::new(&origin, HttpPopRoute::ProofChallenges).map_err(|_| rejected())?;
        Ok(Self { origin })
    }
    /// This operation is injected by a fixed Rust route, never deserialized from the transport.
    pub(crate) fn prepare(
        &self,
        state: &AccountProvisionApprovalState,
        request: &SessionClientHttpRequestV1,
        operation: SessionClientOperation,
        time: UnixMillis,
    ) -> Result<PreparedHttpPop, JsValue> {
        state
            .prepare_test_session_client_http(
                request,
                &SessionClientHttpContext {
                    public_origin: &self.origin,
                    custody_account_id: &state.draft().entry.custody_account_id,
                    provision_id: &state.draft().entry.provision_id,
                    operation,
                    representation: HttpHeaderRepresentation::WorkerMergedSingletons,
                },
                time,
            )
            .map_err(|_| rejected())
    }
}
