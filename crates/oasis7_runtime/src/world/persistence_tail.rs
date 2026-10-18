use super::*;

impl World {
    /// Persist the current Runtime transaction before returning a receipt,
    /// lease or continuation. The indexed distfs generation is the durable
    /// pair boundary; canonical JSON files remain compatibility mirrors.
    pub(in crate::world) fn persist_runtime_transaction_if_configured(
        &self,
    ) -> Result<(), WorldError> {
        let Some(dir) = self.persistence_dir.borrow().clone() else {
            return Ok(());
        };
        let snapshot = self.snapshot();
        let (persisted_snapshot, tick_consensus_archive) =
            split_tick_consensus_snapshot_for_persistence(&snapshot);
        self.journal.save_json(dir.join(JOURNAL_FILE))?;
        persisted_snapshot.save_json(dir.join(SNAPSHOT_FILE))?;
        let archive_bytes = tick_consensus_archive
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|err| WorldError::DistributedValidationFailed {
                reason: format!("serialize tick consensus generation archive failed: {err}"),
            })?;
        persist_tick_consensus_archive(&dir, &persisted_snapshot, tick_consensus_archive.as_ref())?;
        self.save_distfs_sidecar(&dir, &persisted_snapshot, archive_bytes.as_deref(), None)?;
        Ok(())
    }

    pub(super) fn try_load_from_distfs_sidecar(
        dir: &Path,
    ) -> Result<Option<(Snapshot, Journal)>, WorldError> {
        let snapshot_manifest_path = dir.join(DISTFS_SNAPSHOT_MANIFEST_FILE);
        let journal_segments_path = dir.join(DISTFS_JOURNAL_SEGMENTS_FILE);
        let store_root = dir.join(DISTFS_STATE_DIR);
        if store_root.exists()
            && let Some(index) = persistence_support::load_sidecar_generation_index(&store_root)?
        {
            for generation_id in std::iter::once(index.latest_generation.as_str())
                .chain(index.rollback_safe_generation.as_deref())
            {
                let Some(record) = index.generations.get(generation_id) else {
                    continue;
                };
                if persistence_support::validate_sidecar_generation_record(&store_root, record)
                    .is_err()
                {
                    continue;
                }
                let (manifest, journal_segments) =
                    persistence_support::read_sidecar_generation_payloads(&store_root, record)?;
                let ignored_legacy_payload_reason = validate_compatible_legacy_distfs_payloads(
                    dir,
                    &manifest,
                    journal_segments.as_slice(),
                )
                .err()
                .map(|err| format!("legacy_payload_ignored: {:?}", err));
                let store = LocalCasStore::new(&store_root);
                let mut snapshot: Snapshot = assemble_snapshot(&manifest, &store)?;
                if snapshot.tick_consensus_archived_record_count > 0
                    && record.tick_consensus_archive_ref.is_none()
                {
                    continue;
                }
                if let Some(archive) =
                    persistence_support::load_sidecar_tick_consensus_archive(&store_root, record)?
                {
                    hydrate_tick_consensus_snapshot_from_archived_records(
                        &mut snapshot,
                        archive.archived_records,
                    )?;
                }
                let events: Vec<WorldEvent> =
                    assemble_journal(&journal_segments, &store, |event: &WorldEvent| event.id)?;
                let _ = write_distfs_recovery_audit(
                    dir,
                    if record.recovery_metadata_path.is_some() {
                        if generation_id == index.latest_generation {
                            "generation_restored"
                        } else {
                            "rollback_safe_generation_restored"
                        }
                    } else {
                        "distfs_restored"
                    },
                    ignored_legacy_payload_reason,
                );
                return Ok(Some((snapshot, Journal { events })));
            }
            return Err(WorldError::DistributedValidationFailed {
                reason: "sidecar generation index has no valid latest or rollback-safe generation"
                    .to_string(),
            });
        }
        if !snapshot_manifest_path.exists()
            || !journal_segments_path.exists()
            || !store_root.exists()
        {
            return Ok(None);
        }

        let restored = Self::load_from_distfs_sidecar(
            snapshot_manifest_path.as_path(),
            journal_segments_path.as_path(),
            store_root.as_path(),
        );
        match restored {
            Ok(value) => {
                let _ = write_distfs_recovery_audit(dir, "distfs_restored", None);
                Ok(Some(value))
            }
            Err(err) => {
                let _ = write_distfs_recovery_audit(
                    dir,
                    "fallback_json",
                    Some(format!("distfs_restore_failed: {:?}", err)),
                );
                Ok(None)
            }
        }
    }

    pub(super) fn has_indexed_sidecar_generation(dir: &Path) -> Result<bool, WorldError> {
        let store_root = dir.join(DISTFS_STATE_DIR);
        Ok(persistence_support::load_sidecar_generation_index(&store_root)?.is_some())
    }

    pub(super) fn load_from_distfs_sidecar(
        snapshot_manifest_path: &Path,
        journal_segments_path: &Path,
        store_root: &Path,
    ) -> Result<(Snapshot, Journal), WorldError> {
        let manifest: SnapshotManifest = read_json_from_path(snapshot_manifest_path)?;
        let journal_segments: Vec<JournalSegmentRef> = read_json_from_path(journal_segments_path)?;
        let store = LocalCasStore::new(store_root);
        let snapshot: Snapshot = assemble_snapshot(&manifest, &store)?;
        let events: Vec<WorldEvent> =
            assemble_journal(&journal_segments, &store, |event: &WorldEvent| event.id)?;
        Ok((snapshot, Journal { events }))
    }

    pub fn load_tick_consensus_records_from_dir(
        dir: impl AsRef<Path>,
        tick_from: Option<WorldTime>,
        tick_to: Option<WorldTime>,
    ) -> Result<Vec<TickConsensusRecord>, WorldError> {
        let snapshot = load_persisted_tick_consensus_snapshot_from_dir(dir.as_ref())?;
        Ok(snapshot
            .tick_consensus_records
            .into_iter()
            .filter(|record| {
                tick_from
                    .map(|from_tick| record.block.header.tick >= from_tick)
                    .unwrap_or(true)
                    && tick_to
                        .map(|to_tick| record.block.header.tick <= to_tick)
                        .unwrap_or(true)
            })
            .collect())
    }

    pub fn verify_tick_consensus_archive_from_dir(dir: impl AsRef<Path>) -> Result<(), WorldError> {
        let snapshot = load_persisted_tick_consensus_snapshot_from_dir(dir.as_ref())?;
        verify_tick_consensus_record_slice(snapshot.tick_consensus_records.as_slice())
    }
}
