//! Explicit configuration DATA for the production IWS consumer. No defaults
//! or account refills are provided. Conversion preserves each named field;
//! the existing producers perform their ordinary admission and validation.
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
    },
    incremental_page_store::PageIoBudget,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
    incremental_execution::TransactionLimits, incremental_value::ValueIoBudget,
};
use oasis7_runtime::{
    AuthorizationRootCacheLimitsV1, NormalHostLaneLimitsV1, NormalHostWorkLimitsV1,
};
use oasis7_wasm_executor::bounded_worker::WorkerLimitsV1;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_world_current_limits::WorldCurrentStateLimitsV1,
};
use serde::{Deserialize, Serialize};
trait ScalarData: Copy {
    fn build(&self) -> Self {
        *self
    }
}
impl ScalarData for usize {}
impl ScalarData for u64 {}
macro_rules! limit_data {
    ($name:ident => $target:ident { $($field:ident: $ty:ty,)* }) => {
        #[derive(Clone,Serialize,Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(super) struct $name { $(pub $field:$ty,)* }
        impl $name {
            pub(super) fn build(&self)->$target { $target { $($field:self.$field.build(),)* } }
        }
        #[cfg(any(test, feature = "iws-host-native-test-support"))]
        impl From<$target> for $name {
            fn from(value: $target)->Self { Self { $($field:value.$field.into(),)* } }
        }
    };
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CognitionPresenceDataV1 {
    Enabled,
    Disabled,
}
impl CognitionPresenceDataV1 {
    fn build(&self) -> CognitionCurrentPresenceV3 {
        match self {
            Self::Enabled => CognitionCurrentPresenceV3::Enabled,
            Self::Disabled => CognitionCurrentPresenceV3::Disabled,
        }
    }
}
#[cfg(any(test, feature = "iws-host-native-test-support"))]
impl From<CognitionCurrentPresenceV3> for CognitionPresenceDataV1 {
    fn from(value: CognitionCurrentPresenceV3) -> Self {
        match value {
            CognitionCurrentPresenceV3::Enabled => Self::Enabled,
            CognitionCurrentPresenceV3::Disabled => Self::Disabled,
        }
    }
}
limit_data!(ValueIoLimitsV1 => ValueIoBudget {
    nodes_remaining:u64,
    read_bytes_remaining:u64,
    write_bytes_remaining:u64,
});
limit_data!(InputLimitsDataV1 => CborDataLimitsV1 {
    max_wire_bytes:usize,
    max_graph_bytes:usize,
    max_depth:usize,
});
limit_data!(PageIoLimitsV1 => PageIoBudget {
    reads_remaining: u64,
    read_bytes_remaining: u64,
    writes_remaining: u64,
    write_bytes_remaining: u64,
});

limit_data!(TransactionLimitsDataV1 => TransactionLimits {
    max_reads: usize,
    max_writes: usize,
    max_facts: usize,
    max_pending_node_bytes: usize,
});

limit_data!(WorkerLimitsDataV1 => WorkerLimitsV1 {
    address_space_bytes: u64,
    max_request_bytes: u64,
    max_output_bytes: u64,
    wall_time_ms: u64,
});

limit_data!(ProjectionLimitsDataV1 => AuthorizationRootCacheLimitsV1 {
    max_wire_bytes: usize,
    max_depth: usize,
    max_map_keys: usize,
    max_key_bytes: usize,
});

limit_data!(CurrentOwnerLimitsDataV1 => WorldCurrentStateLimitsV1 {
    buckets_remaining: u64,
    bytes_remaining: u64,
    cells_remaining: u64,
    changed_cells_remaining: u64,
    comparison_bytes_remaining: u64,
    index_steps_remaining: u64,
    instance_relations_remaining: u64,
    instances_remaining: u64,
    json_nodes_remaining: u64,
    key_changes_remaining: u64,
    leaves_remaining: u64,
    legacy_cells_remaining: u64,
    logical_bytes_remaining: u64,
    mailbox_occurrences_remaining: u64,
    max_agents: usize,
    max_depth: usize,
    max_instance_scalar_bytes: usize,
    max_key_bytes: usize,
    max_key_heap: usize,
    max_map_keys: usize,
    max_object_bytes: usize,
    max_original_bytes: usize,
    max_payload_bytes: usize,
    max_pending_node_bytes: usize,
    max_pending_pairs: usize,
    max_pending_records: usize,
    max_record_bytes: usize,
    max_retained_trees: usize,
    max_scalar_bytes: usize,
    max_selected_heap: usize,
    max_semantic_bytes: usize,
    max_state_bytes: usize,
    max_text_bytes: usize,
    max_tree_heap: usize,
    membership_cells_remaining: u64,
    nodes_remaining: u64,
    objects_remaining: u64,
    original_bytes_remaining: u64,
    outer_cells_remaining: u64,
    outer_records_remaining: u64,
    outer_rows_remaining: u64,
    paths_remaining: u64,
    read_bytes: u64,
    read_bytes_remaining: u64,
    reads: u64,
    records_remaining: u64,
    states_remaining: u64,
    steps_remaining: u64,
    trees_remaining: u64,
    visits_remaining: u64,
    write_bytes: u64,
    writes: u64,
});

limit_data!(CurrentLimitsDataV1 => IwsCurrentLimitsV1 {
    current: CurrentOwnerLimitsDataV1,
    scratch_pages: usize,
    cognition: CognitionPresenceDataV1,
    decode_bytes: u64,
    decode_values: u64,
    decode_depth: usize,
    decode_max_original_bytes: usize,
    decode_max_heap_bytes: usize,
    canonical_visits: u64,
    canonical_bytes: u64,
    canonical_map_keys: usize,
    canonical_key_bytes: usize,
});

limit_data!(EndpointLimitsDataV1 => IwsEndpointVerificationBudgetsV1 {
    original_objects: u64,
    original_bytes: u64,
    new_pages: u64,
    core_merkle_reads: u64,
    state_scan_nodes: u64,
    state_scan_bytes: u64,
    state_scan_leaves: u64,
    full_value_occurrences: u64,
    full_value_bytes: u64,
    max_full_value_bytes: u64,
    transition_value_occurrences: u64,
    transition_value_bytes: u64,
    max_transition_value_bytes: u64,
    value_nodes: u64,
    value_read_bytes: u64,
    max_pending_node_bytes: usize,
    state_page_io: PageIoLimitsV1,
    value_page_io: PageIoLimitsV1,
    original_page_io: PageIoLimitsV1,
    original_value_nodes: u64,
    original_value_read_bytes: u64,
});

limit_data!(RuntimeLaneLimitsDataV1 => NormalHostLaneLimitsV1 {
    steps_per_counter: u64,
    bytes_per_counter: u64,
    entries_per_counter: usize,
    max_collection_entries: usize,
    max_pending_records: usize,
    max_text_bytes: usize,
    max_key_bytes: usize,
    max_scalar_bytes: usize,
    max_payload_bytes: usize,
    max_record_bytes: usize,
    max_original_bytes: usize,
    max_heap_bytes: usize,
    max_pending_node_bytes: usize,
    max_depth: usize,
});

limit_data!(RuntimeLimitsDataV1 => NormalHostWorkLimitsV1 {
    invocation_invocation: RuntimeLaneLimitsDataV1,
    invocation_manifest: RuntimeLaneLimitsDataV1,
    invocation_event_original: RuntimeLaneLimitsDataV1,
    invocation: RuntimeLaneLimitsDataV1,
    io_overlay: RuntimeLaneLimitsDataV1,
    io_current: RuntimeLaneLimitsDataV1,
    io_parent: RuntimeLaneLimitsDataV1,
    io_admission: RuntimeLaneLimitsDataV1,
    io_factory: RuntimeLaneLimitsDataV1,
    io_recipe: RuntimeLaneLimitsDataV1,
    io_industry: RuntimeLaneLimitsDataV1,
    io_industry_scan: RuntimeLaneLimitsDataV1,
    io_members: RuntimeLaneLimitsDataV1,
    io_fees: RuntimeLaneLimitsDataV1,
    io_events: RuntimeLaneLimitsDataV1,
    materials: RuntimeLaneLimitsDataV1,
    driver_bootstrap_finality: RuntimeLaneLimitsDataV1,
    driver_bootstrap_revocation_write: RuntimeLaneLimitsDataV1,
    driver_bootstrap_authorization_write: RuntimeLaneLimitsDataV1,
    driver_bootstrap_groups: RuntimeLaneLimitsDataV1,
    driver_bootstrap_journal: RuntimeLaneLimitsDataV1,
    driver_factory: RuntimeLaneLimitsDataV1,
    driver_pending: RuntimeLaneLimitsDataV1,
    driver_pending_write: RuntimeLaneLimitsDataV1,
    driver_policy_scan: RuntimeLaneLimitsDataV1,
    driver_policy: RuntimeLaneLimitsDataV1,
    driver_actions_module_lifecycle: RuntimeLaneLimitsDataV1,
    driver_actions_crisis: RuntimeLaneLimitsDataV1,
    driver_actions_contract_accept: RuntimeLaneLimitsDataV1,
    driver_actions_meta_progress: RuntimeLaneLimitsDataV1,
    driver_actions_factory_lifecycle: RuntimeLaneLimitsDataV1,
    driver_actions_route_availability: RuntimeLaneLimitsDataV1,
    driver_actions_material_transfer: RuntimeLaneLimitsDataV1,
    driver_actions_body: RuntimeLaneLimitsDataV1,
    driver_actions_data_access: RuntimeLaneLimitsDataV1,
    driver_actions_collect_data: RuntimeLaneLimitsDataV1,
    driver_actions_authenticated_data: RuntimeLaneLimitsDataV1,
    driver_actions_authenticated_work: RuntimeLaneLimitsDataV1,
    driver_actions_transfer_resource: RuntimeLaneLimitsDataV1,
    driver_actions_main_token_transfer: RuntimeLaneLimitsDataV1,
    driver_actions_factory_schedule: RuntimeLaneLimitsDataV1,
    driver_actions_contracts: RuntimeLaneLimitsDataV1,
    driver_actions_alliance: RuntimeLaneLimitsDataV1,
    driver_actions_logistics_registration: RuntimeLaneLimitsDataV1,
    driver_actions_profile: RuntimeLaneLimitsDataV1,
    driver_actions_extension: RuntimeLaneLimitsDataV1,
    driver_tail_terminal: RuntimeLaneLimitsDataV1,
    driver_tail_recipes: RuntimeLaneLimitsDataV1,
    driver_tail_orders: RuntimeLaneLimitsDataV1,
    driver_tail_world_delivery: RuntimeLaneLimitsDataV1,
    driver_tail_world: RuntimeLaneLimitsDataV1,
    driver_tail_gameplay: RuntimeLaneLimitsDataV1,
    driver_tail_organization: RuntimeLaneLimitsDataV1,
    driver_tail_threat_keys: RuntimeLaneLimitsDataV1,
    driver_tail_history_index: RuntimeLaneLimitsDataV1,
    driver_tail_cognition: RuntimeLaneLimitsDataV1,
    driver_tail_authority: RuntimeLaneLimitsDataV1,
    driver_tail: RuntimeLaneLimitsDataV1,
    driver_tail_path: RuntimeLaneLimitsDataV1,
    driver_product: RuntimeLaneLimitsDataV1,
    driver_audit: RuntimeLaneLimitsDataV1,
    driver_rules: RuntimeLaneLimitsDataV1,
    driver_enumeration: RuntimeLaneLimitsDataV1,
    driver_event_tail: RuntimeLaneLimitsDataV1,
    driver_finish_cut: RuntimeLaneLimitsDataV1,
    driver_final_sources_projection: RuntimeLaneLimitsDataV1,
    driver_final_sources_business: RuntimeLaneLimitsDataV1,
    driver_final_sources_terminal_metadata: RuntimeLaneLimitsDataV1,
    driver_final_sources_runtime_index: RuntimeLaneLimitsDataV1,
    driver_final_sources_originals: RuntimeLaneLimitsDataV1,
    driver_final_sources_cognition_hash: RuntimeLaneLimitsDataV1,
    driver_final_sources_authorization: RuntimeLaneLimitsDataV1,
    driver_final_sources: RuntimeLaneLimitsDataV1,
    bootstrap_directories: RuntimeLaneLimitsDataV1,
});
