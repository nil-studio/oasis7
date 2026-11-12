//! The original finite GEN fixture accounts, shared by native and host harnesses.
use oasis7_distfs::{
    controlled_authority::formal::endpoint::{
        IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
    },
    incremental_page_store::PageIoBudget,
};
use oasis7_formal_protocol::{
    controlled_authority::formal, incremental_material::*, incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_cognition_history_cbor::CognitionHistoryCborWorkV1;
use oasis7_world_state::{
    incremental_cognition_history::*, incremental_field_member::*,
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
    incremental_runtime_history_index::*, incremental_world_result_router::*,
};
pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        // Physical reads include frame/recipe verification and repeated
        // Current qualification, not only the one-time page publication.
        reads_remaining: 128_000_000,
        read_bytes_remaining: 4 << 30,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}
pub(crate) fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}
pub(crate) fn core_budget() -> CoreBudget {
    CoreBudget {
        objects_remaining: 128,
        original_bytes_remaining: 16 << 20,
        new_pages_remaining: 4096,
        merkle_reads_remaining: 100000,
    }
}
pub(crate) fn facts() -> FactBudget {
    FactBudget {
        facts_remaining: 128,
        original_bytes_remaining: 16 << 20,
        max_fact_bytes: 4 << 20,
    }
}
pub(crate) fn work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
    }
}
pub(crate) fn canonical_work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 100000,
        bytes_remaining: 16 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 1024,
    }
}
pub(crate) fn limits() -> IwsCurrentLimitsV1 {
    use oasis7_world_state::incremental_world_current_limits::WorldCurrentStateLimitsV1;
    let current = WorldCurrentStateLimitsV1 {
        buckets_remaining: 1000000,
        bytes_remaining: 1073741824,
        cells_remaining: 1000000,
        changed_cells_remaining: 1000000,
        comparison_bytes_remaining: 1073741824,
        index_steps_remaining: 1000000,
        instance_relations_remaining: 1000000,
        instances_remaining: 1000000,
        json_nodes_remaining: 1000000,
        key_changes_remaining: 1000000,
        leaves_remaining: 1000000,
        legacy_cells_remaining: 1000000,
        logical_bytes_remaining: 1073741824,
        // CandidateMailboxReader pre-admits 256 bytes per occurrence. This
        // fresh GEN has no mailbox history; keep a finite 1 MiB allowance,
        // rather than a 256 MB reservation competing with its page memo.
        mailbox_occurrences_remaining: 4096,
        max_agents: 100,
        max_depth: 64,
        max_instance_scalar_bytes: 16777216,
        max_key_bytes: 4096,
        max_key_heap: 16777216,
        max_map_keys: 1024,
        max_object_bytes: 16777216,
        max_original_bytes: 16777216,
        max_payload_bytes: 16777216,
        max_pending_node_bytes: 16777216,
        max_pending_pairs: 65536,
        // The native field scanner has a hard 256-record page contract.
        max_pending_records: 256,
        max_record_bytes: 16777216,
        max_retained_trees: 100000,
        max_scalar_bytes: 16777216,
        max_selected_heap: 16777216,
        max_semantic_bytes: 1048576,
        max_state_bytes: 16777216,
        max_text_bytes: 16777216,
        max_tree_heap: 16777216,
        membership_cells_remaining: 1000000,
        nodes_remaining: 1000000,
        objects_remaining: 1000000,
        original_bytes_remaining: 1073741824,
        outer_cells_remaining: 1000000,
        outer_records_remaining: 1000000,
        outer_rows_remaining: 1000000,
        paths_remaining: 1000000,
        read_bytes: 1073741824,
        read_bytes_remaining: 1073741824,
        reads: 1000000,
        records_remaining: 1000000,
        states_remaining: 1000000,
        steps_remaining: 1000000,
        trees_remaining: 1000000,
        visits_remaining: 1000000,
        write_bytes: 1073741824,
        writes: 1000000,
    };
    IwsCurrentLimitsV1 { current,scratch_pages:4096,cognition:oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Enabled,
        decode_bytes:1<<30,decode_values:1_000_000,decode_depth:64,decode_max_original_bytes:16<<20,decode_max_heap_bytes:32<<20,
        canonical_visits:1_000_000,canonical_bytes:1<<30,canonical_map_keys:4096,canonical_key_bytes:4096 }
}
pub(crate) fn budgets() -> IwsEndpointVerificationBudgetsV1 {
    IwsEndpointVerificationBudgetsV1 {
        original_objects: 10000,
        original_bytes: 512 << 20,
        new_pages: 65_536,
        core_merkle_reads: 4_000_000,
        state_scan_nodes: 4_000_000,
        state_scan_bytes: 1 << 30,
        state_scan_leaves: 1_000_000,
        full_value_occurrences: 1_000_000,
        full_value_bytes: 1 << 30,
        max_full_value_bytes: 32 << 20,
        transition_value_occurrences: 1_000_000,
        transition_value_bytes: 1 << 30,
        max_transition_value_bytes: 32 << 20,
        value_nodes: 4_000_000,
        value_read_bytes: 1 << 30,
        max_pending_node_bytes: 16 << 20,
        state_page_io: physical(),
        value_page_io: physical(),
        original_page_io: physical(),
        original_value_nodes: 1_000_000,
        original_value_read_bytes: 512 << 20,
    }
}

