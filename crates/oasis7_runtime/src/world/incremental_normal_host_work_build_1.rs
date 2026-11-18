//! Explicit existing work-lane fields; no authority or business source is built.
use super::*;

impl FromLimits for NormalHostMaterialWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            read: NormalHostReadWorkV1::from_limits(limits, core),
            write_keys: SemanticKeyBudget::from_limits(limits, core),
            write_limits: MemberRecordLimitsV1::from_limits(limits, core),
            family: InventoryFamilyWorkV1::from_limits(limits, core),
            scope: InventoryMaterialScopeWorkV1::from_limits(limits, core),
            group: InventoryWorldGroupWorkV1::from_limits(limits, core),
            normalization_max_union_entries: limits.max_collection_entries,
            normalization_cells_remaining: limits.steps_per_counter,
            normalization_comparison_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for RootModuleExecutionWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            input: CanonicalCborWorkV1::from_limits(limits, core),
            routing: RouterWorkV1::from_limits(limits, core),
            max_input_bytes: limits.max_original_bytes,
            max_policy_selections: limits.max_collection_entries,
            max_prepared_effects: limits.max_collection_entries,
            lookup_steps_remaining: limits.steps_per_counter,
            transient_heap_bytes: limits.max_heap_bytes,
            max_state_bytes: limits.max_original_bytes,
            max_owner_bytes: limits.max_text_bytes,
            anchor_scan: StateScanBudget::from_limits(limits, core),
            anchor_cells_remaining: limits.steps_per_counter,
            visual: ModuleVisualOverlayWorkV1::from_limits(limits, core),
            visual_decode: RootModuleVisualDecodeWorkV1::from_limits(limits, core),
            effects: RootModuleEffectWorkV1::from_limits(limits, core),
            effect_records: RuntimeEffectRecordsWorkV1::from_limits(limits, core),
            authorization: ModuleAuthorizationReadWorkV1::from_limits(limits, core),
            output: RootModuleOutputWorkV1::from_limits(limits, core),
            selection_scan: StateScanBudget::from_limits(limits, core),
            selection: ArtifactConfigReadWorkV1::from_limits(limits, core),
            original: RuntimeArtifactOriginalWorkV1::from_limits(limits, core),
            selection_limits: ModuleInvocationPreparationLimitsV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RootWorldManifestHashWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            body_scan: StateScanBudget::from_limits(limits, core),
            fields: ArtifactConfigReadWorkV1::from_limits(limits, core),
            json: JsonReadWorkV1::from_limits(limits, core),
            hash_bytes_remaining: limits.bytes_per_counter,
            max_json_heap_bytes: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for CanonicalCborWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            visits_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            max_depth: limits.max_depth,
            max_map_keys: limits.max_collection_entries,
            max_key_bytes: limits.max_key_bytes,
        }
    }
}

impl FromLimits for NormalHostReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            io: ValueIoBudget::from_limits(limits, core),
            keys: SemanticKeyBudget::from_limits(limits, core),
            semantic_bytes: limits.max_key_bytes as u64,
            record_bytes: limits.max_record_bytes as u64,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl FromLimits for StateScanBudget {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            nodes_remaining: limits.steps_per_counter,
            read_bytes_remaining: limits.bytes_per_counter,
            leaves_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for NormalHostWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            io: ValueIoBudget::from_limits(limits, core),
            keys: SemanticKeyBudget::from_limits(limits, core),
            limits: MemberRecordLimitsV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for SemanticKeyBudget {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for MemberRecordLimitsV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            semantic_bytes: limits.max_key_bytes as u64,
            payload_bytes: limits.max_payload_bytes as u64,
            record_bytes: limits.max_record_bytes as u64,
        }
    }
}

impl FromLimits for InventoryFamilyWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            records_remaining: limits.steps_per_counter,
            index_steps_remaining: limits.steps_per_counter,
            max_semantic_bytes: limits.max_key_bytes,
        }
    }
}

