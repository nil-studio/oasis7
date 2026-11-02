//! Fixed private Owner service→Account Actor Prepare factory. This does not
//! open the public Actions catalog or accept a serialized Current/private body.
use crate::{
    provision_approval_store::OwnerProvisionApprovalStore,
    test_session_client_rpc::TestSessionClientAdmissionRpc,
};
use oasis7_custody_core::provisioning::ExecutionPrepareAllocationV2;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};
pub(crate) const PATH: &str = "/internal/execution-prepare-context-v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestV2 {
    api_version: u8,
    approved_input_digest: String,
    admission: TestSessionClientAdmissionRpc,
    allocation: ExecutionPrepareAllocationV2,
}
fn denied() -> JsValue {
    JsValue::from_str("execution context unavailable")
}
fn enabled(env: &Env) -> Result<(), JsValue> {
    crate::execution_dispatch_rpc_v2::require_enabled(env)?;
    for flag in [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
    ] {
        if env.var(flag).ok().map(|v| v.to_string()).as_deref() != Some("true") {
            return Err(denied());
        }
    }
    Ok(())
}
fn decode(raw: &str) -> Result<RequestV2, JsValue> {
    if raw.len() > 131072 {
        return Err(denied());
    }
    let q: RequestV2 = serde_json::from_str(raw).map_err(|_| denied())?;
    let s = &q.admission.http.selector;
    if q.api_version != 2
        || s.api_version != 2
        || [&s.custody_account_id, &s.provision_id, &s.session_id]
            .iter()
            .any(|v| {
                v.is_empty()
                    || v.len() > 256
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            })
    {
        return Err(denied());
    }
    Ok(q)
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name=clientAccountActorMatches)]
    fn physical(state: &JsValue, env: &Env, account: &str) -> Result<bool, JsValue>;
}
pub(crate) async fn forward(env: &Env, raw: &str) -> Result<String, JsValue> {
    enabled(env)?;
    let q = decode(raw)?;
    crate::owner_test_session_entrypoints::fetch_account(
        env,
        &q.admission.http.selector.custody_account_id,
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
        || request.headers().get("content-type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env else {
        return Response::error("not ready", 409);
    };
    enabled(env).map_err(|_| worker::Error::RustError("execution context unavailable".into()))?;
    let raw = bounded(request.inner().as_ref(), 131072)
        .await
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| worker::Error::RustError("execution context unavailable".into()))?;
    let q = decode(&raw)
        .map_err(|_| worker::Error::RustError("execution context unavailable".into()))?;
    let s = &q.admission.http.selector;
    if physical(state, env, &s.custody_account_id).ok() != Some(true) {
        return Response::error("not ready", 409);
    }
    let store = OwnerProvisionApprovalStore::open(state.clone())?;
    let saved = store.snapshot()?;
    if saved.state.test_sessions().contains_key(&s.session_id)
        || saved
            .state
            .test_session_v2_credential_input_digest(&s.session_id)
            .ok()
            .as_deref()
            != Some(q.approved_input_digest.as_str())
    {
        return Response::error("not ready", 409);
    }
    let result = store
        .execute_prepare_context_v2(env, &q.admission, &q.allocation)
        .await?;
    let mut response = Response::from_json(&result)?;
    response.headers_mut().set("cache-control", "no-store")?;
    Ok(response)
}
