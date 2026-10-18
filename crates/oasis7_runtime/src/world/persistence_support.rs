use super::*;
use oasis7_distfs::BlobStore;

fn sidecar_generation_root_dir(store_root: &Path) -> std::path::PathBuf {
    store_root.join(SIDECAR_GENERATION_ROOT_DIR)
}

fn sidecar_generation_manifests_dir(store_root: &Path) -> std::path::PathBuf {
    sidecar_generation_root_dir(store_root).join(SIDECAR_GENERATION_MANIFESTS_DIR)
}

fn sidecar_generation_payloads_dir(store_root: &Path) -> std::path::PathBuf {
    sidecar_generation_root_dir(store_root).join(SIDECAR_GENERATION_PAYLOADS_DIR)
}

fn sidecar_generation_staging_dir(store_root: &Path) -> std::path::PathBuf {
    sidecar_generation_root_dir(store_root).join(SIDECAR_GENERATION_STAGING_DIR)
}

fn sidecar_generation_payload_dir(store_root: &Path, generation_id: &str) -> std::path::PathBuf {
    sidecar_generation_payloads_dir(store_root).join(generation_id)
}

fn sidecar_generation_staging_payload_dir(
    store_root: &Path,
    generation_id: &str,
) -> std::path::PathBuf {
    sidecar_generation_staging_dir(store_root).join(generation_id)
}

fn sidecar_generation_index_path(store_root: &Path) -> std::path::PathBuf {
    sidecar_generation_root_dir(store_root).join(SIDECAR_GENERATION_INDEX_FILE)
}

fn sidecar_generation_manifest_path(store_root: &Path, generation_id: &str) -> std::path::PathBuf {
    sidecar_generation_manifests_dir(store_root).join(format!("{generation_id}.json"))
}

pub(super) fn load_sidecar_generation_index(
    store_root: &Path,
) -> Result<Option<SidecarGenerationIndex>, WorldError> {
    let index_path = sidecar_generation_index_path(store_root);
    if !index_path.exists() {
        return Ok(None);
    }
    let index: SidecarGenerationIndex = read_json_from_path(index_path.as_path())?;
    Ok(Some(index))
}

fn build_sidecar_generation_pinned_blob_hashes(
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
    tick_consensus_archive_ref: Option<&str>,
) -> Vec<String> {
    let mut pinned_blob_hashes = manifest
        .chunks
        .iter()
        .map(|chunk| chunk.content_hash.clone())
        .collect::<BTreeSet<_>>();
    pinned_blob_hashes.extend(
        journal_segments
            .iter()
            .map(|segment| segment.content_hash.clone()),
    );
    if let Some(archive_ref) = tick_consensus_archive_ref {
        pinned_blob_hashes.insert(archive_ref.to_string());
    }
    pinned_blob_hashes.into_iter().collect()
}

fn sidecar_generation_snapshot_manifest_rel_path(generation_id: &str, staging: bool) -> String {
    if staging {
        format!(
            "{SIDECAR_GENERATION_ROOT_DIR}/{SIDECAR_GENERATION_STAGING_DIR}/{generation_id}/{SIDECAR_GENERATION_SNAPSHOT_MANIFEST_FILE}"
        )
    } else {
        format!(
            "{SIDECAR_GENERATION_ROOT_DIR}/{SIDECAR_GENERATION_PAYLOADS_DIR}/{generation_id}/{SIDECAR_GENERATION_SNAPSHOT_MANIFEST_FILE}"
        )
    }
}

fn sidecar_generation_journal_segments_rel_path(generation_id: &str, staging: bool) -> String {
    if staging {
        format!(
            "{SIDECAR_GENERATION_ROOT_DIR}/{SIDECAR_GENERATION_STAGING_DIR}/{generation_id}/{SIDECAR_GENERATION_JOURNAL_SEGMENTS_FILE}"
        )
    } else {
        format!(
            "{SIDECAR_GENERATION_ROOT_DIR}/{SIDECAR_GENERATION_PAYLOADS_DIR}/{generation_id}/{SIDECAR_GENERATION_JOURNAL_SEGMENTS_FILE}"
        )
    }
}

