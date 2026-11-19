//! One-time production account admission. Configuration values are local
//! per-counter ceilings, not spendable copies of Common. All created accounts
//! are moved into startup/service ownership; retries never call these makers.
use super::iws_live_limits::*;
use crate::iws_host_runtime::{
    HostCognitionStartupWorkV1, HostColdIoWorkV1, HostColdRecoveryPartsV1, HostColdRecoveryWorkV1,
    HostNormalIoWorkV1, HostObservationPartsV1, HostObservationWorkV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalArchiveReadWorkV1, IwsCandidateSourceWorkV1, IwsColdParentWorkV1, IwsCurrentLimitsV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        IncrementalFrozenMaterialWorkV1, bounded_context::FormalBoundedContextV1,
    },
    incremental_material::{
        CoreBudget, ExecutionArtifactBudget, FactBudget, OriginalArtifactBudget,
    },
    incremental_state::StateScanBudget,
};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_runtime::NormalHostWorkV1;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{
    incremental_cognition_history::CognitionHistoryIndexWorkV1,
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
    incremental_field_member::MemberRecordLimitsV1,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1},
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    incremental_runtime_history_source::RuntimeHistoryCandidateReadWorkV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_result_router::WorldResultRouterWorkV1,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IoCeilingsV1 {
    pub physical: PageIoLimitsV1,
    pub logical: ValueIoLimitsV1,
    pub steps_per_counter: u64,
    pub bytes_per_counter: u64,
    pub max_original_bytes: usize,
    pub max_heap_bytes: usize,
    pub max_depth: usize,
    pub max_map_keys: usize,
    pub max_key_bytes: usize,
    pub max_pending_node_bytes: usize,
    pub max_pending_pairs: usize,
    pub max_new_rows: usize,
    pub max_synced_pages: usize,
    pub sync_memo_steps: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ColdWorkConfigV1 {
    pub runtime: RuntimeLimitsDataV1,
    pub io: IoCeilingsV1,
}
impl IoCeilingsV1 {
    pub(super) fn canonical(&self) -> CanonicalCborWorkV1 {
        CanonicalCborWorkV1 {
            visits_remaining: self.steps_per_counter,
            bytes_remaining: self.bytes_per_counter,
            max_depth: self.max_depth,
            max_map_keys: self.max_map_keys,
            max_key_bytes: self.max_key_bytes,
        }
    }
    fn structure(&self) -> CborPreflightWorkV1 {
        CborPreflightWorkV1 {
            bytes_remaining: self.bytes_per_counter,
            values_remaining: self.steps_per_counter,
            max_depth: self.max_depth,
        }
    }
    fn core(&self) -> CoreBudget {
        CoreBudget {
            objects_remaining: self.steps_per_counter,
            original_bytes_remaining: self.bytes_per_counter,
            new_pages_remaining: self.steps_per_counter,
            merkle_reads_remaining: self.steps_per_counter,
        }
    }
    fn facts(&self) -> FactBudget {
        FactBudget {
            facts_remaining: self.steps_per_counter,
            original_bytes_remaining: self.bytes_per_counter,
            max_fact_bytes: self.max_original_bytes as u64,
        }
    }
    fn runtime_index(&self) -> RuntimeHistoryIndexWorkV1 {
        RuntimeHistoryIndexWorkV1 {
            cells_remaining: self.steps_per_counter,
            max_pending_node_bytes: self.max_pending_node_bytes,
        }
    }
    fn decode(&self) -> RuntimeHistoryDecodeWorkV1 {
        RuntimeHistoryDecodeWorkV1 {
            structure: self.structure(),
            canonical: self.canonical(),
            max_original_bytes: self.max_original_bytes,
            max_heap_bytes: self.max_heap_bytes,
        }
    }
    fn cbor(&self) -> CognitionHistoryCborWorkV1 {
        CognitionHistoryCborWorkV1 {
            bytes_remaining: self.bytes_per_counter,
            nodes_remaining: self.steps_per_counter,
            source_reads_remaining: self.steps_per_counter,
            key_bytes_remaining: self.bytes_per_counter,
            max_depth: self.max_depth,
            max_key_bytes: self.max_key_bytes,
        }
    }
    fn json(&self) -> JsonReadWorkV1 {
        JsonReadWorkV1 {
            cells_remaining: self.steps_per_counter,
            json_nodes_remaining: self.steps_per_counter,
            max_depth: self.max_depth,
            max_payload_bytes: self.max_original_bytes as u64,
        }
    }
    fn router(&self) -> WorldResultRouterWorkV1 {
        WorldResultRouterWorkV1 {
            facts: self.facts(),
            envelope: self.structure(),
            runtime: self.decode(),
            cognition: self.cbor(),
        }
    }
    fn keys(&self) -> SemanticKeyBudget {
        SemanticKeyBudget {
            bytes_remaining: self.bytes_per_counter,
        }
    }
    fn cognition_index(&self) -> CognitionHistoryIndexWorkV1 {
        CognitionHistoryIndexWorkV1 {
            cells_remaining: self.steps_per_counter,
            max_pending_node_bytes: self.max_pending_node_bytes,
        }
    }
    fn hashes(&self) -> CognitionHeadHashWorkV1 {
        CognitionHeadHashWorkV1 {
            input_bytes_remaining: self.bytes_per_counter,
            hash_nodes_remaining: self.steps_per_counter,
        }
    }
    pub(super) fn event_decode(&self) -> WorldEventDecodeWorkV1 {
        WorldEventDecodeWorkV1 {
            structure: self.structure(),
            canonical: self.canonical(),
            max_original_bytes: self.max_original_bytes,
            max_heap_bytes: self.max_heap_bytes,
        }
    }
    pub(super) fn frozen(&self) -> IncrementalFrozenMaterialWorkV1 {
        IncrementalFrozenMaterialWorkV1 {
            metadata_entries_remaining: self.steps_per_counter,
            comparison_bytes_remaining: self.bytes_per_counter,
            hash_bytes_remaining: self.bytes_per_counter,
            frozen_wire_bytes_remaining: self.bytes_per_counter,
            original_calls_remaining: self.steps_per_counter,
            original_bytes_remaining: self.bytes_per_counter,
            max_original_bytes: self.max_original_bytes as u64,
        }
    }
    pub(super) fn source(&self) -> IwsCandidateSourceWorkV1 {
        IwsCandidateSourceWorkV1 {
            router: self.router(),
            proof_ledger_metadata_steps: self.steps_per_counter,
            proof_ledger_metadata_bytes: self.bytes_per_counter,
            proof_ledger_cold: self.json(),
            proof_ledger_changed: JsonChangeWorkV1 {
                json: self.json(),
                paths_remaining: self.steps_per_counter,
                max_pending_pairs: self.max_pending_pairs,
            },
            runtime: RuntimeHistoryCandidateReadWorkV1 {
                rows_remaining: self.steps_per_counter,
                max_new_rows: self.max_new_rows,
                index: self.runtime_index(),
                originals: self.facts(),
                decode: self.decode(),
                max_world_event_heap_bytes: self.max_heap_bytes,
            },
            cognition_index: self.cognition_index(),
            cognition_cbor: self.cbor(),
            cognition_hash: self.hashes(),
            cognition_lifecycle: CognitionLifecycleWorkV2 {
                key_bytes_remaining: self.bytes_per_counter,
                cells_remaining: self.steps_per_counter,
                max_pending_node_bytes: self.max_pending_node_bytes,
            },
            history_physical: self.physical.build(),
            history_values: self.logical.build(),
            persist_physical: self.physical.build(),
            persist_values: self.logical.build(),
            write_physical: self.physical.build(),
            write_values: self.logical.build(),
            write_keys: self.keys(),
            write_limits: MemberRecordLimitsV1 {
                semantic_bytes: self.max_key_bytes as u64,
                payload_bytes: self.max_original_bytes as u64,
                record_bytes: self.max_original_bytes as u64,
            },
            history_occurrences: self.steps_per_counter,
            history_hash_bytes: self.bytes_per_counter,
            persist_occurrences: self.steps_per_counter,
            artifacts: ExecutionArtifactBudget {
                structural: self.core(),
                originals: OriginalArtifactBudget {
                    objects_remaining: self.steps_per_counter,
                    bytes_remaining: self.bytes_per_counter,
                    max_object_bytes: self.max_original_bytes as u64,
                },
            },
            native_chunk_occurrences: self.steps_per_counter,
            native_chunk_bytes: self.bytes_per_counter,
            frozen_wire_bytes: self.bytes_per_counter,
            frozen_material: self.frozen(),
            original_sync_occurrences: self.steps_per_counter,
            original_sync_bytes: self.bytes_per_counter,
            original_sync_physical: self.physical.build(),
            original_sync_values: self.logical.build(),
            cognition_sync_physical: self.physical.build(),
            cognition_sync_values: self.logical.build(),
            publish_physical: self.physical.build(),
            max_synced_pages: self.max_synced_pages,
            sync_memo_steps: self.sync_memo_steps,
            catalog_read_physical: self.physical.build(),
            catalog_publish_physical: self.physical.build(),
            catalog_occurrences: self.steps_per_counter,
            transition_scan: StateScanBudget {
                nodes_remaining: self.steps_per_counter,
                read_bytes_remaining: self.bytes_per_counter,
                leaves_remaining: self.steps_per_counter,
            },
            transition_occurrences: self.steps_per_counter,
        }
    }
    pub(super) fn cold_io(&self) -> HostColdIoWorkV1 {
        HostColdIoWorkV1 {
            process_canonical: self.canonical(),
            state: self.physical.build(),
            values: self.physical.build(),
            members: self.physical.build(),
            originals: self.physical.build(),
            original_values: self.logical.build(),
        }
    }
    pub(super) fn cognition(&self) -> HostCognitionStartupWorkV1 {
        HostCognitionStartupWorkV1 {
            core: self.core(),
            facts: self.facts(),
            router: self.router(),
            fields: self.logical.build(),
            keys: self.keys(),
            json: self.json(),
            semantic_bytes: self.max_key_bytes as u64,
            record_bytes: self.max_original_bytes as u64,
        }
    }
    pub(super) fn observe(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<HostObservationWorkV1, String> {
        HostObservationWorkV1::from_parts_bounded(
            HostObservationPartsV1 {
                state_pages: self.physical.build(),
                value_pages: self.physical.build(),
                original_pages: self.physical.build(),
                logical: self.logical.build(),
                original_values: self.logical.build(),
                keys: self.keys(),
                event_decode: self.event_decode(),
                points_remaining: self.steps_per_counter,
                events_remaining: self.steps_per_counter,
                semantic_bytes: self.max_key_bytes as u64,
                record_bytes: self.max_original_bytes as u64,
                max_scalar_bytes: self.max_original_bytes as u64,
            },
            common,
        )
        .map_err(|e| e.to_string())
    }
    pub(super) fn normal_io(&self, replica: &Self) -> HostNormalIoWorkV1 {
        HostNormalIoWorkV1 {
            state_pages: self.physical.build(),
            value_pages: self.physical.build(),
            original_pages: self.physical.build(),
            original_values: self.logical.build(),
            replica_value_pages: replica.physical.build(),
            replica_original_pages: replica.physical.build(),
            replica_original_values: replica.logical.build(),
        }
    }
}
impl ColdWorkConfigV1 {
    pub(super) fn admit(
        &self,
        limits: &IwsCurrentLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<HostColdRecoveryWorkV1, String> {
        let io = &self.io;
        let runtime = NormalHostWorkV1::new_bounded(self.runtime.build(), common)
            .map_err(|e| e.to_string())?;
        let cold = IwsColdParentWorkV1 {
            objects_physical: io.physical.build(),
            objects_values: io.logical.build(),
            state_recovery: io.physical.build(),
            values_recovery: io.physical.build(),
            read_recovery: io.logical.build(),
            keys_recovery: io.keys(),
            state_ordinary: io.physical.build(),
            values_ordinary: io.physical.build(),
            read_ordinary: io.logical.build(),
            keys_ordinary: io.keys(),
            recovery_memo: None,
            core: io.core(),
            facts: io.facts(),
            router: io.router(),
            proof_ledger_read: io.json(),
            proof_ledger_readmission: FormalArchiveReadWorkV1::new(0, 0)
                .map_err(|e| e.to_string())?,
            current: limits.current.work().map_err(|e| e.to_string())?,
            admission: limits.current.work().map_err(|e| e.to_string())?,
            runtime_index: io.runtime_index(),
            cognition_index: io.cognition_index(),
            hashes: io.hashes(),
            mailbox_decode: io.event_decode(),
            mailbox_occurrences: limits.current.mailbox_occurrences_remaining,
            scratch_pages: limits.scratch_pages,
            semantic_bytes: limits.current.max_semantic_bytes as u64,
            record_bytes: limits.current.max_record_bytes as u64,
            local_records_remaining: limits.current.records_remaining,
            local_original_bytes_remaining: limits.current.original_bytes_remaining,
        };
        HostColdRecoveryWorkV1::from_parts_bounded(
            HostColdRecoveryPartsV1 {
                cold,
                source: io.source(),
                physical: io.cold_io(),
                runtime,
            },
            common,
        )
        .map_err(|e| e.to_string())
    }
}
