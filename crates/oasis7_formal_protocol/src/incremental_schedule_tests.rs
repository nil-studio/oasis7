use super::*;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

struct Charged(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Charged {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
struct Pages {
    memory: MemoryBudget,
    class: MemoryClass,
    nodes: BTreeMap<String, Vec<u8>>,
    reads: Cell<u64>,
    bytes: Cell<u64>,
    panic_at: Cell<Option<u64>>,
}
impl Pages {
    fn new() -> Self {
        Self {
            memory: MemoryBudget::new(8 * 1024 * 1024, 1024 * 1024).unwrap(),
            class: MemoryClass::Ordinary,
            nodes: BTreeMap::new(),
            reads: Cell::new(0),
            bytes: Cell::new(0),
            panic_at: Cell::new(None),
        }
    }
    fn persist(&mut self, delta: &ScheduleDeltaV1) {
        for page in &delta.new_nodes {
            page.verified_links().unwrap();
            self.nodes.insert(page.digest.clone(), page.bytes.clone());
        }
    }
    fn reset(&self) {
        self.reads.set(0);
        self.bytes.set(0);
    }
}
impl MerkleNodeReader for Pages {
    type Node = Charged;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.reads.set(self.reads.get() + 1);
        assert_ne!(
            self.panic_at.get(),
            Some(self.reads.get()),
            "injected source panic"
        );
        let Some(bytes) = self.nodes.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("test source allowance".into());
        }
        let charge = self
            .memory
            .reserve(bytes.len(), self.class)
            .map_err(|e| e.to_string())?;
        self.bytes.set(self.bytes.get() + bytes.len() as u64);
        Ok(Some(Charged(charge.retain(bytes.clone()))))
    }
}
fn allowance() -> ScheduleScanBudget {
    ScheduleScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 100_000_000,
        buckets_remaining: 1000,
        instances_remaining: 1000,
    }
}
fn due(
    pages: &Pages,
    root: &ScheduleIndexV1,
    now: u64,
    io: &mut ScheduleScanBudget,
) -> Result<Vec<(u64, String)>, ScheduleScanError<()>> {
    let mut output = Vec::new();
    let count = ScheduleScanContext {
        source: pages,
        memory: &pages.memory,
        class: pages.class,
        io,
    }
    .visit_due(
        root,
        now,
        |wake, instance| {
            output.push((wake, instance.to_owned()));
            Ok(())
        },
        || false,
    )?;
    assert_eq!(count, output.len() as u64);
    Ok(output)
}
fn change(
    pages: &mut Pages,
    root: &ScheduleIndexV1,
    id: &str,
    old: Option<u64>,
    new: Option<u64>,
) -> ScheduleIndexV1 {
    let delta = root
        .update(id, old, new, pages, &pages.memory, 256 * 1024)
        .unwrap();
    delta
        .verify(root.digest().unwrap(), pages, &pages.memory, 256 * 1024)
        .unwrap();
    let after = delta.after.clone();
    pages.persist(&delta);
    after
}
// A separate wire encoder and recursive constructor. It neither invokes the
// production codec nor the sparse Updater nor its empty sentinel table.
fn wire(kind: u8, fields: &[u8]) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.schedule-index-page.v1\0".to_vec();
    bytes.extend_from_slice(&((fields.len() + 1) as u32).to_be_bytes());
    bytes.push(kind);
    bytes.extend_from_slice(fields);
    bytes
}
fn remember(nodes: &mut BTreeMap<String, Vec<u8>>, bytes: Vec<u8>) -> String {
    let id = blake3::hash(&bytes).to_hex().to_string();
    nodes.insert(id.clone(), bytes);
    id
}
fn reference_tree(
    kind: u8,
    leaves: &BTreeMap<[u8; 32], Vec<u8>>,
    nodes: &mut BTreeMap<String, Vec<u8>>,
) -> String {
    fn build(
        kind: u8,
        depth: usize,
        leaves: &[([u8; 32], Vec<u8>)],
        empty: &[String],
        nodes: &mut BTreeMap<String, Vec<u8>>,
    ) -> String {
        if leaves.is_empty() {
            return empty[depth].clone();
        }
        if depth == 256 {
            assert_eq!(leaves.len(), 1);
            return remember(nodes, leaves[0].1.clone());
        }
        let mid = leaves.partition_point(|(key, _)| key[depth / 8] & (128 >> (depth % 8)) == 0);
        let left = build(kind, depth + 1, &leaves[..mid], empty, nodes);
        let right = build(kind, depth + 1, &leaves[mid..], empty, nodes);
        let mut fields = vec![1];
        fields.extend_from_slice(&(depth as u16).to_be_bytes());
        fields.extend(hex::decode(left).unwrap());
        fields.extend(hex::decode(right).unwrap());
        remember(nodes, wire(kind, &fields))
    }
    let mut empty = vec![String::new(); 257];
    empty[256] = blake3::hash(&wire(kind, &[2])).to_hex().to_string();
    for d in (0..256).rev() {
        let mut fields = vec![1];
        fields.extend_from_slice(&(d as u16).to_be_bytes());
        for _ in 0..2 {
            fields.extend(hex::decode(&empty[d + 1]).unwrap());
        }
        empty[d] = blake3::hash(&wire(kind, &fields)).to_hex().to_string();
    }
    build(
        kind,
        0,
        &leaves
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect::<Vec<_>>(),
        &empty,
        nodes,
    )
}
fn independent_key(id: &str) -> [u8; 32] {
    let mut bytes = b"oasis7.iws.schedule-instance.v1\0".to_vec();
    bytes.extend_from_slice(&(id.len() as u16).to_be_bytes());
    bytes.extend_from_slice(id.as_bytes());
    *blake3::hash(&bytes).as_bytes()
}
fn reference(
    map: &BTreeMap<u64, BTreeSet<String>>,
) -> (ScheduleIndexV1, BTreeMap<String, Vec<u8>>) {
    let mut nodes = BTreeMap::new();
    let mut times = BTreeMap::new();
    for (wake, ids) in map {
        let mut leaves = BTreeMap::new();
        for id in ids {
            let mut fields = vec![0];
            fields.extend_from_slice(&(id.len() as u16).to_be_bytes());
            fields.extend_from_slice(id.as_bytes());
            leaves.insert(independent_key(id), wire(2, &fields));
        }
        let bucket = reference_tree(2, &leaves, &mut nodes);
        let mut fields = vec![0];
        fields.extend_from_slice(&wake.to_be_bytes());
        fields.extend_from_slice(&(ids.len() as u64).to_be_bytes());
        fields.extend(hex::decode(bucket).unwrap());
        let mut key = [0; 32];
        key[..8].copy_from_slice(&wake.to_be_bytes());
        times.insert(key, wire(1, &fields));
    }
    let node_root = reference_tree(1, &times, &mut nodes);
    (
        ScheduleIndexV1 {
            buckets: map.len() as u64,
            entries: map.values().map(|s| s.len() as u64).sum(),
            node_root,
        },
        nodes,
    )
}

