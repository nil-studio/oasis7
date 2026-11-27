//! Tests for the simulator module.

use crate::agent::*;
use crate::agent_api_compat;
use crate::agent_execution::async_agent_runner::{self, *};
use crate::agent_execution::continuous_agent_harness::{self, *};
use crate::agent_execution::llm_agent::{self, *};
use crate::agent_execution::runner::*;
use crate::agent_execution::turn_engine::*;
use crate::cognition_policy::*;
use crate::cognition_response_identity::*;
use crate::decision_provider::*;
use crate::provider_loopback_adapter::*;
use crate::provider_loopback_http::*;
use crate::provider_material_capture::*;
use crate::simulator_archive::archive::{self, WorldEvent, WorldSnapshotExtensions};
use crate::simulator_archive::kernel_host::{
    WorldEventRef, WorldJournalView, WorldKernel, initialize_kernel,
};
use crate::simulator_archive::persist::{self, *};
use oasis7_agent_api::CapabilityInvocationContext;
use oasis7_agent_api::intent_summary::*;
use oasis7_agent_harness::BudgetContractV1;
use oasis7_agent_harness::llm_defaults::*;
use oasis7_agent_harness::{
    ModelProvider, ModelProviderError, ModelProviderRequest, ModelProviderResponse,
    ModelProviderTool, ModelProviderTrace, ModelProviderTraceEntry, ModelProviderTurn,
    ModelProviderUsage,
};
use oasis7_model_provider::legacy_model_adapter::LlmCompletionModelProvider;
use oasis7_simulator::WorldKernel as NativeKernel;
use oasis7_simulator::*;
use std::fs;

fn restore_runner_feedback_history(
    runner: &mut AsyncAgentRunner,
    world: &oasis7_runtime::World,
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
) -> AsyncAgentRunner {
    let mut runner = AsyncAgentRunner::provider_backed_fixture(agent_id);
    restore_runner_feedback_history(&mut runner, &oasis7_runtime::World::new());
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
mod micro_depot_measured_supply;
#[cfg(not(target_arch = "wasm32"))]
mod model_provider;
mod module_lifecycle;
mod persist;
#[cfg(not(target_arch = "wasm32"))]
mod provider_loopback_adapter;
mod runner;
mod runner_query_trace;
#[cfg(not(target_arch = "wasm32"))]
mod turn_engine;

mod player_agent_claim_snapshot;
