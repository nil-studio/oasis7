use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct MemoryReader {
    nodes: Mutex<BTreeMap<String, Vec<u8>>>,
    reads: AtomicUsize,
    cached_reads: AtomicUsize,
    prefetches: AtomicUsize,
    cache_writes: AtomicUsize,
}

impl MemoryReader {
    fn insert(&self, bytes: Vec<u8>) -> String {
        let digest = blake3_hex(&bytes);
        self.nodes.lock().unwrap().insert(digest.clone(), bytes);
        digest
    }

    fn set(&self, digest: String, bytes: Vec<u8>) {
        self.nodes.lock().unwrap().insert(digest, bytes);
    }
}

impl BusinessIndexNodeReader for MemoryReader {
    fn prefetch_path(&self, _: &BusinessIndexRootV1, _: &str) -> Result<(), String> {
        self.prefetches.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.nodes.lock().unwrap().get(digest).cloned())
    }

    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.cached_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.nodes.lock().unwrap().get(digest).cloned())
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.cache_writes.fetch_add(1, Ordering::SeqCst);
        self.nodes
            .lock()
            .unwrap()
            .insert(digest.to_owned(), bytes.to_vec());
        Ok(())
    }
}

fn seeded(bytes: &[u8]) -> (String, Vec<u8>) {
    (blake3_hex(bytes), bytes.to_vec())
}

#[test]
fn strict_seeded_reader_works_without_external_nodes_or_prefetch() {
    let external = Arc::new(MemoryReader::default());
    let node = seeded(b"retained H node");
    let missing = blake3_hex(b"not present");
    let reader =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), vec![node.clone()]).unwrap();

    assert_eq!(reader.read_node(&node.0).unwrap(), Some(node.1.clone()));
    assert_eq!(
        reader.read_cached_node(&node.0).unwrap(),
        Some(node.1.clone())
    );
    assert_eq!(reader.read_node(&missing).unwrap(), None);
    reader
        .prefetch_path(&BusinessIndexRootV1::empty().unwrap(), "00")
        .unwrap();
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.cached_reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.prefetches.load(Ordering::SeqCst), 0);
    assert_eq!(reader.recorded_nodes().unwrap(), vec![node]);
}

#[test]
fn external_nodes_are_hash_checked_recorded_once_and_preferred_afterward() {
    let external = Arc::new(MemoryReader::default());
    let bytes = b"external H dependency".to_vec();
    let digest = external.insert(bytes.clone());
    let reader = RecordingBusinessIndexReaderV1::new(external.clone(), Vec::new()).unwrap();

    assert_eq!(reader.read_node(&digest).unwrap(), Some(bytes.clone()));
    external.set(digest.clone(), b"changed external value".to_vec());
    assert_eq!(reader.read_node(&digest).unwrap(), Some(bytes.clone()));
    assert_eq!(
        reader.read_cached_node(&digest).unwrap(),
        Some(bytes.clone())
    );
    assert_eq!(external.reads.load(Ordering::SeqCst), 1);
    assert_eq!(external.cached_reads.load(Ordering::SeqCst), 0);
    assert_eq!(
        reader.recorded_nodes().unwrap(),
        vec![(digest.clone(), bytes)]
    );
    assert_eq!(reader.recorded_refs().unwrap(), vec![digest]);
}

#[test]
fn seed_and_external_hash_mismatches_are_rejected_without_recording() {
    let external = Arc::new(MemoryReader::default());
    assert!(
        RecordingBusinessIndexReaderV1::new(
            external.clone(),
            vec![(blake3_hex(b"different"), b"seed".to_vec())],
        )
        .is_err()
    );

    let actual = b"actual external bytes".to_vec();
    let requested_digest = blake3_hex(b"different bytes");
    external.set(requested_digest.clone(), actual);
    let reader = RecordingBusinessIndexReaderV1::new(external, Vec::new()).unwrap();
    assert!(reader.read_node(&requested_digest).is_err());
    assert!(reader.recorded_nodes().unwrap().is_empty());
}

#[test]
fn invalid_digest_length_and_oversized_nodes_are_rejected() {
    let external = Arc::new(MemoryReader::default());
    let reader = RecordingBusinessIndexReaderV1::new(external.clone(), Vec::new()).unwrap();
    assert!(reader.read_node("00").is_err());

    let oversized = vec![7; BUSINESS_INDEX_MAX_NODE_BYTES + 1];
    let digest = blake3_hex(&oversized);
    assert!(RecordingBusinessIndexReaderV1::new(external, vec![(digest, oversized)]).is_err());
}