#[test]
fn updates_match_independent_full_tree_and_exact_final_new_pages() {
    let mut pages = Pages::new();
    let mut root = ScheduleIndexV1::empty().unwrap();
    let mut map: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for (id, old, new) in [
        ("a", None, Some(0)),
        ("b", None, Some(0)),
        ("c", None, Some(u64::MAX)),
        ("a", Some(0), Some(17)),
        ("b", Some(0), Some(17)),
        ("b", Some(17), Some(17)),
        ("c", Some(u64::MAX), None),
        ("a", Some(17), None),
        ("b", Some(17), None),
        ("absent", None, None),
        // All these pages already exist in historical CAS, but are absent
        // from the exact empty parent and must be emitted again.
        ("a", None, Some(0)),
    ] {
        let (_, before_nodes) = reference(&map);
        let delta = root
            .update(id, old, new, &pages, &pages.memory, 256 * 1024)
            .unwrap();
        if let Some(wake) = old {
            let bucket = map.get_mut(&wake).unwrap();
            assert!(bucket.remove(id));
            if bucket.is_empty() {
                map.remove(&wake);
            }
        }
        if let Some(wake) = new {
            map.entry(wake).or_default().insert(id.to_owned());
        }
        let (expected, nodes) = reference(&map);
        assert_eq!(delta.after, expected);
        let actual: BTreeMap<_, _> = delta
            .new_nodes
            .iter()
            .map(|p| (p.digest.clone(), p.bytes.clone()))
            .collect();
        assert_eq!(
            actual,
            nodes
                .into_iter()
                .filter(|(id, _)| !before_nodes.contains_key(id))
                .collect()
        );
        delta
            .verify(root.digest().unwrap(), &pages, &pages.memory, 256 * 1024)
            .unwrap();
        root = delta.after.clone();
        pages.persist(&delta);
        assert_eq!(
            ScheduleIndexV1::from_canonical_bytes(&root.canonical_bytes().unwrap()).unwrap(),
            root
        );
        let mut result = due(&pages, &root, u64::MAX, &mut allowance()).unwrap();
        result.sort();
        assert_eq!(
            result,
            map.iter()
                .flat_map(|(w, ids)| ids.iter().map(move |id| (*w, id.clone())))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(pages.memory.used(), 0);
}

#[test]
fn sorted_due_boundaries_and_future_growth_keep_requested_reads_fixed() {
    let mut pages = Pages::new();
    let mut root = ScheduleIndexV1::empty().unwrap();
    for (id, wake) in [
        ("zero", 0),
        ("same-a", 5),
        ("same-b", 5),
        ("later", 6),
        ("max", u64::MAX),
    ] {
        root = change(&mut pages, &root, id, None, Some(wake));
    }
    let old = root.clone();
    pages.reset();
    let expected = due(&pages, &root, 5, &mut allowance()).unwrap();
    let reads = pages.reads.get();
    assert_eq!(expected.len(), 3);
    assert_eq!(expected[0], (0, "zero".into()));
    assert_eq!(
        expected[1..]
            .iter()
            .map(|(_, id)| independent_key(id))
            .collect::<Vec<_>>(),
        {
            let mut keys = vec![independent_key("same-a"), independent_key("same-b")];
            keys.sort();
            keys
        }
    );
    for i in 0..12 {
        root = change(
            &mut pages,
            &root,
            &format!("future-{i}"),
            None,
            Some((1u64 << 63) + i),
        );
    }
    pages.reset();
    assert_eq!(due(&pages, &root, 5, &mut allowance()).unwrap(), expected);
    assert_eq!(pages.reads.get(), reads);
    assert_eq!(
        due(&pages, &root, 0, &mut allowance()).unwrap(),
        vec![(0, "zero".into())]
    );
    assert_eq!(
        due(&pages, &old, u64::MAX, &mut allowance()).unwrap().len(),
        5
    );
    assert_eq!(
        due(&pages, &root, u64::MAX, &mut allowance())
            .unwrap()
            .len(),
        17
    );
}

#[test]
fn exact_allowances_one_less_cancel_and_visitor_failure_fail_closed() {
    let mut pages = Pages::new();
    let root = change(
        &mut pages,
        &ScheduleIndexV1::empty().unwrap(),
        "only",
        None,
        Some(0),
    );
    pages.reset();
    due(&pages, &root, 0, &mut allowance()).unwrap();
    let reads = pages.reads.get();
    let bytes = pages.bytes.get();
    let exact = || ScheduleScanBudget {
        nodes_remaining: reads,
        read_bytes_remaining: bytes,
        buckets_remaining: 1,
        instances_remaining: 1,
    };
    let mut io = exact();
    due(&pages, &root, 0, &mut io).unwrap();
    assert_eq!(
        (
            io.nodes_remaining,
            io.read_bytes_remaining,
            io.buckets_remaining,
            io.instances_remaining
        ),
        (0, 0, 0, 0)
    );
    for dimension in 0..4 {
        let mut io = exact();
        match dimension {
            0 => io.nodes_remaining -= 1,
            1 => io.read_bytes_remaining -= 1,
            2 => io.buckets_remaining -= 1,
            _ => io.instances_remaining -= 1,
        };
        assert!(due(&pages, &root, 0, &mut io).is_err());
    }
    let mut io = allowance();
    let mut calls = 0;
    let result = ScheduleScanContext {
        source: &pages,
        memory: &pages.memory,
        class: pages.class,
        io: &mut io,
    }
    .visit_due(
        &root,
        0,
        |_, _| {
            calls += 1;
            Err("visitor")
        },
        || false,
    );
    assert!(matches!(result, Err(ScheduleScanError::Visitor("visitor"))));
    assert_eq!(calls, 1);
    let mut io = allowance();
    pages.reset();
    assert!(matches!(
        ScheduleScanContext {
            source: &pages,
            memory: &pages.memory,
            class: pages.class,
            io: &mut io
        }
        .visit_due::<()>(&root, 0, |_, _| Ok(()), || true),
        Err(ScheduleScanError::Cancelled)
    ));
    assert_eq!(pages.reads.get(), 0);
    let mut cancelled = false;
    let mut io = allowance();
    let cancel = Cell::new(false);
    assert!(matches!(
        ScheduleScanContext {
            source: &pages,
            memory: &pages.memory,
            class: pages.class,
            io: &mut io
        }
        .visit_due::<()>(
            &root,
            0,
            |_, _| {
                cancel.set(true);
                Ok(())
            },
            || {
                cancelled |= cancel.get();
                cancelled
            }
        ),
        Err(ScheduleScanError::Cancelled)
    ));
    assert_eq!(pages.memory.used(), 0);
}

#[test]
fn malformed_parent_pages_counts_and_wrong_kind_never_succeed() {
    let mut pages = Pages::new();
    let root = change(
        &mut pages,
        &ScheduleIndexV1::empty().unwrap(),
        "only",
        None,
        Some(0),
    );
    assert!(
        root.update("missing", Some(0), None, &pages, &pages.memory, 256 * 1024)
            .is_err()
    );
    assert!(
        root.update("only", None, Some(0), &pages, &pages.memory, 256 * 1024)
            .is_err()
    );
    assert!(
        root.update("only", Some(1), None, &pages, &pages.memory, 256 * 1024)
            .is_err()
    );
    assert!(
        root.update("only", Some(0), Some(1), &pages, &pages.memory, 0)
            .is_err()
    );
    let mut wrong = root.clone();
    wrong.entries += 1;
    assert!(due(&pages, &wrong, u64::MAX, &mut allowance()).is_err());
    wrong = root.clone();
    wrong.buckets += 1;
    wrong.entries += 1;
    assert!(due(&pages, &wrong, u64::MAX, &mut allowance()).is_err());
    let delta = root
        .update("next", None, Some(1), &pages, &pages.memory, 256 * 1024)
        .unwrap();
    let mut corrupted = (*delta).clone();
    corrupted.new_nodes.pop();
    assert!(
        corrupted
            .verify(root.digest().unwrap(), &pages, &pages.memory, 256 * 1024)
            .is_err()
    );
    let mut page = delta.new_nodes[0].clone();
    page.kind = match page.kind {
        SchedulePageKindV1::Times => SchedulePageKindV1::Instances,
        SchedulePageKindV1::Instances => SchedulePageKindV1::Times,
    };
    assert!(page.verified_links().is_err());
    drop(delta);
    let bytes = pages.nodes.remove(&root.node_root).unwrap();
    assert!(due(&pages, &root, 0, &mut allowance()).is_err());
    pages.nodes.insert(root.node_root.clone(), bytes);
    pages.nodes.get_mut(&root.node_root).unwrap()[0] ^= 1;
    assert!(due(&pages, &root, 0, &mut allowance()).is_err());
    assert_eq!(pages.memory.used(), 0);
}

#[test]
fn source_unwind_spends_allowance_and_pool_class_and_memory_are_enforced() {
    let mut pages = Pages::new();
    let root = change(
        &mut pages,
        &ScheduleIndexV1::empty().unwrap(),
        "only",
        None,
        Some(0),
    );
    let mut io = allowance();
    pages.reset();
    pages.panic_at.set(Some(1));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        due(&pages, &root, 0, &mut io)
    }));
    assert!(result.is_err());
    assert_eq!(io.nodes_remaining, 99_999);
    assert_eq!(io.read_bytes_remaining, 100_000_000 - 1200);
    assert_eq!(pages.memory.used(), 0);
    pages.panic_at.set(None);
    let other = MemoryBudget::new(8 * 1024 * 1024, 0).unwrap();
    assert!(
        root.update("x", None, Some(0), &pages, &other, 256 * 1024)
            .is_err()
    );
    assert!(
        ScheduleScanContext {
            source: &pages,
            memory: &other,
            class: pages.class,
            io: &mut allowance()
        }
        .visit_due::<()>(&root, 0, |_, _| Ok(()), || false)
        .is_err()
    );
    pages.class = MemoryClass::Recovery;
    assert!(
        root.update("x", None, Some(0), &pages, &pages.memory, 256 * 1024)
            .is_err()
    );
    assert_eq!(due(&pages, &root, 0, &mut allowance()).unwrap().len(), 1);
    let ordinary = pages
        .memory
        .reserve(7 * 1024 * 1024, MemoryClass::Ordinary)
        .unwrap();
    assert_eq!(due(&pages, &root, 0, &mut allowance()).unwrap().len(), 1);
    drop(ordinary);
    assert_eq!(pages.memory.used(), 0);
}

