use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<PageMap>,
    _metadata: MemoryReservation,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 16_384);
        self.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("business test missing page"))?;
        if !value.uses_budget(memory) || class != MemoryClass::Ordinary || value.len() > max {
            return Err(invalid());
        }
        Ok(value)
    }
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.nodes
            .borrow()
            .get(&id)
            .map(|n| {
                if n.len() > max {
                    Err("business state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}

fn seed(memory: &MemoryBudget) -> (Pages, RootSetV1, Vec<ReservedValue<ExecutionWriteV1>>) {
    let mut pages = Pages::new(memory);
    let mut rows = Vec::with_capacity(12);
    let metrics = [0, 1, 2, 3, 4, 5, 6, u64::MAX];
    let heatmap = BTreeMap::from([(String::new(), i64::MIN), (" 原 key ".into(), 0)]);
    prepare_runtime_business_genesis_v1(
        &metrics,
        &heatmap,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1024,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut 12,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("business-test".into(), [43; 32]).unwrap();
    install(memory, &mut pages, &mut roots, &rows);
    (pages, roots, rows)
}
fn install(
    memory: &MemoryBudget,
    pages: &mut Pages,
    roots: &mut RootSetV1,
    rows: &[ReservedValue<ExecutionWriteV1>],
) {
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory
        .reserve((writes.len() + 1) * (512 << 10), MemoryClass::Ordinary)
        .unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            writes,
            pages,
            8 << 20,
        )
        .unwrap(),
    );
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after.clone();
}
fn context<'a>(
    pages: &'a Pages,
    roots: &'a RootSetV1,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError> + use<>,
    impl FnMut() -> bool + use<>,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100_000,
    }
}
#[test]
fn runtime_business_original_counters_signed_values_and_absence() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots, _rows) = seed(&memory);
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut io, &mut keys);
    let proof = qualify_runtime_business_v1(&roots, &mut ctx, &mut scan()).unwrap();
    assert_eq!(proof.heatmap_entries(), 2);
    for (i, key) in LOGISTICS_SLA_COLUMNS_V1.into_iter().enumerate() {
        assert_eq!(
            read_runtime_business_v1(RuntimeBusinessFieldV1::LogisticsSlaMetrics, key, &mut ctx)
                .unwrap(),
            Some(RuntimeBusinessNumberV1::Unsigned(if i == 7 {
                u64::MAX
            } else {
                i as u64
            }))
        );
    }
    for (key, value) in [
        ("", Some(i64::MIN)),
        (" 原 key ", Some(0)),
        ("missing", None),
    ] {
        assert_eq!(
            read_runtime_business_v1(RuntimeBusinessFieldV1::ThreatHeatmap, key, &mut ctx).unwrap(),
            value.map(RuntimeBusinessNumberV1::Signed)
        );
    }
    assert!(
        read_runtime_business_v1(
            RuntimeBusinessFieldV1::LogisticsSlaMetrics,
            "unknown",
            &mut ctx
        )
        .is_err()
    );
    let mut recovered = BTreeMap::new();
    let recovered_metrics =
        recover_runtime_business_v1(&roots, &mut ctx, &mut scan(), |key, value| {
            recovered.insert(key.to_owned(), value);
            Ok(())
        })
        .unwrap();
    assert_eq!(recovered_metrics, [0, 1, 2, 3, 4, 5, 6, u64::MAX]);
    assert_eq!(
        recovered,
        BTreeMap::from([(String::new(), i64::MIN), (" 原 key ".into(), 0)])
    );
    let other = RootSetV1::empty("wrong".into(), [43; 32]).unwrap();
    assert!(proof.validate_binding(&other).is_err());
}
#[test]
fn runtime_business_missing_column_and_cross_namespace_fail_closed() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (mut pages, mut roots, mut rows) = seed(&memory);
    let missing = rows.remove(1);
    install(&memory, &mut pages, &mut roots, &rows);
    let mut io1 = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    assert!(
        qualify_runtime_business_v1(
            &roots,
            &mut context(&pages, &roots, &memory, &mut io1, &mut keys),
            &mut scan()
        )
        .is_err()
    );
    rows.push(missing);
    install(&memory, &mut pages, &mut roots, &rows);
    // Reuse a real bound Business key/value under another actual namespace.
    let row = &rows[0];
    let charge = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
            vec![row.mutation.clone()],
            &pages,
            8 << 20,
        )
        .unwrap(),
    );
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
    let mut io2 = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    assert!(
        qualify_runtime_business_v1(
            &roots,
            &mut context(&pages, &roots, &memory, &mut io2, &mut keys),
            &mut scan()
        )
        .is_err()
    );
}