#[test]
fn seed_deduplicates_and_returns_canonical_digest_order() {
    let external = Arc::new(MemoryReader::default());
    let first = seeded(b"first");
    let second = seeded(b"second");
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    let reader = RecordingBusinessIndexReaderV1::new(
        external,
        vec![second.clone(), first.clone(), second.clone()],
    )
    .unwrap();

    assert_eq!(reader.recorded_nodes().unwrap(), expected);
    assert_eq!(
        reader.recorded_refs().unwrap(),
        expected.iter().map(|v| v.0.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn node_count_and_aggregate_byte_limits_are_enforced() {
    let external = Arc::new(MemoryReader::default());
    let too_many = (0..=MAX_RETAINED_INDEX_NODES)
        .map(|index| seeded(&index.to_be_bytes()))
        .collect();
    assert!(RecordingBusinessIndexReaderV1::new(external.clone(), too_many).is_err());

    let node_bytes = BUSINESS_INDEX_MAX_LEAF_BYTES;
    let count = MAX_RETAINED_INDEX_TOTAL_BYTES / node_bytes + 1;
    let too_large = (0..count)
        .map(|index| {
            let byte = u8::try_from(index).expect("test index fits in one byte");
            let bytes = vec![byte; node_bytes];
            seeded(bytes.as_slice())
        })
        .collect();
    assert!(RecordingBusinessIndexReaderV1::new(external, too_large).is_err());
}

#[test]
fn cache_writes_are_local_and_strict_replay_rejects_new_nodes() {
    let external = Arc::new(MemoryReader::default());
    let reader = RecordingBusinessIndexReaderV1::new(external.clone(), Vec::new()).unwrap();
    let bytes = b"candidate tree node";
    let digest = blake3_hex(bytes);
    reader.cache_content_node(&digest, bytes).unwrap();
    assert_eq!(
        reader.read_cached_node(&digest).unwrap(),
        Some(bytes.to_vec())
    );
    assert_eq!(external.cache_writes.load(Ordering::SeqCst), 0);

    let strict = RecordingBusinessIndexReaderV1::new_strict(external.clone(), Vec::new()).unwrap();
    assert!(strict.cache_content_node(&digest, bytes).is_err());
    assert_eq!(external.cache_writes.load(Ordering::SeqCst), 0);
}

#[test]
fn replay_accesses_exclude_unvisited_preloaded_suffix_nodes() {
    let first = seeded(b"H path actually read");
    let future = seeded(b"later prefix node not yet used");
    let generated = seeded(b"actual new result node");
    let reader = RecordingBusinessIndexReaderV1::new(
        Arc::new(MemoryReader::default()),
        vec![first.clone(), future.clone()],
    )
    .unwrap();
    assert!(reader.accessed_nodes().unwrap().is_empty());
    assert_eq!(reader.read_node(&first.0).unwrap(), Some(first.1.clone()));
    reader
        .cache_content_node(&generated.0, &generated.1)
        .unwrap();
    assert_eq!(
        reader.accessed_nodes().unwrap(),
        BTreeMap::from([first, generated])
    );
    assert!(reader.recorded_refs().unwrap().contains(&future.0));
}

#[test]
fn closed_layer_reads_verified_parent_lazily_with_its_own_node_budget() {
    let external = Arc::new(MemoryReader::default());
    let parent_nodes = (0..MAX_RETAINED_INDEX_NODES)
        .map(|index| seeded(&index.to_be_bytes()))
        .collect::<Vec<_>>();
    let expected = parent_nodes[0].clone();
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), parent_nodes).unwrap();
    let child = RecordingBusinessIndexReaderV1::new_closed_layer(parent, Vec::new()).unwrap();
    assert!(child.recorded_nodes().unwrap().is_empty());
    assert_eq!(
        child.read_node(&expected.0).unwrap(),
        Some(expected.1.clone())
    );
    assert_eq!(child.recorded_nodes().unwrap(), vec![expected]);
    assert_eq!(
        child.read_node(&blake3_hex(b"unverified missing")).unwrap(),
        None
    );
    assert!(
        child
            .cache_content_node(&blake3_hex(b"unknown derived"), b"unknown derived")
            .is_err()
    );
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.cached_reads.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_layer_rejects_an_open_parent_even_when_it_currently_has_seeded_nodes() {
    let external = Arc::new(MemoryReader::default());
    let parent =
        RecordingBusinessIndexReaderV1::new(external.clone(), vec![seeded(b"known")]).unwrap();
    assert!(RecordingBusinessIndexReaderV1::new_closed_layer(parent, Vec::new()).is_err());
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_layer_missing_nodes_never_fall_back_to_the_ancestors_external_reader() {
    let external = Arc::new(MemoryReader::default());
    let digest = external.insert(b"external-only forbidden bytes".to_vec());
    let retained = seeded(b"verified ancestor bytes");
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), vec![retained.clone()])
            .unwrap();
    let child =
        RecordingBusinessIndexReaderV1::new_closed_layer(parent.clone(), Vec::new()).unwrap();
    assert_eq!(child.read_node(&digest).unwrap(), None);
    assert_eq!(child.read_cached_node(&digest).unwrap(), None);
    child.cache_content_node(&retained.0, &retained.1).unwrap();
    assert_eq!(child.recorded_nodes().unwrap(), vec![retained]);
    assert!(parent.accessed_nodes().unwrap().is_empty());
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.cached_reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.cache_writes.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_layer_parent_reads_cannot_exceed_the_child_node_limit() {
    let external = Arc::new(MemoryReader::default());
    let extra = seeded(b"one beyond current segment capacity");
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), vec![extra.clone()]).unwrap();
    let seed = (0..MAX_RETAINED_INDEX_NODES)
        .map(|index| seeded(&index.to_be_bytes()))
        .collect();
    let child = RecordingBusinessIndexReaderV1::new_closed_layer(parent, seed).unwrap();
    assert!(child.read_node(&extra.0).is_err());
    assert!(child.cache_content_node(&extra.0, &extra.1).is_err());
    assert_eq!(
        child.recorded_nodes().unwrap().len(),
        MAX_RETAINED_INDEX_NODES
    );
    assert!(child.accessed_nodes().unwrap().is_empty());
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_layer_chain_depth_is_bounded_independently_of_node_count() {
    let mut parent =
        RecordingBusinessIndexReaderV1::new_strict(Arc::new(MemoryReader::default()), Vec::new())
            .unwrap();
    for _ in 0..64 {
        parent = RecordingBusinessIndexReaderV1::new_closed_layer(parent, Vec::new()).unwrap();
    }
    assert_eq!(
        parent
            .read_node(&blake3_hex(b"missing at deepest ancestor"))
            .unwrap(),
        None
    );
    assert!(RecordingBusinessIndexReaderV1::new_closed_layer(parent, Vec::new()).is_err());
}

#[test]
fn closed_layer_parent_reads_cannot_exceed_the_child_byte_limit() {
    let external = Arc::new(MemoryReader::default());
    let extra = seeded(b"beyond current segment bytes");
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), vec![extra.clone()]).unwrap();
    let mut remaining = MAX_RETAINED_INDEX_TOTAL_BYTES;
    let mut seed = Vec::new();
    while remaining > 0 {
        let length = remaining.min(BUSINESS_INDEX_MAX_NODE_BYTES);
        let mut bytes = vec![0; length];
        bytes[..8].copy_from_slice(&seed.len().to_be_bytes());
        seed.push(seeded(&bytes));
        remaining -= length;
    }
    assert!(seed.len() < MAX_RETAINED_INDEX_NODES);
    let child = RecordingBusinessIndexReaderV1::new_closed_layer(parent, seed).unwrap();
    assert!(child.read_node(&extra.0).is_err());
    assert!(child.cache_content_node(&extra.0, &extra.1).is_err());
    assert!(child.accessed_nodes().unwrap().is_empty());
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
}

