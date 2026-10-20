use std::path::Path;
use std::time::{Duration, Instant};

use oasis7::simulator::{
    WorldJournal as SimulatorJournal, WorldKernel, WorldSnapshot as SimulatorSnapshot,
};
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, Journal as RuntimeJournal,
    Snapshot as RuntimeSnapshot, blake3_hex,
};

use super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_rel_path,
    execution_checkpoint_root_dir, load_execution_bridge_record,
    load_execution_checkpoint_manifest, load_highest_valid_execution_bridge_record,
    persist_execution_bridge_record_only,
};
use super::driver::NodeRuntimeExecutionDriver;
use super::driver_observability::{
    RestoreObservation, emit_stale_height_restore_complete, emit_stale_height_restore_start,
    execution_record_recovery_ref_count,
};
use super::driver_persistence::{
    persist_execution_bridge_state, persist_execution_world_with_chain_resource_context,
};
use super::execution_hash::ExecutionHashPayload;
use super::simulator_mirror::persist_simulator_execution_world;
use super::{
    EXECUTION_BRIDGE_RECORD_SCHEMA_V3, EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2,
    ExecutionBridgeRecord, ExecutionCheckpointManifest, WorldHeadProofV1,
};

fn hydrate_compacted_checkpoint_record(
    records_dir: &Path,
    record: &mut ExecutionBridgeRecord,
) -> Result<(Option<ExecutionCheckpointManifest>, bool), String> {
    if record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V3 {
        return Ok((None, false));
    }
    let Some(checkpoint_ref) = record.checkpoint_ref.as_deref() else {
        return Ok((None, false));
    };
    let expected_checkpoint_ref = execution_checkpoint_manifest_rel_path(record.height);
    if checkpoint_ref != expected_checkpoint_ref {
        return Err(format!(
            "execution driver checkpoint manifest ref mismatch at height {}: expected={} actual={}",
            record.height, expected_checkpoint_ref, checkpoint_ref
        ));
    }
    let manifest = load_execution_checkpoint_manifest(
        execution_checkpoint_root_dir(records_dir)
            .join(checkpoint_ref)
            .as_path(),
    )?;
    if manifest.world_id != record.world_id
        || manifest.height != record.height
        || manifest.execution_block_hash != record.execution_block_hash
        || manifest.execution_state_root != record.execution_state_root
    {
        return Err(format!(
            "execution driver checkpoint manifest identity mismatch at height {}",
            record.height
        ));
    }
    if manifest.schema_version < EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V2
        || manifest
            .predecessor_execution_block_hash
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(format!(
            "execution driver checkpoint manifest missing predecessor binding at height {}",
            record.height
        ));
    }
    let snapshot_ref = manifest
        .snapshot_ref
        .as_deref()
        .unwrap_or(manifest.latest_state_ref.as_str());
    let journal_ref = manifest
        .journal_ref
        .as_deref()
        .filter(|reference| !reference.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "execution driver checkpoint manifest missing journal ref at height {}",
                record.height
            )
        })?;
    let refs_compacted = record.latest_state_ref.is_none()
        && record.snapshot_ref.is_none()
        && record.journal_ref.is_none();
    if refs_compacted {
        record.latest_state_ref = Some(manifest.latest_state_ref.clone());
        record.snapshot_ref = Some(snapshot_ref.to_string());
        record.journal_ref = Some(journal_ref.to_string());
    } else if record.latest_state_ref.as_deref() != Some(manifest.latest_state_ref.as_str())
        || record.snapshot_ref.as_deref() != Some(snapshot_ref)
        || record.journal_ref.as_deref() != Some(journal_ref)
    {
        return Err(format!(
            "execution driver checkpoint manifest refs mismatch at height {}",
            record.height
        ));
    }
    Ok((Some(manifest), refs_compacted))
}

