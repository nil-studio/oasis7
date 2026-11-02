//! Fixed Identity service → Account Actor paid per-leaf reentry.
use crate::{
    execution_dispatch_rpc_v2::{IdentityExecutionDispatchGateV2, MAX_FRAME, denied},
    provision_approval_store::OwnerProvisionApprovalStore,
};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};
pub(crate) const NATIVE_PATH: &str = "/internal/native-execution-dispatch-claim-v2";
pub(crate) const PATH: &str = "/internal/execution-dispatch-claim-v2";
fn enabled(env: &Env) -> Result<(), JsValue> {
    crate::execution_dispatch_rpc_v2::require_enabled(env)?;
    for name in [
        "CUSTODY_TEST_SESSION_ENABLED",
        "CUSTODY_TEST_SESSION_V2_ENABLED",
    ] {
        if env.var(name).ok().map(|v| v.to_string()).as_deref() != Some("true") {
            return Err(denied());
        }
    }
    Ok(())
}
fn decode(raw: &str) -> Result<IdentityExecutionDispatchGateV2, JsValue> {
    if raw.len() > MAX_FRAME {
        return Err(denied());
    }
    let q: IdentityExecutionDispatchGateV2 = serde_json::from_str(raw).map_err(|_| denied())?;
    q.request.wire_bytes().map_err(|_| denied())?;
    if q.api_version != 2 {
        return Err(denied());
    }
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
pub(crate) async fn forward(env: &Env, raw: &str) -> Result<String, JsValue> {
    enabled(env)?;
    let q = decode(raw)?;
    crate::owner_test_session_entrypoints::fetch_account(
        env,
        &q.request
            .proof
            .certificate
            .session
            .scope
            .role_scope
            .context
            .identity
            .account_id,
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
    let Some(env) = env.filter(|e| enabled(e).is_ok()) else {
        return Response::error("not ready", 409);
    };
    let Some(raw) = bounded(request.inner().as_ref(), MAX_FRAME as u32)
        .await
        .ok()
        .and_then(|v| v.as_string())
    else {
        return Response::error("invalid request", 400);
    };
    let Ok(q) = decode(&raw) else {
        return Response::error("invalid request", 400);
    };
    let c = &q.request.proof.certificate;
    if physical(
        state,
        env,
        &c.session.scope.role_scope.context.identity.account_id,
    )
    .ok()
        != Some(true)
    {
        return Response::error("not ready", 409);
    }
    let store = OwnerProvisionApprovalStore::open(state.clone())?;
    let saved = store.snapshot()?;
    if saved.state.test_sessions().contains_key(&c.session_id)
        || saved
            .state
            .test_session_v2_credential_input_digest(&c.session_id)
            .ok()
            .as_deref()
            != Some(c.approved_input_digest.as_str())
    {
        return Response::error("not ready", 409);
    }
    match store.execution_native_identity_leaf_v2(env, &q).await {
        Ok(reply) => {
            let mut response = Response::from_json(&reply)?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("execution dispatch unavailable", 409),
    }
}

fn decode_native(
    raw: &str,
) -> Result<
    oasis7_custody_core::execution_dispatch_gate_v2::NativeExecutionDispatchCallbackV2,
    JsValue,
> {
    if raw.len() > MAX_FRAME {
        return Err(denied());
    }
    let q: oasis7_custody_core::execution_dispatch_gate_v2::NativeExecutionDispatchCallbackV2 =
        serde_json::from_str(raw).map_err(|_| denied())?;
    q.wire_bytes().map_err(|_| denied())?;
    Ok(q)
}
pub(crate) async fn forward_native(env: &Env, raw: &str) -> Result<String, JsValue> {
    enabled(env)?;
    let q = decode_native(raw)?;
    crate::owner_test_session_entrypoints::fetch_account(
        env,
        &q.request
            .proof
            .certificate
            .session
            .scope
            .role_scope
            .context
            .identity
            .account_id,
        NATIVE_PATH,
        raw,
    )
    .await
}
pub(crate) async fn handle_native(
    state: &JsValue,
    env: Option<&Env>,
    request: &Request,
) -> worker::Result<Response> {
    let url = request.url()?;
    if request.method() != Method::Post
        || url.scheme() != "https"
        || url.host_str() != Some("account.internal")
        || url.path() != NATIVE_PATH
        || url.query().is_some()
        || url.fragment().is_some()
        || request.headers().get("content-type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env.filter(|e| enabled(e).is_ok()) else {
        return Response::error("not ready", 409);
    };
    let Some(raw) = bounded(request.inner().as_ref(), MAX_FRAME as u32)
        .await
        .ok()
        .and_then(|v| v.as_string())
    else {
        return Response::error("invalid request", 400);
    };
    let Ok(q) = decode_native(&raw) else {
        return Response::error("invalid request", 400);
    };
    let c = &q.request.proof.certificate;
    if physical(
        state,
        env,
        &c.session.scope.role_scope.context.identity.account_id,
    )
    .ok()
        != Some(true)
    {
        return Response::error("not ready", 409);
    }
    let store = OwnerProvisionApprovalStore::open(state.clone())?;
    let saved = store.snapshot()?;
    if saved.state.test_sessions().contains_key(&c.session_id)
        || saved
            .state
            .test_session_v2_credential_input_digest(&c.session_id)
            .ok()
            .as_deref()
            != Some(c.approved_input_digest.as_str())
    {
        return Response::error("not ready", 409);
    }
    match store.execution_native_world_leaf_v2(env, q).await {
        Ok(reply) => {
            let mut response = Response::from_json(&reply)?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("execution dispatch unavailable", 409),
    }
}
