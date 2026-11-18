//! Explicit existing work-lane fields; no authority or business source is built.
use super::*;

impl FromLimits for MetaProgressWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            native_scan: StateScanBudget::from_limits(limits, core),
            native_read: TypedRecordReadWorkV1::from_limits(limits, core),
            native_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            records_remaining: limits.steps_per_counter,
            mutation: MetaProgressMutationWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for FactoryLifecycleWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_scalar_bytes: limits.max_scalar_bytes,
            max_material_entries: limits.max_collection_entries,
            factory: FactoryWorkBudgetV1::from_limits(limits, core),
            industry: IndustryWorkBudgetV1::from_limits(limits, core),
            lifecycle: FactoryLifecycleIndexWorkV1::from_limits(limits, core),
            native_scan: StateScanBudget::from_limits(limits, core),
            native_read: TypedRecordReadWorkV1::from_limits(limits, core),
            native_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            records_remaining: limits.steps_per_counter,
            output_group: FactoryOutputLedgerGroupWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for RouteAvailabilityWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_scalar_bytes: limits.max_scalar_bytes as u64,
            logistics: LogisticsWorkBudgetV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for MaterialTransferWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            profile: MaterialProfileCurrentReadWorkV1::from_limits(limits, core),
            inventory: AgentLedgerTransferWorkV1::from_limits(limits, core),
            receipt: MaterialWorkBudgetV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for BodyActionWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
        }
    }
}

impl FromLimits for DataAccessWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            controls: WorldCollectionControlsWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for CollectDataWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_scalar_bytes: limits.max_scalar_bytes as u64,
        }
    }
}

impl FromLimits for CollectDataAuthorizationWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            signature: CollectDataSignatureWorkV1::from_limits(limits, core),
            claims: StarterBindingIndexWorkV1::from_limits(limits, core),
            native: WorldNativeFieldReadWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for TransferResourceWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            steps_remaining: limits.steps_per_counter,
            bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_scalar_bytes: limits.max_scalar_bytes as u64,
        }
    }
}

impl FromLimits for FactoryScheduleWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            pending_entries_remaining: limits.entries_per_counter,
            lifecycle: FactoryLifecycleWorkV1::from_limits(limits, core),
            builds: FactoryScheduleIndexWorkV1::from_limits(limits, core),
            receipts: FactoryScheduleReceiptWorkV1::from_limits(limits, core),
            authority: FactoryScheduleAuthorityWorkV1::from_limits(limits, core),
            factory: FactoryScheduleFactoryWorkV1::from_limits(limits, core),
            authority_scan: StateScanBudget::from_limits(limits, core),
            authority_cells: limits.steps_per_counter,
            max_source_records: limits.max_collection_entries,
            max_source_encoding_bytes: limits.max_original_bytes,
            max_module_text_bytes: limits.max_text_bytes,
            logistics: FactoryScheduleLogisticsWorkV1::from_limits(limits, core),
            logistics_write: LogisticsWorkBudgetV1::from_limits(limits, core),
            material_profiles: MaterialProfileCurrentReadWorkV1::from_limits(limits, core),
            invocation: CurrentInvocationEnumerationWorkV1::from_limits(limits, core),
            config: ArtifactConfigReadWorkV1::from_limits(limits, core),
            manifest: SelectedModuleManifestLimitsV1::from_limits(limits, core),
            config_scan: StateScanBudget::from_limits(limits, core),
            inventory_directory: InventoryIdentityDirectoryWorkV1::from_limits(limits, core),
            inventory_family: InventoryFamilyWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for ContractWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            shared: AllianceWorkV1::from_limits(limits, core),
            scalar: ContractAcceptWorkV1::from_limits(limits, core),
            policy_scan: StateScanBudget::from_limits(limits, core),
            policy: TypedRecordReadWorkV1::from_limits(limits, core),
            binding: IndustrialEligibilityWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for AllianceWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            orders: FactoryOrderDirectoryWorkV1::from_limits(limits, core),
            pending_entries_remaining: limits.entries_per_counter,
            index: DiplomacyIndexWorkV1::from_limits(limits, core),
            scope_scan: StateScanBudget::from_limits(limits, core),
            scope_cells: limits.steps_per_counter,
            max_text_bytes: limits.max_text_bytes,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            write_cells: limits.steps_per_counter,
            continuity_read: TypedRecordReadWorkV1::from_limits(limits, core),
            continuity_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            controller: IndustrialEligibilityWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for LogisticsRegistrationWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            pending_entries_remaining: limits.entries_per_counter,
            logistics: LogisticsWorkBudgetV1::from_limits(limits, core),
            max_id_bytes: limits.max_text_bytes,
            max_scalar_bytes: limits.max_scalar_bytes as u64,
        }
    }
}

