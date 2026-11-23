use super::*;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_runtime::{
    incremental_field_read::WorldFieldReadContextV1, incremental_world_rule_supply_root::*,
};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
pub(super) fn outcome(source: SupplyRootSourceV1<'_>, populate: bool) -> (bool, bool, bool) {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let seed = WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    };
    let root = build(&store, &memory, &produce(&store, &memory, &seed));
    let parent = parent(&root);
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut fixed_io = logical();
    let mut fixed_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut fixed = WorldFieldReadContextV1 {
        root: &root,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fixed_io,
        },
        key_work: &mut fixed_keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_supply_family_v1(
        &parent.roots,
        &mut fixed,
        &mut scan(),
        &mut scan(),
        &mut 4096,
        165,
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent, [6; 32], &state, &memory, txn_limits()).unwrap();
    let mut writer = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        limits: MemberRecordLimitsV1 {
            payload_bytes: 165,
            ..limits()
        },
        cancelled: || false,
    };
    let guard = read.transaction.guard();
    change_world_rule_effect_overlay_v1(
        WorldRuleEffectChangeV1 {
            key: "p",
            expected: WorldRuleEffectExpectedV1::Value("original"),
            after: Some("prefix"),
            max_current_payload_bytes: 128,
        },
        &mut read,
        &mut write,
        &guard,
        &live,
    )
    .unwrap();
    let owner = match source {
        SupplyRootSourceV1::Authorization(_) => SupplyRootKeyV1::Authorization("new"),
        SupplyRootSourceV1::Batch(_) => SupplyRootKeyV1::Batch("new"),
    };
    let guard = read.transaction.guard();
    let result = create_qualified_supply_empty_v1(
        SupplyEmptyRootCreateV1 {
            owner,
            source,
            parent: &family,
            max_pending_node_bytes: 4 << 20,
        },
        &mut read,
        &mut write,
        &guard,
        &live,
        &mut 128,
    );
    let rejected = result.is_err();
    if let Ok(mut proof) = result
        && populate
    {
        let members = match source {
            SupplyRootSourceV1::Authorization(_) => vec![
                (
                    SupplyMemberKeyV1::Beneficiary("member"),
                    SupplyMemberValueV1::Unit,
                ),
                (
                    SupplyMemberKeyV1::Request("request"),
                    SupplyMemberValueV1::I64(-7),
                ),
            ],
            SupplyRootSourceV1::Batch(_) => vec![
                (
                    SupplyMemberKeyV1::Demand("member"),
                    SupplyMemberValueV1::I64(3),
                ),
                (
                    SupplyMemberKeyV1::Allocation("request"),
                    SupplyMemberValueV1::I64(-7),
                ),
                (
                    SupplyMemberKeyV1::FrozenLineage("frozen"),
                    SupplyMemberValueV1::Lineage {
                        target: 0,
                        fulfilled: 0,
                        reserved: 0,
                    },
                ),
            ],
        };
        for (member, value) in &members {
            let guard = read.transaction.guard();
            proof = change_qualified_supply_member_v1(
                SupplyMemberChangeV1 {
                    request: SupplyMemberRequestV1 {
                        owner,
                        member: *member,
                        qualification: &proof,
                    },
                    expected: None,
                    after: Some(*value),
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        }
        match source {
            SupplyRootSourceV1::Authorization(source) => {
                let mut expected = source.clone();
                expected.beneficiaries.insert("member".into());
                expected.requests.insert("request".into(), -7);
                let mut after = expected.clone();
                after.active = true;
                let guard = read.transaction.guard();
                proof = change_qualified_supply_scalars_v1(
                    SupplyScalarChangeV1 {
                        owner,
                        qualification: &proof,
                        expected: SupplyRootSourceV1::Authorization(&expected),
                        after: SupplyRootSourceV1::Authorization(&after),
                        max_current_payload_bytes: 128,
                        max_pending_node_bytes: 4 << 20,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap()
                .unwrap();
            }
            SupplyRootSourceV1::Batch(source) => {
                let mut expected = source.clone();
                expected.demands.insert("member".into(), 3);
                expected.allocation.insert("request".into(), -7);
                expected.frozen_lineages.insert(
                    "frozen".into(),
                    oasis7_runtime::world_rules::SupplyLineageV1::default(),
                );
                let mut after = expected.clone();
                after.allocated = true;
                let guard = read.transaction.guard();
                proof = change_qualified_supply_scalars_v1(
                    SupplyScalarChangeV1 {
                        owner,
                        qualification: &proof,
                        expected: SupplyRootSourceV1::Batch(&expected),
                        after: SupplyRootSourceV1::Batch(&after),
                        max_current_payload_bytes: 128,
                        max_pending_node_bytes: 4 << 20,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap()
                .unwrap();
            }
        }
        for (member, value) in members {
            let mut point = WorldFieldReadContextV1 {
                root: proof.root(),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            assert_eq!(
                read_qualified_supply_member_v1(
                    SupplyMemberRequestV1 {
                        owner,
                        member,
                        qualification: &proof
                    },
                    &mut point
                )
                .unwrap(),
                Some(value)
            );
        }
    }
    let guard = tx.guard();
    let sealable = tx.seal(&guard, &live, 1).is_ok();
    (rejected, false, sealable)
}
