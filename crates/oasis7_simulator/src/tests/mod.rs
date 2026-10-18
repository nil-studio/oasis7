use crate::geometry::GeoPos;
use crate::*;
use oasis7_agent_api::world_entity::DEFAULT_AGENT_HEIGHT_CM;

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
    let mut snapshot = kernel.kernel_snapshot();
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
    let journal = kernel.kernel_journal_snapshot();
    *kernel = WorldKernel::from_kernel_snapshot(snapshot, journal)
        .expect("rebuild kernel from seeded snapshot");
}

mod asteroid_fragment;
mod basics;
mod boundary_extremes;
mod chunking;
mod conservation;
mod consistency;
mod fragment_physics;
mod fragment_replenishment_quote;
mod init;
mod init_agent_frag_spawn;
mod init_position_contract;
mod kernel;
mod kernel_rule_decisions;
mod kernel_rule_invariants;
mod kernel_wasm_rule_bridge;
mod kernel_wasm_sandbox_bridge;
mod micro_depot_install_acceptance;
mod micro_depot_measured_supply;
mod micro_depot_schema_compatibility;
mod module_lifecycle;
mod module_visual;
mod monotonicity;
mod native_resolution_contract;
mod persist_journal_invariants;
mod physics_parameters;
mod power;
mod power_sale_quote;
mod social;
mod social_persist;
mod social_revoke_quote;
mod submitter_access;
