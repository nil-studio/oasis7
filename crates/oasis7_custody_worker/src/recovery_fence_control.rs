//! Fixed operational named capability, never forwarded by default HTTP ingress.
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, RequestInit};
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn body(request: &JsValue, limit: u32) -> core::result::Result<JsValue, JsValue>;
}
fn denied() -> JsValue {
    JsValue::from_str("independent recovery control unavailable")
}
pub(crate) async fn bounded(request: Request) -> worker::Result<String> {
    body(request.inner().as_ref(), 65536)
        .await?
        .as_string()
        .ok_or_else(|| denied().into())
}
async fn call(env: &Env, path: &str, raw: &str) -> core::result::Result<String, JsValue> {
    if !crate::recovery_fence_store::control_enabled(env) || raw.len() > 65536 {
        return Err(denied());
    }
    let stub = env
        .durable_object("FENCES")
        .map_err(|_| denied())?
        .id_from_name("service-fence-v1")
        .map_err(|_| denied())?
        .get_stub()
        .map_err(|_| denied())?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(raw.into()));
    let mut request = Request::new_with_init(&format!("https://fence.internal{path}"), &init)
        .map_err(|_| denied())?;
    request
        .headers_mut()
        .map_err(|_| denied())?
        .set("Content-Type", "application/json")
        .map_err(|_| denied())?;
    let response = stub
        .fetch_with_request(request)
        .await
        .map_err(|_| denied())?;
    if response.status_code() != 200 {
        return Err(denied());
    }
    let raw: web_sys::Response = response.into();
    body(raw.as_ref(), 524288)
        .await?
        .as_string()
        .ok_or_else(denied)
}
#[wasm_bindgen]
pub async fn describe_recovery_fence(env: Env) -> core::result::Result<String, JsValue> {
    call(&env, "/internal/recovery-describe", "{}").await
}
#[wasm_bindgen]
pub async fn activate_recovery_fence(
    env: Env,
    raw: String,
) -> core::result::Result<String, JsValue> {
    if raw.len() > 65536 {
        return Err(denied());
    }
    let _: oasis7_custody_core::recovery::SignedRecoveryActivationV1 =
        serde_json::from_str(&raw).map_err(|_| denied())?;
    call(&env, "/internal/recovery-activate", &raw).await
}
#[wasm_bindgen]
pub async fn disable_recovery_fence(env: Env) -> core::result::Result<String, JsValue> {
    call(&env, "/internal/recovery-disable", "{}").await
}