fn sidecar_generation_recovery_metadata_rel_path(generation_id: &str, staging: bool) -> String {
    let area = if staging {
        SIDECAR_GENERATION_STAGING_DIR
    } else {
        SIDECAR_GENERATION_PAYLOADS_DIR
    };
    format!(
        "{SIDECAR_GENERATION_ROOT_DIR}/{area}/{generation_id}/{SIDECAR_GENERATION_RECOVERY_METADATA_FILE}"
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Sidecar generation fields map directly to the durable recovery record schema."
)]
fn build_sidecar_generation_record(
    generation_id: String,
    snapshot_manifest_path: String,
    journal_segments_path: String,
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
    recovery_metadata: Option<&[u8]>,
    tick_consensus_archive_ref: Option<String>,
    created_at_ms: i64,
) -> Result<SidecarGenerationRecord, WorldError> {
    let snapshot_manifest_hash = hash_json(manifest)?;
    let journal_segment_hashes = journal_segments
        .iter()
        .map(|segment| segment.content_hash.clone())
        .collect::<Vec<_>>();
    let pinned_blob_hashes = build_sidecar_generation_pinned_blob_hashes(
        manifest,
        journal_segments,
        tick_consensus_archive_ref.as_deref(),
    );
    let recovery_metadata_path = recovery_metadata.map(|_| {
        sidecar_generation_recovery_metadata_rel_path(
            generation_id.as_str(),
            snapshot_manifest_path.contains(SIDECAR_GENERATION_STAGING_DIR),
        )
    });
    let recovery_metadata_hash = recovery_metadata.map(super::super::super::util::sha256_hex);
    let manifest_hash = hash_json(&SidecarGenerationHashPayload {
        generation_id: generation_id.as_str(),
        snapshot_manifest_path: snapshot_manifest_path.as_str(),
        journal_segments_path: journal_segments_path.as_str(),
        snapshot_manifest_hash: snapshot_manifest_hash.as_str(),
        journal_segment_hashes: journal_segment_hashes.as_slice(),
        recovery_metadata_path: &recovery_metadata_path,
        recovery_metadata_hash: &recovery_metadata_hash,
        tick_consensus_archive_ref: &tick_consensus_archive_ref,
        pinned_blob_hashes: pinned_blob_hashes.as_slice(),
        created_at_ms,
    })?;
    Ok(SidecarGenerationRecord {
        schema_version: SIDECAR_GENERATION_RECORD_SCHEMA_V1,
        generation_id,
        snapshot_manifest_path,
        journal_segments_path,
        snapshot_manifest_hash,
        manifest_hash,
        journal_segment_hashes,
        recovery_metadata_path,
        recovery_metadata_hash,
        tick_consensus_archive_ref,
        pinned_blob_hashes,
        created_at_ms,
    })
}

pub(super) fn read_sidecar_generation_payloads(
    store_root: &Path,
    generation_record: &SidecarGenerationRecord,
) -> Result<(SnapshotManifest, Vec<JournalSegmentRef>), WorldError> {
    let snapshot_manifest_path = store_root.join(generation_record.snapshot_manifest_path.as_str());
    let journal_segments_path = store_root.join(generation_record.journal_segments_path.as_str());
    let manifest: SnapshotManifest = read_json_from_path(snapshot_manifest_path.as_path())?;
    let journal_segments: Vec<JournalSegmentRef> =
        read_json_from_path(journal_segments_path.as_path())?;
    Ok((manifest, journal_segments))
}

