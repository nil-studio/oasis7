//! Dedicated opaque private-context namespace, with no public Worker route.
#[path = "context_body_store.rs"]
mod store;
use oasis7_custody_core::context_body::*;
use wasm_bindgen::prelude::*;
use worker::*;

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> std::result::Result<JsValue, JsValue>;
}
fn denied() -> Error {
    Error::RustError("private context storage rejected".into())
}
fn now() -> Result<u64> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(denied());
    }
    Ok(value as u64)
}
#[durable_object]
pub struct ContextBody {
    state: JsValue,
    env: Env,
    actor_id: String,
}
impl ContextBody {
    fn check_scope(&self, scope: &ContextBodyScopeV1, live: bool) -> Result<()> {
        scope.validate().map_err(|_| denied())?;
        let configured = self
            .env
            .var("CUSTODY_ROLE_KEY_ENVIRONMENT")
            .map_err(|_| denied())?
            .to_string();
        let expected = self
            .env
            .durable_object("CONTEXT_BODIES")?
            .id_from_name(&scope.derived_actor_name().map_err(|_| denied())?)?
            .to_string();
        if configured != scope.identity_scope.context.identity.environment
            || expected != self.actor_id
            || (live && now()? >= scope.write_deadline.0)
        {
            return Err(denied());
        }
        // Structural scope and an actor address do not authenticate a session.
        // The fixed Identity caller must independently hold its paid/current permit.
        Ok(())
    }
    fn handle(&self, path: &str, raw: &str) -> Result<Response> {
        match path {
            "/internal/put" => {
                let candidate: ContextBodyRecordV1 =
                    serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_scope(&candidate.scope, true)?;
                let fact = store::put(&self.state, &candidate).map_err(|_| denied())?;
                Response::from_json(&fact)
            }
            "/internal/read" => {
                let scope: ContextBodyScopeV1 = serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_scope(&scope, true)?;
                let saved = store::read(&self.state, &scope).map_err(|_| denied())?;
                Response::from_json(&saved)
            }
            "/internal/metadata" => {
                let scope: ContextBodyScopeV1 = serde_json::from_str(raw).map_err(|_| denied())?;
                self.check_scope(&scope, false)?;
                let fact = store::metadata(&self.state, &scope).map_err(|_| denied())?;
                Response::from_json(&fact)
            }
            _ => Response::error("not found", 404),
        }
    }
}
impl DurableObject for ContextBody {
    fn new(state: State, env: Env) -> Self {
        Self {
            actor_id: state.id().to_string(),
            state: state._inner().into(),
            env,
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        if request.method() != Method::Post
            || url.scheme() != "https"
            || url.host_str() != Some("context-body.internal")
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(
                url.path(),
                "/internal/put" | "/internal/read" | "/internal/metadata"
            )
            || request.headers().get("Content-Type")?.as_deref() != Some("application/json")
        {
            return Response::error("not found", 404);
        }
        if self
            .env
            .var("CUSTODY_CONTEXT_BODY_ENABLED")
            .ok()
            .map(|value| value.to_string())
            .as_deref()
            != Some("true")
        {
            return Response::error("disabled", 409);
        }
        let raw = match bounded(request.inner().as_ref(), CONTEXT_BODY_MAX_WIRE_BYTES as u32)
            .await
            .ok()
            .and_then(|value| value.as_string())
            .filter(|value| value.len() <= CONTEXT_BODY_MAX_WIRE_BYTES)
        {
            Some(value) => value,
            None => return Response::error("invalid context storage request", 400),
        };
        match self.handle(url.path(), &raw) {
            Ok(response) => Ok(response),
            Err(_) => Response::error("private context storage rejected", 409),
        }
    }
}
