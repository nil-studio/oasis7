//! Fixed NativeRoot-authenticated callback; no Cookie or browser fallback.
use oasis7_custody_core::{
    execution_dispatch_gate_v2::*, native_cleanup_v2::AccountMachinePurposeV2,
};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/execution_prepare_rpc_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=nativeExecutionDispatchGateV2)]
    async fn owner(env: &Env, raw: &str) -> Result<JsValue, JsValue>;
}
fn denied() -> JsValue {
    JsValue::from_str("execution dispatch unavailable")
}
fn clock() -> Result<u64, JsValue> {
    let v = js_sys::Date::now();
    if !v.is_finite() || v < 0.0 || v.fract() != 0.0 || v > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(v as u64)
}
pub(crate) async fn handle(env: &Env, request: Request) -> worker::Result<Response> {
    let url = request.url()?;
    if crate::execution_dispatch_rpc_v2::require_enabled(env).is_err()
        || request.method() != Method::Post
        || url.scheme() != "https"
        || url.path() != NATIVE_EXECUTION_REENTRY_PATH_V2
        || url.query().is_some()
        || url.fragment().is_some()
        || request.headers().get("content-type")?.as_deref() != Some("application/json")
        || request.headers().get("cookie")?.is_some()
        || env
            .var("CUSTODY_TEST_SESSION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Response::error("not found", 404);
    }
    // These are the two actual CLIENT_ENROLLMENTS provider/realm Actor calls.
    // No session is charged for an unauthenticated prefix and no limiter is skipped.
    if crate::client_enrollment_entrypoints::limit_client_http(env, &request)
        .await
        .is_err()
    {
        return Response::error("client ingress unavailable", 429);
    }
    let result = run(env, &request, &url.origin().ascii_serialization()).await;
    match result {
        Ok(raw) => {
            let mut response = Response::ok(raw)?;
            response
                .headers_mut()
                .set("content-type", "application/json")?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("execution dispatch unavailable", 409),
    }
}
async fn run(env: &Env, request: &Request, origin: &str) -> Result<String, JsValue> {
    let raw = bounded(request.inner().as_ref(), 128 * 1024)
        .await?
        .as_string()
        .ok_or_else(denied)?;
    let q: NativeExecutionDispatchCallbackV2 = serde_json::from_str(&raw).map_err(|_| denied())?;
    q.wire_bytes().map_err(|_| denied())?;
    let fixed = crate::execution_dispatch_native_pins_v2::NativeDispatchHistoryPinsV2::load(env)?;
    let now = clock()?;
    q.query
        .verify_native(&q.request, &fixed.key_id, &fixed.public_key, origin, now)
        .map_err(|_| denied())?;
    let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::ExecutionDispatchClaimV2,
    )?;
    q.previous
        .verify_issuer(&q.request, &pin, &floor, now)
        .map_err(|_| denied())?;
    let (np, nf) = crate::execution_context_issuer_pin_v2::pin(
        env,
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
    )?;
    q.request
        .proof
        .verify_issuer(&np, &nf, now)
        .map_err(|_| denied())?;
    // One named service RPC and its single fixed Account Actor fetch. No retry.
    let result = owner(env, &raw).await?.as_string().ok_or_else(denied)?;
    if result.len() > 32768 {
        return Err(denied());
    }
    let signed: SignedExecutionDispatchClaimV2 =
        serde_json::from_str(&result).map_err(|_| denied())?;
    signed
        .verify_issuer(&q.request, &pin, &floor, clock()?)
        .map_err(|_| denied())?;
    if signed.claim.leaf != q.query.query.leaf
        || signed.claim.body_digest != q.query.query.body_digest
    {
        return Err(denied());
    }
    Ok(result)
}