fn validate_sidecar_generation_record_payloads(
    store_root: &Path,
    generation_record: &SidecarGenerationRecord,
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
) -> Result<Vec<String>, WorldError> {
    let snapshot_manifest_hash = hash_json(manifest)?;
    if snapshot_manifest_hash != generation_record.snapshot_manifest_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar generation snapshot manifest hash mismatch: generation_id={} expected={} actual={}",
                generation_record.generation_id,
                generation_record.snapshot_manifest_hash,
                snapshot_manifest_hash,
            ),
        });
    }

    let journal_segment_hashes = journal_segments
        .iter()
        .map(|segment| segment.content_hash.clone())
        .collect::<Vec<_>>();
    if journal_segment_hashes != generation_record.journal_segment_hashes {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar generation journal segment hashes mismatch: generation_id={}",
                generation_record.generation_id,
            ),
        });
    }

    let pinned_blob_hashes = build_sidecar_generation_pinned_blob_hashes(
        manifest,
        journal_segments,
        generation_record.tick_consensus_archive_ref.as_deref(),
    );
    if let Some(archive_ref) = generation_record.tick_consensus_archive_ref.as_deref() {
        let archive_bytes = LocalCasStore::new(store_root).get_verified(archive_ref)?;
        let _: TickConsensusArchiveFile = serde_json::from_slice(archive_bytes.as_slice())
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!(
                    "sidecar tick consensus archive decode failed: generation_id={} err={err}",
                    generation_record.generation_id
                ),
            })?;
    }
    if pinned_blob_hashes != generation_record.pinned_blob_hashes {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar generation pin set mismatch: generation_id={}",
                generation_record.generation_id,
            ),
        });
    }

    let recovery_metadata = match generation_record.recovery_metadata_path.as_deref() {
        Some(relative) => Some(fs::read(store_root.join(relative))?),
        None => None,
    };
    let recovery_metadata_hash = recovery_metadata
        .as_deref()
        .map(super::super::super::util::sha256_hex);
    if recovery_metadata_hash != generation_record.recovery_metadata_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar generation recovery metadata hash mismatch: generation_id={}",
                generation_record.generation_id
            ),
        });
    }

    let manifest_hash = hash_json(&SidecarGenerationHashPayload {
        generation_id: generation_record.generation_id.as_str(),
        snapshot_manifest_path: generation_record.snapshot_manifest_path.as_str(),
        journal_segments_path: generation_record.journal_segments_path.as_str(),
        snapshot_manifest_hash: snapshot_manifest_hash.as_str(),
        journal_segment_hashes: journal_segment_hashes.as_slice(),
        recovery_metadata_path: &generation_record.recovery_metadata_path,
        recovery_metadata_hash: &generation_record.recovery_metadata_hash,
        tick_consensus_archive_ref: &generation_record.tick_consensus_archive_ref,
        pinned_blob_hashes: pinned_blob_hashes.as_slice(),
        created_at_ms: generation_record.created_at_ms,
    })?;
    if manifest_hash != generation_record.manifest_hash {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar generation manifest hash mismatch: generation_id={} expected={} actual={}",
                generation_record.generation_id, generation_record.manifest_hash, manifest_hash,
            ),
        });
    }

    Ok(pinned_blob_hashes)
}

pub(super) fn validate_sidecar_generation_record(
    store_root: &Path,
    generation_record: &SidecarGenerationRecord,
) -> Result<Vec<String>, WorldError> {
    let (manifest, journal_segments) =
        read_sidecar_generation_payloads(store_root, generation_record)?;
    validate_sidecar_generation_record_payloads(
        store_root,
        generation_record,
        &manifest,
        journal_segments.as_slice(),
    )
}

pub(super) fn load_sidecar_tick_consensus_archive(
    store_root: &Path,
    generation_record: &SidecarGenerationRecord,
) -> Result<Option<TickConsensusArchiveFile>, WorldError> {
    let Some(archive_ref) = generation_record.tick_consensus_archive_ref.as_deref() else {
        return Ok(None);
    };
    let bytes = LocalCasStore::new(store_root).get_verified(archive_ref)?;
    let archive = serde_json::from_slice(bytes.as_slice()).map_err(|err| {
        WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar tick consensus archive decode failed: generation_id={} err={err}",
                generation_record.generation_id
            ),
        }
    })?;
    Ok(Some(archive))
}

fn sidecar_active_generation_ids(
    index: &SidecarGenerationIndex,
) -> Result<BTreeSet<String>, WorldError> {
    let mut generation_ids = BTreeSet::from([index.latest_generation.clone()]);
    if let Some(rollback_safe_generation) = index.rollback_safe_generation.as_ref() {
        generation_ids.insert(rollback_safe_generation.clone());
    }
    for generation_id in generation_ids.iter() {
        if !index.generations.contains_key(generation_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "sidecar generation index missing retained generation: generation_id={generation_id}",
                ),
            });
        }
    }
    Ok(generation_ids)
}

fn collect_sidecar_retained_blob_hashes(
    store_root: &Path,
    index: &SidecarGenerationIndex,
) -> Result<BTreeSet<String>, WorldError> {
    let mut retained_blob_hashes = BTreeSet::new();
    for generation_id in sidecar_active_generation_ids(index)? {
        let generation_record = index
            .generations
            .get(generation_id.as_str())
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: format!(
                    "sidecar generation record missing during sweep: generation_id={generation_id}",
                ),
            })?;
        retained_blob_hashes.extend(validate_sidecar_generation_record(
            store_root,
            generation_record,
        )?);
    }
    Ok(retained_blob_hashes)
}

