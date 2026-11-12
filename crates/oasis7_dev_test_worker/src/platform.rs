//! Public ingress routes only to the single authoritative SQLite actor.
use crate::{
    cache::Cache,
    config::{Config, Material},
    native::{Client, nonce},
    protocol, state,
};
use oasis7_custody_core::dev_test::{Arguments, ModelRequest, Scope, Sessions};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;
use worker::*;
#[wasm_bindgen(module = "/platform/store.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded(
        request: &web_sys::Request,
        limit: usize,
        deadline: f64,
    ) -> std::result::Result<JsValue, JsValue>;
}
#[event(fetch)]
async fn main(request: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = request.url()?;
    if url.path() != "/mcp" || url.query().is_some() || request.method() != Method::Post {
        return Response::error("not found", 404);
    }
    let namespace = env.durable_object("DEV_TEST_ACCOUNTS")?;
    namespace
        .id_from_name("dev-test-accounts-v1")?
        .get_stub()?
        .fetch_with_request(request)
        .await
}
#[durable_object]
pub struct DevTestAccounts {
    raw: JsValue,
    env: Env,
    busy: Cell<bool>,
    sessions: RefCell<Option<Sessions>>,
}
struct Busy<'a>(&'a Cell<bool>);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
fn now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
impl DurableObject for DevTestAccounts {
    fn new(state: State, env: Env) -> Self {
        Self {
            raw: state._inner().into(),
            env,
            busy: Cell::new(false),
            sessions: RefCell::new(nonce().ok().and_then(|n| Sessions::with_epoch(n).ok())),
        }
    }
    async fn fetch(&self, request: Request) -> Result<Response> {
        if self.busy.replace(true) {
            return Response::error("request_in_flight", 409);
        }
        let _busy = Busy(&self.busy);
        match self.handle(request).await {
            Ok(r) => Ok(r),
            Err(code) => {
                let status = match code {
                    "unauthorized" => 401,
                    "origin_denied" => 403,
                    "request_denied" | "request_bound_or_deadline" => 400,
                    _ => 503,
                };
                Response::from_json(&json!({"status":"unavailable","code":code}))
                    .map(|r| r.with_status(status))
            }
        }
    }
}
impl DevTestAccounts {
    fn secret(&self, name: &str) -> Option<String> {
        self.env.secret(name).ok().map(|s| s.to_string())
    }
    async fn handle(&self, request: Request) -> std::result::Result<Response, &'static str> {
        let raw = self
            .env
            .var("DEV_TEST_POLICY")
            .map_err(|_| "configuration_unavailable")?
            .to_string();
        let config = Config::parse(&raw)?;
        let url = request.url().map_err(|_| "request_denied")?;
        if url.origin().ascii_serialization() != config.origin
            || url.path() != "/mcp"
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
        let caller = config.authenticate(token, now(), |n| self.secret(n))?;
        let digest = blake3::hash(raw.as_bytes()).to_hex().to_string();
        let state = state::State {
            raw: &self.raw,
            config: &config,
            policy_digest: &digest,
        };
        state.reconcile()?;
        let deadline = js_sys::Date::now() + config.timeout_ms as f64;
        let inner: &web_sys::Request = request.inner();
        let text = bounded(inner, config.max_request_bytes, deadline)
            .await
            .map_err(|_| "request_bound_or_deadline")?
            .as_string()
            .ok_or("request_denied")?;
        if js_sys::Date::now() >= deadline {
            return Err("request_deadline");
        }
        config.authenticate(token, now(), |n| self.secret(n))?;
        match protocol::parse(&text) {
            protocol::Request::Metadata(Some(value)) => {
                Response::from_json(&value).map_err(|_| "response_unavailable")
            }
            protocol::Request::Metadata(None) => Response::empty()
                .map(|r| r.with_status(202))
                .map_err(|_| "response_unavailable"),
            protocol::Request::Business { id, body } => {
                let value = match self
                    .business(&config, &state, caller, token, &body, deadline)
                    .await
                {
                    Ok(v) => json!({"status":"ok","value":v}),
                    Err(code) => json!({"status":"unavailable","code":code}),
                };
                Response::from_json(&protocol::business_reply(id, value))
                    .map_err(|_| "response_unavailable")
            }
        }
    }
    async fn business(
        &self,
        config: &Config,
        state: &state::State<'_>,
        caller: &str,
        token: &str,
        request: &ModelRequest,
        deadline: f64,
    ) -> std::result::Result<Value, &'static str> {
        if request.method == "accounts" {
            if request.grant_ref.is_some()
                || request.session_ref.is_some()
                || request.expires_at_unix.is_some()
                || request.operation.is_some()
            {
                return Err("invalid_arguments");
            }
            state.charge_read(caller)?;
            let accounts:Vec<_>=config.grants.iter().filter(|g|g.caller_id==caller && g.enabled && now()<g.expires_at_unix && !config.accounts[&g.binding.account_ref].disabled).map(|g|json!({"grant_ref":g.grant_ref,"account_ref":g.binding.account_ref,"alias":config.accounts[&g.binding.account_ref].alias,"environment":g.binding.environment,"world":g.binding.world,"agent_id":g.binding.agent_id,"methods":g.methods})).collect();
            return Ok(json!({"accounts":accounts}));
        }
        if request.method == "close" {
            if request.grant_ref.is_some()
                || request.expires_at_unix.is_some()
                || request.operation.is_some()
            {
                return Err("invalid_arguments");
            }
            self.sessions
                .borrow_mut()
                .as_mut()
                .ok_or("entropy_unavailable")?
                .close(
                    request
                        .session_ref
                        .as_deref()
                        .ok_or("session_unavailable")?,
                    caller,
                )
                .map_err(|_| "session_unavailable")?;
            return Ok(json!({"closed":true}));
        }
        let reference = if request.method == "open" {
            request
                .grant_ref
                .as_deref()
                .ok_or("grant_unavailable")?
                .to_owned()
        } else {
            self.sessions
                .borrow()
                .as_ref()
                .ok_or("entropy_unavailable")?
                .policy_reference(
                    request
                        .session_ref
                        .as_deref()
                        .ok_or("session_unavailable")?,
                )
                .map_err(|_| "session_unavailable")?
                .0
                .to_owned()
        };
        let grant = config
            .grants
            .iter()
            .find(|g| {
                g.grant_ref == reference
                    && g.caller_id == caller
                    && g.enabled
                    && now() < g.expires_at_unix
            })
            .ok_or("grant_unavailable")?;
        let account = &config.accounts[&grant.binding.account_ref];
        if account.disabled {
            return Err("account_disabled");
        }
        let resource = &config.resources[&grant.binding.resource];
        let material = Material::parse(
            &self
                .secret(&resource.material_secret)
                .ok_or("private_material_unavailable")?,
        )?;
        let scope = Scope {
            account_ref: grant.binding.account_ref.clone(),
            environment: grant.binding.environment.clone(),
            world: grant.binding.world.clone(),
            genesis_digest: grant.binding.genesis_digest.clone(),
            agent: grant.binding.agent_id.clone(),
            resource: grant.binding.resource.clone(),
            executor: resource
                .fence
                .executor_binding
                .as_ref()
                .ok_or("executor_unavailable")?
                .executor_public_key
                .clone(),
            fence_digest: blake3::hash(
                &serde_json::to_vec(&resource.fence).map_err(|_| "configuration_unavailable")?,
            )
            .to_hex()
            .to_string(),
        };
        let cache_scope =
            blake3::hash(&serde_json::to_vec(&scope).map_err(|_| "configuration_unavailable")?)
                .to_hex()
                .to_string();
        let guard = || {
            if js_sys::Date::now() >= deadline {
                return Err("native_deadline_unknown");
            }
            config.authenticate(token, now(), |n| self.secret(n))?;
            if now() >= grant.expires_at_unix {
                return Err("grant_expired");
            }
            state.reconcile()?;
            if request.method != "open" {
                self.sessions
                    .borrow()
                    .as_ref()
                    .ok_or("session_unavailable")?
                    .authorize(
                        request
                            .session_ref
                            .as_deref()
                            .ok_or("session_unavailable")?,
                        caller,
                        grant,
                        &request.method,
                        now(),
                    )
                    .map_err(|_| "session_scope_denied")?;
            }
            Ok(())
        };
        guard()?;
        let client = Client {
            resource,
            account,
            material: &material,
            cache: Cache {
                state: &self.raw,
                env: &self.env,
                scope: &cache_scope,
            },
            deadline,
            guard: &guard,
        };
        if request.method == "open" {
            if request.session_ref.is_some() || request.operation.is_some() {
                return Err("invalid_arguments");
            }
            let expires = request.expires_at_unix.ok_or("invalid_expiry")?;
            if expires <= now() || expires > grant.expires_at_unix {
                return Err("invalid_expiry");
            }
            state.charge_read(caller)?;
            client.current_account().await?;
            client.view().await?;
            let reference = self
                .sessions
                .borrow_mut()
                .as_mut()
                .ok_or("entropy_unavailable")?
                .open(
                    caller,
                    grant,
                    request.expires_at_unix.ok_or("invalid_expiry")?,
                    now(),
                )
                .map_err(|_| "session_scope_denied")?;
            return Ok(json!({"session_ref":reference,"account_ref":grant.binding.account_ref}));
        }
        if request.grant_ref.is_some() || request.expires_at_unix.is_some() {
            return Err("invalid_arguments");
        }
        if ["describe", "task", "capabilities"].contains(&request.method.as_str()) {
            if request.operation.is_some() {
                return Err("invalid_arguments");
            }
            state.charge_read(caller)?;
            let view = client.view().await?;
            let result = match request.method.as_str() {
                "describe" => {
                    json!({"account_ref":grant.binding.account_ref,"world":resource.fence.world_id,"agent":resource.fence.agent_id})
                }
                "task" => json!(view.task_fence),
                _ => {
                    json!({"api_revision":1,"methods":grant.methods,"move_targets":grant.move_targets,"task_control":view.projection.game_task_control,"native_delegation_authorizations":view.projection.native_delegation_authorizations,"admission_required":true,"scope":"restricted dev-test context/move/lookup only"})
                }
            };
            return Ok(json!({"binding":grant.binding,"result":result}));
        }
        let call = request.operation.as_ref().ok_or("invalid_operation")?;
        if request.method != call.arguments.method() {
            return Err("invalid_operation");
        }
        if let Arguments::Move { to, .. } = &call.arguments
            && !grant.move_targets.contains(to)
        {
            return Err("move_target_denied");
        }
        let (original, fresh) = state.transact(caller, |ledger| {
            ledger.admit(
                &config.service_grant_id,
                caller,
                &scope,
                call,
                config.max_calls,
                |id| state.history(caller, id),
            )
        })?;
        let result = client.execute(&original, fresh).await?;
        state.transact(caller, |ledger| ledger.observe(call, result.clone()))?;
        Ok(result)
    }
}
