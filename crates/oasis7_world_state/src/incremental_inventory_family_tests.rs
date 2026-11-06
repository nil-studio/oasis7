use super::*;
use crate::{
    MaterialLedgerId, WorldState,
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> InventoryFamilyWorkV1 {
    InventoryFamilyWorkV1 {
        records_remaining: 100000,
        index_steps_remaining: 1_000_000,
        max_semantic_bytes: 4096,
    }
}
struct Rows(Vec<ReservedValue<StateWriteV1>>);
impl InventoryWriteSink for Rows {
    fn put_write(&mut self, w: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push(w);
        Ok(())
    }
}
fn source() -> WorldState {
    let mut s = WorldState::default();
    s.resources.clear();
    s.materials.clear();
    s.material_ledgers.clear();
    s.resources.insert(ResourceKind::Electricity, i64::MIN);
    s.materials.insert(" ore/界 ".into(), i64::MAX);
    s.material_ledgers
        .insert(MaterialLedgerId::world(), [(" ore/界 ".into(), -7)].into());
    s
}
fn install(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut rows = Rows(Vec::new());
    let mut writer = Writer(pages);
    prepare_inventory_genesis_v1(
        &source(),
        &mut InventoryGenesisContext {
            values: &mut writer,
            writes: &mut rows,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            records_remaining: 10000,
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.0.iter().map(|r| (**r).clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("inventory-family".into(), [8; 32]).unwrap();
    let ns = StateNamespaceV1::Business;
    let delta = StateDeltaV1::build(roots.state_root(ns).clone(), writes, pages, 8 << 20).unwrap();
    for n in &delta.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes(),
                memory
                    .reserve(n.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(n.bytes.clone()),
            )
            .unwrap();
    }
    *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    roots
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
        root: roots.state_root(StateNamespaceV1::Business),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
#[test]
fn inventory_full_qualification_and_general_delta_close_unchanged_ledger_members() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_inventory_family_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.record_counts(), [1, 1, 1, 1]);
    let field = descriptor("material_ledgers", FieldShape::NestedMap).unwrap();
    let group = crate::incremental_semantic_key::hash_semantic_key_v1(
        &MaterialLedgerId::world(),
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let group =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Group, Some(group.digest)).unwrap();
    let entry = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(MaterialLedgerId::world(), " ore/界 "),
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let entry =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(entry.digest)).unwrap();
    for (remove_group, remove_member, valid) in [
        (false, false, true),
        (true, false, false),
        (true, true, true),
        (false, true, true),
    ] {
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [1; 32],
            authority: AuthorityBinding {
                token: [2; 32],
                epoch: 3,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [3; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        if remove_group {
            tx.write(StateNamespaceV1::Business, group.as_bytes(), None)
                .unwrap();
        }
        if remove_member {
            tx.write(StateNamespaceV1::Business, entry.as_bytes(), None)
                .unwrap();
        }
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
        let result = accept_inventory_transition_v1(
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
        assert_eq!(
            result.is_ok(),
            valid,
            "group={remove_group} member={remove_member}"
        );
        if let Ok(after) = result {
            assert_eq!(
                after.record_counts(),
                [1, 1, u64::from(!remove_group), u64::from(!remove_member)]
            );
        }
    }
}
#[test]
fn inventory_qualification_rejects_missing_required_metadata_and_cancelled_work() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = RootSetV1::empty("missing-inventory".into(), [8; 32]).unwrap();
    assert!(
        qualify_inventory_family_v1(
            &roots,
            &mut context(
                &memory,
                &pages,
                &roots,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20
                }
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = context(&memory, &pages, &roots, &mut quota, &mut keys);
    let mut limited = work();
    limited.index_steps_remaining = 0;
    assert!(matches!(
        qualify_inventory_family_v1(&roots, &mut ctx, &mut scan(), &mut limited),
        Err(ValueTreeError::ResourceLimited(_))
    ));
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
fn inventory_recovery_class_requires_finite_explicit_deep_readmission() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_inventory_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let tx = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
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
        accept_inventory_transition_v1(
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
    let promoted =
        readmit_inventory_family_ordinary_v1(&proof, &roots, &memory, &mut work(), || false)
            .unwrap();
    assert_eq!(promoted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(promoted.record_counts(), proof.record_counts());
    accept_inventory_transition_v1(
        &promoted,
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
        readmit_inventory_family_ordinary_v1(&promoted, &roots, &memory, &mut work(), || false)
            .is_err()
    );
    let foreign = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    assert!(
        readmit_inventory_family_ordinary_v1(&proof, &roots, &foreign, &mut work(), || false)
            .is_err()
    );
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(
        readmit_inventory_family_ordinary_v1(&proof, &wrong, &memory, &mut work(), || false)
            .is_err()
    );
    let mut polls = 0;
    assert!(matches!(
        readmit_inventory_family_ordinary_v1(&proof, &roots, &memory, &mut work(), || {
            polls += 1;
            polls > 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
    let mut finite = work();
    finite.index_steps_remaining = 0;
    assert!(matches!(
        readmit_inventory_family_ordinary_v1(&proof, &roots, &memory, &mut finite, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let held = memory
        .reserve((112 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_inventory_family_ordinary_v1(&proof, &roots, &memory, &mut work(), || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(held);
}