impl FromLimits for ProfileGovernanceWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            shared: FactoryLifecycleWorkV1::from_limits(limits, core),
            proposal: RuntimeProposalReadWorkV1::from_limits(limits, core),
            material: MaterialWorkBudgetV1::from_limits(limits, core),
            material_scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for NormalCoreExtensionWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self::new(core, limits.entries_per_counter, limits.max_text_bytes)
    }
}

impl FromLimits for IndustrialDeliveryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            eligibility: IndustrialEligibilityWorkV1::from_limits(limits, core),
            claims: IndustrialClaimDirectoryWorkV1::from_limits(limits, core),
            fixed_scan: StateScanBudget::from_limits(limits, core),
            fixed_cells_remaining: limits.steps_per_counter,
            native_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            records_remaining: limits.steps_per_counter,
            composite_read: TypedRecordReadWorkV1::from_limits(limits, core),
            composite_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            max_identity_bytes: limits.max_text_bytes,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for WorldRulesFamilyWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            outer_records_remaining: limits.steps_per_counter,
            legacy_cells_remaining: limits.steps_per_counter,
            max_semantic_bytes: limits.max_key_bytes as u64,
            max_record_bytes: limits.max_record_bytes as u64,
            composite: TypedRecordReadWorkV1::from_limits(limits, core),
            remainder: WorldRuleRemainderWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for NormalGameplayCycleWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            directory: GameplayDirectoryWorkV1::from_limits(limits, core),
            diplomacy_index: DiplomacyIndexWorkV1::from_limits(limits, core),
            selected_scan: StateScanBudget::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            native: TypedRecordReadWorkV1::from_limits(limits, core),
            native_scan: StateScanBudget::from_limits(limits, core),
            native_write: TypedRecordWriteWorkV1::from_limits(limits, core),
            meta: MetaProgressMutationWorkV1::from_limits(limits, core),
            due_scan: ScheduleScanBudget::from_limits(limits, core),
            due_limits: DueModuleBatchLimitsV1::from_limits(limits, core),
            max_text_bytes: limits.max_text_bytes,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            max_directive_depth: limits.max_depth,
            journal: ProductValidationDeliveryWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for NormalOrganizationTailWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            diplomacy_index: DiplomacyIndexWorkV1::from_limits(limits, core),
            cells_remaining: limits.steps_per_counter,
            max_text_bytes: limits.max_text_bytes,
            orders: FactoryOrderDirectoryWorkV1::from_limits(limits, core),
            scan: StateScanBudget::from_limits(limits, core),
            read: TypedRecordReadWorkV1::from_limits(limits, core),
            write: TypedRecordWriteWorkV1::from_limits(limits, core),
            eligibility: IndustrialEligibilityWorkV1::from_limits(limits, core),
            pending_entries_remaining: limits.entries_per_counter,
            journal: ProductValidationDeliveryWorkV1::from_limits(limits, core),
        }
    }
}

impl FromLimits for ThreatKeyDirectoryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            visits_remaining: limits.steps_per_counter,
            comparison_bytes_remaining: limits.bytes_per_counter,
            pending_entries_remaining: limits.entries_per_counter,
            max_rows: limits.max_collection_entries,
            max_key_bytes: limits.max_key_bytes,
        }
    }
}

impl FromLimits for RuntimeHistoryIndexWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
        }
    }
}

impl FromLimits for CurrentCognitionExpiryWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            read: JsonReadWorkV1::from_limits(limits, core),
            write: JsonWriteWorkV1::from_limits(limits, core),
            index: CognitionHistoryIndexWorkV1::from_limits(limits, core),
            cbor: CognitionHistoryCborWorkV1::from_limits(limits, core),
            hash: CognitionHeadHashWorkV1::from_limits(limits, core),
            original: CanonicalCborWorkV1::from_limits(limits, core),
            hashes: CanonicalCborWorkV1::from_limits(limits, core),
            ledger: TypedRecordReadWorkV1::from_limits(limits, core),
            ledger_scan: StateScanBudget::from_limits(limits, core),
            max_selected_heap: limits.max_heap_bytes,
        }
    }
}

impl FromLimits for AgentAuthorityWorkBudgetV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            max_pending_node_bytes: limits.max_pending_node_bytes,
            scan: StateScanBudget::from_limits(limits, core),
        }
    }
}

impl FromLimits for JsonScalarPathReadWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            cells_remaining: limits.steps_per_counter,
            json_nodes_remaining: limits.steps_per_counter,
            max_depth: limits.max_depth,
        }
    }
}

impl FromLimits for CborPreflightWorkV1 {
    fn from_limits(limits: &NormalHostLaneLimitsV1, _core: &FormalBoundedWorkV1) -> Self {
        Self {
            bytes_remaining: limits.bytes_per_counter,
            values_remaining: limits.steps_per_counter,
            max_depth: limits.max_depth,
        }
    }
}