pub(crate) fn source_work()
-> oasis7_distfs::controlled_authority::formal::endpoint::IwsCandidateSourceWorkV1 {
    use oasis7_distfs::controlled_authority::formal::endpoint::IwsCandidateSourceWorkV1;
    use oasis7_world_state::{
        incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
        incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
        incremental_runtime_history_source::RuntimeHistoryCandidateReadWorkV1,
    };
    let structure = || CborPreflightWorkV1 {
        bytes_remaining: 512 << 20,
        values_remaining: 1_000_000,
        max_depth: 64,
    };
    let decode = || RuntimeHistoryDecodeWorkV1 {
        structure: structure(),
        canonical: canonical_work(),
        max_original_bytes: 16 << 20,
        max_heap_bytes: 32 << 20,
    };
    let cbor = || CognitionHistoryCborWorkV1 {
        bytes_remaining: 512 << 20,
        nodes_remaining: 1_000_000,
        source_reads_remaining: 1_000_000,
        key_bytes_remaining: 64 << 20,
        max_depth: 64,
        max_key_bytes: 4096,
    };
    IwsCandidateSourceWorkV1 {
        router: WorldResultRouterWorkV1 {
            facts: facts(),
            envelope: structure(),
            runtime: decode(),
            cognition: cbor(),
        },
        runtime: RuntimeHistoryCandidateReadWorkV1 {
            rows_remaining: 1_000_000,
            max_new_rows: 100000,
            index: work(),
            originals: facts(),
            decode: decode(),
            max_world_event_heap_bytes: 32 << 20,
        },
        cognition_index: CognitionHistoryIndexWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
        },
        cognition_cbor: cbor(),
        cognition_hash: CognitionHeadHashWorkV1 {
            input_bytes_remaining: 512 << 20,
            hash_nodes_remaining: 1_000_000,
        },
        cognition_lifecycle: CognitionLifecycleWorkV2 {
            key_bytes_remaining: 64 << 20,
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
        },
        history_physical: physical(),
        history_values: logical(),
        persist_physical: physical(),
        persist_values: logical(),
        write_physical: physical(),
        write_values: logical(),
        write_keys: SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        },
        write_limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 16 << 20,
            record_bytes: 16 << 20,
        },
        history_occurrences: 1_000_000,
        history_hash_bytes: 512 << 20,
        persist_occurrences: 1_000_000,
        artifacts: ExecutionArtifactBudget {
            structural: CoreBudget {
                objects_remaining: 10000,
                original_bytes_remaining: 512 << 20,
                new_pages_remaining: 1_000_000,
                merkle_reads_remaining: 4_000_000,
            },
            originals: OriginalArtifactBudget {
                objects_remaining: 10000,
                bytes_remaining: 512 << 20,
                max_object_bytes: 16 << 20,
            },
        },
        native_chunk_occurrences: 10000,
        native_chunk_bytes: 512 << 20,
        frozen_wire_bytes: 64 << 20,
        frozen_material: formal::IncrementalFrozenMaterialWorkV1 {
            metadata_entries_remaining: 10000,
            comparison_bytes_remaining: 512 << 20,
            hash_bytes_remaining: 512 << 20,
            frozen_wire_bytes_remaining: 64 << 20,
            original_calls_remaining: 10000,
            original_bytes_remaining: 512 << 20,
            max_original_bytes: 16 << 20,
        },
        original_sync_occurrences: 1_000_000,
        original_sync_bytes: 512 << 20,
        original_sync_physical: physical(),
        original_sync_values: logical(),
        cognition_sync_physical: physical(),
        cognition_sync_values: logical(),
        publish_physical: physical(),
        // StoreSessions receives this field, not Core.new_pages. Publisher
        // pre-admits 2048 bytes per memo slot plus 4 KiB: about 256 MiB,
        // within the fixture's real 512 MiB pool. The genuine fixture writes
        // over 100,000 recipes; 65,536 distinct synchronized pages is too low.
        max_synced_pages: 131_072,
        // Publisher charges 304 per lookup and up to 912 per insertion at
        // this bound. Cover state/value logical revisits as well as unique
        // pages; four million work units cannot cover this full GEN graph.
        sync_memo_steps: 4_000_000_000,
        catalog_read_physical: physical(),
        catalog_publish_physical: physical(),
        catalog_occurrences: 1_000_000,
        transition_scan: StateScanBudget {
            nodes_remaining: 4_000_000,
            read_bytes_remaining: 1 << 30,
            leaves_remaining: 1_000_000,
        },
        transition_occurrences: 1_000_000,
    }
}
