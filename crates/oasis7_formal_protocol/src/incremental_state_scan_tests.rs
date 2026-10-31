use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::sync::Arc;
struct Source {
    memory: MemoryBudget,
    nodes: BTreeMap<String, Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<u64>,
    _metadata: MemoryReservation,
}
struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl MerkleNodeReader for Source {
    type Node = Page;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.reads.set(self.reads.get() + 1);
        let value = self.nodes.get(id);
        if value.is_some_and(|bytes| bytes.len() > max) {
            return Err("bounded test source".into());
        }
        Ok(value.cloned().map(Page))
    }
}
fn source(keys: &[&[u8]]) -> (StateRootV1, Source) {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let mut source = Source {
        _metadata: memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap(),
        memory,
        nodes: BTreeMap::new(),
        reads: Cell::new(0),
    };
    let _workspace = source
        .memory
        .reserve(4 << 20, MemoryClass::Ordinary)
        .unwrap();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        keys.iter().map(|key| write(key, None, Some(3))).collect(),
        &source,
        QUOTA,
    )
    .unwrap();
    for node in delta.new_nodes {
        insert(&mut source, node.digest, node.bytes);
    }
    (delta.after, source)
}
fn insert(source: &mut Source, id: String, bytes: Vec<u8>) {
    let charge = source
        .memory
        .reserve(bytes.capacity() + 128, MemoryClass::Ordinary)
        .unwrap();
    source.nodes.insert(id, Arc::new(charge.retain(bytes)));
}
fn io() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10000,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: 8,
    }
}
fn scan<E>(
    root: &StateRootV1,
    source: &Source,
    io: &mut StateScanBudget,
    visitor: impl FnMut(&StateEntryV1) -> Result<(), E>,
    cancel: impl FnMut() -> bool,
) -> Result<u64, StateScanError<E>> {
    StateScanContext {
        source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io,
    }
    .visit_entries(root, visitor, cancel)
}

fn nested_fixture() -> (StateRootV1, StateRootV1, Source) {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let mut source = Source {
        _metadata: memory.reserve(512 * 1024, MemoryClass::Ordinary).unwrap(),
        memory,
        nodes: BTreeMap::new(),
        reads: Cell::new(0),
    };
    let _workspace = source
        .memory
        .reserve(4 << 20, MemoryClass::Ordinary)
        .unwrap();
    let child_delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        [
            write(b"child-a", None, Some(1)),
            write(b"child-b", None, Some(2)),
        ]
        .into(),
        &source,
        QUOTA,
    )
    .unwrap();
    let child = child_delta.after.clone();
    for node in child_delta.new_nodes {
        insert(&mut source, node.digest, node.bytes);
    }
    let outer_delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        [write(b"nested-child-root", None, Some(9))].into(),
        &source,
        QUOTA,
    )
    .unwrap();
    let outer = outer_delta.after.clone();
    for node in outer_delta.new_nodes {
        insert(&mut source, node.digest, node.bytes);
    }
    (outer, child, source)
}

#[test]
fn nested_walk_spends_the_parent_leaf_allowance_and_scan_io() {
    let (outer, child, source) = nested_fixture();
    let mut budget = StateScanBudget {
        nodes_remaining: 10_000,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: outer.leaf_count + child.leaf_count,
    };
    let mut nested_keys = Vec::new();
    let count = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    }
    .visit_entries_with_reader(
        &outer,
        |entry, reader| {
            assert_eq!(entry.key, b"nested-child-root");
            let nested = reader.visit_entries(&child, |entry| {
                nested_keys.push(entry.key.clone());
                Ok::<_, ()>(())
            })?;
            assert_eq!(nested, 2);
            Ok::<_, StateScanError<()>>(())
        },
        || false,
    )
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(budget.leaves_remaining, 0);
    assert_eq!(nested_keys.len(), 2);

    // The outer leaf consumes one slot. The child count is checked against
    // the remaining shared allowance before the child performs any I/O.
    let mut limited = StateScanBudget {
        nodes_remaining: 10_000,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: outer.leaf_count + child.leaf_count - 1,
    };
    let reads_before = source.reads.get();
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut limited,
        }
        .visit_entries_with_reader(
            &outer,
            |_, reader| {
                assert!(matches!(
                    reader.visit_entries(&child, |_| Ok::<_, ()>(())),
                    Err(StateScanError::ResourceLimited)
                ));
                let after_child_rejection = source.reads.get();
                assert!(
                    reader
                        .read_node(&outer.node_root, STATE_MAX_PAGE_BYTES)
                        .is_err()
                );
                assert_eq!(source.reads.get(), after_child_rejection);
                Ok::<_, StateScanError<()>>(())
            },
            || false,
        ),
        Err(StateScanError::ResourceLimited)
    ));
    assert!(source.reads.get() > reads_before);
}

