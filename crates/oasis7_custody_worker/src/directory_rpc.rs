//! Private, fixed service-entrypoint DTOs. Identity supplies authenticated metadata.
use oasis7_custody_core::{DecimalU64, approval::CustodySessionStatusV1, provisioning::*};
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
pub(crate) struct ProvisionGateRpc {
    pub manager_session: CustodySessionStatusV1,
    pub provision_id: String,
    pub custody_account_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
}
