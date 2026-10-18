//! Snapshot/journal segmentation helpers for distributed storage.

use oasis7_distfs as distfs;
use oasis7_proto::distributed::SnapshotManifest;

use super::blob_store::BlobStore;
use super::error::WorldError;
use super::snapshot::{Journal, Snapshot};

pub use oasis7_proto::distributed_storage::{JournalSegmentRef, SegmentConfig};

pub fn segment_snapshot(
    snapshot: &Snapshot,
    world_id: &str,
    epoch: u64,
    store: &impl BlobStore,
    config: SegmentConfig,
) -> Result<SnapshotManifest, WorldError> {
    Ok(distfs::segment_snapshot(
        snapshot, world_id, epoch, store, config,
    )?)
}

pub fn segment_journal(
    journal: &Journal,
    store: &impl BlobStore,
    config: SegmentConfig,
) -> Result<Vec<JournalSegmentRef>, WorldError> {
    Ok(distfs::segment_journal(
        &journal.events,
        store,
        config,
        |event| event.id,
    )?)
}

#[cfg(test)]
mod tests {
    use super::super::{Action, LocalCasStore, World};
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-segmenter-{prefix}-{unique}"))
    }

    #[test]
    fn segment_snapshot_writes_chunks() {
        let dir = temp_dir("snapshot-seg");
        let store = LocalCasStore::new(&dir);
        let world = World::new();
        let snapshot = world.snapshot();

        let manifest = segment_snapshot(
            &snapshot,
            "w1",
            1,
            &store,
            SegmentConfig {
                snapshot_chunk_bytes: 64,
                ..SegmentConfig::default()
            },
        )
        .expect("segment snapshot");

        assert_eq!(manifest.world_id, "w1");
        assert_eq!(manifest.epoch, 1);
        assert!(!manifest.chunks.is_empty());
        for chunk in &manifest.chunks {
            assert!(store.has(&chunk.content_hash).expect("has chunk"));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn segment_journal_splits_by_event_count() {
        let dir = temp_dir("journal-seg");
        let store = LocalCasStore::new(&dir);
        let mut world = World::new();

        world.submit_action(Action::RegisterAgent {
            agent_id: "agent-1".to_string(),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
        world.step().expect("step world");
        world.submit_action(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: oasis7_agent_api::world_action::GeoPos::new(1, 1, 0),
        });
        world.step().expect("step world");

        let journal = world.journal().clone();
        let segments = segment_journal(
            &journal,
            &store,
            SegmentConfig {
                journal_events_per_segment: 1,
                ..SegmentConfig::default()
            },
        )
        .expect("segment journal");

        assert_eq!(segments.len(), journal.len());
        for segment in segments {
            assert!(store.has(&segment.content_hash).expect("has segment"));
        }

        let _ = fs::remove_dir_all(&dir);
    }
}
