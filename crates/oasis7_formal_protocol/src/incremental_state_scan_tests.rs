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
            assert!(result.is_err());
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