fn collect_sidecar_orphan_blob_hashes(
    store: &LocalCasStore,
    retained_blob_hashes: &BTreeSet<String>,
) -> Result<Vec<String>, WorldError> {
    let external_pinned_hashes = store
        .list_effective_pins()?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let orphan_blob_hashes = store
        .list_blob_hashes()?
        .into_iter()
        .filter(|content_hash| {
            !retained_blob_hashes.contains(content_hash)
                && !external_pinned_hashes.contains(content_hash)
        })
        .collect::<Vec<_>>();
    Ok(orphan_blob_hashes)
}

fn sweep_sidecar_orphan_blobs(
    store_root: &Path,
    index: &SidecarGenerationIndex,
) -> Result<SidecarGcResult, WorldError> {
    let store = LocalCasStore::new(store_root);
    let retained_blob_hashes = collect_sidecar_retained_blob_hashes(store_root, index)?;
    let orphan_blob_hashes = collect_sidecar_orphan_blob_hashes(&store, &retained_blob_hashes)?;
    let mut freed_blob_count = 0usize;
    let mut freed_bytes = 0u64;

    for content_hash in orphan_blob_hashes {
        let blob_path = store.blobs_dir().join(format!("{content_hash}.blob"));
        if !blob_path.exists() {
            continue;
        }
        freed_bytes = freed_bytes.saturating_add(fs::metadata(blob_path.as_path())?.len());
        fs::remove_file(blob_path.as_path())?;
        freed_blob_count += 1;
    }

    let remaining_orphan_hashes =
        collect_sidecar_orphan_blob_hashes(&store, &retained_blob_hashes)?;
    if !remaining_orphan_hashes.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "sidecar orphan blob sweep incomplete: orphan_blob_hashes={}",
                remaining_orphan_hashes.join(","),
            ),
        });
    }

    Ok(SidecarGcResult::success(freed_blob_count, freed_bytes))
}

fn cleanup_stale_sidecar_generation_staging(store_root: &Path) -> Result<(), WorldError> {
    let staging_root = sidecar_generation_staging_dir(store_root);
    if !staging_root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(staging_root.as_path())? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry_path.as_path())?;
        } else {
            fs::remove_file(entry_path.as_path())?;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
fn maybe_fail_after_sidecar_stage(store_root: &Path) -> Result<(), WorldError> {
    let fail_path = sidecar_generation_root_dir(store_root).join(".test-fail-after-stage");
    if fail_path.exists() {
        return Err(WorldError::DistributedValidationFailed {
            reason: "sidecar test failpoint after stage".to_string(),
        });
    }
    Ok(())
}

#[cfg(any(test, feature = "test-support"))]
fn maybe_fail_sidecar_commit_seam(store_root: &Path, seam: &str) -> Result<(), WorldError> {
    let fail_path = sidecar_generation_root_dir(store_root).join(format!(".test-fail-{seam}"));
    if fail_path.exists() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("sidecar test failpoint {seam}"),
        });
    }
    Ok(())
}

#[cfg(not(any(test, feature = "test-support")))]
fn maybe_fail_sidecar_commit_seam(_store_root: &Path, _seam: &str) -> Result<(), WorldError> {
    Ok(())
}

#[cfg(not(any(test, feature = "test-support")))]
fn maybe_fail_after_sidecar_stage(_store_root: &Path) -> Result<(), WorldError> {
    Ok(())
}

