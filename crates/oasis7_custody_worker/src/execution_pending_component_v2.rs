//! Harness-only actual signed Actions component. Never mounted by production fetch.
use wasm_bindgen::prelude::*;
#[cfg(all(feature = "owner", feature = "approval-harness"))]
use worker::Response;
use worker::{Env, Request};
#[cfg(all(feature = "owner", feature = "approval-harness"))]
pub(crate) const PATH: &str = "/internal/execution-pending-component-v2";
fn denied() -> JsValue {
    JsValue::from_str("execution pending component unavailable")
}
fn enabled(env: &Env) -> Result<(), JsValue> {
    if env
        .var("CUSTODY_EXECUTION_PENDING_COMPONENT_V2_TEST")
        .ok()
        .is_none_or(|v| v.to_string() != "true")
    {
        return Err(denied());
    }
    Ok(())
}
#[cfg(all(feature = "identity", feature = "native-runtime-harness"))]
#[wasm_bindgen(module = "/platform/execution_pending_component_v2.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=callPendingExecutionComponent)]
    async fn owner(env: &JsValue, raw: &str) -> Result<JsValue, JsValue>;
}
#[cfg(all(feature = "identity", feature = "native-runtime-harness"))]
#[wasm_bindgen]
pub async fn identity_pending_execution_component_v2(
    env: Env,
    request: web_sys::Request,
) -> Result<String, JsValue> {
    enabled(&env)?;
    let request: Request = request.into();
    crate::client_enrollment_entrypoints::limit_client_http(&env, &request)
        .await
        .map_err(|_| denied())?;
    let q = crate::test_session_client_ingress::capture_execution_pending(&env, &request).await?;
    let raw = serde_json::to_string(&q).map_err(|_| denied())?;
    owner(env.as_ref(), &raw)
        .await?
        .as_string()
        .ok_or_else(denied)
}
#[cfg(all(feature = "owner", feature = "approval-harness"))]
#[wasm_bindgen]
pub async fn owner_pending_execution_component_v2(
    env: Env,
    raw: String,
) -> Result<String, JsValue> {
    enabled(&env)?;
    if raw.len() > 65536 {
        return Err(denied());
    }
    let q: crate::test_session_client_rpc::TestSessionClientAdmissionRpc =
        serde_json::from_str(&raw).map_err(|_| denied())?;
    if q.http.selector.api_version != 2 {
        return Err(denied());
    }
    crate::owner_test_session_entrypoints::fetch_account(
        &env,
        &q.http.selector.custody_account_id,
        PATH,
        &raw,
    )
    .await
}
#[cfg(all(feature = "owner", feature = "approval-harness"))]
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
#[cfg(all(feature = "owner", feature = "approval-harness"))]
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch,js_name=clientAccountActorMatches)]
    fn physical(state: &JsValue, env: &Env, account: &str) -> Result<bool, JsValue>;
}
#[cfg(all(feature = "owner", feature = "approval-harness"))]
pub(crate) async fn handle(
    state: &JsValue,
    env: Option<&Env>,
    request: &Request,
) -> worker::Result<Response> {
    let url = request.url()?;
    if request.method() != worker::Method::Post
        || url.scheme() != "https"
        || url.host_str() != Some("account.internal")
        || url.path() != PATH
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env.filter(|e| enabled(e).is_ok()) else {
        return Response::error("component unavailable", 409);
    };
    let Some(raw) = bounded(request.inner().as_ref(), 65536)
        .await
        .ok()
        .and_then(|v| v.as_string())
    else {
        return Response::error("invalid component request", 400);
    };
    let Ok(q) =
        serde_json::from_str::<crate::test_session_client_rpc::TestSessionClientAdmissionRpc>(&raw)
    else {
        return Response::error("invalid component request", 400);
    };
    if physical(state, env, &q.http.selector.custody_account_id).ok() != Some(true) {
        return Response::error("component unavailable", 409);
    }
    let store = crate::provision_approval_store::OwnerProvisionApprovalStore::open(state.clone())?;
    match crate::owner_provision::native::pending_execution_source_component_v2(env, &store, &q)
        .await
    {
        Ok(raw) => {
            let mut response = Response::ok(raw)?;
            response
                .headers_mut()
                .set("Content-Type", "application/json")?;
            response.headers_mut().set("Cache-Control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("component unavailable", 409),
    }
}
