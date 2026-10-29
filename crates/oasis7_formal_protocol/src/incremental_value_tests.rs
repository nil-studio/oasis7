use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;

struct Store {
    nodes: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    reads: Cell<usize>,
    bytes: Cell<usize>,
}
impl Store {
    fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            reads: Cell::new(0),
            bytes: Cell::new(0),
        }
    }
}
impl ValueNodeWriter for Store {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
        self.nodes.insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl ValueNodeReader for Store {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        _class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let bytes = self
            .nodes
            .get(id)
            .ok_or(ValueTreeError::Invalid("missing source node"))?;
        if bytes.len() > max_bytes || !bytes.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("source scope/bound"));
        }
        self.reads.set(self.reads.get() + 1);
        self.bytes.set(self.bytes.get() + bytes.len());
        Ok(bytes.clone())
    }
}

fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: u64::MAX,
        read_bytes_remaining: u64::MAX,
        write_bytes_remaining: u64::MAX,
    }
}

fn range(
    root: &ValueRootV1,
    offset: u64,
    length: u64,
    source: &Store,
    output: &mut Vec<u8>,
    memory: &MemoryBudget,
) -> Result<u64, ValueTreeError> {
    ValueReadContext {
        source,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut io(),
    }
    .read_range(root, offset, length, output, || false)
}

fn build(data: &[u8]) -> (ValueRootV1, Store, MemoryBudget) {
    let memory = MemoryBudget::new(128 * 1024 * 1024, 256 * 1024).unwrap();
    let mut store = Store::new();
    let root = write_value(
        &mut std::io::Cursor::new(data),
        data.len() as u64,
        &mut store,
        &memory,
        &mut io(),
        || false,
    )
    .unwrap();
    assert_eq!(root.0.logical_len, data.len() as u64);
    (root, store, memory)
}

#[test]
fn logical_chunks_and_roots_do_not_depend_on_input_read_buffering() {
    struct Short<'a> {
        bytes: &'a [u8],
        width: usize,
    }
    impl Read for Short<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let count = self.width.min(out.len()).min(self.bytes.len());
            out[..count].copy_from_slice(&self.bytes[..count]);
            self.bytes = &self.bytes[count..];
            Ok(count)
        }
    }
    for size in [
        0,
        1,
        VALUE_CHUNK_BYTES - 1,
        VALUE_CHUNK_BYTES,
        VALUE_CHUNK_BYTES + 1,
        VALUE_CHUNK_BYTES * 129 + 7,
    ] {
        let data: Vec<u8> = (0..size).map(|n| (n % 251) as u8).collect();
        let (expected, first, _) = build(&data);
        let memory = MemoryBudget::new(32 * 1024 * 1024, 0).unwrap();
        let mut second = Store::new();
        let actual = write_value(
            &mut Short {
                bytes: &data,
                width: 997,
            },
            size as u64,
            &mut second,
            &memory,
            &mut io(),
            || false,
        )
        .unwrap();
        assert_eq!(expected, actual);
        assert_eq!(
            first.nodes.keys().collect::<Vec<_>>(),
            second.nodes.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            ValueRootV1::from_canonical_bytes(&actual.canonical_bytes().unwrap()).unwrap(),
            actual
        );
        assert_eq!(expected.digest().unwrap(), actual.digest().unwrap());
    }
}

#[test]
fn late_and_cross_chunk_ranges_read_only_intersecting_paths_and_chunks() {
    let data: Vec<u8> = (0..VALUE_CHUNK_BYTES * 130 + 11)
        .map(|n| (n % 251) as u8)
        .collect();
    let (root, store, memory) = build(&data);
    assert_eq!(root.0.depth, 2);
    for (offset, length) in [
        (0, 1),
        (65535, 4),
        (VALUE_CHUNK_BYTES * 128 - 2, 6),
        (VALUE_CHUNK_BYTES * 129 + 1, 8),
        (data.len() - 11, 11),
        (data.len(), 0),
    ] {
        store.reads.set(0);
        store.bytes.set(0);
        let mut output = Vec::new();
        assert_eq!(
            range(
                &root,
                offset as u64,
                length as u64,
                &store,
                &mut output,
                &memory
            )
            .unwrap(),
            length as u64
        );
        assert_eq!(output, data[offset..offset + length]);
        assert!(store.reads.get() <= 5);
        assert!(store.bytes.get() <= 2 * VALUE_MAX_NODE_BYTES + 2 * 5200);
    }
    assert!(range(&root, u64::MAX, 2, &store, &mut Vec::new(), &memory).is_err());
}

