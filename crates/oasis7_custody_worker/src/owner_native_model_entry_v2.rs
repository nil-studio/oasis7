//! Closed Owner service/Account Actor N reservation port. No generic RPC.
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::http::Role;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};
pub(crate) const PATH: &str = "/internal/native-model-reservation-v2";
fn denied() -> JsValue {
    JsValue::from_str("native model reservation unavailable")
}
fn enabled(env: &Env) -> bool {
    [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
    ]
    .into_iter()
    .all(|k| env.var(k).ok().map(|v| v.to_string()).as_deref() == Some("true"))
}
fn decode(raw: &str) -> Result<TestSessionClientAdmissionRpc, JsValue> {
    if raw.len() > 128 * 1024 {
        return Err(denied());
    }
    let r: TestSessionClientAdmissionRpc = serde_json::from_str(raw).map_err(|_| denied())?;
    if r.http.selector.api_version != 2
        || [
            &r.http.selector.custody_account_id,
            &r.http.selector.provision_id,
            &r.http.selector.session_id,
        ]
        .into_iter()
        .any(|s| {
            s.is_empty()
                || s.len() > 256
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        })
    {
        return Err(denied());
    }
    Ok(r)
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=clientAccountActorMatches)]
    fn physical(state: &JsValue, env: &Env, account: &str) -> Result<bool, JsValue>;
}
pub(crate) async fn forward(env: &Env, raw: &str) -> Result<String, JsValue> {
    if !enabled(env) {
        return Err(denied());
    }
    let r = decode(raw)?;
    crate::owner_test_session_entrypoints::fetch_account(
        env,
        &r.http.selector.custody_account_id,
        PATH,
        raw,
    )
    .await
}
pub(crate) async fn handle(
    state: &JsValue,
    env: Option<&Env>,
    request: &Request,
) -> worker::Result<Response> {
    let url = request.url()?;
    if request.method() != Method::Post
        || url.scheme() != "https"
        || url.host_str() != Some("account.internal")
        || url.path() != PATH
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env.filter(|e| enabled(e)) else {
        return Response::error("not ready", 409);
    };
    if request.headers().get("content-type")?.as_deref() != Some("application/json") {
        return Response::error("invalid request", 400);
    }
    let raw = match bounded(request.inner().as_ref(), 128 * 1024)
        .await
        .ok()
        .and_then(|v| v.as_string())
    {
        Some(s) => zeroize::Zeroizing::new(s),
        None => return Response::error("invalid request", 400),
    };
    let r = match decode(&raw) {
        Ok(r) => r,
        Err(_) => return Response::error("invalid request", 400),
    };
    if physical(state, env, &r.http.selector.custody_account_id).ok() != Some(true) {
        return Response::error("not ready", 409);
    }
    match crate::owner_provision::native::test_session::dispatch_native_model_reservation_v2(
        state,
        env,
        &r,
        Role::Identity,
    )
    .await
    {
        Ok(body) => {
            let mut response = Response::ok(body)?;
            response
                .headers_mut()
                .set("content-type", "application/json")?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("native model reservation pending", 409),
    }
}
