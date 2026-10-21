//! Authenticated session-only model receiver. No arbitrary prompt proxy or world writer.
mod config;
mod execution;
mod executor;
mod http;
mod management;
pub(super) use http::handle_request;
#[cfg(test)]
mod tests;
use super::hosted_identity::{IdentityReply, IdentityService};
use config::Config;
use oasis7_model_provider::connection::{
    BindingTarget, ConnectionError, ConnectionManager, ConnectionSnapshot, MonotonicClock,
    Principal,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

pub(super) const PREFIX: &str = "/v1/agent-client";
pub(super) type Identity = Arc<Mutex<Option<IdentityService>>>;
pub(super) struct HostedModelClient {
    config: Config,
    pub manager: ConnectionManager,
    executors: Arc<executor::Executors>,
    executions: execution::Executions,
}
impl HostedModelClient {
    pub fn from_env(identity: &Identity) -> Result<Option<Self>, String> {
        let Some(config) = Config::from_env()? else {
            return Ok(None);
        };
        Self::new(config, Arc::clone(identity)).map(Some)
    }
    fn new(config: Config, identity: Identity) -> Result<Self, String> {
        config.validate()?;
        {
            let guard = identity
                .lock()
                .map_err(|_| "identity authority unavailable")?;
            let service = guard
                .as_ref()
                .ok_or("agent client requires hosted authentication")?;
            if !service
                .config
                .rp
                .allowed_origins
                .contains(&config.public_origin)
            {
                return Err("agent client origin must match hosted authentication".into());
            }
        }
        let mut ids = std::collections::HashSet::new();
        if config.presets.iter().any(|p| !ids.insert(p.id.as_str())) {
            return Err("duplicate model preset".into());
        }
        let executors = Arc::new(executor::Executors::load(
            &config.executors,
            Arc::clone(&identity),
        )?);
        disable_core_dumps()?;
        let authority = move |principal: &Principal| {
            identity
                .lock()
                .ok()
                .and_then(|mut service| {
                    service.as_mut().map(|s| s.model_session_current(principal))
                })
                .unwrap_or(false)
        };
        let checked_executors = Arc::clone(&executors);
        let bindings = move |principal: &Principal, target: &BindingTarget| {
            checked_executors.authorize(principal, target)
        };
        let manager = ConnectionManager::new(
            Arc::new(authority),
            Arc::new(bindings),
            Arc::new(MonotonicClock::default()),
            config.limits.clone(),
        )
        .map_err(|_| "invalid model session limits")?;
        Ok(Self {
            config,
            manager,
            executors,
            executions: execution::Executions::default(),
        })
    }
    pub fn capabilities(&self) -> Value {
        json!({"ok":true,"enabled":true,"authenticated":true,
            "trusted_executor":{"trusted":true,"display_name":self.config.display_name,"location":self.config.location,"origin":self.config.public_origin},
            "presets":self.config.presets.iter().map(|p|json!({"id":p.id,"provider_id":p.provider,"model_id":p.model,"protocol":p.protocol,"revision":p.revision,"routes":p.routes,"streaming":false,"verification":"experimental_fixture_only"})).collect::<Vec<_>>(),
            "limits":self.manager.limits(),"executor_available":false,"reason":"game_executor_unavailable"})
    }
    pub fn handle(
        &self,
        p: &Principal,
        method: &str,
        path: &str,
        mut body: Value,
    ) -> IdentityReply {
        let result = self.route(p, method, path, &mut body);
        // Unknown fields can contain credentials too: never retain the body.
        erase_json_strings(&mut body);
        match result {
            Ok(value) => IdentityReply::ok(value),
            Err(error) => error,
        }
    }
    fn route(
        &self,
        p: &Principal,
        method: &str,
        path: &str,
        body: &mut Value,
    ) -> Result<Value, IdentityReply> {
        let relative = path
            .strip_prefix(PREFIX)
            .ok_or_else(|| error(404, "not_found"))?;
        if method == "GET" && relative == "/capabilities" {
            let mut capabilities = self.capabilities();
            let available = self.executors.available(p);
            capabilities["executor_available"] = json!(available);
            capabilities["reason"] = if available {
                Value::Null
            } else {
                json!("game_executor_unavailable")
            };
            return Ok(capabilities);
        }
        if method == "GET" && relative == "/model-connections" {
            return self.connection_page(p, None);
        }
        if method == "GET"
            && let Some(cursor) = relative.strip_prefix("/model-connections/pages/")
        {
            return self.connection_page(p, Some(cursor));
        }
        if method == "POST" && relative == "/model-connections" {
            // Move the sole decoded key allocation into zeroizing storage before
            // validating the remaining JSON. Never derive Debug for this input.
            let key = body
                .get_mut("raw_key")
                .map(Value::take)
                .unwrap_or(Value::Null);
            let key = match key {
                Value::String(key) => Zeroizing::new(key.into_bytes()),
                _ => return Err(error(400, "invalid_request")),
            };
            body.as_object_mut()
                .ok_or_else(|| error(400, "invalid_request"))?
                .remove("raw_key");
            let input: Create = serde::Deserialize::deserialize(&*body)
                .map_err(|_| error(400, "invalid_request"))?;
            if !input.consent {
                return Err(error(400, "consent_required"));
            }
            let preset = self
                .config
                .presets
                .iter()
                .find(|v| v.id == input.preset_id)
                .ok_or_else(|| error(400, "capability_unsupported"))?;
            let connection = self
                .manager
                .create(p, preset.clone(), key, true)
                .map_err(connection_error)?;
            return Ok(json!({"ok":true,"connection":self.execution_snapshot(&connection)}));
        }
        let path = relative
            .strip_prefix("/model-connections/")
            .ok_or_else(|| error(404, "not_found"))?;
        let (id, action) = path.split_once('/').unwrap_or((path, ""));
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error(404, "not_found"));
        }
        if method == "GET" && action.is_empty() {
            return self
                .manager
                .snapshot(p, id)
                .map(|v| json!({"ok":true,"connection":self.execution_snapshot(&v)}))
                .map_err(connection_error);
        }
        // Resolve ownership before reporting an action-specific error.
        self.manager.snapshot(p, id).map_err(connection_error)?;
        match (method, action) {
            ("POST", "probe") => {
                let input: Probe = decode(body)?;
                if !input.consent {
                    return Err(error(400, "consent_required"));
                }
                let result = oasis7_model_provider::probe::run_probe(
                    &self.manager,
                    p,
                    id,
                    input.expected_version,
                    &input.operation_id,
                );
                let operation_state = self
                    .manager
                    .probe_status(p, id, &input.operation_id)
                    .map_err(connection_error)?;
                let connection = self.manager.snapshot(p, id).map_err(connection_error)?;
                if let Err(failure) = result {
                    let code = if operation_state.is_none()
                        && connection.version != input.expected_version
                    {
                        "version_conflict".to_string()
                    } else {
                        failure.to_string()
                    };
                    let mut reply = error(409, &code);
                    reply.body["operation_state"] =
                        operation_state.map_or(json!("not_started"), |state| json!(state));
                    return Err(reply);
                }
                return Ok(
                    json!({"ok":true,"connection":self.execution_snapshot(&connection),"operation_state":operation_state}),
                );
            }
            ("POST", "lease") => {
                let input: Version = decode(body)?;
                self.manager
                    .renew(p, id, input.expected_version)
                    .map_err(connection_error)?;
            }
            ("POST", "stop") => {
                let input: Operation = decode(body)?;
                let stopped = self
                    .manager
                    .stop_operation(p, id, input.expected_version, &input.operation_id)
                    .map_err(|failure| {
                        let mut reply = connection_error(failure);
                        if let Ok(recorded) = self.manager.stop_recorded(p, id, &input.operation_id)
                        {
                            reply.body["operation_state"] =
                                json!(if recorded { "ready" } else { "not_started" });
                        }
                        reply
                    })?;
                // An old idempotent receipt must not alter a later run display.
                if input.expected_version.checked_add(1) == Some(stopped.version)
                    && stopped.active_version.is_none()
                {
                    self.executions.set(id, json!({"phase":"stopped","reason":"control_changed","generation":stopped.generation,"goal_completion_verified":false}));
                }
            }
            ("DELETE", "") => {
                let input: Version = decode(body)?;
                self.manager
                    .forget(p, id, input.expected_version)
                    .map_err(connection_error)?;
            }
            ("POST", "activate") => {
                // With no operator provisioned authority, remain fail closed
                // before accepting any purported browser-supplied target.
                if !self.executors.available(p) {
                    return Err(error(409, "game_executor_unavailable"));
                }
                let input: execution::Activate = decode(body)?;
                self.start_execution(p, id, input)?;
            }
            _ => return Err(error(404, "not_found")),
        }
        let connection = self.manager.snapshot(p, id).map_err(connection_error)?;
        Ok(json!({"ok":true,"connection":self.execution_snapshot(&connection)}))
    }
}
fn decode<T: serde::de::DeserializeOwned>(body: &mut Value) -> Result<T, IdentityReply> {
    serde::Deserialize::deserialize(&*body).map_err(|_| error(400, "invalid_request"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    preset_id: String,
    consent: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    expected_version: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    expected_version: u64,
    operation_id: String,
    consent: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    expected_version: u64,
    operation_id: String,
}
fn snapshot(connection: &ConnectionSnapshot) -> Value {
    let mut value = serde_json::to_value(connection).unwrap_or(json!({}));
    value["run_generation"] = json!(connection.generation);
    value["run_state"] = json!(if connection.run_state
        == oasis7_model_provider::connection::RunState::Active
    {
        "running"
    } else {
        "stopped"
    });
    value["calls_spent"] = json!(connection.budget.calls);
    value["tokens_reserved"] = json!(connection.budget.tokens);
    if connection.state == oasis7_model_provider::connection::ConnectionState::Draft {
        value["state"] = json!("checking");
    }
    value
}
pub(super) fn error(status: u16, code: &str) -> IdentityReply {
    IdentityReply {
        status,
        body: json!({"ok":false,"error_code":code,"next_action":"refresh_state"}),
        cookies: vec![],
    }
}
fn connection_error(error_code: ConnectionError) -> IdentityReply {
    match error_code {
        ConnectionError::Unavailable => error(404, "not_found"),
        ConnectionError::Conflict => error(409, "version_conflict"),
        ConnectionError::BudgetExceeded => error(409, "budget_exhausted"),
        ConnectionError::CapacityExceeded => error(409, "connection_capacity_exceeded"),
        ConnectionError::BindingDenied => error(409, "game_executor_unavailable"),
        ConnectionError::NotReady => error(409, "model_unavailable"),
        _ => error(400, "invalid_request"),
    }
}
pub(super) fn disabled(reason: &str) -> IdentityReply {
    IdentityReply::ok(
        json!({"ok":true,"enabled":false,"authenticated":false,"executor_available":false,"reason":reason,"presets":[]}),
    )
}
fn erase_json_strings(value: &mut Value) {
    use zeroize::Zeroize;
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(erase_json_strings),
        Value::Object(fields) => fields.values_mut().for_each(erase_json_strings),
        _ => {}
    }
}
#[cfg(unix)]
fn disable_core_dumps() -> Result<(), String> {
    let limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: setrlimit only reads this initialized structure; disabling core
    // dumps is process-scoped and does not access arbitrary memory.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        return Err("cannot disable model receiver core dumps".into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn disable_core_dumps() -> Result<(), String> {
    Err("session model receiver is not qualified on this platform".into())
}
