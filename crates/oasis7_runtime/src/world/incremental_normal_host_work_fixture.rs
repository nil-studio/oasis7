//! Explicit finite resource inputs for the shared real two-agent GEN/Host
//! integration. This creates no World, accepted source, worker or permission.
use super::{NormalHostLaneLimitsV1, NormalHostWorkLimitsV1};

fn lanes(steps: u64, bytes: u64, entries: usize) -> NormalHostLaneLimitsV1 {
    NormalHostLaneLimitsV1 {
        steps_per_counter: steps,
        bytes_per_counter: bytes,
        entries_per_counter: entries,
        max_collection_entries: 4096,
        max_pending_records: 256,
        max_text_bytes: 4096,
        max_key_bytes: 16 << 10,
        max_scalar_bytes: 4096,
        max_payload_bytes: 8 << 20,
        max_record_bytes: 16 << 20,
        max_original_bytes: 8 << 20,
        max_heap_bytes: 64 << 20,
        max_pending_node_bytes: 16 << 20,
        max_depth: 64,
    }
}

/// The caller still supplies the original Common and MemoryBudget. These
/// named lanes cannot increase either balance; each work group is explicit.
pub fn two_agent_host_work_limits_v1() -> NormalHostWorkLimitsV1 {
    NormalHostWorkLimitsV1 {
        invocation_invocation: lanes(1_000_000, 64 << 20, 100_000),
        invocation_manifest: lanes(1_000_000, 64 << 20, 100_000),
        invocation_event_original: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        invocation: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        io_overlay: lanes(1_000_000, 64 << 20, 100_000),
        io_current: lanes(1_000_000, 64 << 20, 100_000),
        io_parent: lanes(1_000_000, 64 << 20, 100_000),
        io_admission: lanes(1_000_000, 64 << 20, 100_000),
        io_factory: lanes(1_000_000, 64 << 20, 100_000),
        io_recipe: lanes(1_000_000, 64 << 20, 100_000),
        io_industry: lanes(1_000_000, 64 << 20, 100_000),
        io_industry_scan: lanes(1_000_000, 64 << 20, 100_000),
        io_members: lanes(1_000_000, 64 << 20, 100_000),
        io_fees: lanes(1_000_000, 64 << 20, 100_000),
        io_events: lanes(1_000_000, 64 << 20, 100_000),
        materials: lanes(1_000_000, 64 << 20, 100_000),
        driver_bootstrap_finality: lanes(1_000_000, 64 << 20, 100_000),
        driver_bootstrap_revocation_write: lanes(1_000_000, 64 << 20, 100_000),
        driver_bootstrap_authorization_write: lanes(1_000_000, 64 << 20, 100_000),
        driver_bootstrap_groups: lanes(1_000_000, 64 << 20, 100_000),
        driver_bootstrap_journal: lanes(1_000_000, 64 << 20, 100_000),
        driver_factory: lanes(1_000_000, 64 << 20, 100_000),
        driver_pending: lanes(1_000_000, 64 << 20, 100_000),
        driver_pending_write: lanes(1_000_000, 64 << 20, 100_000),
        driver_policy_scan: lanes(1_000_000, 64 << 20, 100_000),
        driver_policy: lanes(1_000_000, 64 << 20, 100_000),
        driver_actions_module_lifecycle: lanes(10_000, 16 << 20, 100_000),
        driver_actions_crisis: lanes(10_000, 16 << 20, 100_000),
        driver_actions_contract_accept: lanes(10_000, 16 << 20, 100_000),
        driver_actions_meta_progress: lanes(10_000, 16 << 20, 100_000),
        driver_actions_factory_lifecycle: lanes(10_000, 16 << 20, 100_000),
        driver_actions_route_availability: lanes(10_000, 16 << 20, 100_000),
        driver_actions_material_transfer: lanes(10_000, 16 << 20, 100_000),
        driver_actions_body: lanes(10_000, 16 << 20, 100_000),
        driver_actions_data_access: lanes(10_000, 16 << 20, 100_000),
        driver_actions_collect_data: lanes(10_000, 16 << 20, 100_000),
        driver_actions_authenticated_data: lanes(10_000, 16 << 20, 100_000),
        driver_actions_authenticated_work: lanes(10_000, 16 << 20, 100_000),
        driver_actions_transfer_resource: lanes(10_000, 16 << 20, 100_000),
        driver_actions_factory_schedule: lanes(10_000, 16 << 20, 100_000),
        driver_actions_contracts: lanes(10_000, 16 << 20, 100_000),
        driver_actions_alliance: lanes(10_000, 16 << 20, 100_000),
        driver_actions_logistics_registration: lanes(10_000, 16 << 20, 100_000),
        driver_actions_profile: lanes(10_000, 16 << 20, 100_000),
        driver_actions_extension: lanes(10_000, 16 << 20, 100_000),
        driver_tail_terminal: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_recipes: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_orders: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_world_delivery: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_world: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_gameplay: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_organization: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_threat_keys: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_history_index: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_cognition: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_authority: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail: lanes(1_000_000, 64 << 20, 100_000),
        driver_tail_path: lanes(1_000_000, 64 << 20, 100_000),
        driver_product: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        driver_audit: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        driver_rules: lanes(1_000_000, 64 << 20, 100_000),
        driver_enumeration: lanes(1_000_000, 64 << 20, 100_000),
        driver_event_tail: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        driver_finish_cut: NormalHostLaneLimitsV1 {
            max_original_bytes: 64 << 10,
            ..lanes(100_000, 16 << 20, 100_000)
        },
        driver_final_sources_projection: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_business: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_terminal_metadata: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_runtime_index: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_originals: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_cognition_hash: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources_authorization: lanes(1_000_000, 128 << 20, 100_000),
        driver_final_sources: lanes(1_000_000, 64 << 20, 100_000),
        bootstrap_directories: lanes(1_000_000, 64 << 20, 100_000),
    }
}