#[test]
fn closed_layer_live_derivation_keeps_parent_reads_closed_and_seals_for_replay() {
    let external = Arc::new(MemoryReader::default());
    let inherited = seeded(b"original verified parent dependency");
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(external.clone(), vec![inherited.clone()])
            .unwrap();
    let current = RecordingBusinessIndexReaderV1::new_closed_layer(parent, Vec::new()).unwrap();
    let live = current.live_closed_layer().unwrap();
    assert_eq!(
        live.read_node(&inherited.0).unwrap(),
        Some(inherited.1.clone())
    );
    let derived = seeded(b"actual locally derived node");
    live.cache_content_node(&derived.0, &derived.1).unwrap();
    assert!(RecordingBusinessIndexReaderV1::new_closed_layer(live.clone(), Vec::new()).is_err());
    let replay = live
        .replay_current_layer(live.recorded_nodes().unwrap())
        .unwrap();
    replay.cache_content_node(&derived.0, &derived.1).unwrap();
    assert_eq!(replay.read_node(&inherited.0).unwrap(), Some(inherited.1));
    let unknown = seeded(b"not derived by original execution");
    assert!(replay.cache_content_node(&unknown.0, &unknown.1).is_err());
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
    assert_eq!(external.cache_writes.load(Ordering::SeqCst), 0);
    // Repeated live/replay cycles within a segment do not consume archive depth.
    let mut replay = replay;
    for _ in 0..128 {
        let live = replay.live_closed_layer().unwrap();
        replay = live
            .replay_current_layer(live.recorded_nodes().unwrap())
            .unwrap();
    }
}

