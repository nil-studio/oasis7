use super::super::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct StorageFootprintFixture {
    pub root_dir: PathBuf,
    pub snapshot_size_bytes: u64,
    pub total_record_count: usize,
    pub archived_record_count: usize,
}

impl StorageFootprintFixture {
    pub(super) fn cleanup(self) {
        let _ = fs::remove_dir_all(self.root_dir);
    }
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-storage-footprint-{prefix}-{unique}"))
}

pub(super) fn build_storage_footprint_fixture(
    prefix: &str,
    tick_record_count: usize,
) -> StorageFootprintFixture {
    let mut world = World::new();
    for _ in 0..tick_record_count {
        world.step().expect("step");
    }

    let dir = temp_dir(prefix);
    world
        .save_to_dir(&dir)
        .expect("save world for storage footprint fixture");

    let snapshot_path = dir.join("snapshot.json");
    let snapshot_size_bytes = fs::metadata(snapshot_path.as_path())
        .expect("snapshot metadata")
        .len();
    let snapshot_json: serde_json::Value = serde_json::from_slice(
        &fs::read(snapshot_path.as_path()).expect("read persisted snapshot"),
    )
    .expect("decode persisted snapshot");
    let total_record_count = snapshot_json
        .get("tick_consensus_total_record_count")
        .and_then(|value| value.as_u64())
        .expect("tick_consensus_total_record_count") as usize;
    let archived_record_count = snapshot_json
        .get("tick_consensus_archived_record_count")
        .and_then(|value| value.as_u64())
        .expect("tick_consensus_archived_record_count") as usize;

    StorageFootprintFixture {
        root_dir: dir,
        snapshot_size_bytes,
        total_record_count,
        archived_record_count,
    }
}

#[test]
fn storage_footprint_fixture_baseline_covers_2500_ticks() {
    let fixture = build_storage_footprint_fixture("storage-footprint-baseline", 2_500);

    assert_eq!(fixture.total_record_count, 2_500);
    assert!(fixture.archived_record_count > 0);
    assert!(fixture.snapshot_size_bytes > 0);
    assert!(
        fixture
            .root_dir
            .join("tick-consensus.archive.index.json")
            .exists()
    );

    let restored = World::load_from_dir(&fixture.root_dir).expect("restore fixture world");
    assert_eq!(restored.tick_consensus_records().len(), 2_500);

    let range_records =
        World::load_tick_consensus_records_from_dir(&fixture.root_dir, Some(2_048), Some(2_500))
            .expect("load retained fixture range");
    assert!(!range_records.is_empty());
    assert!(
        range_records
            .iter()
            .all(|record| record.block.header.tick >= 2_048)
    );
    assert!(
        range_records
            .iter()
            .all(|record| record.block.header.tick <= 2_500)
    );

    fixture.cleanup();
}
