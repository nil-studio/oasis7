use super::super::*;
use oasis7_proto::storage_cold_index::StorageColdIndexRange;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-storage-cold-index-{prefix}-{unique}"))
}

#[test]
fn tick_consensus_archive_seek_matches_shared_range_semantics() {
    let mut world = World::new();
    for _ in 0..260 {
        world.step().expect("step");
    }

    let dir = temp_dir("tick-archive-seek");
    world
        .save_to_dir(&dir)
        .expect("save world with archive index");

    let snapshot_json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("snapshot.json")).expect("read snapshot json"))
            .expect("decode snapshot json");
    let archive_index: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("tick-consensus.archive.index.json"))
            .expect("read tick consensus archive index json"),
    )
    .expect("decode archive index json");

    let hot_range = StorageColdIndexRange {
        from_key: archive_index
            .get("hot_from_tick")
            .and_then(|value| value.as_u64())
            .expect("hot_from_tick"),
        to_key: archive_index
            .get("hot_to_tick")
            .and_then(|value| value.as_u64())
            .expect("hot_to_tick"),
    };
    assert_eq!(
        Some(hot_range.from_key),
        snapshot_json
            .get("tick_consensus_hot_from_tick")
            .and_then(|value| value.as_u64())
    );
    assert_eq!(
        Some(hot_range.to_key),
        snapshot_json
            .get("tick_consensus_hot_to_tick")
            .and_then(|value| value.as_u64())
    );
    assert!(hot_range.from_key <= hot_range.to_key);

    let archived_segments = archive_index
        .get("archived_segments")
        .and_then(|value| value.as_array())
        .expect("archived tick consensus segments");
    assert!(
        archived_segments.len() >= 2,
        "expected multiple archive segments"
    );

    let seek_from = archived_segments[0]
        .get("from_tick")
        .and_then(|value| value.as_u64())
        .expect("first segment from tick");
    let seek_to = archived_segments[1]
        .get("to_tick")
        .and_then(|value| value.as_u64())
        .expect("second segment to tick");
    let seek_records =
        World::load_tick_consensus_records_from_dir(&dir, Some(seek_from), Some(seek_to))
            .expect("load tick consensus seek range");
    assert_eq!(
        seek_records.first().map(|record| record.block.header.tick),
        Some(seek_from)
    );
    assert_eq!(
        seek_records.last().map(|record| record.block.header.tick),
        Some(seek_to)
    );

    let full_records = World::load_tick_consensus_records_from_dir(&dir, None, None)
        .expect("load full tick consensus records");
    assert_eq!(full_records, world.tick_consensus_records());

    let _ = fs::remove_dir_all(&dir);
}
