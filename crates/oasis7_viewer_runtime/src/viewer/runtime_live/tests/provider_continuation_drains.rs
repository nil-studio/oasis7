use super::*;
use std::time::{Duration, Instant};

fn wait_for_provider_phase(
    label: &str,
    timeout: Duration,
    mut poll: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if poll()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("{label} did not complete within {timeout:?}"));
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn install_cognition_scheduler(server: &mut ViewerRuntimeLiveServer) {
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode continuation scheduler policy"),
        8,
    );
}

pub(super) fn drain_step_provider_response(
    server: &mut ViewerRuntimeLiveServer,
) -> Result<(), String> {
    wait_for_provider_phase(
        "multi-step provider response drain",
        Duration::from_secs(5),
        || {
            server.llm_sidecar.request_decision();
            match server.enqueue_llm_action_from_sidecar() {
                Ok(Some(trace))
                    if matches!(trace.decision, crate::simulator::AgentDecision::Wait) =>
                {
                    Ok(true)
                }
                Ok(Some(_)) | Ok(None) => Ok(false),
                Err(trace) => Err(format!("multi-step provider drain failed: {trace:?}")),
            }
        },
    )
}

pub(super) fn drain_final_continuation(server: &mut ViewerRuntimeLiveServer) -> Result<(), String> {
    server.world.step().map_err(|error| {
        format!("normal Runtime tick selects final continuation wake: {error:?}")
    })?;
    server
        .sync_runtime_wake_projection()
        .map_err(|error| format!("mirror final Runtime-selected wake into Viewer: {error:?}"))?;
    wait_for_provider_phase(
        "final continuation budget drain",
        Duration::from_secs(5),
        || {
            server.llm_sidecar.request_decision();
            match server.enqueue_llm_action_from_sidecar() {
                Ok(None) => {
                    let cognition = server.world.cognition();
                    let budget_exhausted =
                        cognition["continuations"]
                            .as_array()
                            .is_some_and(|continuations| {
                                continuations.iter().any(|continuation| {
                                    continuation["status"] == "completed"
                                        && continuation["terminal_disposition"]
                                            == "budget_exhausted"
                                })
                            });
                    let no_selected_wake = server
                        .world
                        .cognition_in_flight_wakes()
                        .map_err(|error| format!("inspect final continuation wake: {error:?}"))?
                        .is_empty();
                    Ok(budget_exhausted && no_selected_wake)
                }
                Ok(Some(trace)) => Err(format!(
                    "final continuation wake must not issue another provider request: {trace:?}"
                )),
                Err(trace) => Err(format!("final continuation budget drain failed: {trace:?}")),
            }
        },
    )
}
