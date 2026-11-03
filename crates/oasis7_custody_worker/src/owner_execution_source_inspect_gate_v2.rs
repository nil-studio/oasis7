//! One fixed Source callback service -> actual Account Actor. This route performs
//! local causal validation, never a recursive C3 or a free Native observation.
use crate::provision_approval_store::OwnerProvisionApprovalStore;
use oasis7_custody_core::execution_pending_source_inspect_v2::*;
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};
pub(crate) const PATH: &str = "/internal/execution-pending-source-native-claim-v2";
fn rejected() -> JsValue {
    JsValue::from_str("execution pending source unavailable")
}
fn enabled(env: &Env) -> Result<(), JsValue> {
    for name in [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
        "CUSTODY_EXECUTION_PENDING_START_V2_ENABLED",
        "CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED",
    ] {
        if env.var(name).ok().is_none_or(|v| v.to_string() != "true") {
            return Err(rejected());
        }
    }
    Ok(())
}
fn clock() -> Result<u64, JsValue> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n <= 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(rejected());
    }
    Ok(n as u64)
}
fn decode(env: &Env, raw: &str) -> Result<SignedPendingSourceInspectDispatchV2, JsValue> {
    enabled(env)?;
    if raw.is_empty() || raw.len() > 32768 {
        return Err(rejected());
    }
    let q = SignedPendingSourceInspectDispatchV2::decode(raw.as_bytes()).map_err(|_| rejected())?;
    let [pin, _] = crate::execution_source_inspect_pins_v2::load(env).map_err(|_| rejected())?;
    let floor = oasis7_custody_core::native_cleanup_v2::AccountMachineIssuerFloorV2 {
        issuer_epoch: pin.issuer_epoch,
        config_generation: pin.config_generation,
    };
    q.verify(&pin, &floor, &q.binding.dispatcher_object_id, clock()?)
        .map_err(|_| rejected())?;
    Ok(q)
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
#[wasm_bindgen]
pub async fn owner_execution_pending_source_native_v2(
    env: Env,
    raw: String,
) -> Result<String, JsValue> {
    let q = decode(&env, &raw)?;
    crate::owner_test_session_entrypoints::fetch_account(
        &env,
        &q.binding.scope.context.identity.account_id,
        PATH,
        &raw,
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
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != PATH
        || url.query().is_some()
        || url.fragment().is_some()
        || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env.filter(|e| enabled(e).is_ok()) else {
        return Response::error("execution pending source unavailable", 409);
    };
    let Some(raw) = bounded(request.inner().as_ref(), 32768)
        .await
        .ok()
        .and_then(|v| v.as_string())
    else {
        return Response::error("invalid request", 400);
    };
    let Ok(q) = decode(env, &raw) else {
        return Response::error("execution pending source unavailable", 409);
    };
    if physical(state, env, &q.binding.scope.context.identity.account_id).ok() != Some(true) {
        return Response::error("execution pending source unavailable", 409);
    }
    let store = OwnerProvisionApprovalStore::open(state.clone())?;
    match store.claim_pending_source_native_v2(env, &q).await {
        Ok(proof) => {
            let mut response = Response::from_json(&proof)?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("execution pending source unavailable", 409),
    }
}
