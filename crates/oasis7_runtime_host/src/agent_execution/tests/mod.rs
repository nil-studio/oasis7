//! Original host-private cognition, recovery and lease tests.
use super::*;

fn restore_runner_feedback_history(runner: &mut AsyncAgentRunner, world: &oasis7_runtime::World) {
    let history = world
        .runtime_feedback_outbox()
        .expect("read host-validated Runtime feedback history");
    runner
        .restore_runtime_feedback_outbox(&history)
        .expect("restore Runner replay fence from Runtime history");
}

mod agent_api_boundary;
mod agent_cognition_economy;
mod agent_cognition_identity;
mod agent_cognition_live_harness;
mod agent_feedback_recovery;