fn stage_sidecar_generation(
    store_root: &Path,
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
    tick_consensus_archive_bytes: Option<&[u8]>,
    recovery_metadata: Option<&[u8]>,
) -> Result<(String, SidecarGenerationRecord), WorldError> {
    cleanup_stale_sidecar_generation_staging(store_root)?;
    fs::create_dir_all(sidecar_generation_manifests_dir(store_root).as_path())?;
    fs::create_dir_all(sidecar_generation_payloads_dir(store_root).as_path())?;
    fs::create_dir_all(sidecar_generation_staging_dir(store_root).as_path())?;

    let created_at_ms = now_unix_ms();
    let generation_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let snapshot_manifest_hash = hash_json(manifest)?;
    let generation_id = format!(
        "gen-{}-{}-{}",
        created_at_ms,
        generation_nonce,
        snapshot_manifest_hash.chars().take(12).collect::<String>()
    );
    let staging_payload_dir =
        sidecar_generation_staging_payload_dir(store_root, generation_id.as_str());
    fs::create_dir_all(staging_payload_dir.as_path())?;
    let tick_consensus_archive_ref = tick_consensus_archive_bytes
        .map(|bytes| LocalCasStore::new(store_root).put_bytes(bytes))
        .transpose()?;
    super::super::super::util::atomic_write_json_to_path(
        manifest,
        staging_payload_dir
            .join(SIDECAR_GENERATION_SNAPSHOT_MANIFEST_FILE)
            .as_path(),
    )?;
    if let Some(metadata) = recovery_metadata {
        super::super::super::util::atomic_write_bytes_to_path(
            metadata,
            staging_payload_dir
                .join(SIDECAR_GENERATION_RECOVERY_METADATA_FILE)
                .as_path(),
        )?;
    }
    super::super::super::util::atomic_write_json_to_path(
        &journal_segments.to_vec(),
        staging_payload_dir
            .join(SIDECAR_GENERATION_JOURNAL_SEGMENTS_FILE)
            .as_path(),
    )?;
    let generation_record = build_sidecar_generation_record(
        generation_id.clone(),
        sidecar_generation_snapshot_manifest_rel_path(generation_id.as_str(), true),
        sidecar_generation_journal_segments_rel_path(generation_id.as_str(), true),
        manifest,
        journal_segments,
        recovery_metadata,
        tick_consensus_archive_ref,
        created_at_ms,
    )?;
    super::super::super::util::atomic_write_json_to_path(
        &generation_record,
        staging_payload_dir.join("generation.json").as_path(),
    )?;
    super::super::super::util::sync_directory(staging_payload_dir.as_path())?;
    Ok((generation_id, generation_record))
}

fn validate_staged_sidecar_generation(
    store_root: &Path,
    generation_record: &SidecarGenerationRecord,
) -> Result<(), WorldError> {
    let (manifest, journal_segments) =
        read_sidecar_generation_payloads(store_root, generation_record)?;
    let _ = validate_sidecar_generation_record_payloads(
        store_root,
        generation_record,
        &manifest,
        journal_segments.as_slice(),
    )?;
    let store = LocalCasStore::new(store_root);
    let restored_snapshot: Snapshot = assemble_snapshot(&manifest, &store)?;
    let restored_events: Vec<WorldEvent> =
        assemble_journal(journal_segments.as_slice(), &store, |event: &WorldEvent| {
            event.id
        })?;
    let _ = restored_snapshot;
    let _ = restored_events;
    Ok(())
}

fn finalize_sidecar_generation(
    store_root: &Path,
    generation_id: &str,
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
    tick_consensus_archive_ref: Option<String>,
    recovery_metadata: Option<&[u8]>,
) -> Result<SidecarGenerationRecord, WorldError> {
    let staging_payload_dir = sidecar_generation_staging_payload_dir(store_root, generation_id);
    let payload_dir = sidecar_generation_payload_dir(store_root, generation_id);
    if payload_dir.exists() {
        fs::remove_dir_all(payload_dir.as_path())?;
    }
    fs::rename(staging_payload_dir.as_path(), payload_dir.as_path())?;
    super::super::super::util::sync_directory(
        sidecar_generation_payloads_dir(store_root).as_path(),
    )?;
    let staged_root = sidecar_generation_staging_dir(store_root);
    if staged_root.exists() {
        fs::create_dir_all(staged_root.as_path())?;
    }
    build_sidecar_generation_record(
        generation_id.to_string(),
        sidecar_generation_snapshot_manifest_rel_path(generation_id, false),
        sidecar_generation_journal_segments_rel_path(generation_id, false),
        manifest,
        journal_segments,
        recovery_metadata,
        tick_consensus_archive_ref,
        now_unix_ms(),
    )
}