#[test]
fn streaming_frontier_matches_independent_level_by_level_encoding() {
    use std::collections::BTreeSet;
    let data: Vec<u8> = (0..VALUE_CHUNK_BYTES * 129 + 7)
        .map(|n| (n % 251) as u8)
        .collect();
    let (actual, store, _) = build(&data);
    let mut expected_ids = BTreeSet::new();
    let mut level: Vec<([u8; 32], u64, u8)> = data
        .chunks(65536)
        .map(|chunk| {
            let mut bytes = b"oasis7.iws.value-tree.v1\0".to_vec();
            bytes.extend_from_slice(&((1 + chunk.len()) as u32).to_be_bytes());
            bytes.push(0);
            bytes.extend_from_slice(chunk);
            let id = *blake3::hash(&bytes).as_bytes();
            expected_ids.insert(id);
            (id, chunk.len() as u64, 0)
        })
        .collect();
    while level.len() > 1 {
        level = level
            .chunks(128)
            .map(|group| {
                let depth = group[0].2 + 1;
                let total: u64 = group.iter().map(|child| child.1).sum();
                let mut bytes = b"oasis7.iws.value-tree.v1\0".to_vec();
                bytes.extend_from_slice(&((12 + 40 * group.len()) as u32).to_be_bytes());
                bytes.extend_from_slice(&[1, depth]);
                bytes.extend_from_slice(&total.to_be_bytes());
                bytes.extend_from_slice(&(group.len() as u16).to_be_bytes());
                for (id, length, _) in group {
                    bytes.extend_from_slice(&length.to_be_bytes());
                    bytes.extend_from_slice(id);
                }
                let id = *blake3::hash(&bytes).as_bytes();
                expected_ids.insert(id);
                (id, total, depth)
            })
            .collect();
    }
    let (id, logical_len, depth) = level[0];
    assert_eq!(
        actual,
        ValueRootV1(ValueNodeRefV1 {
            id,
            logical_len,
            depth
        })
    );
    expected_ids.insert(actual.digest().unwrap());
    assert_eq!(
        store.nodes.keys().copied().collect::<BTreeSet<_>>(),
        expected_ids
    );
}

