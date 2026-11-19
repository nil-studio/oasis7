//! Explicit bounded smoke-fixture allowances; no qualification factory.
use super::*;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_runtime::incremental_genesis_raw::RuntimeRawGenesisWorkV1;
use oasis7_runtime::incremental_genesis_typed::*;
use oasis7_world_state::incremental_module_state_recovery::ModuleStateContentBudgetV1;
use oasis7_world_state::incremental_world_state_genesis::*;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    }
}
fn json() -> JsonWriteWorkV1 {
    JsonWriteWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 100_000,
        max_json_depth: 64,
        max_pending_node_bytes: 8 << 20,
    }
}
fn world_work() -> WorldStateGenesisWorkV1 {
    WorldStateGenesisWorkV1 {
        records_remaining: 1000000,
        factory: FactoryWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        factory_authority: FactoryAuthorityWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        material: MaterialWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        logistics: LogisticsWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        industry: IndustryWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        agent_authority: AgentAuthorityWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        governance: GovernanceWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        fixed_economy: FixedEconomyWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        main_token_policy: MainTokenPolicyWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        rewards: RewardRecordsWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
        },
        rule_composites: WorldRuleCompositeWriteWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        },
        native: TypedRecordWriteWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        },
        rules_max_pending_node_bytes: 8 << 20,
        diplomacy_max_pending_node_bytes: 8 << 20,
    }
}
fn typed_work() -> RuntimeTypedGenesisWorkV1 {
    RuntimeTypedGenesisWorkV1 {
        output_rows_remaining: 1_000_000,
        records_remaining: 1_000_000,
        world: world_work(),
        pending_actions: oasis7_world_state::incremental_runtime_pending_action_record::PendingActionWriteWorkV1 { cells_remaining: 1_000_000, max_pending_node_bytes: 8 << 20, json: json() },
        artifact_config: ArtifactConfigWriteWorkV1 {
            cells_remaining: 1_000_000,
            json: json(),
            max_pending_node_bytes: 8 << 20,
        },
        artifact_resources: RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        artifact_originals: RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 1000,
            bytes_remaining: 64 << 20,
            max_object_bytes: 16 << 20,
        },
        effects: RuntimeEffectRecordsWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        },
        capabilities: RuntimeCapabilityRecordsWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        module_authorization: ModuleAuthorizationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        authorization: AuthorizationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        json: json(),
        proposals: RuntimeProposalWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: json(),
        },
        revocation: RevocationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: json(),
        },
        governance: RuntimeGovernanceWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        rollback: RollbackWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        scheduler_controls: RuntimeSchedulerControlsWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
pub(super) fn work() -> RuntimeGenesisWorkV1 {
    RuntimeGenesisWorkV1 {
        typed: typed_work(),
        raw: RuntimeRawGenesisWorkV1 {
            writes_remaining: 100000,
            agents_remaining: 10000,
            instances_remaining: 10000,
            instance_count_rows_remaining: 10000,
            schedule_entries_remaining: 10000,
            module_states: ModuleStateContentBudgetV1 {
                states_remaining: 10000,
                logical_bytes_remaining: 64 << 20,
                max_state_bytes: 16 << 20,
            },
            max_scalar_bytes: 2 << 20,
        },
    }
}
