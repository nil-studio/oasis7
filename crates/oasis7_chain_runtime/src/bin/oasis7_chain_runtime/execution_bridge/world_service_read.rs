//! One immutable record pins snapshot, journal and proof. Mutable cache paths
//! are never a read authority for WorldService.
use super::{ExecutionBridgeRecord, WorldHeadProofV1, checkpoint};
use oasis7::world_service::*;
use oasis7_runtime::{Journal, LocalCasStore, Snapshot, World, blake3_hex};
use std::path::Path;

#[path = "world_service_restore_cache.rs"]
mod restore_cache;

pub(crate) struct PinnedWorld {
    pub world: World,
    pub(super) record: ExecutionBridgeRecord,
    pub commit: CommitRef,
}

pub(crate) fn identity(world_dir: &Path, expected_world_id: &str) -> Result<WorldIdentity, String> {
    let bytes = std::fs::read(world_dir.join("world-service-identity.json"))
        .map_err(|_| "service identity configuration unavailable")?;
    let identity: WorldIdentity = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    identity.validate().map_err(|e| e.to_string())?;
    if identity.world_id != expected_world_id {
        return Err("service identity differs from runtime world".into());
    }
    Ok(identity)
}

pub(crate) fn pin(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    fixed: Option<&CommitRef>,
) -> Result<PinnedWorld, String> {
    load(
        records,
        storage,
        identity,
        fixed.map(|commit| commit.position),
        fixed,
    )
}

/// Verify the immutable initial publication package without consulting live
/// CAS. The caller authenticates each archived byte string and the original
/// node signature before using this result; current heads still use `pin`.
pub(crate) fn verify_archived_initial_publication(
    record_bytes: &[u8],
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    proof_bytes: &[u8],
    identity: &WorldIdentity,
    expected_commit: &CommitRef,
) -> Result<(), String> {
    let record: ExecutionBridgeRecord =
        serde_json::from_slice(record_bytes).map_err(|_| "initial execution record invalid")?;
    let snapshot_ref = record
        .snapshot_ref
        .as_deref()
        .ok_or("record snapshot ref unavailable")?;
    let journal_ref = record
        .journal_ref
        .as_deref()
        .ok_or("record journal ref unavailable")?;
    let proof_ref = record
        .world_head_proof_ref
        .as_deref()
        .ok_or("record world head proof unavailable")?;
    if record.world_id != identity.world_id
        || record.height == 0
        || record.height != expected_commit.position
        || record.latest_state_ref.as_deref() != Some(snapshot_ref)
        || record.execution_state_root != blake3_hex(snapshot_bytes)
        || blake3_hex(snapshot_bytes) != snapshot_ref
        || blake3_hex(journal_bytes) != journal_ref
        || blake3_hex(proof_bytes) != proof_ref
        || expected_commit.world != *identity
        || expected_commit.state_root_ref != snapshot_ref
        || expected_commit.execution_block_hash != record.execution_block_hash
    {
        return Err("archived initial refs differ from the signed original commit".into());
    }
    let snapshot: Snapshot =
        serde_cbor::from_slice(snapshot_bytes).map_err(|_| "initial snapshot invalid")?;
    let journal: Journal =
        serde_cbor::from_slice(journal_bytes).map_err(|_| "initial journal invalid")?;
    if snapshot.journal_len != record.journal_len
        || journal.len() != record.journal_len
        || journal.events.last().map_or(0, |event| event.id) != snapshot.last_event_id
    {
        return Err("pinned snapshot/journal root or boundary mismatch".into());
    }
    let proof: WorldHeadProofV1 =
        serde_cbor::from_slice(proof_bytes).map_err(|_| "initial world head proof invalid")?;
    proof.validate_contract()?;
    if Some(proof.proof_hash()?.as_str()) != record.world_head_proof_hash.as_deref()
        || proof.world_id != record.world_id
        || proof.height != record.height
        || proof.timestamp_ms != record.timestamp_ms
        || proof.execution.execution_state_root != record.execution_state_root
        || proof.execution.execution_block_hash != record.execution_block_hash
        || proof.execution.node_block_hash != record.node_block_hash.as_deref().unwrap_or("")
        || proof.execution.action_root != record.action_root.as_deref().unwrap_or("")
        || proof.snapshot_manifest_ref.content_hash != snapshot_ref
        || proof.journal_segments_ref.content_hash != journal_ref
    {
        return Err("pinned proof does not bind execution record and CAS refs".into());
    }
    let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        .map_err(|_| "pinned world snapshot could not be restored")?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|_| "pinned binding unavailable")?;
    if binding.world_id != identity.world_id {
        return Err("pinned binding world mismatch".into());
    }
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: binding.world_id,
            branch_id: binding.branch_id,
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    binding.finality_status,
                    binding.finality_block_hash,
                ),
            )?,
            reorg_generation: binding.reorg_epoch,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: world
                .capability_revocation_state()
                .authority_records
                .values()
                .map(|record| record.governance_epoch)
                .max()
                .unwrap_or(0),
            permission_generation: world.capability_revocation_state().epoch,
        },
        position: record.height,
        execution_block_hash: record.execution_block_hash,
        state_root_ref: snapshot_ref.into(),
    };
    commit.validate().map_err(|e| e.to_string())?;
    if &commit != expected_commit {
        return Err("archived initial proof differs from signed original commit".into());
    }
    Ok(())
}