#[test]
fn wire_bounds_unicode_zero_max_and_root_rejection_are_explicit() {
    let mut pages = Pages::new();
    let id = "界".repeat(341) + "x";
    assert_eq!(id.len(), 1024);
    let root = change(
        &mut pages,
        &ScheduleIndexV1::empty().unwrap(),
        &id,
        None,
        Some(u64::MAX),
    );
    assert!(
        due(&pages, &root, u64::MAX - 1, &mut allowance())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        due(&pages, &root, u64::MAX, &mut allowance()).unwrap(),
        vec![(u64::MAX, id.clone())]
    );
    for id in [String::new(), "x".repeat(1025)] {
        assert!(
            root.update(&id, None, Some(0), &pages, &pages.memory, 256 * 1024)
                .is_err()
        );
    }
    let mut bytes = root.canonical_bytes().unwrap();
    bytes.push(0);
    assert!(ScheduleIndexV1::from_canonical_bytes(&bytes).is_err());
    let mut bytes = root.canonical_bytes().unwrap();
    bytes[0] ^= 1;
    assert!(ScheduleIndexV1::from_canonical_bytes(&bytes).is_err());
    assert_ne!(empty::<1>().unwrap(), empty::<2>().unwrap());
}

#[test]
fn missing_future_bucket_is_pruned_and_authenticated_wrong_leaf_paths_fail() {
    let mut pages = Pages::new();
    let root = change(
        &mut pages,
        &ScheduleIndexV1::empty().unwrap(),
        "early",
        None,
        Some(0),
    );
    let root = change(&mut pages, &root, "future", None, Some(u64::MAX));
    let mut view = Updater::<ScheduleCodec<1>, _>::new(&pages, empty::<1>().unwrap(), 0);
    let Some(Leaf::Time { root: missing, .. }) =
        view.lookup(&root.node_root, &time_key(u64::MAX)).unwrap()
    else {
        panic!()
    };
    drop(view);
    pages.nodes.remove(&missing);
    assert_eq!(
        due(&pages, &root, 0, &mut allowance()).unwrap(),
        vec![(0, "early".into())]
    );
    assert!(due(&pages, &root, u64::MAX, &mut allowance()).is_err());

    for wrong_path in [false, true] {
        let mut leaves = BTreeMap::new();
        let id = "one";
        let mut fields = vec![0];
        fields.extend_from_slice(&(id.len() as u16).to_be_bytes());
        fields.extend_from_slice(id.as_bytes());
        leaves.insert(independent_key(id), wire(2, &fields));
        let bucket = reference_tree(2, &leaves, &mut pages.nodes);
        let mut fields = vec![0];
        fields.extend_from_slice(&(u64::from(wrong_path)).to_be_bytes());
        fields.extend_from_slice(&(if wrong_path { 1u64 } else { 2 }).to_be_bytes());
        fields.extend(hex::decode(bucket).unwrap());
        // In the wrong-path case the leaf says time=1 but is placed at time=0.
        // In the other case the leaf count=2 disagrees with its real set of 1.
        let node_root = reference_tree(
            1,
            &BTreeMap::from([([0; 32], wire(1, &fields))]),
            &mut pages.nodes,
        );
        let root = ScheduleIndexV1 {
            buckets: 1,
            entries: if wrong_path { 1 } else { 2 },
            node_root,
        };
        assert!(due(&pages, &root, u64::MAX, &mut allowance()).is_err());
    }
}
