//! Explicit caller supplied finite allowances for one complete Current pass.
//! Each fixed owner receives the named local allowance; these are not a global
//! work allowance. State/value IO, semantic key work and memory remain shared
//! by the caller context and are never reset here. No qualification is minted.
use crate::{
    incremental_agent_subtree::{AgentSubtreeScanWorkV1, AgentSubtreeTransitionWorkV1},
    incremental_cognition_current_complete_v3::CognitionCurrentCompleteWorkV3,
    incremental_factory_records as factory, incremental_industry_jobs as industry,
    incremental_inventory as inventory, incremental_logistics_records as logistics,
    incremental_module_instance_recovery::InstanceScratchBudget,
    incremental_module_marketplace as market,
    incremental_module_state_recovery::ModuleStateContentBudgetV1,
    incremental_owner_closure as primitive,
    incremental_pending_action_original_binding::PendingActionOriginalWorkV1,
    incremental_rollback_genesis::RollbackWorkV1,
    incremental_runtime_allocators_family::RuntimeAllocatorsWorkV1,
    incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
    incremental_runtime_governance::RuntimeGovernanceWorkV1,
    incremental_runtime_revocation::RevocationReadWorkV1 as TypedRecordReadWorkV1,
    incremental_runtime_typed_complete::RuntimeTypedCompleteWorkV1,
    incremental_world_collection_controls as controls,
    incremental_world_current_complete::WorldCurrentStateWorkV1,
    incremental_world_native as native,
    incremental_world_native_complete::WorldNativeCompleteWorkV1,
    incremental_world_raw_complete::*,
    incremental_world_rules_family as rules,
};
use oasis7_formal_protocol::{
    incremental_schedule::ScheduleScanBudget, incremental_state::StateScanBudget,
    incremental_value::ValueTreeError,
};
#[derive(Debug, Clone, Copy)]
pub struct WorldCurrentStateLimitsV1 {
    pub buckets_remaining: u64,
    pub bytes_remaining: u64,
    pub cells_remaining: u64,
    pub changed_cells_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub index_steps_remaining: u64,
    pub instance_relations_remaining: u64,
    pub instances_remaining: u64,
    pub json_nodes_remaining: u64,
    pub key_changes_remaining: u64,
    pub leaves_remaining: u64,
    pub legacy_cells_remaining: u64,
    pub logical_bytes_remaining: u64,
    pub mailbox_occurrences_remaining: u64,
    pub max_agents: usize,
    pub max_depth: usize,
    pub max_instance_scalar_bytes: usize,
    pub max_key_bytes: usize,
    pub max_key_heap: usize,
    pub max_map_keys: usize,
    pub max_object_bytes: usize,
    pub max_original_bytes: usize,
    pub max_payload_bytes: usize,
    pub max_pending_node_bytes: usize,
    pub max_pending_pairs: usize,
    pub max_pending_records: usize,
    pub max_record_bytes: usize,
    pub max_retained_trees: usize,
    pub max_scalar_bytes: usize,
    pub max_selected_heap: usize,
    pub max_semantic_bytes: usize,
    pub max_state_bytes: usize,
    pub max_text_bytes: usize,
    pub max_tree_heap: usize,
    pub membership_cells_remaining: u64,
    pub nodes_remaining: u64,
    pub objects_remaining: u64,
    pub original_bytes_remaining: u64,
    pub outer_cells_remaining: u64,
    pub outer_records_remaining: u64,
    pub outer_rows_remaining: u64,
    pub paths_remaining: u64,
    pub read_bytes: u64,
    pub read_bytes_remaining: u64,
    pub reads: u64,
    pub records_remaining: u64,
    pub states_remaining: u64,
    pub steps_remaining: u64,
    pub trees_remaining: u64,
    pub visits_remaining: u64,
    pub write_bytes: u64,
    pub writes: u64,
}
impl WorldCurrentStateLimitsV1 {
    pub fn validate(&self) -> Result<(), ValueTreeError> {
        u64::try_from(self.max_agents)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_depth)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_instance_scalar_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_key_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_key_heap)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_map_keys)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_object_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_original_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_payload_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_pending_node_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_pending_pairs)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_pending_records)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_record_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_retained_trees)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_scalar_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_selected_heap)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_semantic_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_state_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_text_bytes)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        u64::try_from(self.max_tree_heap)
            .map_err(|_| ValueTreeError::Invalid("Current limit conversion"))?;
        usize::try_from(self.trees_remaining)
            .map_err(|_| ValueTreeError::Invalid("Current tree count conversion"))?;
        if self.buckets_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.bytes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.cells_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.changed_cells_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.comparison_bytes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.index_steps_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.instance_relations_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.instances_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.json_nodes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.key_changes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.leaves_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.legacy_cells_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.logical_bytes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.mailbox_occurrences_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_agents == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_depth == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_instance_scalar_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_key_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_key_heap == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_map_keys == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_object_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_original_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_payload_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_pending_node_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_pending_pairs == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_pending_records == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_record_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_retained_trees == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_scalar_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_selected_heap == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_semantic_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_state_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_text_bytes == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.max_tree_heap == usize::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.membership_cells_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.nodes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.objects_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.original_bytes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.outer_cells_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.outer_records_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.outer_rows_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.paths_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.read_bytes == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.read_bytes_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.reads == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.records_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.states_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.steps_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.trees_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.visits_remaining == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.write_bytes == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        if self.writes == u64::MAX {
            return Err(ValueTreeError::Invalid("Current finite limit"));
        }
        self.max_tree_heap
            .checked_mul(self.max_retained_trees)
            .ok_or(ValueTreeError::Invalid(
                "Current retained tree limit overflow",
            ))?;
        self.max_pending_pairs
            .checked_mul(self.max_payload_bytes)
            .ok_or(ValueTreeError::Invalid(
                "Current pending pair limit overflow",
            ))?;
        Ok(())
    }
    fn scan(&self) -> StateScanBudget {
        StateScanBudget {
            nodes_remaining: self.nodes_remaining,
            read_bytes_remaining: self.read_bytes_remaining,
            leaves_remaining: self.leaves_remaining,
        }
    }
    fn json_work(&self) -> crate::incremental_json_records::JsonReadWorkV1 {
        crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: self.cells_remaining,
            json_nodes_remaining: self.json_nodes_remaining,
            max_depth: self.max_depth,
            max_payload_bytes: u64::try_from(self.max_payload_bytes)
                .expect("validated Current limit conversion"),
        }
    }
    fn typed(&self) -> TypedRecordReadWorkV1 {
        TypedRecordReadWorkV1 {
            max_retained_trees: self.max_retained_trees,
            max_tree_heap: self.max_tree_heap,
            json: self.json_work(),
        }
    }
    fn rules_work(&self) -> rules::WorldRulesFamilyWorkV1 {
        rules::WorldRulesFamilyWorkV1 {
            outer_records_remaining: self.outer_records_remaining,
            legacy_cells_remaining: self.legacy_cells_remaining,
            max_semantic_bytes: u64::try_from(self.max_semantic_bytes)
                .expect("validated Current limit conversion"),
            max_record_bytes: u64::try_from(self.max_record_bytes)
                .expect("validated Current limit conversion"),
            composite: rules::WorldRulesTypedReadWorkV1 {
                max_retained_trees: self.max_retained_trees,
                max_tree_heap: self.max_tree_heap,
                json: crate::incremental_json_records::JsonReadWorkV1 {
                    cells_remaining: self.cells_remaining,
                    json_nodes_remaining: self.json_nodes_remaining,
                    max_depth: self.max_depth,
                    max_payload_bytes: u64::try_from(self.max_payload_bytes)
                        .expect("validated Current limit conversion"),
                },
            },
            remainder: crate::incremental_world_rule_effects::WorldRuleRemainderWorkV1 {
                cells_remaining: self.cells_remaining,
                max_semantic_bytes: self.max_semantic_bytes,
                max_payload_bytes: u64::try_from(self.max_payload_bytes)
                    .expect("validated Current limit conversion"),
            },
        }
    }
    fn native_work(&self) -> WorldNativeCompleteWorkV1 {
        WorldNativeCompleteWorkV1 {
            legacy_cells_remaining: self.legacy_cells_remaining,
            max_payload_bytes: u64::try_from(self.max_payload_bytes)
                .expect("validated Current limit conversion"),
            primitive: primitive::WorldPrimitiveFamilyWorkV1 {
                records_remaining: self.records_remaining,
                max_record_bytes: u64::try_from(self.max_record_bytes)
                    .expect("validated Current limit conversion"),
            },
            native: native::WorldNativeQualificationWorkV1 {
                records_remaining: self.records_remaining,
                trees_remaining: usize::try_from(self.trees_remaining)
                    .expect("validated Current limit conversion"),
                max_key_heap: self.max_key_heap,
                max_pending_records: self.max_pending_records,
                typed: crate::incremental_runtime_revocation::RevocationReadWorkV1 {
                    json: self.json_work(),
                    max_retained_trees: self.max_retained_trees,
                    max_tree_heap: self.max_tree_heap,
                },
            },
            inventory: inventory::InventoryFamilyWorkV1 {
                records_remaining: self.records_remaining,
                index_steps_remaining: self.index_steps_remaining,
                max_semantic_bytes: self.max_semantic_bytes,
            },
            factory: factory::FactoryWorkBudgetV1 {
                cells_remaining: self.cells_remaining,
                max_pending_node_bytes: self.max_pending_node_bytes,
                scan: self.scan(),
            },
            industry: industry::IndustryWorkBudgetV1 {
                cells_remaining: self.cells_remaining,
                max_pending_node_bytes: self.max_pending_node_bytes,
                scan: self.scan(),
            },
            logistics: logistics::LogisticsWorkBudgetV1 {
                cells_remaining: self.cells_remaining,
                max_pending_node_bytes: self.max_pending_node_bytes,
                scan: self.scan(),
            },
            market: market::ModuleMarketplaceFamilyWorkV1 {
                outer_rows_remaining: self.outer_rows_remaining,
                changed_cells_remaining: self.changed_cells_remaining,
                membership_cells_remaining: self.membership_cells_remaining,
                max_record_bytes: u64::try_from(self.max_record_bytes)
                    .expect("validated Current limit conversion"),
            },
            rules: self.rules_work(),
            controls: controls::WorldCollectionControlsWorkV1 {
                records_remaining: self.records_remaining,
                index_steps_remaining: self.index_steps_remaining,
                max_record_bytes: u64::try_from(self.max_record_bytes)
                    .expect("validated Current limit conversion"),
            },
        }
    }
    fn read_work(&self) -> RuntimeTypedCompleteWorkV1 {
        use crate::{
            incremental_runtime_artifact_config::ArtifactConfigReadWorkV1,
            incremental_runtime_module_authorization::ModuleAuthorizationReadWorkV1,
            incremental_runtime_proposal::RuntimeProposalReadWorkV1,
        };
        RuntimeTypedCompleteWorkV1 {outer_scan:self.scan(),body_scan:self.scan(),resource_scans:std::array::from_fn(|_|self.scan()),cells_remaining: self.cells_remaining,max_text_bytes: u64::try_from(self.max_text_bytes).expect("validated Current limit conversion"),max_record_bytes: u64::try_from(self.max_record_bytes).expect("validated Current limit conversion"),max_selected_heap: self.max_selected_heap,
 config:ArtifactConfigReadWorkV1 {cells_remaining: self.cells_remaining,max_payload_bytes: u64::try_from(self.max_payload_bytes).expect("validated Current limit conversion"),json:self.json_work()},resources:RuntimeArtifactResourcesWorkV1 {cells_remaining: self.cells_remaining,max_pending_node_bytes: self.max_pending_node_bytes},content:RuntimeArtifactOriginalWorkV1 {objects_remaining: self.objects_remaining,bytes_remaining: self.bytes_remaining,max_object_bytes: u64::try_from(self.max_object_bytes).expect("validated Current limit conversion")},effects:self.json_work(),module_authorization:ModuleAuthorizationReadWorkV1 {cells_remaining: self.cells_remaining,comparison_bytes_remaining: self.comparison_bytes_remaining,max_payload_bytes: u64::try_from(self.max_payload_bytes).expect("validated Current limit conversion")},json:self.json_work(),revocation:self.typed(),proposals:RuntimeProposalReadWorkV1 {cells_remaining: self.cells_remaining,json:self.json_work(),max_selected_heap: self.max_selected_heap,max_text_bytes: u64::try_from(self.max_text_bytes).expect("validated Current limit conversion")},governance:RuntimeGovernanceWorkV1 {cells_remaining: self.cells_remaining,max_pending_node_bytes: self.max_pending_node_bytes},rollback:RollbackWorkV1 {cells_remaining: self.cells_remaining,max_pending_node_bytes: self.max_pending_node_bytes},pending:crate::incremental_pending_action_family::PendingActionFamilyWorkV1 {index:crate::incremental_pending_action_family::PendingActionIndexWorkV1 {steps_remaining: self.steps_remaining},typed:self.typed(),original:crate::incremental_pending_action_original_binding::PendingActionOriginalWorkV1 {objects_remaining: self.objects_remaining,original_bytes_remaining: self.original_bytes_remaining,max_original_bytes: u64::try_from(self.max_original_bytes).expect("validated Current limit conversion"),canonical:oasis7_wasm_abi::CanonicalCborWorkV1 {visits_remaining: self.visits_remaining,bytes_remaining: self.bytes_remaining,max_depth: self.max_depth,max_map_keys: self.max_map_keys,max_key_bytes: self.max_key_bytes}},selected_scan:self.scan()}}
    }
    fn raw_work(&self) -> WorldRawCompleteWorkV1 {
        WorldRawCompleteWorkV1 {
            state_scan: self.scan(),
            agents: AgentSubtreeScanWorkV1 {
                max_agents: self.max_agents,
                cells_remaining: self.cells_remaining,
                max_scalar_bytes: u64::try_from(self.max_scalar_bytes)
                    .expect("validated Current limit conversion"),
            },
            agent_changes: AgentSubtreeTransitionWorkV1 {
                cells_remaining: self.cells_remaining,
                max_scalar_bytes: u64::try_from(self.max_scalar_bytes)
                    .expect("validated Current limit conversion"),
            },
            module_states: ModuleStateContentBudgetV1 {
                states_remaining: self.states_remaining,
                logical_bytes_remaining: self.logical_bytes_remaining,
                max_state_bytes: u64::try_from(self.max_state_bytes)
                    .expect("validated Current limit conversion"),
            },
            instance_scratch: InstanceScratchBudget {
                reads: self.reads,
                read_bytes: self.read_bytes,
                writes: self.writes,
                write_bytes: self.write_bytes,
            },
            instance_relations_remaining: self.instance_relations_remaining,
            max_instance_scalar_bytes: u64::try_from(self.max_instance_scalar_bytes)
                .expect("validated Current limit conversion"),
            allocators: RuntimeAllocatorsWorkV1 {
                records_remaining: self.records_remaining,
            },
            schedule: WorldRawScheduleWorkV1 {
                cells_remaining: self.cells_remaining,
                index: ScheduleScanBudget {
                    nodes_remaining: self.nodes_remaining,
                    read_bytes_remaining: self.read_bytes_remaining,
                    buckets_remaining: self.buckets_remaining,
                    instances_remaining: self.instances_remaining,
                },
            },
            mailbox_occurrences_remaining: self.mailbox_occurrences_remaining,
            originals: PendingActionOriginalWorkV1 {
                objects_remaining: self.objects_remaining,
                original_bytes_remaining: self.original_bytes_remaining,
                max_original_bytes: u64::try_from(self.max_original_bytes)
                    .expect("validated Current limit conversion"),
                canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: self.visits_remaining,
                    bytes_remaining: self.bytes_remaining,
                    max_depth: self.max_depth,
                    max_map_keys: self.max_map_keys,
                    max_key_bytes: self.max_key_bytes,
                },
            },
        }
    }
    pub fn work(&self) -> Result<WorldCurrentStateWorkV1, ValueTreeError> {
        self.validate()?;
        Ok(WorldCurrentStateWorkV1 {
            key_scan: self.scan(),
            native_outer_scan: self.scan(),
            native_inner_scan: self.scan(),
            key_changes_remaining: self.key_changes_remaining,
            native: self.native_work(),
            runtime: self.read_work(),
            raw: self.raw_work(),
            cognition: CognitionCurrentCompleteWorkV3 {
                scan: self.scan(),
                json: crate::incremental_json_records::JsonChangeWorkV1 {
                    json: self.json_work(),
                    paths_remaining: self.paths_remaining,
                    max_pending_pairs: self.max_pending_pairs,
                },
                outer_cells_remaining: self.outer_cells_remaining,
            },
        })
    }
}