pub(crate) fn pin_at_height(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: u64,
) -> Result<PinnedWorld, String> {
    load(records, storage, identity, Some(height), None)
}

/// Historical result reads may outlive the retained full material window.
pub(crate) fn pin_retained_at_height(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: u64,
) -> Result<Option<PinnedWorld>, String> {
    let path = checkpoint::execution_bridge_record_path(records, height);
    match std::fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("historical record unavailable".into()),
        Ok(_) => {}
    }
    let record = checkpoint::load_execution_bridge_record(&path)?;
    if record.world_id != identity.world_id || record.height != height || height == 0 {
        return Err("historical record world or height invalid".into());
    }
    if record.snapshot_ref.is_none()
        && record.latest_state_ref.is_none()
        && record.journal_ref.is_none()
    {
        return Ok(None);
    }
    pin_at_height(records, storage, identity, height).map(Some)
}

fn load(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: Option<u64>,
    fixed: Option<&CommitRef>,
) -> Result<PinnedWorld, String> {
    load_with_cache(
        records,
        storage,
        identity,
        height,
        fixed,
        restore_cache::shared(),
    )
}

fn load_with_cache(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: Option<u64>,
    fixed: Option<&CommitRef>,
    cache: &std::sync::Mutex<restore_cache::RestoreCache>,
) -> Result<PinnedWorld, String> {
    if let Some(commit) = fixed
        && &commit.world != identity
    {
        return Err("fixed commit world mismatch".into());
    }
    let record = if let Some(height) = height {
        checkpoint::load_execution_bridge_record(&checkpoint::execution_bridge_record_path(
            records, height,
        ))?
    } else {
        checkpoint::load_highest_valid_execution_bridge_record(records)?
            .ok_or("committed view unavailable")?
    };
    if record.world_id != identity.world_id || record.height == 0 {
        return Err("record world or height invalid".into());
    }
    let store = LocalCasStore::new(storage);
    let snapshot_ref = record
        .snapshot_ref
        .as_deref()
        .ok_or("record snapshot ref unavailable")?;
    let journal_ref = record
        .journal_ref
        .as_deref()
        .ok_or("record journal ref unavailable")?;
    if record.latest_state_ref.as_deref() != Some(snapshot_ref) {
        return Err("record snapshot refs disagree".into());
    }
    let snapshot_bytes = store
        .get_verified(snapshot_ref)
        .map_err(|e| format!("snapshot CAS: {e:?}"))?;
    let journal_bytes = store
        .get_verified(journal_ref)
        .map_err(|e| format!("journal CAS: {e:?}"))?;
    let snapshot: Snapshot = serde_cbor::from_slice(&snapshot_bytes).map_err(|e| e.to_string())?;
    let journal: Journal = serde_cbor::from_slice(&journal_bytes).map_err(|e| e.to_string())?;
    if blake3_hex(&snapshot_bytes) != record.execution_state_root
        || snapshot.journal_len != record.journal_len
        || journal.len() != record.journal_len
        || journal.events.last().map_or(0, |e| e.id) != snapshot.last_event_id
    {
        return Err("pinned snapshot/journal root or boundary mismatch".into());
    }
    let proof_ref = record
        .world_head_proof_ref
        .as_deref()
        .ok_or("record world head proof unavailable")?;
    let proof_bytes = store
        .get_verified(proof_ref)
        .map_err(|e| format!("proof CAS: {e:?}"))?;
    let proof: WorldHeadProofV1 =
        serde_cbor::from_slice(&proof_bytes).map_err(|e| e.to_string())?;
    proof.validate_contract()?;
    if Some(proof.proof_hash()?.as_str()) != record.world_head_proof_hash.as_deref()
        || proof.world_id != record.world_id
        || proof.height != record.height
        || proof.timestamp_ms != record.timestamp_ms
        || proof.execution.execution_state_root != record.execution_state_root
        || proof.execution.execution_block_hash != record.execution_block_hash
        || proof.execution.node_block_hash != record.node_block_hash.as_deref().unwrap_or("")
        || proof.execution.action_root != record.action_root.as_deref().unwrap_or("")
        || proof.snapshot_manifest_ref.content_hash != snapshot_ref
        || proof.journal_segments_ref.content_hash != journal_ref
    {
        return Err("pinned proof does not bind execution record and CAS refs".into());
    }
    let world = restore_cache::restore(cache, &snapshot_bytes, &journal_bytes, snapshot, journal)
        .map_err(|e| format!("pinned world: {e:?}"))?;
    let binding = world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("binding unavailable: {e:?}"))?;
    if binding.world_id != identity.world_id {
        return Err("pinned binding world mismatch".into());
    }
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: binding.world_id,
            branch_id: binding.branch_id,
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    binding.finality_status,
                    binding.finality_block_hash,
                ),
            )?,
            reorg_generation: binding.reorg_epoch,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: world
                .capability_revocation_state()
                .authority_records
                .values()
                .map(|record| record.governance_epoch)
                .max()
                .unwrap_or(0),
            permission_generation: world.capability_revocation_state().epoch,
        },
        position: record.height,
        execution_block_hash: record.execution_block_hash.clone(),
        state_root_ref: snapshot_ref.into(),
    };
    commit.validate().map_err(|e| e.to_string())?;
    if fixed.is_some_and(|fixed| fixed != &commit) {
        return Err("fixed commit reference conflict".into());
    }
    Ok(PinnedWorld {
        world,
        record,
        commit,
    })
}

