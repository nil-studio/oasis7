use super::*;
pub(super) use crate::incremental_typed_record_test_support::{ClassedPages, Pages, Writer, io};
use crate::{
    WorldState, incremental_field_member::*, incremental_semantic_key::SemanticKeyBudget,
    incremental_world_state_genesis::*,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateDeltaV1, StateRootV1, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueNodeWriter},
};
pub(super) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
pub(super) fn genesis_work() -> WorldStateGenesisWorkV1 {
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
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 100000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        },
        native: TypedRecordWriteWorkV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 8 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
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
fn rules_work() -> rules::WorldRulesFamilyWorkV1 {
    rules::WorldRulesFamilyWorkV1 {
        outer_records_remaining: 1_000_000,
        legacy_cells_remaining: 1_000_000,
        max_semantic_bytes: 1 << 20,
        max_record_bytes: 8 << 20,
        composite: rules::WorldRulesTypedReadWorkV1 {
            max_retained_trees: 100000,
            max_tree_heap: 1 << 20,
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_depth: 64,
                max_payload_bytes: 4 << 20,
            },
        },
        remainder: crate::incremental_world_rule_effects::WorldRuleRemainderWorkV1 {
            cells_remaining: 1_000_000,
            max_semantic_bytes: 1 << 20,
            max_payload_bytes: 4 << 20,
        },
    }
}
pub(super) fn work() -> WorldNativeCompleteWorkV1 {
    WorldNativeCompleteWorkV1 {
        legacy_cells_remaining: 1_000_000,
        max_payload_bytes: 8 << 20,
        primitive: primitive::WorldPrimitiveFamilyWorkV1 {
            records_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
        native: native::WorldNativeQualificationWorkV1 {
            records_remaining: 1_000_000,
            trees_remaining: 1_000_000,
            max_key_heap: 1 << 20,
            max_pending_records: 8,
            typed: crate::incremental_typed_record_codec::TypedRecordReadWorkV1 {
                json: json_work(),
                max_retained_trees: 100_000,
                max_tree_heap: 1 << 20,
            },
        },
        inventory: inventory::InventoryFamilyWorkV1 {
            records_remaining: 1_000_000,
            index_steps_remaining: 1_000_000,
            max_semantic_bytes: 1 << 20,
        },
        factory: factory::FactoryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        industry: industry::IndustryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        logistics: logistics::LogisticsWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            scan: scan(),
        },
        market: market::ModuleMarketplaceFamilyWorkV1 {
            outer_rows_remaining: 1_000_000,
            changed_cells_remaining: 1_000_000,
            membership_cells_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
        rules: rules_work(),
        controls: controls::WorldCollectionControlsWorkV1 {
            records_remaining: 1_000_000,
            index_steps_remaining: 1_000_000,
            max_record_bytes: 8 << 20,
        },
    }
}
fn json_work() -> crate::incremental_json_records::JsonReadWorkV1 {
    crate::incremental_json_records::JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}
pub(super) fn context<'a, S: MerkleNodeReader + ValueNodeReader>(
    memory: &'a MemoryBudget,
    pages: &'a S,
    root: &'a StateRootV1,
    quota: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
    class: MemoryClass,
) -> WorldFieldReadContextV1<'a, S, S, impl FnMut() -> Option<ValueTreeError>, impl FnMut() -> bool>
{
    WorldFieldReadContextV1 {
        root,
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 8 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
pub(super) fn rows(
    memory: &MemoryBudget,
    pages: &Pages,
    source: &WorldState,
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut writer = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 128 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 8 << 20,
            record_bytes: 16 << 20,
        },
        cancelled: || false,
    };
    let mut rows = Vec::with_capacity(1024);
    let summary =
        prepare_world_state_genesis_v1(source, &mut ctx, &mut genesis_work(), &mut |row| {
            assert!(rows.len() < 1024);
            rows.push(row);
            Ok(())
        })
        .unwrap();
    assert_eq!(summary.native_fields, 97);
    assert_eq!(summary.delegated_fields, 4);
    rows
}
pub(super) fn install(
    memory: &MemoryBudget,
    pages: &Pages,
    rows: &[ReservedValue<ExecutionWriteV1>],
) -> RootSetV1 {
    let mut roots = RootSetV1::empty("native-complete-97".into(), [71; 32]).unwrap();
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Artifacts,
    ] {
        let count = rows.iter().filter(|row| row.namespace == ns).count();
        let _phase = memory
            .reserve((count + 1) * (512 << 10), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|row| row.namespace == ns)
            .map(|row| row.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta =
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, pages, 8 << 20).unwrap();
        persist(memory, pages, &delta.new_nodes);
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    }
    roots
}
pub(super) fn persist(
    memory: &MemoryBudget,
    pages: &Pages,
    nodes: &[oasis7_formal_protocol::incremental_state::StateNodeWriteV1],
) {
    for node in nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
