use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) struct TickConsensusArchiveFile {
    pub(in crate::world) archived_records: Vec<TickConsensusRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct TickConsensusArchiveIndex {
    hot_from_tick: Option<WorldTime>,
    hot_to_tick: Option<WorldTime>,
    archived_segments: Vec<TickConsensusArchiveSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TickConsensusArchiveSegment {
    from_tick: WorldTime,
    to_tick: WorldTime,
    content_hash: String,
    record_count: usize,
    hash_chain_anchor: String,
    relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct TickConsensusArchiveSegmentFile {
    records: Vec<TickConsensusRecord>,
}

fn tick_consensus_archive_path(dir: &Path) -> std::path::PathBuf {
    dir.join(TICK_CONSENSUS_ARCHIVE_FILE)
}
fn tick_consensus_archive_index_path(dir: &Path) -> std::path::PathBuf {
    dir.join(TICK_CONSENSUS_ARCHIVE_INDEX_FILE)
}
fn tick_consensus_archive_segments_dir(dir: &Path) -> std::path::PathBuf {
    dir.join(TICK_CONSENSUS_ARCHIVE_SEGMENTS_DIR)
}
fn tick_consensus_archive_segment_relative_path(
    from_tick: WorldTime,
    to_tick: WorldTime,
) -> String {
    format!("{TICK_CONSENSUS_ARCHIVE_SEGMENTS_DIR}/segment-{from_tick:020}-{to_tick:020}.json")
}
fn tick_consensus_hot_tick_bounds(
    records: &[TickConsensusRecord],
) -> (Option<WorldTime>, Option<WorldTime>) {
    (
        records.first().map(|record| record.block.header.tick),
        records.last().map(|record| record.block.header.tick),
    )
}
pub(super) fn split_tick_consensus_snapshot_for_persistence(
    snapshot: &Snapshot,
) -> (Snapshot, Option<TickConsensusArchiveFile>) {
    let mut persisted_snapshot = snapshot.clone();
    if snapshot.tick_consensus_archived_record_count > 0
        && snapshot.state.retained_history_root.is_some()
    {
        return (persisted_snapshot, None);
    }
    persisted_snapshot.tick_consensus_total_record_count =
        snapshot.tick_consensus_records.len() as u64;
    if snapshot.tick_consensus_records.len() <= TICK_CONSENSUS_HOT_LIMIT {
        persisted_snapshot.tick_consensus_archived_record_count = 0;
        let (hot_from_tick, hot_to_tick) =
            tick_consensus_hot_tick_bounds(persisted_snapshot.tick_consensus_records.as_slice());
        persisted_snapshot.tick_consensus_hot_from_tick = hot_from_tick;
        persisted_snapshot.tick_consensus_hot_to_tick = hot_to_tick;
        return (persisted_snapshot, None);
    }

    let archived_record_count = snapshot
        .tick_consensus_records
        .len()
        .saturating_sub(TICK_CONSENSUS_HOT_LIMIT);
    let archived_records = snapshot.tick_consensus_records[..archived_record_count].to_vec();
    persisted_snapshot.tick_consensus_records =
        snapshot.tick_consensus_records[archived_record_count..].to_vec();
    persisted_snapshot.tick_consensus_archived_record_count = archived_records.len() as u64;
    let (hot_from_tick, hot_to_tick) =
        tick_consensus_hot_tick_bounds(persisted_snapshot.tick_consensus_records.as_slice());
    persisted_snapshot.tick_consensus_hot_from_tick = hot_from_tick;
    persisted_snapshot.tick_consensus_hot_to_tick = hot_to_tick;
    (
        persisted_snapshot,
        Some(TickConsensusArchiveFile { archived_records }),
    )
}
fn build_tick_consensus_archive_index(
    snapshot: &Snapshot,
    archive: &TickConsensusArchiveFile,
) -> Result<
    (
        TickConsensusArchiveIndex,
        Vec<(String, TickConsensusArchiveSegmentFile)>,
    ),
    WorldError,
> {
    let mut archived_segments = Vec::new();
    let mut segment_files = Vec::new();
    for records_chunk in archive
        .archived_records
        .chunks(TICK_CONSENSUS_ARCHIVE_SEGMENT_LEN)
    {
        if records_chunk.is_empty() {
            continue;
        }
        let segment_file = TickConsensusArchiveSegmentFile {
            records: records_chunk.to_vec(),
        };
        let from_tick = segment_file
            .records
            .first()
            .map(|record| record.block.header.tick)
            .expect("segment records");
        let to_tick = segment_file
            .records
            .last()
            .map(|record| record.block.header.tick)
            .expect("segment records");
        let relative_path = tick_consensus_archive_segment_relative_path(from_tick, to_tick);
        let content_hash = hash_json(&segment_file)?;
        let hash_chain_anchor = segment_file
            .records
            .last()
            .map(|record| record.certificate.block_hash.clone())
            .expect("segment records");
        archived_segments.push(TickConsensusArchiveSegment {
            from_tick,
            to_tick,
            content_hash,
            record_count: segment_file.records.len(),
            hash_chain_anchor,
            relative_path: relative_path.clone(),
        });
        segment_files.push((relative_path, segment_file));
    }

    Ok((
        TickConsensusArchiveIndex {
            hot_from_tick: snapshot.tick_consensus_hot_from_tick,
            hot_to_tick: snapshot.tick_consensus_hot_to_tick,
            archived_segments,
        },
        segment_files,
    ))
}
pub(super) fn persist_tick_consensus_archive(
    dir: &Path,
    snapshot: &Snapshot,
    archive: Option<&TickConsensusArchiveFile>,
) -> Result<(), WorldError> {
    let legacy_archive_path = tick_consensus_archive_path(dir);
    let archive_index_path = tick_consensus_archive_index_path(dir);
    let archive_segments_dir = tick_consensus_archive_segments_dir(dir);
    match archive {
        Some(archive) if !archive.archived_records.is_empty() => {
            let (archive_index, segment_files) =
                build_tick_consensus_archive_index(snapshot, archive)?;
            if archive_segments_dir.exists() {
                fs::remove_dir_all(archive_segments_dir.as_path())?;
            }
            fs::create_dir_all(archive_segments_dir.as_path())?;
            for (relative_path, segment_file) in segment_files {
                write_json_to_path(&segment_file, dir.join(relative_path.as_str()).as_path())?;
            }
            write_json_to_path(&archive_index, archive_index_path.as_path())?;
            write_json_to_path(archive, legacy_archive_path.as_path())?;
            Ok(())
        }
        _ => {
            if legacy_archive_path.exists() {
                fs::remove_file(legacy_archive_path.as_path())?;
            }
            if archive_index_path.exists() {
                fs::remove_file(archive_index_path.as_path())?;
            }
            if archive_segments_dir.exists() {
                fs::remove_dir_all(archive_segments_dir.as_path())?;
            }
            Ok(())
        }
    }
}
fn load_tick_consensus_archive_records_from_index(
    dir: &Path,
    snapshot: &Snapshot,
) -> Result<Option<Vec<TickConsensusRecord>>, WorldError> {
    let archive_index_path = tick_consensus_archive_index_path(dir);
    if !archive_index_path.exists() {
        return Ok(None);
    }
    let archive_index: TickConsensusArchiveIndex =
        read_json_from_path(archive_index_path.as_path())?;
    if archive_index.hot_from_tick != snapshot.tick_consensus_hot_from_tick
        || archive_index.hot_to_tick != snapshot.tick_consensus_hot_to_tick
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus archive index hot range mismatch: expected_from={:?} actual_from={:?} expected_to={:?} actual_to={:?}",
                snapshot.tick_consensus_hot_from_tick,
                archive_index.hot_from_tick,
                snapshot.tick_consensus_hot_to_tick,
                archive_index.hot_to_tick,
            ),
        });
    }

    let indexed_record_count = archive_index
        .archived_segments
        .iter()
        .map(|segment| segment.record_count)
        .sum::<usize>();
    if indexed_record_count as u64 != snapshot.tick_consensus_archived_record_count {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus archive index count mismatch: expected={} actual={}",
                snapshot.tick_consensus_archived_record_count, indexed_record_count,
            ),
        });
    }

    let mut archived_records = Vec::with_capacity(indexed_record_count);
    let mut previous_to_tick = None;
    for segment in archive_index.archived_segments {
        if let Some(previous_to_tick) = previous_to_tick
            && segment.from_tick <= previous_to_tick
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment ordering invalid: previous_to_tick={} current_from_tick={}",
                    previous_to_tick, segment.from_tick,
                ),
            });
        }
        let segment_path = dir.join(segment.relative_path.as_str());
        if !segment_path.exists() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment missing: path={}",
                    segment_path.display(),
                ),
            });
        }
        let segment_file: TickConsensusArchiveSegmentFile =
            read_json_from_path(segment_path.as_path())?;
        if segment_file.records.len() != segment.record_count {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment count mismatch: expected={} actual={} path={}",
                    segment.record_count,
                    segment_file.records.len(),
                    segment_path.display(),
                ),
            });
        }
        if segment_file
            .records
            .first()
            .map(|record| record.block.header.tick)
            != Some(segment.from_tick)
            || segment_file
                .records
                .last()
                .map(|record| record.block.header.tick)
                != Some(segment.to_tick)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment range mismatch: path={}",
                    segment_path.display(),
                ),
            });
        }
        let content_hash = hash_json(&segment_file)?;
        if content_hash != segment.content_hash {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment content hash mismatch: expected={} actual={} path={}",
                    segment.content_hash,
                    content_hash,
                    segment_path.display(),
                ),
            });
        }
        let hash_chain_anchor = segment_file
            .records
            .last()
            .map(|record| record.certificate.block_hash.clone())
            .unwrap_or_default();
        if hash_chain_anchor != segment.hash_chain_anchor {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "tick consensus archive segment anchor mismatch: expected={} actual={} path={}",
                    segment.hash_chain_anchor,
                    hash_chain_anchor,
                    segment_path.display(),
                ),
            });
        }
        previous_to_tick = Some(segment.to_tick);
        archived_records.extend(segment_file.records);
    }

    Ok(Some(archived_records))
}
fn tick_consensus_archive_segment_missing(reason: &str) -> bool {
    reason.starts_with("tick consensus archive segment missing:")
}
fn load_tick_consensus_legacy_archive_records(
    dir: &Path,
    snapshot: &Snapshot,
) -> Result<Vec<TickConsensusRecord>, WorldError> {
    let archive_path = tick_consensus_archive_path(dir);
    if !archive_path.exists() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus archive missing: path={}",
                archive_path.display()
            ),
        });
    }
    let archive: TickConsensusArchiveFile = read_json_from_path(archive_path.as_path())?;
    if archive.archived_records.len() as u64 != snapshot.tick_consensus_archived_record_count {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus archive count mismatch: expected={} actual={}",
                snapshot.tick_consensus_archived_record_count,
                archive.archived_records.len(),
            ),
        });
    }
    Ok(archive.archived_records)
}
pub(super) fn hydrate_tick_consensus_snapshot_from_archive(
    dir: &Path,
    snapshot: &mut Snapshot,
) -> Result<(), WorldError> {
    let (actual_hot_from_tick, actual_hot_to_tick) =
        tick_consensus_hot_tick_bounds(snapshot.tick_consensus_records.as_slice());
    if (snapshot.tick_consensus_hot_from_tick.is_some()
        || snapshot.tick_consensus_hot_to_tick.is_some())
        && (snapshot.tick_consensus_hot_from_tick != actual_hot_from_tick
            || snapshot.tick_consensus_hot_to_tick != actual_hot_to_tick)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus hot summary mismatch: expected_from={:?} actual_from={:?} expected_to={:?} actual_to={:?}",
                snapshot.tick_consensus_hot_from_tick,
                actual_hot_from_tick,
                snapshot.tick_consensus_hot_to_tick,
                actual_hot_to_tick,
            ),
        });
    }

    if snapshot.tick_consensus_total_record_count == 0 {
        snapshot.tick_consensus_total_record_count = snapshot.tick_consensus_records.len() as u64;
    }
    if snapshot.tick_consensus_archived_record_count == 0 {
        return Ok(());
    }

    let archived_records = match load_tick_consensus_archive_records_from_index(dir, snapshot) {
        Ok(Some(records)) => records,
        Ok(None) => load_tick_consensus_legacy_archive_records(dir, snapshot)?,
        Err(WorldError::DistributedValidationFailed { reason })
            if tick_consensus_archive_segment_missing(reason.as_str()) =>
        {
            if tick_consensus_archive_path(dir).exists() {
                load_tick_consensus_legacy_archive_records(dir, snapshot)?
            } else {
                return Err(WorldError::DistributedValidationFailed { reason });
            }
        }
        Err(err) => return Err(err),
    };
    hydrate_tick_consensus_snapshot_from_archived_records(snapshot, archived_records)
}
pub(super) fn hydrate_tick_consensus_snapshot_from_archived_records(
    snapshot: &mut Snapshot,
    archived_records: Vec<TickConsensusRecord>,
) -> Result<(), WorldError> {
    let mut records = archived_records;
    records.extend(snapshot.tick_consensus_records.clone());
    if records.len() as u64 != snapshot.tick_consensus_total_record_count {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "tick consensus total count mismatch: expected={} actual={}",
                snapshot.tick_consensus_total_record_count,
                records.len(),
            ),
        });
    }
    snapshot.tick_consensus_records = records;
    // This counter describes the on-disk split only. Once the archive is
    // materialized, the in-memory snapshot is complete and a second hydration
    // pass must be a no-op.
    snapshot.tick_consensus_archived_record_count = 0;
    snapshot.tick_consensus_total_record_count = snapshot.tick_consensus_records.len() as u64;
    let (hot_from_tick, hot_to_tick) =
        tick_consensus_hot_tick_bounds(snapshot.tick_consensus_records.as_slice());
    snapshot.tick_consensus_hot_from_tick = hot_from_tick;
    snapshot.tick_consensus_hot_to_tick = hot_to_tick;
    Ok(())
}
pub(super) fn load_persisted_tick_consensus_snapshot_from_dir(
    dir: &Path,
) -> Result<Snapshot, WorldError> {
    if let Some((mut snapshot, _)) = World::try_load_from_distfs_sidecar(dir)? {
        if !World::has_indexed_sidecar_generation(dir)? {
            hydrate_tick_consensus_snapshot_from_archive(dir, &mut snapshot)?;
        }
        return Ok(snapshot);
    }
    let mut snapshot = Snapshot::load_json(dir.join(SNAPSHOT_FILE))?;
    hydrate_tick_consensus_snapshot_from_archive(dir, &mut snapshot)?;
    Ok(snapshot)
}
