//! Metadata-only fixed N query ingress. No SID/private reader/Native callback.
use wasm_bindgen::prelude::*;
use worker::{Env, Request};
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=ownerNativeModelReservationV2)]
    async fn call(env: &Env, raw: &str) -> Result<JsValue, JsValue>;
}
pub(crate) async fn run(env: &Env, request: &Request) -> Result<String, JsValue> {
    let admission = crate::test_session_client_ingress::capture_native_model(env, request).await?;
    let raw = zeroize::Zeroizing::new(
        serde_json::to_string(&admission)
            .map_err(|_| JsValue::from_str("native model reservation unavailable"))?,
    );
    if raw.len() > 128 * 1024 {
        return Err(JsValue::from_str("native model reservation unavailable"));
    }
    call(env, &raw)
        .await?
        .as_string()
        .filter(|s| s.len() <= 32768)
        .ok_or_else(|| JsValue::from_str("native model reservation unavailable"))
}