#[test]
fn caught_nested_cancellation_is_sticky_and_blocks_more_source_reads() {
    let (outer, child, source) = nested_fixture();
    let cancelled = Cell::new(false);
    let mut budget = StateScanBudget {
        nodes_remaining: 10_000,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: outer.leaf_count + child.leaf_count,
    };
    let reads_before = source.reads.get();
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut budget,
    }
    .visit_entries_with_reader(
        &outer,
        |_, reader| {
            cancelled.set(true);
            assert!(matches!(
                reader.visit_entries(&child, |_| Ok::<_, ()>(())),
                Err(StateScanError::Cancelled)
            ));
            let after_cancel = source.reads.get();
            assert!(
                reader
                    .read_node(&outer.node_root, STATE_MAX_PAGE_BYTES)
                    .is_err()
            );
            assert_eq!(source.reads.get(), after_cancel);
            Ok::<_, StateScanError<()>>(())
        },
        || cancelled.get(),
    );
    assert!(matches!(result, Err(StateScanError::Cancelled)));
    assert!(source.reads.get() > reads_before);
}

#[test]
fn caught_cancellation_precedes_nested_memory_and_leaf_limits() {
    for exhaust_memory in [false, true] {
        let (outer, child, source) = nested_fixture();
        let cancelled = Cell::new(false);
        let mut budget = StateScanBudget {
            nodes_remaining: 10_000,
            read_bytes_remaining: 1 << 24,
            leaves_remaining: outer.leaf_count + child.leaf_count,
        };
        let reads_before = source.reads.get();
        let mut reads_after_cancel = None;
        let result = StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut budget,
        }
        .visit_entries_with_reader(
            &outer,
            |_, reader| {
                let mut nested_visits = 0;
                let reads_before_cancel = source.reads.get();
                assert!(matches!(
                    reader.visit_entries(&child, |_| {
                        nested_visits += 1;
                        if nested_visits == 1 {
                            cancelled.set(true);
                        }
                        Ok::<_, ()>(())
                    }),
                    Err(StateScanError::Cancelled)
                ));
                assert_eq!(nested_visits, 1);
                assert!(source.reads.get() > reads_before_cancel);
                reads_after_cancel = Some(source.reads.get());

                // The first cancelled traversal has already consumed a leaf.
                // Exhaust both later allowances to verify that the sticky
                // cancellation remains the reported cause on the retry.
                reader.leaves.set(0);
                // In one case the nested reader's workspace cannot be
                // admitted either. The sticky cancellation must take priority
                // over that later memory failure as well as the leaf limit.
                let _memory_pressure = if exhaust_memory {
                    let ordinary_limit: usize = (16 << 20) - (1 << 20);
                    let remaining = ordinary_limit.saturating_sub(source.memory.used());
                    Some(
                        source
                            .memory
                            .reserve(remaining, MemoryClass::Ordinary)
                            .unwrap(),
                    )
                } else {
                    None
                };
                assert!(matches!(
                    reader.visit_entries(&child, |_| Ok::<_, ()>(())),
                    Err(StateScanError::Cancelled)
                ));
                assert_eq!(source.reads.get(), reads_after_cancel.unwrap());
                Ok::<_, StateScanError<()>>(())
            },
            || cancelled.get(),
        );
        assert!(matches!(result, Err(StateScanError::Cancelled)));
        assert!(reads_after_cancel.unwrap() > reads_before);
    }
}

