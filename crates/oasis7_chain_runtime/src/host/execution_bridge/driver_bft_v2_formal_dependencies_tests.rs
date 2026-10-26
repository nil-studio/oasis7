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
