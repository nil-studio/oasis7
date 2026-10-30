//! One immutable record pins snapshot, journal and proof. Mutable cache paths
//! are never a read authority for WorldService.
use super::{ExecutionBridgeRecord, WorldHeadProofV1, checkpoint};
use oasis7::world_service::*;
use oasis7_runtime::{Journal, LocalCasStore, Snapshot, World, blake3_hex};
use std::path::Path;

#[path = "world_service_restore_cache.rs"]
mod restore_cache;

/// Trusted host restore configuration; never decoded from an HTTP request.
/// Defaults preserve legacy release behavior without granting local authority.
/// Running services always pass their effective host context explicitly.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RestoreContext {
    release: oasis7_runtime::ReleaseSecurityPolicy,
    local: Option<oasis7_runtime::DevLocalParticipantAuthority>,
}
impl RestoreContext {
    pub(crate) fn new(
        release: oasis7_runtime::ReleaseSecurityPolicy,
        local: Option<oasis7_runtime::DevLocalParticipantAuthority>,
    ) -> Result<Self, String> {
        if local
            .as_ref()
            .is_some_and(|authority| authority.release_security_policy() != &release)
        {
            return Err(
                "WorldService restore authority differs from effective release policy".into(),
            );
        }
        Ok(Self { release, local })
    }
    pub(crate) fn restore(
        &self,
        snapshot: Snapshot,
        journal: Journal,
    ) -> Result<World, oasis7_runtime::WorldError> {
        if let Some(local) = &self.local {
            oasis7::runtime_host::world::from_snapshot_with_devlocal_participant_authority(
                snapshot,
                journal,
                local.clone(),
            )
        } else {
            oasis7::runtime_host::world::from_snapshot_with_release_security_policy(
                snapshot,
                journal,
                self.release.clone(),
            )
        }
    }
}

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
    pin_with_context(
        records,
        storage,
        identity,
        fixed,
        &RestoreContext::default(),
    )
}

pub(crate) fn pin_with_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    fixed: Option<&CommitRef>,
    context: &RestoreContext,
) -> Result<PinnedWorld, String> {
    load_with_context(
        records,
        storage,
        identity,
        fixed.map(|commit| commit.position),
        fixed,
        context,
    )
}

/// Offline administrative reads require the exact trusted current head. Unlike
/// ordinary historical recovery, a malformed newest numbered record must not
/// silently select an older valid prefix or a mutable world cache.
pub(crate) fn pin_offline_current_with_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    expected: Option<&CommitRef>,
    context: &RestoreContext,
) -> Result<PinnedWorld, String> {
    fn latest_record(records: &Path) -> Result<Option<ExecutionBridgeRecord>, String> {
        let path = records.join("latest.json");
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                checkpoint::load_execution_bridge_record(&path).map(Some)
            }
            Ok(_) => Err("offline latest record is not a regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("offline latest record unavailable".into()),
        }
    }
    let heights = checkpoint::list_execution_bridge_record_heights(records)?;
    let height = heights.last().copied().ok_or(
        "offline canonical read requires numbered execution records; bootstrap unsupported",
    )?;
    // A convenience pointer cannot choose the head, but an advertised newer
    // tail is evidence that selecting an older numbered prefix would downgrade.
    let latest = latest_record(records)?;
    if latest.as_ref().is_some_and(|record| record.height > height) {
        return Err("offline latest pointer advertises a missing numbered tail".into());
    }
    if expected.is_some_and(|expected| expected.world != *identity || height != expected.position) {
        return Err("offline canonical head differs from trusted expected commit".into());
    }
    let pinned = pin_at_height_with_context(records, storage, identity, height, context)?;
    if pinned.commit.position != height
        || expected.is_some_and(|expected| pinned.commit != *expected)
        || checkpoint::list_execution_bridge_record_heights(records)? != heights
        || latest_record(records)? != latest
    {
        return Err(
            "offline canonical head changed or differs from trusted expected commit".into(),
        );
    }
    Ok(pinned)
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
    verify_archived_initial_publication_inner(
        record_bytes,
        snapshot_bytes,
        journal_bytes,
        proof_bytes,
        identity,
        expected_commit,
        None,
    )
}

