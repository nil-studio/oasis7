//! Actual candidate-root walks over verified v1/v2 delta overlays. The source
//! mimics a bounded remote content cache; prefetch validates the complete path.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::business_index::{
    BusinessIndexLeafKindV1, BusinessIndexNodeWriteV1,
};
use std::sync::atomic::{AtomicUsize, Ordering};

struct ColdParent {
    root: BusinessIndexRootV1,
    source: SharedMemoryNodes,
    cached: Mutex<BTreeMap<String, Vec<u8>>>,
    prefetches: AtomicUsize,
    uncached: AtomicUsize,
}
impl ColdParent {
    fn evict(&self) {
        self.cached.lock().unwrap().clear();
    }
}
impl BusinessIndexNodeReader for ColdParent {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        assert_eq!(root, &self.root, "unpublished overlay root must stay local");
        self.prefetches.fetch_add(1, Ordering::Relaxed);
        let path = root.path_nodes(key, &self.source)?;
        let mut cached = self.cached.lock().unwrap();
        for node in path.nodes {
            cached.insert(node.digest, node.bytes);
        }
        Ok(())
    }
    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.cached.lock().unwrap().get(digest).cloned())
    }
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.read_cached_node(digest)? {
            return Ok(Some(bytes));
        }
        self.uncached.fetch_add(1, Ordering::Relaxed);
        self.source.read_node(digest)
    }
}
fn key(prefix: u8) -> String {
    let mut bytes = [0; 32];
    bytes[0] = prefix;
    hex::encode(bytes)
}
fn leaf(prefix: u8) -> BusinessIndexLeafV1 {
    BusinessIndexLeafV1 {
        key_digest: key(prefix),
        kind: BusinessIndexLeafKindV1::Outcome,
        canonical_value: serde_cbor::to_vec(&prefix).unwrap(),
    }
}
fn persist(nodes: &SharedMemoryNodes, writes: &[BusinessIndexNodeWriteV1]) {
    let mut stored = nodes.0.lock().unwrap();
    for node in writes {
        stored.insert(node.digest.clone(), node.bytes.clone());
    }
}
fn fixture(
    version: u32,
) -> (
    BusinessIndexDeltaV1,
    Arc<ColdParent>,
    CandidateBusinessIndexReader,
) {
    let nodes = SharedMemoryNodes::default();
    let parent = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf(0), leaf(128)],
        &nodes,
    )
    .unwrap();
    persist(&nodes, &parent.node_writes);
    let delta = if version == 1 {
        BusinessIndexDeltaV1::build(parent.new.clone(), vec![leaf(64)], &nodes)
    } else {
        BusinessIndexDeltaV1::build_v2(parent.new.clone(), vec![leaf(64)], &nodes)
    }
    .unwrap();
    delta.verify(&delta.base_root, &nodes).unwrap();
    let source = Arc::new(ColdParent {
        root: parent.new.clone(),
        source: nodes,
        cached: Mutex::new(BTreeMap::new()),
        prefetches: AtomicUsize::new(0),
        uncached: AtomicUsize::new(0),
    });
    let overlay = CandidateBusinessIndexReader {
        base_root: parent.new,
        base: super::super::super::SharedFormalHistoryReader::new(source.clone()),
        node_writes: delta
            .node_writes
            .iter()
            .map(|node| (node.digest.clone(), node.bytes.clone()))
            .collect(),
    };
    (delta, source, overlay)
}
#[test]
fn candidate_overlay_prefetch_preserves_v1_v2_mixed_insert_and_absence_walks() {
    for version in [1, 2] {
        let (delta, source, overlay) = fixture(version);
        assert_eq!(delta.new.get(&key(64), &overlay).unwrap(), Some(leaf(64)));
        assert_eq!(
            source.prefetches.load(Ordering::Relaxed),
            0,
            "new path is entirely local"
        );
        assert_eq!(source.uncached.load(Ordering::Relaxed), 0);
        assert_eq!(delta.new.get(&key(0), &overlay).unwrap(), Some(leaf(0)));
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 1);
        assert_eq!(
            source.uncached.load(Ordering::Relaxed),
            0,
            "one parent batch warms all unchanged nodes"
        );
        // Simulate eviction by a bounded shared cache; warming must work again.
        source.evict();
        assert_eq!(delta.new.get(&key(0), &overlay).unwrap(), Some(leaf(0)));
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 2);
        assert_eq!(source.uncached.load(Ordering::Relaxed), 0);
        // A new candidate branch below an old empty subtree stays local.
        assert_eq!(delta.new.get(&key(96), &overlay).unwrap(), None);
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 2);
        // Absence reached through unchanged parent nodes uses a parent batch.
        source.evict();
        assert_eq!(delta.new.get(&key(192), &overlay).unwrap(), None);
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 3);
        assert_eq!(source.uncached.load(Ordering::Relaxed), 0);
        // A complete cached path also skips remote prefetch.
        assert_eq!(delta.new.get(&key(192), &overlay).unwrap(), None);
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 3);
    }
}
#[test]
fn candidate_overlay_prefetch_failure_does_not_authorize_or_reject_independent_path() {
    for version in [1, 2] {
        let (delta, source, overlay) = fixture(version);
        // Parent root was replaced by a candidate-local branch. Its missing
        // bytes break the hint, but are not part of the actual candidate path.
        source
            .source
            .0
            .lock()
            .unwrap()
            .remove(&source.root.node_root);
        assert_eq!(delta.new.get(&key(0), &overlay).unwrap(), Some(leaf(0)));
        assert_eq!(source.prefetches.load(Ordering::Relaxed), 1);
        assert!(source.uncached.load(Ordering::Relaxed) > 0);
        source
            .source
            .0
            .lock()
            .unwrap()
            .insert(source.root.node_root.clone(), vec![0xff]);
        assert_eq!(
            delta.new.get(&key(0), &overlay).unwrap(),
            Some(leaf(0)),
            "invalid replaced parent prefix is only a failed hint"
        );
        let hints = source.prefetches.load(Ordering::Relaxed);
        assert_eq!(delta.new.get(&key(64), &overlay).unwrap(), Some(leaf(64)));
        assert_eq!(
            source.prefetches.load(Ordering::Relaxed),
            hints,
            "all-local path needs no parent even when its prefix is lost"
        );
        // An actually needed unchanged node must still pass hash validation.
        let path = delta.new.path_nodes(&key(0), &overlay).unwrap();
        let needed = path
            .nodes
            .iter()
            .find(|node| !overlay.node_writes.contains_key(&node.digest))
            .unwrap();
        source.evict();
        source
            .source
            .0
            .lock()
            .unwrap()
            .insert(needed.digest.clone(), vec![0xff]);
        assert!(delta.new.get(&key(0), &overlay).is_err());
        source.source.0.lock().unwrap().remove(&needed.digest);
        assert!(delta.new.get(&key(0), &overlay).is_err());
    }
}
