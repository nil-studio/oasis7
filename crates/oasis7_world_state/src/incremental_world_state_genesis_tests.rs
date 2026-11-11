use super::*;
use crate::incremental_typed_record_test_support::{Pages, io};
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_proto::memory_budget::MemoryBudget;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> WorldStateGenesisWorkV1 {
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
#[test]
fn native_world_genesis_covers_every_registered_state_field_and_delegates_four() {
    for rules in [None, Some(crate::WorldRulesStateV1::default())] {
        let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
        let _source = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let mut source = WorldState::default();
        source.world_rules = rules;
        source
            .module_states
            .insert("delegated-module".into(), vec![1, 2, 3]);
        let mut pages = Pages::with_limit(&memory, 65536);
        let mut quota = io();
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut context = MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut quota,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        };
        let mut writes = 0u64;
        let mut metadata = 0u64;
        let mut rules_metadata = 0u64;
        let mut work = work();
        let before_records = work.records_remaining;
        let before_write_bytes = context.io.write_bytes_remaining;
        let summary =
            prepare_world_state_genesis_v1(&source, &mut context, &mut work, &mut |row| {
                assert!(row.uses_budget(&memory));
                let field = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
                assert!(
                    ![
                        "module_states",
                        "agents",
                        "module_instances",
                        "retained_history_root"
                    ]
                    .contains(&field.name)
                );
                if field.kind == WorldFieldRecordKindV1::Metadata {
                    metadata += 1;
                    if field.name == "world_rules" {
                        rules_metadata += 1;
                    }
                }
                writes += 1;
                drop(row);
                Ok(())
            })
            .unwrap();
        assert_eq!(summary.native_fields, 98);
        assert_eq!(summary.delegated_fields, 4);
        assert_eq!(summary.writes, writes);
        assert_eq!(metadata, 98);
        assert_eq!(rules_metadata, 1);
        assert!(work.records_remaining < before_records);
        assert!(context.io.write_bytes_remaining < before_write_bytes);
    }
}
#[test]
fn native_world_genesis_sink_failure_stops_and_keeps_consumed_work() {
    let memory = MemoryBudget::new(64 << 20, 2 << 20).unwrap();
    let _source = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let source = WorldState::default();
    let mut pages = Pages::with_limit(&memory, 16384);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut work = work();
    let initial = work.records_remaining;
    let mut calls = 0u64;
    let result = prepare_world_state_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut quota,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut work,
        &mut |row| {
            calls += 1;
            drop(row);
            Err(ValueTreeError::ResourceLimited("test GEN sink"))
        },
    );
    assert!(matches!(
        result,
        Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
            "test GEN sink"
        )))
    ));
    assert_eq!(calls, 1);
    assert_eq!(work.records_remaining, initial - 1);
}
