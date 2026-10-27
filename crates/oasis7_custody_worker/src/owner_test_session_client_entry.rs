//! Fixed Account paths. No request field chooses a role, operation or physical actor.
use crate::test_session_client_rpc::TestSessionClientStageRpc;
use oasis7_custody_core::{
    http::Role,
    provisioning::{SessionClientOperation, SessionControlKindV1},
};
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
    Some(match path {
        "/internal/test-session-client-challenge" => SessionClientOperation::Challenge,
        "/internal/test-session-client-claim" => SessionClientOperation::CredentialClaim,
        "/internal/test-session-client-initialize" => {
            SessionClientOperation::Control(SessionControlKindV1::Initialize)
        }
        "/internal/test-session-client-tools-list" => {
            SessionClientOperation::Control(SessionControlKindV1::ToolsList)
        }
        "/internal/test-session-client-heartbeat" => {
            SessionClientOperation::Control(SessionControlKindV1::Heartbeat)
        }
        "/internal/test-session-client-finish" => {
            SessionClientOperation::Control(SessionControlKindV1::Finish)
        }
        "/internal/test-session-client-abort" => {
            SessionClientOperation::Control(SessionControlKindV1::Abort)
        }
        _ => return None,
    })
}

#[cfg(feature = "approval-harness")]
pub(crate) fn diagnostic_path(path: &str) -> bool {
    matches!(
        path,
        "/internal/performance/client-source-drain" | "/internal/performance/client-direct-admit"
    )
}

#[cfg(feature = "approval-harness")]
pub(crate) async fn diagnostic(
    state: &JsValue,
    env: Option<&Env>,
    request: &Request,
) -> worker::Result<Response> {
    let url = request.url()?;
    if request.method() != Method::Post
        || url.scheme() != "https"
        || url.host_str() != Some("account.internal")
        || url.query().is_some()
        || url.fragment().is_some()
        || request.headers().get("content-type")?.as_deref() != Some("application/json")
    {
        return Response::error("not found", 404);
    }
    let Some(env) = env else {
        return Response::error("not ready", 409);
    };
    let store =
        match crate::provision_approval_store::OwnerProvisionApprovalStore::open(state.clone()) {
            Ok(store) => store,
            Err(_) => return Response::error("not ready", 409),
        };
    let saved = match store.snapshot() {
        Ok(saved) => saved,
        Err(_) => return Response::error("not ready", 409),
    };
    if physical(state, env, &saved.state.draft().entry.custody_account_id).ok() != Some(true) {
        return Response::error("not ready", 409);
    }
    let raw = match bounded(request.inner().as_ref(), 128 * 1024)
        .await
        .ok()
        .and_then(|v| v.as_string())
    {
        Some(raw) => zeroize::Zeroizing::new(raw),
        None => return Response::error("invalid request", 400),
    };
    if url.path() == "/internal/performance/client-source-drain" {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Drain {
            session_id: String,
        }
        let q: Drain = match serde_json::from_str(&raw) {
            Ok(q) => q,
            Err(_) => return Response::error("invalid request", 400),
        };
        return match store.drain_client_source_budget(&q.session_id) {
            Ok((reserved_native, native_remaining, host_remaining)) => Response::from_json(
                &serde_json::json!({"reserved_native":reserved_native,"native_remaining":native_remaining,"host_remaining":host_remaining}),
            ),
            Err(_) => Response::error("not ready", 409),
        };
    }
    let rpc: crate::test_session_client_rpc::TestSessionClientAdmissionRpc =
        match serde_json::from_str(&raw) {
            Ok(rpc) => rpc,
            Err(_) => return Response::error("invalid request", 400),
        };
    let q: oasis7_custody_core::provisioning::SessionChallengeRequestV1 =
        match serde_json::from_str(&rpc.http.original.body) {
            Ok(q) => q,
            Err(_) => return Response::error("invalid request", 400),
        };
    use oasis7_custody_core::provisioning::{SessionChallengeKindV1, SessionPurposeV1};
    if q.kind != SessionChallengeKindV1::CredentialClaim || q.purpose != SessionPurposeV1::Execution
    {
        return Response::error("invalid request", 400);
    }
    match store
        .direct_admit_client_http(
            env,
            &rpc,
            SessionClientOperation::Challenge,
            SessionPurposeV1::Execution,
        )
        .await
    {
        Ok(_) => Response::from_json(&serde_json::json!({"admitted":true})),
        Err(_) => Response::error("not ready", 409),
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
    let rpc: TestSessionClientStageRpc = serde_json::from_str(raw)
        .map_err(|_| JsValue::from_str("test session client unavailable"))?;
    let selector = &rpc.admission.http.selector;
    if selector.api_version != 1
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
    let rpc: TestSessionClientStageRpc = match serde_json::from_str(&raw) {
        Ok(rpc) => rpc,
        Err(_) => return Response::error("invalid request", 400),
    };
    if physical(state, env, &rpc.admission.http.selector.custody_account_id).ok() != Some(true) {
        return Response::error("not ready", 409);
    }
    match crate::owner_provision::native::test_session::dispatch_client(
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