#[test]
fn complete_scan_has_independent_tree_and_key_order_with_exact_allowances() {
    let keys: [&[u8]; 3] = [b"a", b"b", b"c"];
    let (root, source) = source(&keys);
    let expected: BTreeMap<_, _> = keys.iter().map(|key| (key.to_vec(), [3; 32])).collect();
    assert_eq!(
        root.node_root,
        reference(StateNamespaceV1::Business, &expected).0
    );
    let mut expected_order = keys.to_vec();
    expected_order.sort_by_key(|key| {
        let mut bytes = b"oasis7.iws.state-key.v1\0".to_vec();
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&(key.len() as u16).to_be_bytes());
        bytes.extend_from_slice(key);
        *blake3::hash(&bytes).as_bytes()
    });
    for mode in 0..4 {
        let mut io = StateScanBudget {
            nodes_remaining: source.nodes.len() as u64 - u64::from(mode == 1),
            read_bytes_remaining: source.nodes.values().map(|v| v.len() as u64).sum::<u64>()
                - u64::from(mode == 2),
            leaves_remaining: 3 - u64::from(mode == 3),
        };
        let mut actual = vec![];
        let result = scan(
            &root,
            &source,
            &mut io,
            |entry| {
                actual.push(entry.key.clone());
                Ok::<_, ()>(())
            },
            || false,
        );
        if mode == 0 {
            assert_eq!(result.unwrap(), 3);
            assert_eq!(actual, expected_order);
            assert_eq!(
                (
                    io.nodes_remaining,
                    io.read_bytes_remaining,
                    io.leaves_remaining
                ),
                (0, 0, 0)
            );
        } else {
            assert!(matches!(result, Err(StateScanError::ResourceLimited)));
        }
    }
}
#[test]
fn authenticated_wrong_path_depth_namespace_count_and_missing_page_fail() {
    for mode in 0..5 {
        let (mut root, mut source) = source(&[b"a"]);
        if mode <= 1 {
            let bytes = source.nodes[&root.node_root].clone();
            let crate::merkle::Node::Branch { depth, left, right } =
                StateCodec::decode(&bytes).unwrap()
            else {
                panic!()
            };
            let bytes = if mode == 0 {
                StateCodec::encode(crate::merkle::Node::Branch {
                    depth,
                    left: right,
                    right: left,
                })
                .unwrap()
            } else {
                StateCodec::encode(crate::merkle::Node::Branch {
                    depth: 1,
                    left,
                    right,
                })
                .unwrap()
            };
            root.node_root = blake3::hash(&bytes).to_hex().to_string();
            insert(&mut source, root.node_root.clone(), bytes);
        } else if mode == 2 {
            root.namespace = StateNamespaceV1::Scheduler;
        } else if mode == 3 {
            root.leaf_count = 2;
        } else {
            source.nodes.remove(&root.node_root);
        }
        assert!(scan(&root, &source, &mut io(), |_| Ok::<_, ()>(()), || false).is_err());
    }
}
#[test]
fn cancel_visitor_failure_and_foreign_pool_never_return_partial_count() {
    let (root, source) = source(&[b"a", b"b"]);
    let before = source.memory.used();
    let mut io = io();
    let mut calls = 0;
    assert!(matches!(
        scan(
            &root,
            &source,
            &mut io,
            |_| Ok::<_, ()>(()),
            || {
                calls += 1;
                calls > 10
            }
        ),
        Err(StateScanError::Cancelled)
    ));
    assert_eq!(10000 - io.nodes_remaining, source.reads.get());
    assert_eq!(source.memory.used(), before);
    assert!(matches!(
        scan(&root, &source, &mut io, |_| Err("visitor error"), || false),
        Err(StateScanError::Visitor("visitor error"))
    ));
    let foreign = MemoryBudget::new(1 << 20, 0).unwrap();
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &foreign,
            class: MemoryClass::Ordinary,
            io: &mut io
        }
        .visit_entries(&root, |_| Ok::<_, ()>(()), || false),
        Err(StateScanError::Invalid(_))
    ));
}

#[test]
fn caught_source_unwind_does_not_refund_scan_admission() {
    struct Panics<'a>(&'a Source);
    impl MerkleNodeReader for Panics<'_> {
        type Node = Page;
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            Some(&self.0.memory)
        }
        fn allocation_class(&self) -> Option<MemoryClass> {
            Some(MemoryClass::Ordinary)
        }
        fn read_node(&self, _: &str, _: usize) -> Result<Option<Page>, String> {
            panic!("source unwind")
        }
    }
    let (root, source) = source(&[b"a"]);
    let mut io = io();
    let before = source.memory.used();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            StateScanContext {
                source: &Panics(&source),
                memory: &source.memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            }
            .visit_entries(&root, |_| Ok::<_, ()>(()), || false)
        }))
        .is_err()
    );
    assert_eq!(io.nodes_remaining, 9999);
    assert_eq!(
        io.read_bytes_remaining,
        (1 << 24) - STATE_MAX_PAGE_BYTES as u64
    );
    assert_eq!(source.memory.used(), before);
}
#[test]
fn empty_root_needs_no_pages_but_still_observes_cancel() {
    let (root, source) = source(&[]);
    let mut io = StateScanBudget {
        nodes_remaining: 0,
        read_bytes_remaining: 0,
        leaves_remaining: 0,
    };
    assert_eq!(
        scan(
            &root,
            &source,
            &mut io,
            |_| Err::<(), _>("must not visit"),
            || false
        )
        .unwrap(),
        0
    );
    assert!(matches!(
        scan(&root, &source, &mut io, |_| Ok::<_, ()>(()), || true),
        Err(StateScanError::Cancelled)
    ));
    assert_eq!(source.reads.get(), 0);
}