/// Engineering reader uses the writer's verified publication fence. A missing
/// tail cannot turn an older valid prefix into current read authority.
pub(crate) fn pin_guarded(
    records: &Path,
    config: &Path,
    authority: &crate::controlled_live_config::GuardedReadAuthority,
    fixed: Option<&CommitRef>,
    height: Option<u64>,
) -> Result<PinnedWorld, String> {
    pin_guarded_with_capture_barrier(records, config, authority, fixed, height, || {})
}

pub(super) fn pin_guarded_with_capture_barrier(
    records: &Path,
    config: &Path,
    authority: &crate::controlled_live_config::GuardedReadAuthority,
    fixed: Option<&CommitRef>,
    height: Option<u64>,
    after_capture: impl FnOnce(),
) -> Result<PinnedWorld, String> {
    let trusted = crate::controlled_live_config::GuardedAuthority::load(config)?;
    authority.check_configuration(&trusted.configuration_digest)?;
    if records != trusted.config.records_directory {
        return Err("guarded read private directory changed".into());
    }
    let captured_fence = authority.current()?;
    let current = super::controlled_live_history::load_head(&trusted, records, None)?;
    if current.head != captured_fence {
        return Err("guarded current publication tail is missing or changed".into());
    }
    after_capture();
    let head = match (height, fixed) {
        (None, None) => current,
        (Some(height), _) => {
            super::controlled_live_history::load_at_height(&trusted, records, height)?
        }
        (None, Some(fixed)) => {
            super::controlled_live_history::load_head(&trusted, records, Some(fixed))?
        }
    };
    if head.head.position > captured_fence.position
        || (head.head.position == captured_fence.position && head.head != captured_fence)
    {
        return Err("guarded pin exceeds or conflicts with captured publication fence".into());
    }
    let commit = head.commit(&trusted)?;
    if fixed.is_some_and(|fixed| fixed != &commit) {
        return Err("guarded fixed commit reference conflict".into());
    }
    let world = head.world(&trusted.release_policy)?;
    Ok(PinnedWorld {
        world,
        record: head.record,
        commit,
    })
}

#[cfg(test)]
#[path = "tests/world_service_read_cache.rs"]
mod cache_tests;
