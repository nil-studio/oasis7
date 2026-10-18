use super::*;
use oasis7_proto::distributed::{SnapshotManifest, StateChunkRef};
use oasis7_proto::distributed_storage::{JournalSegmentRef, SegmentConfig};

pub fn segment_snapshot<T: Serialize>(
    snapshot: &T,
    world_id: &str,
    epoch: u64,
    store: &impl BlobStore,
    config: SegmentConfig,
) -> Result<SnapshotManifest, WorldError> {
    let bytes = to_canonical_cbor(snapshot)?;
    let state_root = blake3_hex(&bytes);
    let chunk_size = config.snapshot_chunk_bytes.max(1);
    let mut chunks = Vec::new();

    for (index, chunk) in bytes.chunks(chunk_size).enumerate() {
        let content_hash = store.put_bytes(chunk)?;
        chunks.push(StateChunkRef {
            chunk_id: format!("{epoch}-{index:04}"),
            content_hash,
            size_bytes: chunk.len() as u64,
        });
    }

    Ok(SnapshotManifest {
        world_id: world_id.to_string(),
        epoch,
        chunks,
        state_root,
    })
}

pub fn segment_journal<E: Serialize>(
    events: &[E],
    store: &impl BlobStore,
    config: SegmentConfig,
    event_id_of: impl Fn(&E) -> u64,
) -> Result<Vec<JournalSegmentRef>, WorldError> {
    if events.is_empty() {
        return Ok(Vec::new());
    }

    let max_events = config.journal_events_per_segment.max(1);
    let mut segments = Vec::new();

    for chunk in events.chunks(max_events) {
        let from_event_id = chunk.first().map(&event_id_of).unwrap_or(0);
        let to_event_id = chunk.last().map(&event_id_of).unwrap_or(0);
        let bytes = to_canonical_cbor(&chunk)?;
        let content_hash = store.put_bytes(&bytes)?;
        segments.push(JournalSegmentRef {
            from_event_id,
            to_event_id,
            content_hash,
            size_bytes: bytes.len() as u64,
        });
    }

    Ok(segments)
}

pub fn assemble_snapshot<T: DeserializeOwned>(
    manifest: &SnapshotManifest,
    store: &impl BlobStore,
) -> Result<T, WorldError> {
    let mut bytes = Vec::new();
    for chunk in &manifest.chunks {
        let chunk_bytes = store.get(&chunk.content_hash)?;
        let actual_hash = blake3_hex(&chunk_bytes);
        if actual_hash != chunk.content_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "snapshot chunk hash mismatch: expected={}, actual={}",
                    chunk.content_hash, actual_hash
                ),
            });
        }
        bytes.extend_from_slice(&chunk_bytes);
    }

    let actual_root = blake3_hex(&bytes);
    if actual_root != manifest.state_root {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "snapshot state_root mismatch: expected={}, actual={}",
                manifest.state_root, actual_root
            ),
        });
    }

    Ok(serde_cbor::from_slice(&bytes)?)
}

pub fn assemble_journal<E: DeserializeOwned>(
    segments: &[JournalSegmentRef],
    store: &impl BlobStore,
    event_id_of: impl Fn(&E) -> u64,
) -> Result<Vec<E>, WorldError> {
    let mut events = Vec::new();
    let mut expected_next: Option<u64> = None;

    for segment in segments {
        let segment_bytes = store.get(&segment.content_hash)?;
        let actual_hash = blake3_hex(&segment_bytes);
        if actual_hash != segment.content_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "journal segment hash mismatch: expected={}, actual={}",
                    segment.content_hash, actual_hash
                ),
            });
        }
        let segment_events: Vec<E> = serde_cbor::from_slice(&segment_bytes)?;
        let (first, last) = match (segment_events.first(), segment_events.last()) {
            (Some(first), Some(last)) => (first, last),
            _ => {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "journal segment empty".to_string(),
                });
            }
        };

        let first_id = event_id_of(first);
        let last_id = event_id_of(last);
        if first_id != segment.from_event_id || last_id != segment.to_event_id {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "journal segment range mismatch: segment={}..{}, events={}..{}",
                    segment.from_event_id, segment.to_event_id, first_id, last_id
                ),
            });
        }
        if let Some(expected) = expected_next
            && first_id != expected
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "journal discontinuity: expected={}, got={}",
                    expected, first_id
                ),
            });
        }
        expected_next = last_id.checked_add(1);

        events.extend(segment_events);
    }

    Ok(events)
}
