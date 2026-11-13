use super::{
    config::Config,
    state::{Request as AccountRequest, State},
};
use crate::{protocol, store};
use serde_json::{Value, json};
use wasm_bindgen::JsValue;
use worker::{Env, Method, Request, Response};
fn now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
fn secret(env: &Env, name: &str) -> Option<String> {
    env.secret(name).ok().map(|v| v.to_string())
}
pub async fn handle(raw: &JsValue, env: &Env, request: Request) -> Result<Response, &'static str> {
    let policy =
        secret(env, "ACCOUNT_SERVICE_POLICY").ok_or("account_configuration_unavailable")?;
    let config = Config::parse(&policy)?;
    let url = request.url().map_err(|_| "request_denied")?;
    if url.origin().ascii_serialization() != config.origin
        || url.path() != "/accounts/mcp"
        || url.query().is_some()
        || request.method() != Method::Post
    {
        return Err("request_denied");
    }
    if request
        .headers()
        .get("Origin")
        .map_err(|_| "request_denied")?
        .is_some_and(|v| v != config.origin)
    {
        return Err("origin_denied");
    }
    if request
        .headers()
        .get("Content-Type")
        .map_err(|_| "request_denied")?
        .is_none_or(|v| v.split(';').next() != Some("application/json"))
    {
        return Err("request_denied");
    }
    let auth = request
        .headers()
        .get("Authorization")
        .map_err(|_| "unauthorized")?
        .ok_or("unauthorized")?;
    let token = auth.strip_prefix("Bearer ").ok_or("unauthorized")?;
    // Adopt trusted deployment policy even when every caller has been revoked.
    // Authentication failures cannot revive an older deployed generation.
    let previous = store::load(raw, "policy-meta").map_err(|_| "account_state_unknown")?;
    let mut state = State::load(&previous)?;
    state.reconcile(&config, blake3::hash(policy.as_bytes()).to_hex().as_ref())?;
    let baseline = state.encode()?;
    if baseline != previous {
        store::commit(raw, "policy-meta", &previous, &baseline)
            .map_err(|_| "account_state_unknown")?;
    }
    config.authenticate(token, now(), |n| secret(env, n))?;
    let deadline = js_sys::Date::now() + 5000.0;
    let text = crate::platform::bounded(request.inner(), 16384, deadline)
        .await
        .map_err(|_| "request_bound_or_deadline")?
        .as_string()
        .ok_or("request_denied")?;
    let caller = config.authenticate(token, now(), |n| secret(env, n))?;
    if js_sys::Date::now() >= deadline {
        return Err("request_bound_or_deadline");
    }
    let mut envelope: Value = oasis7_agent_api::game::parse_bounded_json(text.as_bytes(), 16384)
        .map_err(|_| "request_denied")?;
    if envelope["method"] == "tools/call" {
        if envelope["params"]["name"] != "account_service_call" {
            return Err("request_denied");
        }
        if envelope["params"]["arguments"]
            .as_object()
            .is_none_or(|args| {
                args.keys().any(|k| {
                    !["method", "grant_ref", "session_ref", "expires_at_unix"].contains(&k.as_str())
                })
            })
        {
            return Err("request_denied");
        }
        envelope["params"]["name"] = json!("dev_test_accounts_call");
    }
    let parsed = protocol::parse(&serde_json::to_string(&envelope).map_err(|_| "request_denied")?);
    let response = match parsed {
        protocol::Request::Metadata(Some(mut value)) => {
            if envelope["method"] == "initialize" && value.get("result").is_some() {
                value["result"]["serverInfo"] = json!({"name":"account-service","version":"1"});
            }
            if envelope["method"] == "tools/list" && value.get("result").is_some() {
                value["result"]["tools"] = json!([{"name":"account_service_call","description":"Select authorized accounts without exposing credentials.","inputSchema":{"type":"object","additionalProperties":false,"required":["method"],"properties":{"method":{"enum":["accounts","open","describe","close"]},"grant_ref":{"type":"string"},"session_ref":{"type":"string"},"expires_at_unix":{"type":"integer","minimum":0}}}}]);
            }
            Response::from_json(&value).map_err(|_| "response_unavailable")?
        }
        protocol::Request::Metadata(None) => Response::empty()
            .map_err(|_| "response_unavailable")?
            .with_status(202),
        protocol::Request::Business { id, body } => {
            let req = AccountRequest {
                method: body.method,
                grant_ref: body.grant_ref,
                session_ref: body.session_ref,
                expires_at_unix: body.expires_at_unix,
            };
            let result = if body.operation.is_some() {
                Err("invalid_arguments")
            } else {
                state.dispatch(
                    &config,
                    caller,
                    &req,
                    now(),
                    &crate::native::nonce()?,
                    |n| secret(env, n).is_some_and(|s| !s.is_empty() && s.len() <= 5120),
                )
            };
            let value = match result {
                Ok(v) => json!({"status":"ok","value":v}),
                Err(code) => json!({"status":"unavailable","code":code}),
            };
            Response::from_json(&protocol::business_reply(id, value))
                .map_err(|_| "response_unavailable")?
        }
    };
    let next = state.encode()?;
    if next != baseline {
        store::commit(raw, "policy-meta", &baseline, &next).map_err(|_| "account_state_unknown")?;
    }
    Ok(response)
}