#[test]
fn live_preview_forks_keep_parent_accesses_and_isolate_failed_candidate_nodes() {
    let external = Arc::new(MemoryReader::default());
    let first = seeded(b"verified parent read");
    let unvisited = seeded(b"preloaded but unvisited");
    let later = seeded(b"node produced only by failed alternative");
    let missing = external.insert(b"unrecorded external node".to_vec());
    let parent = RecordingBusinessIndexReaderV1::new_strict(
        external.clone(),
        vec![first.clone(), unvisited.clone()],
    )
    .unwrap();
    parent.read_node(&first.0).unwrap();
    let baseline = parent.accessed_nodes().unwrap();
    let a = parent.fork_live_preview().unwrap();
    a.read_node(&unvisited.0).unwrap();
    a.cache_content_node(&later.0, &later.1).unwrap();
    assert_eq!(a.read_node(&missing).unwrap(), None);
    assert_eq!(external.reads.load(Ordering::SeqCst), 0);
    assert_eq!(parent.accessed_nodes().unwrap(), baseline);
    let b = parent.fork_live_preview().unwrap();
    assert_eq!(b.accessed_nodes().unwrap(), baseline);
    assert_eq!(b.read_node(&later.0).unwrap(), None);
    assert_eq!(
        b.recorded_nodes().unwrap(),
        parent.recorded_nodes().unwrap()
    );
    assert!(a.cache_content_node(&first.0, b"foreign bytes").is_err());
    assert_eq!(b.read_node(&first.0).unwrap(), Some(first.1));
}

#[test]
fn live_preview_fork_preserves_closed_parent_without_mutating_ancestor() {
    let ancestor_node = seeded(b"authenticated ancestor");
    let local_node = seeded(b"current segment");
    let ancestor = RecordingBusinessIndexReaderV1::new_strict(
        Arc::new(MemoryReader::default()),
        vec![ancestor_node.clone()],
    )
    .unwrap();
    let parent = RecordingBusinessIndexReaderV1::new_closed_layer(
        ancestor.clone(),
        vec![local_node.clone()],
    )
    .unwrap();
    let fork = parent.fork_live_preview().unwrap();
    assert_eq!(
        fork.read_node(&ancestor_node.0).unwrap(),
        Some(ancestor_node.1)
    );
    assert!(ancestor.accessed_nodes().unwrap().is_empty());
    assert!(parent.accessed_nodes().unwrap().is_empty());
    assert_eq!(parent.recorded_nodes().unwrap(), vec![local_node]);
    assert!(
        parent
            .fork_live_preview()
            .unwrap()
            .accessed_nodes()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn derived_write_capacity_plan_includes_unread_writes_without_partial_mutation() {
    let seed = (0..MAX_RETAINED_INDEX_NODES - 1)
        .map(|n| seeded(format!("actual bounded content {n}").as_bytes()))
        .collect::<Vec<_>>();
    let parent =
        RecordingBusinessIndexReaderV1::new_strict(Arc::new(MemoryReader::default()), seed.clone())
            .unwrap();
    let first = seeded(b"first derived write");
    let second = seeded(b"second derived write");
    assert!(matches!(
        parent
            .plan_content_nodes(&[first.clone(), first.clone()])
            .unwrap(),
        ContentNodeCapacityV1::Fits {
            nodes: MAX_RETAINED_INDEX_NODES,
            ..
        }
    ));
    assert!(
        matches!(parent.plan_content_nodes(&[first, second]).unwrap(),
        ContentNodeCapacityV1::Exceeded { nodes, .. } if nodes == MAX_RETAINED_INDEX_NODES + 1)
    );
    assert_eq!(parent.recorded_nodes().unwrap(), {
        let mut ordered = seed;
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
        ordered
    });
    assert!(parent.accessed_nodes().unwrap().is_empty());
}