#[test]
fn missing_page_with_reduced_positive_admission_still_reports_tree_error() {
    let (root, mut source) = source(&[b"a"]);
    source.nodes.remove(&root.node_root);
    let mut allowance = io();
    allowance.read_bytes_remaining = 1;
    assert!(matches!(
        scan(
            &root,
            &source,
            &mut allowance,
            |_| Ok::<_, ()>(()),
            || false
        ),
        Err(StateScanError::Tree(_))
    ));
}

#[test]
fn nested_lookup_uses_the_outer_scan_nodes_bytes_and_leaf_allowance() {
    let keys: [&[u8]; 3] = [b"a", b"b", b"c"];
    let (root, source) = source(&keys);

    let mut scan_io = io();
    let start_reads = source.reads.get();
    scan(&root, &source, &mut scan_io, |_| Ok::<_, ()>(()), || false).unwrap();
    let scan_reads = source.reads.get() - start_reads;
    let scan_bytes = (1 << 24) - scan_io.read_bytes_remaining;

    let start_reads = source.reads.get();
    assert!(root.get(b"a", &source).unwrap().is_some());
    let point_reads = source.reads.get() - start_reads;
    assert!(point_reads > 0);

    let exact_nodes = scan_reads + point_reads;
    let mut exact_io = StateScanBudget {
        nodes_remaining: exact_nodes,
        read_bytes_remaining: scan_bytes + (1 << 16),
        leaves_remaining: 3,
    };
    let before = source.reads.get();
    let mut looked_up = false;
    assert_eq!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut exact_io,
        }
        .visit_entries_with_reader(
            &root,
            |_, guarded| {
                if !looked_up {
                    looked_up = true;
                    assert!(
                        root.get(b"a", guarded)
                            .map_err(|_| "nested lookup")?
                            .is_some()
                    );
                }
                Ok::<_, &'static str>(())
            },
            || false,
        )
        .unwrap(),
        3
    );
    assert_eq!(source.reads.get() - before, exact_nodes);
    assert_eq!(exact_io.nodes_remaining, 0);
    assert_eq!(exact_io.leaves_remaining, 0);

    let mut one_fewer_node = StateScanBudget {
        nodes_remaining: exact_nodes - 1,
        read_bytes_remaining: scan_bytes + (1 << 16),
        leaves_remaining: 3,
    };
    let mut looked_up = false;
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut one_fewer_node,
        }
        .visit_entries_with_reader(
            &root,
            |_, guarded| {
                if !looked_up {
                    looked_up = true;
                    root.get(b"a", guarded)
                        .map(|_| ())
                        .map_err(|_| "nested lookup")?;
                }
                Ok::<(), &'static str>(())
            },
            || false,
        ),
        Err(StateScanError::ResourceLimited)
    ));

    let mut one_fewer_byte = StateScanBudget {
        nodes_remaining: exact_nodes,
        read_bytes_remaining: scan_bytes + (1 << 16),
        leaves_remaining: 3,
    };
    let mut looked_up = false;
    // Measure the exact successful combined read first so that the failing
    // budget differs by only one byte, independent of tree shape.
    let initial = one_fewer_byte.read_bytes_remaining;
    StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut one_fewer_byte,
    }
    .visit_entries_with_reader(
        &root,
        |_, guarded| {
            if !looked_up {
                looked_up = true;
                root.get(b"a", guarded)
                    .map(|_| ())
                    .map_err(|_| "nested lookup")?;
            }
            Ok::<(), &'static str>(())
        },
        || false,
    )
    .unwrap();
    let combined_bytes = initial - one_fewer_byte.read_bytes_remaining;
    let mut short_bytes = StateScanBudget {
        nodes_remaining: exact_nodes,
        read_bytes_remaining: combined_bytes - 1,
        leaves_remaining: 3,
    };
    let mut looked_up = false;
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut short_bytes,
        }
        .visit_entries_with_reader(
            &root,
            |_, guarded| {
                if !looked_up {
                    looked_up = true;
                    root.get(b"a", guarded)
                        .map(|_| ())
                        .map_err(|_| "nested lookup")?;
                }
                Ok::<(), &'static str>(())
            },
            || false,
        ),
        Err(StateScanError::ResourceLimited)
    ));
}

