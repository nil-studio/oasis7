//! Concrete state codecs over the byte-preserving CAS segmentation protocol.
use crate::{Snapshot, WorldEvent};
use oasis7_cas::BlobStore;
use oasis7_proto::distributed::SnapshotManifest;
use oasis7_proto::distributed_storage::{JournalSegmentRef, SegmentConfig};
use oasis7_proto::world_error::WorldError;

// Keep the Serde instantiations here, independent of the consumer's store type.
struct Store<'a>(&'a dyn BlobStore);
impl BlobStore for Store<'_> {
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), WorldError> {
        self.0.put(hash, bytes)
    }
    fn get(&self, hash: &str) -> Result<Vec<u8>, WorldError> {
        self.0.get(hash)
    }
    fn has(&self, hash: &str) -> Result<bool, WorldError> {
        self.0.has(hash)
    }
    fn put_bytes(&self, bytes: &[u8]) -> Result<String, WorldError> {
        self.0.put_bytes(bytes)
    }
}

pub fn segment_snapshot(
    snapshot: &Snapshot,
    world_id: &str,
    epoch: u64,
    store: &dyn BlobStore,
    config: SegmentConfig,
) -> Result<SnapshotManifest, WorldError> {
    oasis7_cas::segment_snapshot(snapshot, world_id, epoch, &Store(store), config)
}
/// Journal segments encode event arrays, not complete Journal objects.
pub fn segment_journal(
    events: &[WorldEvent],
    store: &dyn BlobStore,
    config: SegmentConfig,
) -> Result<Vec<JournalSegmentRef>, WorldError> {
    oasis7_cas::segment_journal(events, &Store(store), config, |event| event.id)
}
pub fn assemble_snapshot(
    manifest: &SnapshotManifest,
    store: &dyn BlobStore,
) -> Result<Snapshot, WorldError> {
    oasis7_cas::assemble_snapshot(manifest, &Store(store))
}
pub fn assemble_journal(
    segments: &[JournalSegmentRef],
    store: &dyn BlobStore,
) -> Result<Vec<WorldEvent>, WorldError> {
    oasis7_cas::assemble_journal(segments, &Store(store), |event: &WorldEvent| event.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::BTreeMap};
    #[derive(Default)]
    struct MemoryStore(RefCell<BTreeMap<String, Vec<u8>>>);
    impl BlobStore for MemoryStore {
        fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), WorldError> {
            self.0.borrow_mut().insert(hash.into(), bytes.into());
            Ok(())
        }
        fn get(&self, hash: &str) -> Result<Vec<u8>, WorldError> {
            self.0
                .borrow()
                .get(hash)
                .cloned()
                .ok_or_else(|| WorldError::BlobNotFound {
                    content_hash: hash.into(),
                })
        }
        fn has(&self, hash: &str) -> Result<bool, WorldError> {
            Ok(self.0.borrow().contains_key(hash))
        }
    }
    #[test]
    fn concrete_segments_preserve_store_put_bytes_override() {
        struct RejectWrites;
        impl BlobStore for RejectWrites {
            fn put(&self, _: &str, _: &[u8]) -> Result<(), WorldError> {
                panic!("put_bytes override must be used")
            }
            fn get(&self, hash: &str) -> Result<Vec<u8>, WorldError> {
                Err(WorldError::BlobNotFound {
                    content_hash: hash.into(),
                })
            }
            fn has(&self, _: &str) -> Result<bool, WorldError> {
                Ok(false)
            }
            fn put_bytes(&self, _: &[u8]) -> Result<String, WorldError> {
                Err(WorldError::DistributedValidationFailed {
                    reason: "store write rejected".into(),
                })
            }
        }
        let snapshot =
            crate::codec::snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let journal =
            crate::codec::journal_from_json(include_bytes!("../fixtures/journal.json")).unwrap();
        let config = SegmentConfig::default();
        assert_eq!(
            segment_snapshot(&snapshot, "fixture", 7, &RejectWrites, config),
            oasis7_cas::segment_snapshot(&snapshot, "fixture", 7, &RejectWrites, config)
        );
        assert_eq!(
            segment_journal(&journal.events, &RejectWrites, config),
            oasis7_cas::segment_journal(&journal.events, &RejectWrites, config, |e| e.id)
        );
    }
    #[test]
    fn concrete_segments_preserve_bytes_manifests_and_validation_errors() {
        let snapshot =
            crate::codec::snapshot_from_json(include_bytes!("../fixtures/snapshot.json")).unwrap();
        let journal =
            crate::codec::journal_from_json(include_bytes!("../fixtures/journal.json")).unwrap();
        let concrete = MemoryStore::default();
        let legacy = MemoryStore::default();
        let config = SegmentConfig {
            snapshot_chunk_bytes: 64,
            journal_events_per_segment: 1,
        };
        let manifest = segment_snapshot(&snapshot, "fixture", 7, &concrete, config).unwrap();
        let expected =
            oasis7_cas::segment_snapshot(&snapshot, "fixture", 7, &legacy, config).unwrap();
        assert_eq!(manifest, expected);
        let segments = segment_journal(&journal.events, &concrete, config).unwrap();
        let expected =
            oasis7_cas::segment_journal(&journal.events, &legacy, config, |e| e.id).unwrap();
        assert_eq!(segments, expected);
        assert_eq!(*concrete.0.borrow(), *legacy.0.borrow());
        assert_eq!(assemble_snapshot(&manifest, &concrete).unwrap(), snapshot);
        assert_eq!(
            assemble_journal(&segments, &concrete).unwrap(),
            journal.events
        );
        for segment in &segments {
            let bytes = concrete.get(&segment.content_hash).unwrap();
            assert!(serde_cbor::from_slice::<Vec<WorldEvent>>(&bytes).is_ok());
            assert!(crate::codec::journal_from_cbor(&bytes).is_err());
        }
        let first = &manifest.chunks[0].content_hash;
        concrete.0.borrow_mut().insert(first.clone(), vec![0xff]);
        assert_eq!(
            assemble_snapshot(&manifest, &concrete),
            oasis7_cas::assemble_snapshot::<Snapshot>(&manifest, &concrete)
        );
        let first = &segments[0].content_hash;
        concrete.0.borrow_mut().insert(first.clone(), vec![0xff]);
        assert_eq!(
            assemble_journal(&segments, &concrete),
            oasis7_cas::assemble_journal::<WorldEvent>(&segments, &concrete, |e| e.id)
        );
    }
}
