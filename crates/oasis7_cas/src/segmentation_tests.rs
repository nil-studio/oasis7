use super::*;
use oasis7_proto::distributed_storage::SegmentConfig;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DemoEvent {
    id: u64,
    kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DemoSnapshot {
    tick: u64,
    world: String,
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-cas-segmentation-{prefix}-{unique}"))
}

#[test]
fn segment_and_assemble_roundtrip() {
    let dir = temp_dir("segment");
    let store = LocalCasStore::new(&dir);
    let snapshot = DemoSnapshot {
        tick: 42,
        world: "w1".to_string(),
    };
    let events = vec![
        DemoEvent {
            id: 1,
            kind: "a".to_string(),
        },
        DemoEvent {
            id: 2,
            kind: "b".to_string(),
        },
        DemoEvent {
            id: 3,
            kind: "c".to_string(),
        },
    ];

    let manifest = segment_snapshot(
        &snapshot,
        "w1",
        1,
        &store,
        SegmentConfig {
            snapshot_chunk_bytes: 8,
            ..SegmentConfig::default()
        },
    )
    .expect("segment snapshot");

    let segments = segment_journal(
        &events,
        &store,
        SegmentConfig {
            journal_events_per_segment: 2,
            ..SegmentConfig::default()
        },
        |event| event.id,
    )
    .expect("segment journal");

    let snapshot_loaded: DemoSnapshot =
        assemble_snapshot(&manifest, &store).expect("assemble snapshot");
    let events_loaded: Vec<DemoEvent> =
        assemble_journal(&segments, &store, |event: &DemoEvent| event.id)
            .expect("assemble journal");

    assert_eq!(snapshot_loaded, snapshot);
    assert_eq!(events_loaded, events);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn segmentation_preserves_canonical_bytes_and_validation_errors() {
    let dir = temp_dir("canonical");
    let store = LocalCasStore::new(&dir);
    let snapshot = DemoSnapshot {
        tick: 42,
        world: "w1".into(),
    };
    let bytes = to_canonical_cbor(&snapshot).unwrap();
    let mut manifest = segment_snapshot(
        &snapshot,
        "w1",
        1,
        &store,
        SegmentConfig {
            snapshot_chunk_bytes: 0,
            ..SegmentConfig::default()
        },
    )
    .unwrap();
    assert_eq!(manifest.state_root, blake3_hex(&bytes));
    assert_eq!(manifest.chunks.len(), bytes.len());
    for (chunk, expected) in manifest.chunks.iter().zip(&bytes) {
        assert_eq!(chunk.size_bytes, 1);
        assert_eq!(store.get(&chunk.content_hash).unwrap(), [*expected]);
        assert_eq!(chunk.content_hash, blake3_hex(&[*expected]));
    }
    manifest.state_root = "incorrect".into();
    let error = assemble_snapshot::<DemoSnapshot>(&manifest, &store).unwrap_err();
    match error {
        WorldError::DistributedValidationFailed { reason } => assert_eq!(
            reason,
            format!(
                "snapshot state_root mismatch: expected=incorrect, actual={}",
                blake3_hex(&bytes)
            )
        ),
        other => panic!("unexpected error: {other:?}"),
    }
    let events = [
        DemoEvent {
            id: 1,
            kind: "a".into(),
        },
        DemoEvent {
            id: 3,
            kind: "b".into(),
        },
    ];
    let segments = segment_journal(
        &events,
        &store,
        SegmentConfig {
            journal_events_per_segment: 0,
            ..SegmentConfig::default()
        },
        |event| event.id,
    )
    .unwrap();
    for (segment, event) in segments.iter().zip(&events) {
        let expected = to_canonical_cbor(&[event]).unwrap();
        assert_eq!(store.get(&segment.content_hash).unwrap(), expected);
        assert_eq!(segment.content_hash, blake3_hex(&expected));
    }
    match assemble_journal::<DemoEvent>(&segments, &store, |event| event.id).unwrap_err() {
        WorldError::DistributedValidationFailed { reason } => {
            assert_eq!(reason, "journal discontinuity: expected=2, got=3")
        }
        other => panic!("unexpected error: {other:?}"),
    }
    fs::remove_dir_all(&dir).unwrap();
}
