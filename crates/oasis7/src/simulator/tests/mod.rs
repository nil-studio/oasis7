//! Tests for the simulator module.

use super::*;
use crate::geometry::GeoPos;
use std::fs;

fn restore_runner_feedback_history(
    runner: &mut crate::simulator::AsyncAgentRunner,
    world: &crate::runtime::World,
) {
    let history = world
        .runtime_feedback_outbox()
        .expect("read host-validated Runtime feedback history");
    runner
        .restore_runtime_feedback_outbox(&history)
        .expect("restore Runner replay fence from Runtime history");
}

fn provider_backed_runner_with_empty_runtime_history(
    agent_id: &str,
) -> crate::simulator::AsyncAgentRunner {
    let mut runner = crate::simulator::AsyncAgentRunner::provider_backed_fixture(agent_id);
    restore_runner_feedback_history(&mut runner, &crate::runtime::World::new());
    runner
}

fn pos(x: i64, y: i64) -> GeoPos {
    GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

fn seed_owner_resource(
    kernel: &mut WorldKernel,
    owner: ResourceOwner,
    kind: ResourceKind,
    amount: i64,
) {
    let mut snapshot = kernel.snapshot();
    match owner {
        ResourceOwner::Agent { agent_id } => {
            let stock = snapshot
                .model
                .agents
                .get_mut(&agent_id)
                .expect("agent exists in snapshot");
            stock
                .resources
                .add(kind, amount)
                .expect("seed agent resource");
        }
        ResourceOwner::Location { location_id } => {
            let stock = snapshot
                .model
                .locations
                .get_mut(&location_id)
                .expect("location exists in snapshot");
            stock
                .resources
                .add(kind, amount)
                .expect("seed location resource");
        }
    }
    let journal = kernel.journal_snapshot();
    *kernel =
        WorldKernel::from_snapshot(snapshot, journal).expect("rebuild kernel from seeded snapshot");
}

mod agent_api_boundary;
mod agent_cognition_budget;
mod agent_cognition_continuation;
mod agent_cognition_economy;
mod agent_cognition_goal_snapshot;
mod agent_cognition_identity;
mod agent_cognition_live_actor;
mod agent_cognition_live_harness;
mod agent_cognition_live_pilot_parity;
mod agent_cognition_memory;
mod agent_feedback_recovery;
mod decision_provider;
mod fine_grain_persistence;
mod kernel_wasm_sandbox_bridge;
mod memory;
mod micro_depot_measured_supply;
#[cfg(not(target_arch = "wasm32"))]
mod model_provider;
mod module_lifecycle;
mod persist;
#[cfg(not(target_arch = "wasm32"))]
mod provider_loopback_adapter;
#[cfg(not(target_arch = "wasm32"))]
mod provider_loopback_http;
mod runner;
mod runner_query_trace;
#[cfg(not(target_arch = "wasm32"))]
mod turn_engine;

mod player_agent_claim_snapshot;
