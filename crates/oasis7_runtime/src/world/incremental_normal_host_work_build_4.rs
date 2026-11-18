//! Explicit existing work-lane fields; no authority or business source is built.
use super::*;

impl FromLimits for RuleDecisionPayloadDecodeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            structure: CborPreflightWorkV1::from_limits(limits, core),
            max_payload_bytes: limits.max_payload_bytes,
            max_heap_bytes: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for NormalProjectionWorkBankV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            finality: CurrentBootstrapFinalityWorkV1::from_limits(limits, core),
            controls: WorldCollectionControlsWorkV1::from_limits(limits, core),
            runtime_inner_scan: StateScanBudget::from_limits(limits, core),
            runtime_config: ArtifactConfigReadWorkV1::from_limits(limits, core),
            runtime_resource_scans: std::array::from_fn(|_| {
                StateScanBudget::from_limits(limits, core)
            }),
            runtime_resource_cells: RuntimeArtifactResourcesWorkV1::from_limits(limits, core),
            runtime_governance: RuntimeGovernanceWorkV1::from_limits(limits, core),
            runtime_authorization_cells: limits.steps_per_counter,
            runtime_controls_cells: limits.steps_per_counter,
            cognition_history_index: CognitionHistoryIndexWorkV1::from_limits(limits, core),
            cognition_history_cbor: CognitionHistoryCborWorkV1::from_limits(limits, core),
            cognition_json: JsonScalarChangeWorkV1::from_limits(limits, core),
            cognition_controls_remaining: limits.steps_per_counter,
            aux_authorization: AuthorizationRootSourceReadWorkV1::from_limits(limits, core),
            aux_processing: BusinessDiffProcessingWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
            module_states: ModuleStateContentBudgetV1::from_limits(limits, core),
            inventory: InventoryFamilyWorkV1::from_limits(limits, core),
            primitives: WorldPrimitiveFamilyWorkV1::from_limits(limits, core),
            factory: FactoryWorkBudgetV1::from_limits(limits, core),
            industry: IndustryWorkBudgetV1::from_limits(limits, core),
            logistics: LogisticsWorkBudgetV1::from_limits(limits, core),
            world_rules: NormalProjectionWorldRulesWorkV1::from_limits(limits, core),
            processing: BusinessDiffProcessingWorkV1::from_limits(limits, core),
            pending_entries: limits.entries_per_counter,
            originals: FactBudget::from_limits(limits, core),
            events: WorldEventDecodeWorkV1::from_limits(limits, core),
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
            max_json_output_bytes: limits.max_original_bytes,
        }
    }
}

impl FromLimits for NormalBusinessReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            authorization: AuthorizationRootSourceReadWorkV1::from_limits(limits, core),
            processing: BusinessDiffProcessingWorkV1::from_limits(limits, core),
            starter_native: WorldNativeFieldReadWorkV1::from_limits(limits, core),
            starter_scan: StateScanBudget::from_limits(limits, core),
            starter: StarterBindingIndexWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for NormalTerminalMetadataReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
            native: TypedRecordReadWorkV1::from_limits(limits, core),
            native_scan: StateScanBudget::from_limits(limits, core),
            native_cells: limits.steps_per_counter,
            pending_entries: limits.entries_per_counter,
            ledger: TypedRecordReadWorkV1::from_limits(limits, core),
            ledger_scan: StateScanBudget::from_limits(limits, core),
            originals: ProductValidationDeliveryWorkV1::from_limits(limits, core),
            max_text_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for CognitionHeadHashWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            input_bytes_remaining: limits.bytes_per_counter,
            hash_nodes_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for AuthorizationRootSourceReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            module_authorization: ModuleAuthorizationReadWorkV1::from_limits(limits, core),
            json: JsonReadWorkV1::from_limits(limits, core),
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
            capabilities: RuntimeCapabilityRecordsWorkV1::from_limits(limits, core),
            pending_entries: limits.entries_per_counter,
            max_selected_heap: limits.max_heap_bytes,
            max_text_bytes: limits.max_text_bytes as u64,
        }
    }
}

impl FromLimits for RootModuleAuthorizationLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_text_bytes: limits.max_text_bytes,
            max_effect_kinds: limits.max_collection_entries,
        }
    }
}

impl FromLimits for SelectedModuleManifestLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            heap_bytes: limits.max_heap_bytes,
            json_heap_bytes: limits.max_heap_bytes,
            max_collection_items: limits.max_collection_entries as u64,
        }
    }
}

impl FromLimits for RuntimeHistoryDecodeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            structure: CborPreflightWorkV1::from_limits(limits, core),
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
            max_original_bytes: limits.max_original_bytes,
            max_heap_bytes: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for FactBudget {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            facts_remaining: limits.steps_per_counter,
            original_bytes_remaining: limits.bytes_per_counter,
            max_fact_bytes: limits.max_original_bytes as u64,
        }
    }
}

impl FromLimits for CborDataLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_wire_bytes: limits.max_original_bytes,
            max_graph_bytes: limits.max_heap_bytes,
            max_depth: limits.max_depth,
        }
    }
}

impl FromLimits for AgentOrderDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_ids: limits.max_collection_entries,
            max_id_bytes: limits.max_text_bytes,
            comparison_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for IndustrialEligibilityReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_text_bytes: limits.max_text_bytes as u64,
        }
    }
}

impl FromLimits for WorldCollectionControlsWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            records_remaining: limits.steps_per_counter,
            index_steps_remaining: limits.steps_per_counter,
            max_record_bytes: limits.max_record_bytes as u64,
        }
    }
}

impl FromLimits for ArtifactConfigWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json: JsonWriteWorkV1::from_limits(limits, core),
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for RuntimeProposalReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json: JsonReadWorkV1::from_limits(limits, core),
            max_selected_heap: limits.max_heap_bytes,
            max_text_bytes: limits.max_text_bytes as u64,
        }
    }
}

impl FromLimits for RuntimeProposalWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            json: JsonWriteWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RuntimeGovernanceWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for RuntimeArtifactResourcesWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for ModuleMarketplaceFamilyWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            outer_rows_remaining: limits.steps_per_counter,
            changed_cells_remaining: limits.steps_per_counter,
            membership_cells_remaining: limits.steps_per_counter,
            max_record_bytes: limits.max_record_bytes as u64,
        }
    }
}

impl FromLimits for MetaProgressMutationWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_entries: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for FactoryLifecycleIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_factories: limits.max_collection_entries,
            max_recipe_jobs: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
            max_pending_records: limits.max_pending_records,
        }
    }
}

impl FromLimits for FactoryOutputLedgerGroupWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            pending_entries_remaining: limits.entries_per_counter,
            max_ledger_bytes: limits.max_original_bytes,
        }
    }
}

impl FromLimits for LogisticsWorkBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for MaterialProfileCurrentReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            scan: StateScanBudget::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}
