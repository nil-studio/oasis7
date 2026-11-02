//! Fixed Account paths. No request field chooses a role, operation or physical actor.
use crate::test_session_v2_credential_rpc::TestSessionCredentialStageRpcV2;
use oasis7_custody_core::{http::Role, provisioning::SessionClientOperation};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request, Response};

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
#[wasm_bindgen(module = "/platform/test_session_client_rpc.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = clientAccountActorMatches)]
    fn physical(state: &JsValue, env: &Env, account: &str) -> Result<bool, JsValue>;
}

pub(crate) fn operation(path: &str) -> Option<SessionClientOperation> {
    match path {
        "/internal/test-session-v2-credential-challenge" => Some(SessionClientOperation::Challenge),
        "/internal/test-session-v2-credential-claim" => {
            Some(SessionClientOperation::CredentialClaim)
        }
        _ => None,
    }
}
pub(crate) async fn forward(env: &Env, raw: &str, path: &'static str) -> Result<String, JsValue> {
    if raw.len() > 128 * 1024
        || operation(path).is_none()
        || env
            .var("CUSTODY_TEST_SESSION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return Err(JsValue::from_str("test session client unavailable"));
    }
    if env
        .var("CUSTODY_TEST_SESSION_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(JsValue::from_str("test session V2 credentials unavailable"));
    }
    let rpc: TestSessionCredentialStageRpcV2 = serde_json::from_str(raw)
        .map_err(|_| JsValue::from_str("test session client unavailable"))?;
    let selector = &rpc.admission.http.selector;
    if selector.api_version != 2
        || rpc.api_version != 2
        || [
            &selector.custody_account_id,
            &selector.provision_id,
            &selector.session_id,
        ]
        .iter()
        .any(|id| {
            id.is_empty()
                || id.len() > 256
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        })
    {
        return Err(JsValue::from_str("test session client unavailable"));
    }
    crate::owner_test_session_entrypoints::fetch_account(
        env,
        &selector.custody_account_id,
        path,
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
    let Some(operation) = operation(url.path()) else {
        return Response::error("not found", 404);
    };
    if request.method() != Method::Post
        || url.scheme() != "https"
        || url.host_str() != Some("account.internal")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env else {
        return Response::error("not ready", 409);
    };
    if env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Response::error("not ready", 409);
    }
    if env
        .var("CUSTODY_TEST_SESSION_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Response::error("not ready", 409);
    }
    if request.headers().get("content-type")?.as_deref() != Some("application/json") {
        return Response::error("invalid request", 400);
    }
    let raw = match bounded(request.inner().as_ref(), 128 * 1024)
        .await
        .ok()
        .and_then(|v| v.as_string())
    {
        Some(raw) => zeroize::Zeroizing::new(raw),
        None => return Response::error("invalid request", 400),
    };
    let rpc: TestSessionCredentialStageRpcV2 = match serde_json::from_str(&raw) {
        Ok(rpc) => rpc,
        Err(_) => return Response::error("invalid request", 400),
    };
    if physical(state, env, &rpc.admission.http.selector.custody_account_id).ok() != Some(true) {
        return Response::error("not ready", 409);
    }
    match crate::owner_provision::native::test_session::dispatch_credentials_v2(
        state,
        env,
        rpc,
        operation,
        Role::Identity,
    )
    .await
    {
        Ok(raw) => {
            let mut reply = Response::ok(raw)?;
            reply
                .headers_mut()
                .set("content-type", "application/json")?;
            reply.headers_mut().set("cache-control", "no-store")?;
            Ok(reply)
        }
        Err(_) => Response::error("not ready", 409),
    }
}
