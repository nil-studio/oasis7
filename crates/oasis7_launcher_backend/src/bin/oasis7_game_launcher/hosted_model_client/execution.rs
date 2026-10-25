//! Bounded asynchronous native runner; neither model keys nor SDK signers leave this process.
use super::*;
use oasis7_agent_harness::{NativeTaskRunner, SdkNativeTaskHost};
use oasis7_model_provider::{connection::Limits, harness_adapter::SessionModelProvider};
use std::collections::HashMap;

#[derive(Default, Clone)]
pub(super) struct Executions(Arc<Mutex<HashMap<String, Value>>>);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activate {
    expected_version: u64,
    consent: bool,
    target_ref: TargetRef,
    limits: Limits,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetRef {
    agent_id: String,
}
impl HostedModelClient {
    pub(super) fn execution_snapshot(&self, connection: &ConnectionSnapshot) -> Value {
        let mut result = snapshot(connection);
        if let Ok(executions) = self.executions.0.lock()
            && let Some(status) = executions.get(&connection.id)
        {
            result["execution"] = status.clone();
        }
        result
    }
    pub(super) fn start_execution(
        &self,
        principal: &Principal,
        id: &str,
        input: Activate,
    ) -> Result<(), IdentityReply> {
        // Defense in depth: internal callers cannot bypass the HTTP migration gate.
        if self.management_only {
            return Err(error(409, "legacy_execution_disabled"));
        }
        if !input.consent {
            return Err(error(400, "consent_required"));
        }
        // The first slice accepts exactly the displayed deployment limits.
        // It must never silently ignore a player's smaller submitted budget.
        if serde_json::to_value(&input.limits).ok()
            != serde_json::to_value(self.manager.limits()).ok()
        {
            return Err(error(400, "invalid_request"));
        }
        let executor = self
            .executors
            .resolve(principal, &input.target_ref.agent_id)
            .ok_or_else(|| error(409, "game_executor_unavailable"))?;
        let claim = self
            .executors
            .claim(&executor, id)
            .map_err(|_| error(409, "executor_busy"))?;
        let before = self
            .manager
            .snapshot(principal, id)
            .map_err(connection_error)?;
        let preset = self
            .config
            .presets
            .iter()
            .find(|p| p.id == before.preset_id)
            .ok_or_else(|| error(409, "capability_unsupported"))?;
        let config = executor::run_config(&executor, preset, self.manager.limits());
        config
            .validate()
            .map_err(|_| error(409, "game_executor_unavailable"))?;
        if before.budget.calls >= self.manager.limits().max_calls {
            return Err(error(409, "budget_exhausted"));
        }
        let active = self
            .manager
            .activate(
                principal,
                id,
                input.expected_version,
                executor::target(&executor.fence),
            )
            .map_err(connection_error)?;
        let active_version = active
            .active_version
            .ok_or_else(|| error(409, "game_executor_unavailable"))?;
        let provider = match SessionModelProvider::new(
            self.manager.clone(),
            principal.clone(),
            id.to_owned(),
            active_version,
            active.generation,
        ) {
            Ok(provider) => provider,
            Err(_) => {
                self.stop_generation(principal, id, active.generation);
                return Err(error(409, "game_executor_unavailable"));
            }
        };
        let runner = match NativeTaskRunner::new(
            Arc::new(SdkNativeTaskHost::new(Arc::clone(&executor.client))),
            Box::new(provider),
            config,
        ) {
            Ok(runner) => runner,
            Err(_) => {
                self.stop_generation(principal, id, active.generation);
                return Err(error(409, "game_executor_unavailable"));
            }
        };
        let executions = self.executions.clone();
        executions.set(
            id,
            json!({"phase":"running","reason":null,"generation":active.generation}),
        );
        let manager = self.manager.clone();
        let owner = principal.clone();
        let connection = id.to_owned();
        let generation = active.generation;
        let spawned = std::thread::Builder::new().name("byok-native-task".into()).spawn(move || {
            let _claim = claim;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||runner.run()));
            let status = match result {
                Ok(Ok(report)) => finished_status(&report, generation),
                _ => json!({"phase":"stopped","reason":"host_failure","generation":generation,"goal_completion_verified":false}),
            };
            executions.set(&connection,status);
            stop_generation(&manager,&owner,&connection,generation);
        });
        if spawned.is_err() {
            self.executions.set(
                id,
                json!({"phase":"stopped","reason":"host_failure","generation":generation}),
            );
            self.stop_generation(principal, id, generation);
            return Err(error(503, "executor_unavailable"));
        }
        Ok(())
    }
    fn stop_generation(&self, p: &Principal, id: &str, generation: u64) {
        stop_generation(&self.manager, p, id, generation);
    }
}
impl Executions {
    pub(super) fn set(&self, id: &str, value: Value) {
        if let Ok(mut states) = self.0.lock() {
            let incoming = value["generation"].as_u64().unwrap_or(0);
            if states
                .get(id)
                .is_some_and(|current| current["generation"].as_u64().unwrap_or(0) > incoming)
            {
                return;
            }
            states.insert(id.to_owned(), value);
        }
    }
}
fn stop_generation(manager: &ConnectionManager, p: &Principal, id: &str, generation: u64) {
    if let Ok(current) = manager.snapshot(p, id) {
        // A late completion cannot stop a newer activation; renewals retain the
        // run generation but use the current management CAS version.
        if current.generation == generation && current.active_version.is_some() {
            let _ = manager.stop(p, id, current.version);
        }
    }
}

fn finished_status(report: &oasis7_agent_harness::NativeTaskRunReport, generation: u64) -> Value {
    json!({"phase":"finished","reason":report.stop_reason,"generation":generation,
        "model_failure":report.model_failure,"successful_effects":report.turns_completed,
        "model_calls_spent":report.model_calls_spent,"goal_completion_verified":false})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn execution_projection_exposes_only_typed_failure_and_preserves_stop_reason() {
        use oasis7_agent_harness::{
            NativeTaskModelFailure, NativeTaskRunReport, NativeTaskStopReason,
        };
        let report = NativeTaskRunReport {
            run_id: "private-run-id".into(),
            model_calls_spent: 1,
            turns_completed: 0,
            last_operation: Some("private-operation".into()),
            stop_reason: NativeTaskStopReason::ModelFailed,
            model_failure: Some(NativeTaskModelFailure::InvalidToolIdentity),
        };
        let status = finished_status(&report, 7);
        assert_eq!(status["reason"], "model_failed");
        assert_eq!(status["model_failure"], "invalid_tool_identity");
        assert_eq!(status["goal_completion_verified"], false);
        assert!(!status.to_string().contains("private-"));
    }
}
