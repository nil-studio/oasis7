//! Explicit existing work-lane fields; no authority or business source is built.
use super::*;

impl FromLimits for AgentLedgerTransferWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for MaterialWorkBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for CollectDataSignatureWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            checks_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for StarterBindingIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_rows: limits.max_collection_entries,
            max_row_text_bytes: limits.max_text_bytes,
            max_pending_rows: limits.max_pending_records,
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            moved_slots_remaining: limits.steps_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for WorldNativeFieldReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            records_remaining: limits.steps_per_counter,
            trees_remaining: limits.entries_per_counter,
            max_key_heap: limits.max_heap_bytes,
            max_pending_records: limits.max_pending_records,
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for FactoryScheduleIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_builds: limits.max_collection_entries,
            max_factory_id_bytes: limits.max_text_bytes,
            max_pending_changes: limits.max_pending_records,
        }
    }
}

impl FromLimits for FactoryScheduleReceiptWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_receipts: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
            max_produce_stacks: limits.max_collection_entries,
            max_pending_changes: limits.max_pending_records,
        }
    }
}

impl FromLimits for FactoryScheduleAuthorityWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_text_bytes: limits.max_text_bytes,
            max_authorized_agents: limits.max_collection_entries,
        }
    }
}

impl FromLimits for FactoryScheduleFactoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_text_bytes: limits.max_text_bytes,
            max_tags: limits.max_collection_entries,
            max_stacks: limits.max_collection_entries,
        }
    }
}

impl FromLimits for FactoryScheduleLogisticsWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_text_bytes: limits.max_text_bytes,
            max_path_routes: limits.max_collection_entries,
        }
    }
}

impl FromLimits for InventoryIdentityDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            visits_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_identity_bytes: limits.max_text_bytes,
            max_output_entries: limits.max_collection_entries,
        }
    }
}

impl FromLimits for DiplomacyIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_rows: limits.max_collection_entries,
            max_members: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
            max_pending_rows: limits.max_pending_records,
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for IndustrialClaimDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            visits_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_identity_bytes: limits.max_text_bytes,
            max_output_entries: limits.max_collection_entries,
            max_pending_records: limits.max_pending_records,
        }
    }
}

impl FromLimits for WorldRuleRemainderWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_semantic_bytes: limits.max_key_bytes,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl FromLimits for GameplayDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            visits_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_rows: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl FromLimits for ScheduleScanBudget {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            nodes_remaining: limits.steps_per_counter,
            read_bytes_remaining: limits.bytes_per_counter,
            buckets_remaining: limits.steps_per_counter,
            instances_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for DueModuleBatchLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            entries: limits.max_collection_entries,
            instance_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for CognitionHistoryIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for CognitionHistoryCborWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            bytes_remaining: limits.bytes_per_counter,
            nodes_remaining: limits.steps_per_counter,
            source_reads_remaining: limits.steps_per_counter,
            key_bytes_remaining: limits.bytes_per_counter,
            max_depth: limits.max_depth,
            max_key_bytes: limits.max_key_bytes,
        }
    }
}

impl FromLimits for JsonScalarChangeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            json: JsonReadWorkV1::from_limits(limits, core),
            paths_remaining: limits.steps_per_counter,
            max_pending_pairs: limits.max_pending_records,
        }
    }
}

impl FromLimits for BusinessDiffProcessingWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for ModuleStateContentBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            states_remaining: limits.steps_per_counter,
            logical_bytes_remaining: limits.bytes_per_counter,
            max_state_bytes: limits.max_original_bytes as u64,
        }
    }
}

impl FromLimits for WorldPrimitiveFamilyWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            records_remaining: limits.steps_per_counter,
            max_record_bytes: limits.max_record_bytes as u64,
        }
    }
}

impl FromLimits for NormalProjectionWorldRulesWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            source: AuthorizationRootSourceReadWorkV1::from_limits(limits, core),
            processing: BusinessDiffProcessingWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RuntimeCapabilityRecordsWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}