impl NodeRuntimeExecutionDriver {
    fn recover_runtime_journal_from_loaded_world(
        &self,
        snapshot: &RuntimeSnapshot,
        target_height: u64,
    ) -> Result<RuntimeJournal, String> {
        let loaded_journal = self.execution_world.journal().clone();
        if snapshot.journal_len > loaded_journal.len() {
            return Err(format!(
                "execution record at height {} missing journal_ref and loaded execution world only has {} events, need at least {}",
                target_height,
                loaded_journal.len(),
                snapshot.journal_len
            ));
        }

        let mut recovered = loaded_journal;
        recovered.events.truncate(snapshot.journal_len);
        let recovered_last_event_id = recovered.events.last().map(|event| event.id).unwrap_or(0);
        if recovered_last_event_id != snapshot.last_event_id {
            return Err(format!(
                "execution record at height {} missing journal_ref and loaded execution world journal prefix mismatches snapshot last_event_id expected={} actual={}",
                target_height, snapshot.last_event_id, recovered_last_event_id
            ));
        }

        Ok(recovered)
    }

    pub(super) fn restore_startup_execution_head(&mut self) -> Result<(), String> {
        let target_height = self.state.last_applied_committed_height;
        let record = load_highest_valid_execution_bridge_record(self.records_dir.as_path())?
            .ok_or_else(|| {
                format!(
                    "execution driver authoritative startup has no valid durable record while reconciling state head {}",
                    target_height
                )
            })?;
        if !self.restore_execution_head_from_record(record.world_id.as_str(), record.height)? {
            return Err(format!(
                "execution driver authoritative startup durable record missing at height {} while state head is {}",
                record.height, target_height
            ));
        }
        Ok(())
    }