#[test]
fn nested_lookup_errors_and_cancellation_are_not_treated_as_absence() {
    let (root, source) = source(&[b"a"]);
    let missing = StateRootV1 {
        namespace: root.namespace,
        node_root: "ff".repeat(32),
        leaf_count: 1,
    };
    let mut missing_io = io();
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut missing_io,
        }
        .visit_entries_with_reader(
            &root,
            |_, guarded| {
                missing
                    .get(b"absent", guarded)
                    .map(|_| ())
                    .map_err(|_| "missing nested tree")
            },
            || false,
        ),
        Err(StateScanError::Visitor("missing nested tree"))
    ));

    let mut cancel_io = io();
    let mut checks = 0;
    assert!(matches!(
        StateScanContext {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            io: &mut cancel_io,
        }
        .visit_entries_with_reader(
            &root,
            |_, guarded| {
                root.get(b"a", guarded)
                    .map(|_| ())
                    .map_err(|_| "nested cancelled lookup")
            },
            || {
                checks += 1;
                checks == 3
            },
        ),
        Err(StateScanError::Cancelled)
    ));
}

#[test]
fn caught_nested_cancellation_is_sticky_and_cannot_become_resource_limited() {
    let (root, source) = source(&[b"a"]);

    // Calibrate the outer traversal and one complete point read independently,
    // then leave exactly enough node allowance for the post-cancel lookup to
    // read the tree once. A third lookup would otherwise reach the node limit.
    let mut scan_io = io();
    let before_scan = source.reads.get();
    scan(&root, &source, &mut scan_io, |_| Ok::<_, ()>(()), || false).unwrap();
    let scan_reads = source.reads.get() - before_scan;

    let before_point = source.reads.get();
    assert!(root.get(b"a", &source).unwrap().is_some());
    let point_reads = source.reads.get() - before_point;
    assert!(point_reads > 0);

    let mut limited = StateScanBudget {
        nodes_remaining: scan_reads + point_reads,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: 1,
    };
    let cancel_next_read = Cell::new(false);
    let mut first_lookup_failed = None;
    let mut followup_lookups_failed = [None, None];
    let mut reads_after_cancellation = None;
    let mut reads_after_followups = None;
    let result = StateScanContext {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        io: &mut limited,
    }
    .visit_entries_with_reader(
        &root,
        |_, reader| {
            cancel_next_read.set(true);
            first_lookup_failed = Some(matches!(
                reader.get(&root, b"a"),
                Err(StateScanError::Cancelled)
            ));
            let reads = source.reads.get();
            reads_after_cancellation = Some(reads);

            // A caught cancellation stays the dominant result even after the
            // remaining scan allowances and Ordinary memory have been
            // exhausted. A follow-up point lookup must not read more pages.
            reader.nodes.set(0);
            reader.bytes.set(0);
            let ordinary_limit = (16 << 20) - (1 << 20);
            let memory_used = source.memory.used();
            assert!(memory_used <= ordinary_limit);
            let _memory_pressure = source
                .memory
                .reserve(ordinary_limit - memory_used, MemoryClass::Ordinary)
                .unwrap();

            // A visitor is permitted to catch a failed sibling lookup. The
            // scan reader must still reject every later lookup without I/O.
            followup_lookups_failed = [
                Some(matches!(
                    reader.get(&root, b"a"),
                    Err(StateScanError::Cancelled)
                )),
                Some(matches!(
                    reader.get(&root, b"a"),
                    Err(StateScanError::Cancelled)
                )),
            ];
            reads_after_followups = Some(source.reads.get());
            Ok::<_, ()>(())
        },
        || cancel_next_read.replace(false),
    );

    assert!(
        matches!(&result, Err(StateScanError::Cancelled)),
        "terminal scan result: {result:?}"
    );
    assert_eq!(first_lookup_failed, Some(true));
    assert_eq!(followup_lookups_failed, [Some(true), Some(true)]);
    assert_eq!(reads_after_cancellation, reads_after_followups);
}