struct Combined<'a> {
    base: &'a Pages,
    extra: &'a RefCell<Pages>,
}
impl ValueNodeReader for Combined<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.extra.borrow().nodes.borrow().contains_key(id) {
            ValueNodeReader::read_node(&*self.extra.borrow(), id, max, memory, class)
        } else {
            ValueNodeReader::read_node(self.base, id, max, memory, class)
        }
    }
}
impl MerkleNodeReader for Combined<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.base.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        if let Some(value) = MerkleNodeReader::read_node(&*self.extra.borrow(), id, max)? {
            Ok(Some(value))
        } else {
            MerkleNodeReader::read_node(self.base, id, max)
        }
    }
}
struct Sink<'a>(&'a RefCell<Pages>);
impl ValueNodeWriter for Sink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().put_node(id, bytes)
    }
}

#[test]
fn runtime_business_shared_transaction_replay() {
    use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
    use oasis7_formal_protocol::incremental_execution::*;
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots, _rows) = seed(&memory);
    let extra = RefCell::new(Pages::new(&memory));
    let values = Combined {
        base: &pages,
        extra: &extra,
    };
    let mut ri = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let proof = qualify_runtime_business_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut ri, &mut keys),
        &mut scan(),
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let limits = TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    };
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &pages, &memory, limits).unwrap();
    let mut reads = io();
    let mut writes = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    {
        let mut read = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut rk,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut writes,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1024,
                record_bytes: 4096,
            },
            cancelled: || false,
        };
        for (field, key, expected, after) in [
            (
                RuntimeBusinessFieldV1::LogisticsSlaMetrics,
                "completed_transits",
                Some(RuntimeBusinessNumberV1::Unsigned(0)),
                Some(RuntimeBusinessNumberV1::Unsigned(10)),
            ),
            (
                RuntimeBusinessFieldV1::ThreatHeatmap,
                "",
                Some(RuntimeBusinessNumberV1::Signed(i64::MIN)),
                None,
            ),
            (
                RuntimeBusinessFieldV1::ThreatHeatmap,
                "new",
                None,
                Some(RuntimeBusinessNumberV1::Signed(i64::MAX)),
            ),
        ] {
            let guard = read.transaction.guard();
            assert!(
                change_runtime_business_overlay_v1(
                    RuntimeBusinessChangeV1 {
                        field,
                        key,
                        expected,
                        after
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
        }
    }
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        extra
            .borrow_mut()
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: prepared.record.after.state_root(StateNamespaceV1::Business),
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || None,
        cancelled: || false,
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let inherited =
        accept_runtime_business_transition_v1(&proof, &verifier, &prepared.record, &mut ctx)
            .unwrap();
    assert_eq!(inherited.heatmap_entries(), 2);
    let rescanned =
        qualify_runtime_business_v1(&prepared.record.after, &mut ctx, &mut scan()).unwrap();
    assert_eq!(rescanned.heatmap_entries(), inherited.heatmap_entries());
    assert_eq!(
        read_runtime_business_v1(
            RuntimeBusinessFieldV1::LogisticsSlaMetrics,
            "completed_transits",
            &mut ctx
        )
        .unwrap(),
        Some(RuntimeBusinessNumberV1::Unsigned(10))
    );
    assert_eq!(
        read_runtime_business_v1(RuntimeBusinessFieldV1::ThreatHeatmap, "", &mut ctx).unwrap(),
        None
    );
}

#[test]
fn runtime_business_later_expected_error_or_cancel_poisons_prior_write() {
    use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
    use oasis7_formal_protocol::incremental_execution::*;
    for cancel_later in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (pages, roots, _rows) = seed(&memory);
        let extra = RefCell::new(Pages::new(&memory));
        let values = Combined {
            base: &pages,
            extra: &extra,
        };
        let parent = ExecutionParent {
            roots,
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let cancelled = std::cell::Cell::new(false);
        let mut reads = io();
        let mut writes = io();
        let mut rk = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut rk,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || None,
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut writes,
                key_work: &mut wk,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1024,
                    record_bytes: 4096,
                },
                cancelled: || cancelled.get(),
            };
            let guard = read.transaction.guard();
            assert!(
                change_runtime_business_overlay_v1(
                    RuntimeBusinessChangeV1 {
                        field: RuntimeBusinessFieldV1::ThreatHeatmap,
                        key: "fresh",
                        expected: None,
                        after: Some(RuntimeBusinessNumberV1::Signed(1))
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
            cancelled.set(cancel_later);
            let guard = read.transaction.guard();
            assert!(
                change_runtime_business_overlay_v1(
                    RuntimeBusinessChangeV1 {
                        field: RuntimeBusinessFieldV1::ThreatHeatmap,
                        key: "fresh",
                        expected: Some(RuntimeBusinessNumberV1::Signed(if cancel_later {
                            1
                        } else {
                            2
                        })),
                        after: Some(RuntimeBusinessNumberV1::Signed(1))
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live
                )
                .is_err()
            );
        }
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