    fn validate_recovered_execution_record(
        &self,
        record: &ExecutionBridgeRecord,
        snapshot_ref: &str,
        snapshot_bytes: &[u8],
        snapshot: &RuntimeSnapshot,
        journal: &RuntimeJournal,
        checkpoint_manifest: Option<&ExecutionCheckpointManifest>,
    ) -> Result<(), String> {
        if record.height == 0 || record.world_id.trim().is_empty() {
            return Err(format!(
                "execution driver authoritative record has invalid identity height={} world_id={}",
                record.height, record.world_id
            ));
        }
        if record.schema_version >= EXECUTION_BRIDGE_RECORD_SCHEMA_V3 {
            let checkpoint_install_record = record.checkpoint_ref.is_some()
                && record.proposer_id.is_none()
                && record.action_root.is_none();
            if record.latest_state_ref.as_deref() != Some(snapshot_ref)
                || record.snapshot_ref.as_deref() != Some(snapshot_ref)
                || record.journal_ref.as_deref().is_none_or(str::is_empty)
                || record.node_block_hash.as_deref().is_none_or(str::is_empty)
                || (!checkpoint_install_record
                    && (record.proposer_id.as_deref().is_none_or(str::is_empty)
                        || record.action_root.as_deref().is_none_or(str::is_empty)))
            {
                return Err(format!(
                    "execution driver authoritative v3 record missing or mismatching refs at height {}",
                    record.height
                ));
            }
        }
        let actual_state_root = blake3_hex(snapshot_bytes);
        if actual_state_root != record.execution_state_root {
            return Err(format!(
                "execution driver authoritative snapshot root mismatch at height {}: expected={} actual={}",
                record.height, record.execution_state_root, actual_state_root
            ));
        }
        if snapshot.journal_len != record.journal_len || journal.len() != record.journal_len {
            return Err(format!(
                "execution driver authoritative journal length mismatch at height {}: snapshot={} record={} actual={}",
                record.height,
                snapshot.journal_len,
                record.journal_len,
                journal.len()
            ));
        }
        let checkpoint_install_record = record.checkpoint_ref.is_some()
            && record.proposer_id.is_none()
            && record.action_root.is_none();
        if record.schema_version >= EXECUTION_BRIDGE_RECORD_SCHEMA_V3 && !checkpoint_install_record
        {
            let proof_ref = record.world_head_proof_ref.as_deref().ok_or_else(|| {
                format!(
                    "execution driver authoritative v3 record missing world head proof ref at height {}",
                    record.height
                )
            })?;
            let expected_proof_hash = record.world_head_proof_hash.as_deref().ok_or_else(|| {
                format!(
                    "execution driver authoritative v3 record missing world head proof hash at height {}",
                    record.height
                )
            })?;
            let proof_bytes = self.execution_store.get_verified(proof_ref).map_err(|err| {
                format!(
                    "execution driver authoritative world head proof ref {} failed at height {}: {:?}",
                    proof_ref, record.height, err
                )
            })?;
            let proof = serde_cbor::from_slice::<WorldHeadProofV1>(proof_bytes.as_slice())
                .map_err(|err| {
                    format!(
                        "execution driver decode world head proof failed at height {}: {}",
                        record.height, err
                    )
                })?;
            let actual_proof_hash = proof.proof_hash()?;
            if actual_proof_hash != expected_proof_hash
                || proof.world_id != record.world_id
                || proof.height != record.height
                || proof.timestamp_ms != record.timestamp_ms
                || proof.execution.execution_block_hash != record.execution_block_hash
                || proof.execution.execution_state_root != record.execution_state_root
                || proof.execution.node_block_hash
                    != record.node_block_hash.as_deref().unwrap_or("")
                || proof.execution.action_root != record.action_root.as_deref().unwrap_or("")
                || proof.consensus.proposer_id != record.proposer_id.as_deref().unwrap_or("")
                || proof.snapshot_manifest_ref.content_hash != snapshot_ref
                || proof.journal_segments_ref.content_hash
                    != record.journal_ref.as_deref().unwrap_or("")
            {
                return Err(format!(
                    "execution driver authoritative world head proof mismatch at height {}",
                    record.height
                ));
            }
        }
        let previous_execution_block_hash = if let Some(manifest) = checkpoint_manifest {
            manifest
                .predecessor_execution_block_hash
                .as_deref()
                .filter(|hash| !hash.is_empty())
                .ok_or_else(|| {
                    format!(
                        "execution checkpoint manifest missing predecessor execution block hash at height {}",
                        record.height
                    )
                })?
                .to_string()
        } else if checkpoint_install_record {
            let checkpoint_ref = record.checkpoint_ref.as_deref().ok_or_else(|| {
                format!(
                    "execution checkpoint-install record missing checkpoint ref at height {}",
                    record.height
                )
            })?;
            let manifest = load_execution_checkpoint_manifest(
                execution_checkpoint_root_dir(self.records_dir.as_path())
                    .join(checkpoint_ref)
                    .as_path(),
            )?;
            if manifest.world_id != record.world_id
                || manifest.height != record.height
                || manifest.execution_block_hash != record.execution_block_hash
                || manifest.execution_state_root != record.execution_state_root
            {
                return Err(format!(
                    "execution checkpoint-install manifest mismatch at height {}",
                    record.height
                ));
            }
            manifest
                .predecessor_execution_block_hash
                .filter(|hash| !hash.is_empty())
                .ok_or_else(|| {
                    format!(
                        "execution checkpoint-install manifest missing predecessor execution block hash at height {}",
                        record.height
                    )
                })?
        } else if let Some(baseline) = self
            .local_execution_bootstrap
            .as_ref()
            .filter(|baseline| record.height == baseline.height.saturating_add(1))
        {
            baseline.execution_block_hash.clone()
        } else if record.height == 1 {
            "genesis".to_string()
        } else if self.world_dir.join("world-service-identity.json").exists()
            && !execution_bridge_record_path(self.records_dir.as_path(), record.height - 1).exists()
        {
            super::service_bootstrap_boundary::predecessor(self, record)?
        } else {
            let predecessor_path =
                execution_bridge_record_path(self.records_dir.as_path(), record.height - 1);
            let predecessor = load_execution_bridge_record(predecessor_path.as_path()).map_err(|err| {
                format!(
                    "execution driver authoritative predecessor record unavailable at height {}: {}",
                    record.height - 1,
                    err
                )
            })?;
            if predecessor.height != record.height - 1 || predecessor.world_id != record.world_id {
                return Err(format!(
                    "execution driver authoritative predecessor record mismatch at height {}",
                    record.height - 1
                ));
            }
            predecessor.execution_block_hash
        };
        let expected_execution_block_hash = blake3_hex(
            super::to_cbor(ExecutionHashPayload {
                world_id: record.world_id.as_str(),
                height: record.height,
                prev_execution_block_hash: previous_execution_block_hash.as_str(),
                execution_state_root: record.execution_state_root.as_str(),
                journal_len: record.journal_len,
            })?
            .as_slice(),
        );
        if expected_execution_block_hash != record.execution_block_hash {
            return Err(format!(
                "execution driver authoritative execution block mismatch at height {}: expected={} actual={}",
                record.height, expected_execution_block_hash, record.execution_block_hash
            ));
        }
        Ok(())
    }