#[test]
fn guarded_get_spends_exact_node_and_byte_budget_without_leaf_visits() {
    let build_source = source;
    let (root, source) = build_source(&[b"a"]);

    let run = |nodes_remaining: u64, read_bytes_remaining: u64| {
        let nodes = Cell::new(nodes_remaining);
        let bytes = Cell::new(read_bytes_remaining);
        let leaves = Cell::new(7);
        let cancelled = RefCell::new(|| false);
        let stop = Cell::new(None);
        let reader = StateScanReader {
            source: &source,
            memory: &source.memory,
            class: MemoryClass::Ordinary,
            nodes: &nodes,
            bytes: &bytes,
            leaves: &leaves,
            cancelled: &cancelled,
            stop: &stop,
        };
        let result = reader.get(&root, b"a");
        (result, nodes.get(), bytes.get(), leaves.get())
    };

    let initial_bytes = 1 << 24;
    let (result, nodes_left, bytes_left, leaves_left) = run(10_000, initial_bytes);
    let expected = result.unwrap();
    assert!(expected.is_some());
    let node_cost = 10_000 - nodes_left;
    let byte_cost = initial_bytes - bytes_left;
    assert!(node_cost > 0);
    assert!(byte_cost > 0);
    assert_eq!(leaves_left, 7);

    let (exact, nodes_left, bytes_left, leaves_left) = run(node_cost, byte_cost);
    assert!(exact.unwrap().is_some());
    assert_eq!(nodes_left, 0);
    assert_eq!(bytes_left, 0);
    assert_eq!(leaves_left, 7);

    assert!(matches!(
        run(node_cost - 1, byte_cost).0,
        Err(StateScanError::ResourceLimited)
    ));
    assert!(matches!(
        run(node_cost, byte_cost - 1).0,
        Err(StateScanError::ResourceLimited)
    ));

    assert_eq!(run(node_cost, byte_cost).0.unwrap(), expected);
    assert!(run(10_000, initial_bytes).0.unwrap().is_some());
    assert_eq!(run(10_000, initial_bytes).3, 7);

    let (cancelled_root, source) = build_source(&[b"a"]);
    let foreign_memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let nodes = Cell::new(10_000);
    let bytes = Cell::new(1 << 24);
    let leaves = Cell::new(7);
    let cancelled = RefCell::new(|| false);
    let stop = Cell::new(None);
    let reader = StateScanReader {
        source: &source,
        memory: &foreign_memory,
        class: MemoryClass::Ordinary,
        nodes: &nodes,
        bytes: &bytes,
        leaves: &leaves,
        cancelled: &cancelled,
        stop: &stop,
    };
    assert!(matches!(
        reader.get(&cancelled_root, b"a"),
        Err(StateScanError::Invalid("state scan source pool/class"))
    ));
    assert_eq!(source.reads.get(), 0);

    let mut other_namespace = cancelled_root.clone();
    other_namespace.namespace = StateNamespaceV1::Allocator;
    let nodes = Cell::new(10_000);
    let bytes = Cell::new(1 << 24);
    let leaves = Cell::new(7);
    let cancelled = RefCell::new(|| false);
    let stop = Cell::new(None);
    let reader = StateScanReader {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        nodes: &nodes,
        bytes: &bytes,
        leaves: &leaves,
        cancelled: &cancelled,
        stop: &stop,
    };
    assert_eq!(reader.get(&cancelled_root, b"missing").unwrap(), None);
    assert!(!matches!(
        reader.get(&other_namespace, b"a"),
        Ok(value) if value == expected
    ));
    assert_eq!(leaves.get(), 7);

    let ordinary_limit = (16 << 20) - (1 << 20);
    let memory_used = source.memory.used();
    let _memory_pressure = source
        .memory
        .reserve(ordinary_limit - memory_used, MemoryClass::Ordinary)
        .unwrap();
    let nodes = Cell::new(10_000);
    let bytes = Cell::new(1 << 24);
    let leaves = Cell::new(7);
    let cancelled = RefCell::new(|| false);
    let stop = Cell::new(None);
    let reader = StateScanReader {
        source: &source,
        memory: &source.memory,
        class: MemoryClass::Ordinary,
        nodes: &nodes,
        bytes: &bytes,
        leaves: &leaves,
        cancelled: &cancelled,
        stop: &stop,
    };
    let reads_before_memory_error = source.reads.get();
    assert!(matches!(
        reader.get(&cancelled_root, b"a"),
        Err(StateScanError::Memory(_))
    ));
    assert_eq!(source.reads.get(), reads_before_memory_error);
}
