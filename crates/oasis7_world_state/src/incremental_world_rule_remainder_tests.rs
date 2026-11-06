use super::*;
use crate::{
    WorldState,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_world_rule_effects::prepare_world_rule_effects_genesis_v1,
    incremental_world_rule_lineages::prepare_world_rule_lineages_genesis_members_v1,
    incremental_world_rule_shipments::prepare_world_rule_shipments_genesis_members_v1,
    world_rules::{SupplyLineageV1, SupplyShipmentV1, WorldRulesStateV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
fn work() -> WorldRuleRemainderWorkV1 {
    WorldRuleRemainderWorkV1 {
        cells_remaining: 100000,
        max_semantic_bytes: 4096,
        max_payload_bytes: 2 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 128 << 20,
        leaves_remaining: 100000,
    }
}
fn source() -> WorldState {
    let mut source = WorldState::default();
    let mut rules = WorldRulesStateV1::default();
    rules
        .governance_effects
        .insert(" original/effect ".into(), "Original/Ω/result".into());
    rules.supply_lineages.insert(
        " original/lineage ".into(),
        SupplyLineageV1 {
            target: i64::MIN,
            fulfilled: 0,
            reserved: i64::MAX,
        },
    );
    rules.supply_shipments.insert(
        u64::MAX,
        SupplyShipmentV1 {
            authorization_id: "α".repeat(128 << 10),
            batch_id: "original-batch-id".into(),
            beneficiary_id: "original-beneficiary".into(),
            lineage_id: "Different-Inner-Lineage-ID".into(),
            input_amount: i64::MIN,
            delivered_amount: i64::MAX,
            settled: true,
        },
    );
    source.world_rules = Some(rules);
    source
}
fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn persist(
    memory: &MemoryBudget,
    pages: &Pages,
    nodes: &[oasis7_formal_protocol::incremental_state::StateNodeWriteV1],
) {
    for node in nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn install(memory: &MemoryBudget, pages: &Pages, source: &WorldState) -> RootSetV1 {
    let mut rows = Vec::new();
    let mut writer = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut records = 100000;
    prepare_world_rule_effects_genesis_v1(source, &mut ctx, &mut records, &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_world_rule_lineages_genesis_members_v1(source, &mut ctx, &mut records, &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_world_rule_shipments_genesis_members_v1(source, &mut ctx, &mut records, &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("rule-remainder".into(), [7; 32]).unwrap();
    let delta = StateDeltaV1::build(
        roots.state_root(StateNamespaceV1::Authorization).clone(),
        writes,
        pages,
        8 << 20,
    )
    .unwrap();
    persist(memory, pages, &delta.new_nodes);
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after;
    roots
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 32,
        max_writes: 32,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
#[test]
fn rule_remainder_full_cold_qualifies_original_three_flat_families_and_optional_parent() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let mut empty = WorldState::default();
    empty.world_rules = Some(WorldRulesStateV1::default());
    for source in [WorldState::default(), empty, source()] {
        let pages = Pages::with_limit(&memory, 32768);
        let roots = install(&memory, &pages, &source);
        let proof = qualify_world_rule_remainder_v1(
            &roots,
            &mut context(
                &memory,
                &pages,
                &roots,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                },
            ),
            &mut scan(),
            &mut work(),
        )
        .unwrap();
        assert_eq!(proof.rules_present(), source.world_rules.is_some());
        assert_eq!(
            proof.record_counts(),
            if source
                .world_rules
                .as_ref()
                .is_some_and(|r| !r.governance_effects.is_empty())
            {
                [1, 1, 1]
            } else {
                [0; 3]
            }
        );
    }
}
#[test]
fn rule_remainder_general_receiver_rejects_orphan_lineage_and_inherits_large_shipment_text() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let roots = install(&memory, &pages, &source());
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let proof = qualify_world_rule_remainder_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    for complete in [false, true] {
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits())
                .unwrap();
        let mut ctx = context(&memory, &pages, &roots, &mut quota, &mut keys);
        for column in if complete {
            vec![None, Some("target"), Some("fulfilled"), Some("reserved")]
        } else {
            vec![None]
        } {
            let (k, _) = SupplyRecordKeyV1::Lineage(" original/lineage ")
                .fixed_key(column, &mut ctx)
                .unwrap();
            tx.write(StateNamespaceV1::Authorization, k.as_bytes(), None)
                .unwrap();
        }
        drop(ctx);
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        persist(&memory, &pages, &prepared.new_nodes);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [3; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let result = accept_world_rule_remainder_transition_v1(
            &proof,
            &verifier,
            &prepared.record,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys,
            ),
            &mut work(),
        );
        assert_eq!(result.is_ok(), complete);
        let cold = qualify_world_rule_remainder_v1(
            &prepared.record.after,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys,
            ),
            &mut scan(),
            &mut work(),
        );
        assert_eq!(cold.is_ok(), complete);
        if let Ok(after) = result {
            assert_eq!(after.record_counts(), [1, 0, 1]);
        }
    }
    // Removing this unchanged value page demonstrates logical parent inheritance,
    // not physical availability or WorldReady. Only fixed presence paths are read.
    let mut ctx = context(&memory, &pages, &roots, &mut quota, &mut keys);
    let (text, _) = SupplyRecordKeyV1::Shipment(u64::MAX)
        .fixed_key(Some("authorization_id"), &mut ctx)
        .unwrap();
    let value = ctx.lookup(&text).unwrap().unwrap();
    drop(ctx);
    pages.remove(&value);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let mut writer = Writer(&pages);
    let row = SupplyRecordKeyV1::Shipment(u64::MAX)
        .write(
            Some("input_amount"),
            MemberPayloadRefV1::I64(42),
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut quota,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap();
    tx.write(
        StateNamespaceV1::Authorization,
        row.field_key.as_bytes(),
        Some(row.value_root),
    )
    .unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &prepared.new_nodes);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let mut finite = work();
    finite.max_payload_bytes = 1;
    accept_world_rule_remainder_transition_v1(
        &proof,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut finite,
    )
    .unwrap();
}

use std::sync::Arc;
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}

#[test]
fn rule_remainder_recovery_requires_explicit_class_readmission() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let roots = install(&memory, &pages, &source());
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let proof = qualify_world_rule_remainder_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    assert!(
        accept_world_rule_remainder_transition_v1(
            &proof,
            &verifier,
            &prepared.record,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys
            ),
            &mut work()
        )
        .is_err()
    );
    let ordinary =
        readmit_world_rule_remainder_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.record_counts(), [1, 1, 1]);
    accept_world_rule_remainder_transition_v1(
        &ordinary,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut work(),
    )
    .unwrap();
    assert!(
        readmit_world_rule_remainder_ordinary_v1(&ordinary, &roots, &memory, || false).is_err()
    );
    let foreign = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    assert!(readmit_world_rule_remainder_ordinary_v1(&proof, &roots, &foreign, || false).is_err());
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(readmit_world_rule_remainder_ordinary_v1(&proof, &wrong, &memory, || false).is_err());
    let mut polls = 0;
    assert!(matches!(
        readmit_world_rule_remainder_ordinary_v1(&proof, &roots, &memory, || {
            polls += 1;
            polls > 1
        }),
        Err(ValueTreeError::Cancelled)
    ));
    let held = memory
        .reserve((240 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_world_rule_remainder_ordinary_v1(&proof, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(held);
}