    pub(super) fn restore_execution_head_from_record(
        &mut self,
        expected_world_id: &str,
        target_height: u64,
    ) -> Result<bool, String> {
        let restore_started_at = Instant::now();
        let record_path = execution_bridge_record_path(self.records_dir.as_path(), target_height);
        if !record_path.exists() {
            return Ok(false);
        }

        let mut record = load_execution_bridge_record(record_path.as_path())?;
        if record.height != target_height {
            return Err(format!(
                "execution driver record height mismatch at path height {}: record_height={}",
                target_height, record.height
            ));
        }
        if record.world_id != expected_world_id {
            return Err(format!(
                "execution driver stale-height restore world_id mismatch at height {}: expected={} actual={}",
                target_height, expected_world_id, record.world_id
            ));
        }
        let (checkpoint_manifest, record_was_compacted) =
            hydrate_compacted_checkpoint_record(self.records_dir.as_path(), &mut record)?;
        let world_policy = self.execution_world.release_security_policy().clone();
        let snapshot_ref = record
            .recovery_snapshot_ref()
            .ok_or_else(|| {
                format!(
                    "execution record at height {} missing latest_state_ref",
                    target_height
                )
            })?
            .to_string();
        if record.schema_version >= EXECUTION_BRIDGE_RECORD_SCHEMA_V3
            && (record.latest_state_ref.as_deref() != Some(snapshot_ref.as_str())
                || record.snapshot_ref.as_deref() != Some(snapshot_ref.as_str())
                || record.journal_ref.as_deref().is_none_or(str::is_empty))
        {
            return Err(format!(
                "execution driver authoritative v3 record missing exact CAS refs at height {}",
                target_height
            ));
        }
        let pinned_ref_count = execution_record_recovery_ref_count(&record);
        let simulator_mirror_present = record.simulator_mirror.is_some();
        emit_stale_height_restore_start(
            expected_world_id,
            target_height,
            pinned_ref_count,
            simulator_mirror_present,
        );

        let snapshot_blob_started_at = Instant::now();
        let snapshot_bytes = self
            .execution_store
            .get_verified(snapshot_ref.as_str())
            .map_err(|err| {
                format!(
                    "execution driver authoritative CAS snapshot ref {} failed at height {}: {:?}",
                    snapshot_ref, target_height, err
                )
            })?;
        let mut blob_store_ms = snapshot_blob_started_at.elapsed();
        let mut blob_count = 1_usize;
        let mut bundle_bytes = snapshot_bytes.len();
        let snapshot_bytes_len = snapshot_bytes.len();

        let runtime_decode_started_at = Instant::now();
        let snapshot = serde_cbor::from_slice::<RuntimeSnapshot>(snapshot_bytes.as_slice())
            .map_err(|err| {
                format!(
                    "execution driver decode runtime snapshot failed at height {}: {}",
                    target_height, err
                )
            })?;
        let journal_bytes_len;
        let (journal, recovered_journal_ref) = match record.journal_ref.as_deref() {
            Some(journal_ref) => {
                let journal_blob_started_at = Instant::now();
                let journal_bytes =
                    self.execution_store
                        .get_verified(journal_ref)
                        .map_err(|err| {
                            format!(
                                "execution driver authoritative CAS journal ref {} failed at height {}: {:?}",
                                journal_ref, target_height, err
                            )
                        })?;
                blob_store_ms += journal_blob_started_at.elapsed();
                blob_count = blob_count.saturating_add(1);
                journal_bytes_len = journal_bytes.len();
                bundle_bytes = bundle_bytes.saturating_add(journal_bytes_len);
                let journal = serde_cbor::from_slice::<RuntimeJournal>(journal_bytes.as_slice())
                    .map_err(|err| {
                        format!(
                            "execution driver decode runtime journal failed at height {}: {}",
                            target_height, err
                        )
                    })?;
                (journal, None)
            }
            None => {
                let journal =
                    self.recover_runtime_journal_from_loaded_world(&snapshot, target_height)?;
                let journal_bytes = super::to_cbor(journal.clone())?;
                journal_bytes_len = journal_bytes.len();
                bundle_bytes = bundle_bytes.saturating_add(journal_bytes_len);
                let recovered_ref = self
                    .execution_store
                    .put_bytes(journal_bytes.as_slice())
                    .map_err(|err| {
                        format!(
                            "execution driver recover journal CAS put failed at height {}: {:?}",
                            target_height, err
                        )
                    })?;
                (journal, Some(recovered_ref))
            }
        };
        self.validate_recovered_execution_record(
            &record,
            snapshot_ref.as_str(),
            snapshot_bytes.as_slice(),
            &snapshot,
            &journal,
            checkpoint_manifest.as_ref(),
        )?;
        let decode_ms = runtime_decode_started_at.elapsed();

        let restored_resource_manifest = snapshot.chain_resource_manifest.clone();
        let restored_resource_delta = snapshot.latest_chain_resource_delta.clone();
        let runtime_rebuild_started_at = Instant::now();
        let mut restored_world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
            .map_err(|err| {
                format!(
                    "execution driver rebuild runtime world failed at height {}: {:?}",
                    target_height, err
                )
            })?;
        // Module artifact bytes are restored by `World::from_snapshot` from
        // the canonical CAS snapshot payload. Do not consult `world_dir`
        // here: it may contain a newer module-store cache than this
        // historical execution head.
        restored_world.set_release_security_policy(world_policy);
        let mut rebuild_ms = runtime_rebuild_started_at.elapsed();
        let restored_commit_block_hash = restored_resource_delta
            .as_ref()
            .and_then(|delta| delta.commit_block_hash.as_deref())
            .or(restored_resource_manifest.created_at_block_hash.as_deref());
        let restored_resource_context = ChainResourceDerivationContext {
            world_id: restored_resource_manifest.world_id.as_str(),
            chain_id: restored_resource_manifest.chain_id.as_str(),
            genesis_ref: restored_resource_manifest.genesis_ref.as_deref(),
            created_at_height: restored_resource_manifest.created_at_height,
            manifest_height: restored_resource_manifest.manifest_height,
            commit_block_hash: restored_commit_block_hash,
            tick: restored_world.state().time,
        };
        let mut persist_ms = Duration::default();
        let world_persist_started_at = Instant::now();
        persist_execution_world_with_chain_resource_context(
            self.world_dir.as_path(),
            &restored_world,
            restored_resource_context,
            restored_resource_manifest.world_config_hash.as_str(),
            restored_resource_manifest
                .generation_algorithm_hash
                .as_str(),
        )?;
        persist_ms += world_persist_started_at.elapsed();
        self.execution_world = restored_world;

        let simulator_restore_started_at = Instant::now();
        let mut simulator_snapshot_bytes_len = 0_usize;
        let mut simulator_journal_bytes_len = 0_usize;
        if let Some(simulator_mirror) = record.simulator_mirror.as_ref() {
            let simulator_blob_started_at = Instant::now();
            let simulator_snapshot_bytes = self
                .execution_store
                .get_verified(simulator_mirror.snapshot_ref.as_str())
                .map_err(|err| {
                    format!(
                        "execution driver restore simulator snapshot ref {} failed at height {}: {:?}",
                        simulator_mirror.snapshot_ref, target_height, err
                    )
                })?;
            let simulator_journal_bytes = self
                .execution_store
                .get_verified(simulator_mirror.journal_ref.as_str())
                .map_err(|err| {
                    format!(
                        "execution driver restore simulator journal ref {} failed at height {}: {:?}",
                        simulator_mirror.journal_ref, target_height, err
                    )
                })?;
            blob_store_ms += simulator_blob_started_at.elapsed();
            blob_count = blob_count.saturating_add(2);
            simulator_snapshot_bytes_len = simulator_snapshot_bytes.len();
            simulator_journal_bytes_len = simulator_journal_bytes.len();
            bundle_bytes = bundle_bytes
                .saturating_add(simulator_snapshot_bytes_len)
                .saturating_add(simulator_journal_bytes_len);
            let simulator_snapshot =
                serde_cbor::from_slice::<SimulatorSnapshot>(simulator_snapshot_bytes.as_slice())
                    .map_err(|err| {
                        format!(
                            "execution driver decode simulator snapshot failed at height {}: {}",
                            target_height, err
                        )
                    })?;
            let simulator_journal =
                serde_cbor::from_slice::<SimulatorJournal>(simulator_journal_bytes.as_slice())
                    .map_err(|err| {
                        format!(
                            "execution driver decode simulator journal failed at height {}: {}",
                            target_height, err
                        )
                    })?;
            let simulator_rebuild_started_at = Instant::now();
            let restored_simulator =
                WorldKernel::from_snapshot(simulator_snapshot, simulator_journal).map_err(
                    |err| {
                        format!(
                            "execution driver rebuild simulator mirror failed at height {}: {:?}",
                            target_height, err
                        )
                    },
                )?;
            rebuild_ms += simulator_rebuild_started_at.elapsed();
            let simulator_persist_started_at = Instant::now();
            persist_simulator_execution_world(
                self.simulator_world_dir.as_path(),
                &restored_simulator,
                None,
            )?;
            persist_ms += simulator_persist_started_at.elapsed();
            self.simulator_mirror = restored_simulator;
        }
        let simulator_restore_ms = if simulator_mirror_present {
            simulator_restore_started_at.elapsed()
        } else {
            Duration::default()
        };

        if record_was_compacted
            || record.latest_state_ref.is_none()
            || record.snapshot_ref.is_none()
            || record.journal_ref.is_none()
        {
            if record.latest_state_ref.is_none() {
                record.latest_state_ref = Some(snapshot_ref.clone());
            }
            if record.snapshot_ref.is_none() {
                record.snapshot_ref = Some(snapshot_ref.clone());
            }
            if record.journal_ref.is_none() {
                record.journal_ref = recovered_journal_ref;
            }
            let record_persist_started_at = Instant::now();
            persist_execution_bridge_record_only(self.records_dir.as_path(), &record)?;
            persist_ms += record_persist_started_at.elapsed();
        }

        self.state.last_applied_committed_height = record.height;
        self.state.last_execution_block_hash = Some(record.execution_block_hash);
        self.state.last_execution_state_root = Some(record.execution_state_root);
        self.state.last_node_block_hash = record.node_block_hash;
        let state_persist_started_at = Instant::now();
        persist_execution_bridge_state(self.state_path.as_path(), &self.state)?;
        persist_ms += state_persist_started_at.elapsed();

        emit_stale_height_restore_complete(RestoreObservation {
            world_id: expected_world_id,
            height: target_height,
            pinned_ref_count,
            blob_count,
            bundle_bytes,
            snapshot_bytes: snapshot_bytes_len,
            journal_bytes: journal_bytes_len,
            simulator_mirror_present,
            simulator_snapshot_bytes: simulator_snapshot_bytes_len,
            simulator_journal_bytes: simulator_journal_bytes_len,
            blob_store_ms,
            decode_ms,
            rebuild_ms,
            simulator_restore_ms,
            persist_ms,
            total_ms: restore_started_at.elapsed(),
        });

        Ok(true)
    }
}