/// Same immutable archive checks with a separately trusted Host restore context.
/// Snapshot fields never select a DevLocal authority or release policy.
pub(crate) fn verify_archived_initial_publication_with_context(
    record_bytes: &[u8],
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    proof_bytes: &[u8],
    identity: &WorldIdentity,
    expected_commit: &CommitRef,
    context: &RestoreContext,
) -> Result<(), String> {
    verify_archived_initial_publication_inner(
        record_bytes,
        snapshot_bytes,
        journal_bytes,
        proof_bytes,
        identity,
        expected_commit,
        Some(context),
    )
}
fn verify_archived_initial_publication_inner(
    record_bytes: &[u8],
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    proof_bytes: &[u8],
    identity: &WorldIdentity,
    expected_commit: &CommitRef,
    context: Option<&RestoreContext>,
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
    let snapshot: Snapshot = oasis7_world_state::codec::snapshot_from_cbor(snapshot_bytes)
        .map_err(|_| "initial snapshot invalid")?;
    let journal: Journal = oasis7_world_state::codec::journal_from_cbor(journal_bytes)
        .map_err(|_| "initial journal invalid")?;
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
    let world = match context {
        Some(context) => context.restore(snapshot, journal),
        None => oasis7::runtime_host::world::from_snapshot(snapshot, journal),
    }
    .map_err(|error| match (context, error) {
        (None, _) => "pinned world snapshot could not be restored",
        (Some(_), oasis7_runtime::WorldError::JournalMismatch) => {
            "archived World restore rejected: journal_mismatch"
        }
        (Some(_), oasis7_runtime::WorldError::DistributedValidationFailed { reason }) => {
            if reason.contains("DevLocal participant authority") {
                "archived World restore rejected: devlocal_participant_context"
            } else {
                "archived World restore rejected: distributed_validation"
            }
        }
        (Some(_), _) => "archived World restore rejected: runtime_validation",
    })?;
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
    pin_at_height_with_context(
        records,
        storage,
        identity,
        height,
        &RestoreContext::default(),
    )
}

pub(crate) fn pin_at_height_with_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: u64,
    context: &RestoreContext,
) -> Result<PinnedWorld, String> {
    load_with_context(records, storage, identity, Some(height), None, context)
}

/// Historical result reads may outlive the retained full material window.
#[cfg(test)]
pub(crate) fn pin_retained_at_height(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: u64,
) -> Result<Option<PinnedWorld>, String> {
    pin_retained_at_height_with_context(
        records,
        storage,
        identity,
        height,
        &RestoreContext::default(),
    )
}

pub(crate) fn pin_retained_at_height_with_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: u64,
    context: &RestoreContext,
) -> Result<Option<PinnedWorld>, String> {
    let Some(record) = super::historical_material::load_record(records, height)? else {
        return Ok(None);
    };
    if record.world_id != identity.world_id || record.height != height || height == 0 {
        return Err("historical record world or height invalid".into());
    }
    if record.snapshot_ref.is_none()
        && record.latest_state_ref.is_none()
        && record.journal_ref.is_none()
        && super::historical_material::hydrate(&record, &LocalCasStore::new(storage))?.is_none()
    {
        return Ok(None);
    }
    pin_at_height_with_context(records, storage, identity, height, context).map(Some)
}

fn load_with_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: Option<u64>,
    fixed: Option<&CommitRef>,
    context: &RestoreContext,
) -> Result<PinnedWorld, String> {
    load_with_cache_context(
        records,
        storage,
        identity,
        height,
        fixed,
        restore_cache::shared(),
        context,
    )
}

#[cfg(test)]
fn load_with_cache(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: Option<u64>,
    fixed: Option<&CommitRef>,
    cache: &std::sync::Mutex<restore_cache::RestoreCache>,
) -> Result<PinnedWorld, String> {
    load_with_cache_context(
        records,
        storage,
        identity,
        height,
        fixed,
        cache,
        &RestoreContext::default(),
    )
}

fn load_with_cache_context(
    records: &Path,
    storage: &Path,
    identity: &WorldIdentity,
    height: Option<u64>,
    fixed: Option<&CommitRef>,
    cache: &std::sync::Mutex<restore_cache::RestoreCache>,
    context: &RestoreContext,
) -> Result<PinnedWorld, String> {
    if let Some(commit) = fixed
        && &commit.world != identity
    {
        return Err("fixed commit world mismatch".into());
    }
    let mut record = if let Some(height) = height {
        super::historical_material::load_record(records, height)?
            .ok_or("historical record unavailable")?
    } else {
        checkpoint::load_highest_valid_execution_bridge_record(records)?
            .ok_or("committed view unavailable")?
    };
    if record.world_id != identity.world_id || record.height == 0 {
        return Err("record world or height invalid".into());
    }
    let store = LocalCasStore::new(storage);
    let rehydrated = height.is_some()
        && record.snapshot_ref.is_none()
        && record.latest_state_ref.is_none()
        && record.journal_ref.is_none();
    if rehydrated {
        record = super::historical_material::hydrate(&record, &store)?
            .ok_or("record historical material unavailable")?;
    }
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
    let read_blob = |hash: &str| -> Result<Vec<u8>, String> {
        if rehydrated {
            super::historical_material::cas(&store, hash)?
                .ok_or("historical CAS disappeared".into())
        } else {
            store
                .get_verified(hash)
                .map_err(|e| format!("pinned CAS: {e:?}"))
        }
    };
    let snapshot_bytes = read_blob(snapshot_ref)?;
    let journal_bytes = read_blob(journal_ref)?;
    let snapshot: Snapshot = oasis7_world_state::codec::snapshot_from_cbor(&snapshot_bytes)
        .map_err(|e| e.to_string())?;
    let journal: Journal =
        oasis7_world_state::codec::journal_from_cbor(&journal_bytes).map_err(|e| e.to_string())?;
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
    let proof_bytes = read_blob(proof_ref)?;
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
    let world = restore_cache::restore_with_context(
        cache,
        &snapshot_bytes,
        &journal_bytes,
        snapshot,
        journal,
        context,
    )
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