#[test]
fn typed_edges_and_corrupt_index_boundaries_are_verified_before_output() {
    let (root, store, memory) = build(&vec![9; VALUE_CHUNK_BYTES + 3]);
    for (id, bytes) in &store.nodes {
        verified_value_links(id, bytes).unwrap();
        let mut bad = (**bytes).clone();
        bad[0] ^= 1;
        assert!(verified_value_links(blake3::hash(&bad).as_bytes(), &bad).is_err());
    }
    let index = &store.nodes[&root.0.id];
    let mut bad = (**index).clone();
    // First child claims a short interior chunk. Rehashing must not bypass
    // fixed logical boundaries, even when total length is left unchanged.
    bad[codec::INDEX_OVERHEAD..codec::INDEX_OVERHEAD + 8].copy_from_slice(&65535u64.to_be_bytes());
    assert!(verified_value_links(blake3::hash(&bad).as_bytes(), &bad).is_err());
    let mut quota = io();
    quota.nodes_remaining = 0;
    let mut output = Vec::new();
    let mut context = ValueReadContext {
        source: &store,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    assert!(matches!(
        context.read_range(&root, 0, 1, &mut output, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(output.is_empty());
    let mut quota = io();
    quota.write_bytes_remaining = 0;
    let mut context = ValueReadContext {
        source: &store,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    assert!(matches!(
        context.read_range(&root, 0, 1, &mut output, || false),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(output.is_empty());
    assert!(matches!(
        context.read_range(&root, 0, 1, &mut output, || true),
        Err(ValueTreeError::Cancelled)
    ));
}

#[test]
fn failure_cancellation_budget_and_last_reader_accounting_remain_explicit() {
    let (root, mut store, memory) = build(&vec![1; VALUE_CHUNK_BYTES + 3]);
    let used = memory.used();
    let retained = store.nodes.get(&root.0.id).unwrap().clone();
    store.nodes.remove(&root.0.id);
    assert_eq!(memory.used(), used);
    drop(retained);
    assert!(memory.used() < used);
    assert!(range(&root, 0, 1, &store, &mut Vec::new(), &memory).is_err());
    let tiny = MemoryBudget::new(1, 0).unwrap();
    assert!(matches!(
        write_value(
            &mut &b"x"[..],
            1,
            &mut Store::new(),
            &tiny,
            &mut io(),
            || false
        ),
        Err(ValueTreeError::Memory(_))
    ));
    assert!(matches!(
        write_value(
            &mut &b"x"[..],
            1,
            &mut Store::new(),
            &memory,
            &mut io(),
            || true
        ),
        Err(ValueTreeError::Cancelled)
    ));
    let mut limited = io();
    limited.nodes_remaining = 0;
    assert!(matches!(
        write_value(
            &mut &b"x"[..],
            1,
            &mut Store::new(),
            &memory,
            &mut limited,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        write_value(
            &mut &b"x"[..],
            2,
            &mut Store::new(),
            &memory,
            &mut io(),
            || false
        ),
        Err(ValueTreeError::Io(_))
    ));
}

#[test]
fn value_profile_vectors() {
    for data in [&b""[..], &b"abc"[..]] {
        let (root, _, _) = build(data);
        let (node_id, descriptor_id) = if data.is_empty() {
            (
                "281dc006a045e2b360a7f422d7b2a6e3aa2d26c2ed0e0844b5e950b5228767c2",
                "131cf4bc0fb0b993461ec42e0bcb2891ea95c57f33b244f7173ca2a786ed88a2",
            )
        } else {
            (
                "e59908bfdbda7d4944bed2cec9cfe80ca2c5177c71b08b6dfe322576ef217ae5",
                "26975daa7484d7c45c85144e29f43a16624ec134ded327edca9576c9e8b726ea",
            )
        };
        assert_eq!(hex::encode(root.0.id), node_id);
        assert_eq!(hex::encode(root.digest().unwrap()), descriptor_id);
        let mut bad = root.canonical_bytes().unwrap();
        bad[0] ^= 1;
        assert!(ValueRootV1::from_canonical_bytes(&bad).is_err());
        bad = root.canonical_bytes().unwrap();
        *bad.last_mut().unwrap() = 8;
        assert!(ValueRootV1::from_canonical_bytes(&bad).is_err());
    }
}

#[test]
fn root_loading_rejects_wrong_types_and_cross_pool_or_undercharged_carriers() {
    let (root, store, memory) = build(b"abc");
    let mut quota = io();
    let mut context = ValueReadContext {
        source: &store,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    assert_eq!(context.load_root(&root.digest().unwrap()).unwrap(), root);
    assert!(context.load_root(&root.0.id).is_err());

    struct BadSource(Arc<ReservedValue<Vec<u8>>>);
    impl ValueNodeReader for BadSource {
        fn read_node(
            &self,
            _id: &[u8; 32],
            _max_bytes: usize,
            _memory: &MemoryBudget,
            _class: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            Ok(self.0.clone())
        }
    }
    let other = MemoryBudget::new(1024 * 1024, 0).unwrap();
    let source = BadSource(store.nodes[&root.digest().unwrap()].clone());
    let mut quota = io();
    assert!(
        ValueReadContext {
            source: &source,
            memory: &other,
            class: MemoryClass::Ordinary,
            io: &mut quota
        }
        .load_root(&root.digest().unwrap())
        .is_err()
    );
    let source = BadSource(Arc::new(
        other
            .reserve(1, MemoryClass::Ordinary)
            .unwrap()
            .retain(root.canonical_bytes().unwrap()),
    ));
    let mut quota = io();
    assert!(
        ValueReadContext {
            source: &source,
            memory: &other,
            class: MemoryClass::Ordinary,
            io: &mut quota
        }
        .load_root(&root.digest().unwrap())
        .is_err()
    );
}

#[test]
fn operation_allowances_accept_exact_work_and_reject_one_less_in_each_dimension() {
    let data = vec![4; VALUE_CHUNK_BYTES + 7];
    let memory = MemoryBudget::new(8 * 1024 * 1024, 0).unwrap();
    let mut store = Store::new();
    let mut baseline = io();
    let root = write_value(
        &mut data.as_slice(),
        data.len() as u64,
        &mut store,
        &memory,
        &mut baseline,
        || false,
    )
    .unwrap();
    let exact = ValueIoBudget {
        nodes_remaining: u64::MAX - baseline.nodes_remaining,
        read_bytes_remaining: u64::MAX - baseline.read_bytes_remaining,
        write_bytes_remaining: u64::MAX - baseline.write_bytes_remaining,
    };
    for reduced in 0..4 {
        let mut quota = exact.clone();
        match reduced {
            1 => quota.nodes_remaining -= 1,
            2 => quota.read_bytes_remaining -= 1,
            3 => quota.write_bytes_remaining -= 1,
            _ => {}
        }
        let result = write_value(
            &mut data.as_slice(),
            data.len() as u64,
            &mut Store::new(),
            &memory,
            &mut quota,
            || false,
        );
        if reduced == 0 {
            assert_eq!(result.unwrap(), root);
        } else {
            assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
        }
    }
    let mut baseline = io();
    ValueReadContext {
        source: &store,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut baseline,
    }
    .read_range(
        &root,
        VALUE_CHUNK_BYTES as u64,
        7,
        &mut std::io::sink(),
        || false,
    )
    .unwrap();
    let exact = ValueIoBudget {
        nodes_remaining: u64::MAX - baseline.nodes_remaining,
        read_bytes_remaining: u64::MAX - baseline.read_bytes_remaining,
        write_bytes_remaining: 7,
    };
    for reduced in 0..4 {
        let mut quota = exact.clone();
        match reduced {
            1 => quota.nodes_remaining -= 1,
            2 => quota.read_bytes_remaining -= 1,
            3 => quota.write_bytes_remaining -= 1,
            _ => {}
        }
        let result = ValueReadContext {
            source: &store,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        }
        .read_range(
            &root,
            VALUE_CHUNK_BYTES as u64,
            7,
            &mut std::io::sink(),
            || false,
        );
        if reduced == 0 {
            assert_eq!(result.unwrap(), 7);
        } else {
            assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
        }
    }
}

#[test]
fn push_serializer_boundaries_match_pull_identity_and_pages() {
    for length in [
        0,
        1,
        VALUE_CHUNK_BYTES,
        VALUE_CHUNK_BYTES + 17,
        VALUE_CHUNK_BYTES * 129 + 7,
    ] {
        let data: Vec<_> = (0..length).map(|i| (i % 251) as u8).collect();
        let (expected, pull, memory) = build(&data);
        for step in [1, 4093, VALUE_CHUNK_BYTES + 3] {
            // Bytewise test need not repeat the multi-MiB case.
            if step == 1 && length > VALUE_CHUNK_BYTES + 17 {
                continue;
            }
            let mut push = Store::new();
            let mut quota = io();
            let mut writer =
                ValueStreamWriter::new(&mut push, &memory, &mut quota, length as u64, || false)
                    .unwrap();
            for chunk in data.chunks(step) {
                writer.write_all(chunk).unwrap();
                writer.flush().unwrap();
            }
            let root = writer.finish().unwrap();
            assert_eq!(root, expected);
            assert_eq!(
                push.nodes.keys().collect::<Vec<_>>(),
                pull.nodes.keys().collect::<Vec<_>>()
            );
            for (id, bytes) in &push.nodes {
                assert_eq!(bytes.as_slice(), pull.nodes[id].as_slice());
            }
        }
    }
}

#[test]
fn failed_push_cannot_finish_even_when_cause_disappears() {
    let memory = MemoryBudget::new(2 * 1024 * 1024, 256 * 1024).unwrap();
    for mode in 0..3 {
        let mut store = Store::new();
        let mut quota = io();
        if mode == 1 {
            quota.nodes_remaining = 0;
        }
        let cancel = Cell::new(false);
        let mut writer = ValueStreamWriter::new(
            &mut store,
            &memory,
            &mut quota,
            if mode == 1 {
                VALUE_CHUNK_BYTES as u64
            } else {
                1
            },
            || cancel.get(),
        )
        .unwrap();
        match mode {
            0 => assert!(writer.write_all(&[0; 2]).is_err()),
            1 => {
                // A full chunk requires node admission during Write.
                assert!(writer.write_all(&vec![0; VALUE_CHUNK_BYTES]).is_err());
            }
            _ => {
                cancel.set(true);
                assert!(writer.write_all(b"x").is_err());
                cancel.set(false);
            }
        }
        assert!(writer.finish().is_err());
        assert!(store.nodes.is_empty());
    }
    assert_eq!(memory.used(), 0);
}

#[test]
fn caught_cancellation_panic_does_not_restore_push_writer() {
    let memory = MemoryBudget::new(2 * 1024 * 1024, 0).unwrap();
    let mut store = Store::new();
    let mut quota = io();
    let panic_once = Cell::new(true);
    let mut writer = ValueStreamWriter::new(&mut store, &memory, &mut quota, 10, || {
        if panic_once.replace(false) {
            panic!("cancel hook panic");
        }
        false
    })
    .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| writer.write_all(b"x"))).is_err()
    );
    assert!(writer.finish().is_err());
    assert!(store.nodes.is_empty());
    assert_eq!(memory.used(), 0);
}

#[path = "incremental_value_closure_tests.rs"]
mod closure_tests;