impl FromLimits for InventoryMaterialScopeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            max_entries_per_map: limits.max_collection_entries,
            max_material_bytes: limits.max_text_bytes,
            pending_entries_remaining: limits.entries_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for InventoryWorldGroupWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for FactoryPhaseWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            orders: FactoryOrderDirectoryWorkV1::from_limits(limits, core),
            records: FactoryRecipeDirectoryWorkV1::from_limits(limits, core),
            material_family: InventoryFamilyWorkV1::from_limits(limits, core),
            material_scope: InventoryMaterialScopeWorkV1::from_limits(limits, core),
            factory: FactoryWorkBudgetV1::from_limits(limits, core),
            recipe: IndustryWorkBudgetV1::from_limits(limits, core),
            industrial: IndustrialEligibilityWorkV1::from_limits(limits, core),
            prepare: FactoryDepreciationWorkV1::from_limits(limits, core),
            reduce: FactoryDepreciationReduceWorkV1::from_limits(limits, core),
            group: InventoryWorldGroupWorkV1::from_limits(limits, core),
            enumeration: CurrentInvocationEnumerationWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for PendingActionFamilyWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            index: PendingActionIndexWorkV1::from_limits(limits, core),
            typed: TypedRecordReadWorkV1::from_limits(limits, core),
            original: PendingActionOriginalWorkV1::from_limits(limits, core),
            selected_scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for TypedRecordWriteWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            json: JsonWriteWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for TypedRecordReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            json: JsonReadWorkV1::from_limits(limits, core),
            max_retained_trees: limits.max_collection_entries,
            max_tree_heap: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for ProductValidationDeliveryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            index: RuntimeHistoryIndexWorkV1::from_limits(limits, core),
            decode: WorldEventDecodeWorkV1::from_limits(limits, core),
            originals_remaining: limits.steps_per_counter,
            original_bytes_remaining: limits.bytes_per_counter,
        }
    }
}

impl FromLimits for WorldEventDecodeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            structure: CborPreflightWorkV1::from_limits(limits, core),
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
            max_original_bytes: limits.max_original_bytes,
            max_heap_bytes: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for RootRulePhaseWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            enumeration: CurrentInvocationEnumerationWorkV1::from_limits(limits, core),
            payload: RuleDecisionPayloadDecodeWorkV1::from_limits(limits, core),
            canonical: CanonicalCborWorkV1::from_limits(limits, core),
            max_captured_outputs: limits.max_collection_entries,
            max_payload_bytes: limits.max_payload_bytes,
            merge_units_remaining: limits.steps_per_counter,
        }
    }
}

impl FromLimits for CurrentInvocationEnumerationWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            instance_scan: StateScanBudget::from_limits(limits, core),
            registry_scan: StateScanBudget::from_limits(limits, core),
            pending_entries: limits.entries_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            max_keys: limits.max_collection_entries,
            max_key_bytes: limits.max_key_bytes,
            max_text_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for CurrentWorldEventTailWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            originals_remaining: limits.steps_per_counter,
            original_bytes_remaining: limits.bytes_per_counter,
            decode: WorldEventDecodeWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for NormalHostFinishCutWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for RouterWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            nodes_remaining: limits.steps_per_counter,
            edges_remaining: limits.steps_per_counter,
            string_bytes_remaining: limits.bytes_per_counter,
            match_steps_remaining: limits.steps_per_counter,
            max_depth: limits.max_depth,
        }
    }
}

impl FromLimits for ModuleVisualOverlayWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            pending_entries_remaining: limits.entries_per_counter,
            body_scan: StateScanBudget::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            max_owner_bytes: limits.max_text_bytes,
        }
    }
}

impl FromLimits for RootModuleVisualDecodeWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            nodes_remaining: limits.steps_per_counter,
            text_bytes_remaining: limits.bytes_per_counter,
            max_depth: limits.max_depth,
            max_heap_bytes: limits.max_heap_bytes,
        }
    }
}