pub(super) fn persist_sidecar_generation_index(
    store_root: &Path,
    manifest: &SnapshotManifest,
    journal_segments: &[JournalSegmentRef],
    tick_consensus_archive_bytes: Option<&[u8]>,
    recovery_metadata: Option<&[u8]>,
) -> Result<(), WorldError> {
    let (generation_id, staged_record) = stage_sidecar_generation(
        store_root,
        manifest,
        journal_segments,
        tick_consensus_archive_bytes,
        recovery_metadata,
    )?;
    maybe_fail_after_sidecar_stage(store_root)?;
    validate_staged_sidecar_generation(store_root, &staged_record)?;
    maybe_fail_sidecar_commit_seam(store_root, "after-stage-validation")?;
    let generation_record = finalize_sidecar_generation(
        store_root,
        generation_id.as_str(),
        manifest,
        journal_segments,
        staged_record.tick_consensus_archive_ref.clone(),
        recovery_metadata,
    )?;
    super::super::super::util::atomic_write_json_to_path(
        &generation_record,
        sidecar_generation_manifest_path(store_root, generation_record.generation_id.as_str())
            .as_path(),
    )?;
    maybe_fail_sidecar_commit_seam(store_root, "before-index-commit")?;

    let mut index = load_sidecar_generation_index(store_root)?.unwrap_or(SidecarGenerationIndex {
        schema_version: SIDECAR_GENERATION_INDEX_SCHEMA_V1,
        latest_generation: generation_record.generation_id.clone(),
        rollback_safe_generation: None,
        generations: BTreeMap::new(),
        last_gc_result: SidecarGcResult::not_run(),
    });
    let previous_latest = if index.latest_generation != generation_record.generation_id {
        Some(index.latest_generation.clone())
    } else {
        index.rollback_safe_generation.clone()
    };
    index.schema_version = SIDECAR_GENERATION_INDEX_SCHEMA_V1;
    index.latest_generation = generation_record.generation_id.clone();
    index.rollback_safe_generation = previous_latest;
    index
        .generations
        .insert(generation_record.generation_id.clone(), generation_record);

    let mut retained_generation_ids = BTreeSet::from([index.latest_generation.clone()]);
    if let Some(rollback_safe_generation) = index.rollback_safe_generation.as_ref() {
        retained_generation_ids.insert(rollback_safe_generation.clone());
    }
    if retained_generation_ids.len() < SIDECAR_GENERATION_KEEP_LATEST {
        for generation_id in index.generations.keys().rev() {
            retained_generation_ids.insert(generation_id.clone());
            if retained_generation_ids.len() >= SIDECAR_GENERATION_KEEP_LATEST {
                break;
            }
        }
    }
    let stale_generation_ids = index
        .generations
        .keys()
        .filter(|generation_id| !retained_generation_ids.contains(*generation_id))
        .cloned()
        .collect::<Vec<_>>();
    for generation_id in stale_generation_ids {
        index.generations.remove(generation_id.as_str());
        let manifest_path = sidecar_generation_manifest_path(store_root, generation_id.as_str());
        if manifest_path.exists() {
            fs::remove_file(manifest_path.as_path())?;
        }
        let payload_dir = sidecar_generation_payload_dir(store_root, generation_id.as_str());
        if payload_dir.exists() {
            fs::remove_dir_all(payload_dir.as_path())?;
        }
    }

    index.last_gc_result = SidecarGcResult::not_run();
    super::super::super::util::atomic_write_json_to_path(
        &index,
        sidecar_generation_index_path(store_root).as_path(),
    )?;
    maybe_fail_sidecar_commit_seam(store_root, "after-index-commit")?;
    index.last_gc_result = match sweep_sidecar_orphan_blobs(store_root, &index) {
        Ok(result) => result,
        Err(err) => SidecarGcResult::failed(format!("{err:?}")),
    };
    super::super::super::util::atomic_write_json_to_path(
        &index,
        sidecar_generation_index_path(store_root).as_path(),
    )?;
    Ok(())
}

pub(super) fn distfs_world_id(dir: &Path) -> String {
    dir.file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(DISTFS_WORLD_ID_FALLBACK)
        .to_string()
}

pub(super) fn write_distfs_recovery_audit(
    dir: &Path,
    status: &str,
    reason: Option<String>,
) -> Result<(), WorldError> {
    let record = DistfsRecoveryAuditRecord {
        timestamp_ms: now_unix_ms(),
        status: status.to_string(),
        reason,
    };
    write_json_to_path(&record, dir.join(DISTFS_RECOVERY_AUDIT_FILE).as_path())
}

pub(super) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
