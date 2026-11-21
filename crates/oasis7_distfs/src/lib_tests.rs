use crate::{
    LocalCasStore, assemble_journal, assemble_snapshot, segment_journal, segment_snapshot,
};
use oasis7_proto::distributed_storage::SegmentConfig;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn cas_segmentation_exports_support_existing_distfs_callers() {
    // CAS owns the boundary cases and multi-segment behavior. Verify only
    // that callers can still assemble a snapshot and journal via DistFS.
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-distfs-cas-exports-{unique}"));
    let store = LocalCasStore::new(&dir);
    let snapshot = 42_u64;
    let events = [1_u64];
    let manifest = segment_snapshot(&snapshot, "w1", 1, &store, SegmentConfig::default())
        .expect("segment snapshot through the DistFS export");
    let segments = segment_journal(&events, &store, SegmentConfig::default(), |event| *event)
        .expect("segment journal through the DistFS export");
    let snapshot_loaded: u64 = assemble_snapshot(&manifest, &store).expect("assemble snapshot");
    let events_loaded: Vec<u64> =
        assemble_journal(&segments, &store, |event| *event).expect("assemble journal");

    let _ = fs::remove_dir_all(&dir);
    assert_eq!(snapshot_loaded, snapshot);
    assert_eq!(events_loaded.as_slice(), &events);
}
