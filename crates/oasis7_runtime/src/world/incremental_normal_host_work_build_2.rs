//! Explicit existing work-lane fields; no authority or business source is built.
use super::*;

impl FromLimits for RootModuleEffectWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            clone_nodes_remaining: limits.steps_per_counter,
            clone_bytes_remaining: limits.bytes_per_counter,
            max_json_depth: limits.max_depth,
            max_policy_rules: limits.max_collection_entries as u64,
            limits: RootModuleAuthorizationLimitsV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RuntimeEffectRecordsWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json_nodes_remaining: limits.steps_per_counter,
            max_json_depth: limits.max_depth,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for ModuleAuthorizationReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl FromLimits for RootModuleOutputWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            calls_remaining: limits.steps_per_counter,
            structure: CborPreflightWorkV1::from_limits(limits, core),
            max_identity_bytes: limits.max_text_bytes,
            max_original_output_bytes: limits.max_original_bytes,
            max_output_heap_bytes: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for ArtifactConfigReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_payload_bytes: limits.max_payload_bytes as u64,
            json: JsonReadWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RuntimeArtifactOriginalWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            objects_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_object_bytes: limits.max_original_bytes as u64,
        }
    }
}

impl FromLimits for ModuleInvocationPreparationLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            selection_heap_bytes: limits.max_heap_bytes,
            max_text_bytes: limits.max_text_bytes,
            manifest: SelectedModuleManifestLimitsV1::from_limits(limits, core),
            manifest_json_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for JsonReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json_nodes_remaining: limits.steps_per_counter,
            max_depth: limits.max_depth,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl FromLimits for ValueIoBudget {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            nodes_remaining: limits.steps_per_counter,
            read_bytes_remaining: limits.bytes_per_counter,
            write_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for CurrentBootstrapFinalityWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            index: RuntimeHistoryIndexWorkV1::from_limits(limits, core),
            decode: RuntimeHistoryDecodeWorkV1::from_limits(limits, core),
            originals: FactBudget::from_limits(limits, core),
            input_limits: CborDataLimitsV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for AuthorizationWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for CognitionGroupWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            json: JsonWriteWorkV1::from_limits(limits, core),
            records_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for FactoryOrderDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            ids: AgentOrderDirectoryWorkV1::from_limits(limits, core),
            pending_entries_remaining: limits.entries_per_counter,
            merge_comparison_bytes_remaining: limits.bytes_per_counter,
            max_index_text_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for FactoryRecipeDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_factories: limits.max_collection_entries,
            max_recipe_jobs: limits.max_collection_entries,
            max_factory_id_bytes: limits.max_text_bytes,
            max_outer_changes: limits.max_pending_records,
            comparisons_remaining: limits.steps_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for FactoryWorkBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for IndustryWorkBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for IndustrialEligibilityWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            read: IndustrialEligibilityReadWorkV1::from_limits(limits, core),
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
            revocation: TypedRecordReadWorkV1::from_limits(limits, core),
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for FactoryDepreciationWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_factories: limits.max_collection_entries,
            max_workload_entries: limits.max_collection_entries,
            max_text_bytes: limits.max_text_bytes,
            cells_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for FactoryDepreciationReduceWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_material_entries: limits.max_collection_entries,
            comparison_bytes_remaining: limits.bytes_per_counter,
            cells_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for PendingActionIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for PendingActionOriginalWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            objects_remaining: limits.steps_per_counter,
            original_bytes_remaining: limits.bytes_per_counter,
            max_original_bytes: limits.max_original_bytes as u64,
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for JsonWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json_nodes_remaining: limits.steps_per_counter,
            max_json_depth: limits.max_depth,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for NormalModuleLifecycleWorkSetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            controls: WorldCollectionControlsWorkV1::from_limits(limits, core),
            capability_authorization: AuthorizationRootSourceReadWorkV1::from_limits(limits, core),
            config_write: ArtifactConfigWriteWorkV1::from_limits(limits, core),
            proposal_read: RuntimeProposalReadWorkV1::from_limits(limits, core),
            proposal_write: RuntimeProposalWriteWorkV1::from_limits(limits, core),
            proposal_scan: StateScanBudget::from_limits(limits, core),
            governance: RuntimeGovernanceWorkV1::from_limits(limits, core),
            artifacts: RuntimeArtifactResourcesWorkV1::from_limits(limits, core),
            content: RuntimeArtifactOriginalWorkV1::from_limits(limits, core),
            marketplace: ModuleMarketplaceFamilyWorkV1::from_limits(limits, core),
            marketplace_scan: StateScanBudget::from_limits(limits, core),
            instance_scan: StateScanBudget::from_limits(limits, core),
            instance_cells: limits.steps_per_counter,
            pending_entries: limits.entries_per_counter,
            max_selected_heap: limits.max_heap_bytes,
            max_original_bytes: limits.max_original_bytes,
        }
    }
}

impl FromLimits for CrisisWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            crisis_scan: StateScanBudget::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            max_text_bytes: limits.max_text_bytes as u64,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for ContractAcceptWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            contract_scan: StateScanBudget::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            max_text_bytes: limits.max_text_bytes as u64,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}
