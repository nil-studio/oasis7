//! Private, fixed service-entrypoint DTOs. Identity supplies authenticated metadata.
#[cfg(feature = "owner")]
use oasis7_custody_core::DecimalU64;
use oasis7_custody_core::{approval::CustodySessionStatusV1, provisioning::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
#[cfg(feature = "identity")]
use worker::Env;
use worker::{Request, Result};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReserveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub request: ProvisionDraftRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FreezeRpc {
    pub manager_session: CustodySessionStatusV1,
    pub request: ManagerFreezeRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApproveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: ProvisionApproveRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagedLoginApproveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: ManagedLoginApproveRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DriveRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub request: ProvisionDriveRequestV1,
}
#[cfg(feature = "owner")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RolePermitRpc {
    pub role: oasis7_custody_core::http::Role,
    pub request: ProvisionRoleKeyRequestV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct ProvisionGateRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub custody_account_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(feature = "owner")]
pub(crate) struct ProvisionGateView {
    pub api_version: u8,
    pub draft: AccountDraftV1,
    pub security_epoch: DecimalU64,
    pub observed_at: DecimalU64,
    pub directory_revision: String,
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded_text(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
pub(crate) async fn request_text(request: &Request) -> Result<String> {
    // Request's raw Web Request is exposed by the existing worker wrapper.
    let text = bounded_text(request.inner().as_ref(), 32768)
        .await
        .map_err(|_| worker::Error::RustError("request too large or unavailable".into()))?;
    text.as_string()
        .ok_or_else(|| worker::Error::RustError("invalid request text".into()))
}
#[cfg(feature = "identity")]
#[wasm_bindgen(module = "/platform/directory_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = approveOwnerSetup)]
    pub(crate) async fn approve_owner_setup(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = continueOwnerSetup)]
    pub(crate) async fn continue_owner_setup(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=testSessionBasis)]
    pub(crate) async fn test_session_basis(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=testSessionInvite)]
    pub(crate) async fn test_session_invite(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=prepareTestSession)]
    pub(crate) async fn prepare_test_session(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=approveTestSession)]
    pub(crate) async fn approve_test_session(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch,js_name=continueTestSession)]
    pub(crate) async fn continue_test_session(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = ownerSetupBasis)]
    pub(crate) async fn owner_setup_basis(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = prepareOwnerSetup)]
    pub(crate) async fn prepare_owner_setup(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = approveOwnerSetupReconcile)]
    pub(crate) async fn approve_owner_setup_reconcile(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = continueOwnerSetupReconcile)]
    pub(crate) async fn continue_owner_setup_reconcile(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = ownerSetupReconcileBasis)]
    pub(crate) async fn owner_setup_reconcile_basis(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = prepareOwnerSetupReconcile)]
    pub(crate) async fn prepare_owner_setup_reconcile(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = continueOwnerManagedLogin)]
    pub(crate) async fn continue_managed_login_owner(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = approveOwnerManagedLogin)]
    pub(crate) async fn approve_managed_login_owner(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reserveOwnerDraft)]
    pub(crate) async fn reserve_owner(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = readOwnerDraft)]
    pub(crate) async fn read_owner(env: &Env, raw: &str) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = freezeOwnerManager)]
    pub(crate) async fn freeze_owner(env: &Env, raw: &str)
    -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = approveOwnerProvision)]
    pub(crate) async fn approve_owner(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = advanceOwnerProvision)]
    pub(crate) async fn advance_owner(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = readOwnerProvisionProgress)]
    pub(crate) async fn read_progress(
        env: &Env,
        raw: &str,
    ) -> std::result::Result<JsValue, JsValue>;
}

/// Immutable source reference for charged native inspection preflight only.
/// It deliberately carries no observed-at timestamp or live-session assertion.
#[cfg(feature = "owner")]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeInspectReference {
    pub identity_namespace: String,
    pub identity_instance_epoch: oasis7_custody_core::DecimalU64,
    pub account_id: String,
    pub session_ref: String,
    pub credential_ref: String,
    pub credential_version: oasis7_custody_core::DecimalU64,
    pub security_version: oasis7_custody_core::DecimalU64,
    pub policy_version: oasis7_custody_core::DecimalU64,
    pub expires_at: oasis7_custody_core::DecimalU64,
    pub idle_expires_at: oasis7_custody_core::DecimalU64,
}
#[cfg(feature = "owner")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProvisionInspectGateRpc {
    pub manager_reference: NativeInspectReference,
    pub provision_id: String,
    pub custody_account_id: String,
}
