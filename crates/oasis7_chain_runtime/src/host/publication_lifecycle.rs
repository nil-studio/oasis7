use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_node::{
    NodeConsensusProgressObserver, NodeConsensusProgressObserverError, NodeConsensusSnapshot,
    NodeRole, NodeSnapshot, ObserverLifecycleAuthority,
};
use serde::{Deserialize, Serialize};

use super::publication_proof::{
    PublicationExecutionRecord, PublicationProofError, evaluate_publication_episode, load_record,
    validate_record,
};
use super::status_payload::{
    ChainConsensusNetworkHeadStatus, build_network_head_status, readiness_policy,
};

pub(super) const SEQUENCER_HEAD_PUBLICATION_GRACE_MS: i64 = 30_000;
pub(super) const PUBLICATION_LAG_STATE_FILE: &str = "sequencer-publication-lag-state.json";
const PUBLICATION_LAG_STATE_SCHEMA_VERSION: u32 = 1;

pub(super) fn has_authoritative_publication_stake(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
) -> bool {
    snapshot.consensus.required_stake > 0
        && network_head.stake_quorum_met
        && network_head.observed_stake >= snapshot.consensus.required_stake
}

#[derive(Clone)]
pub(super) struct PublicationLifecycleObserver {
    node_id: String,
    player_id: String,
    world_id: String,
    role: NodeRole,
    replication_enabled: bool,
    manifest: Option<LoadedNetworkTierManifest>,
    execution_world_dir: PathBuf,
    execution_records_dir: PathBuf,
    authority: Option<ObserverLifecycleAuthority>,
    #[cfg(test)]
    before_final_generation_check_hook: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    before_native_replace_hook: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    #[cfg(test)]
    publication_result_hook: Option<std::sync::Arc<dyn Fn(Option<String>) + Send + Sync>>,
}

impl PublicationLifecycleObserver {
    #[expect(
        clippy::too_many_arguments,
        reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
    )]
    pub(super) fn new(
        node_id: String,
        player_id: String,
        world_id: String,
        role: NodeRole,
        replication_enabled: bool,
        manifest: Option<LoadedNetworkTierManifest>,
        execution_world_dir: PathBuf,
        execution_records_dir: PathBuf,
    ) -> Self {
        Self {
            node_id,
            player_id,
            world_id,
            role,
            replication_enabled,
            manifest,
            execution_world_dir,
            execution_records_dir,
            authority: None,
            #[cfg(test)]
            before_final_generation_check_hook: None,
            #[cfg(test)]
            before_native_replace_hook: None,
            #[cfg(test)]
            publication_result_hook: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_before_final_generation_check_hook_for_test(
        mut self,
        hook: std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        self.before_final_generation_check_hook = Some(hook);
        self
    }

    #[cfg(test)]
    pub(super) fn with_before_native_replace_hook_for_test(
        mut self,
        hook: std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        self.before_native_replace_hook = Some(hook);
        self
    }

    #[cfg(test)]
    pub(super) fn with_publication_result_hook_for_test(
        mut self,
        hook: std::sync::Arc<dyn Fn(Option<String>) + Send + Sync>,
    ) -> Self {
        self.publication_result_hook = Some(hook);
        self
    }
}

impl NodeConsensusProgressObserver for PublicationLifecycleObserver {
    fn observe_consensus_progress(
        &mut self,
        consensus: &NodeConsensusSnapshot,
        observed_at_ms: i64,
    ) -> Result<(), NodeConsensusProgressObserverError> {
        let snapshot = NodeSnapshot {
            node_id: self.node_id.clone(),
            player_id: self.player_id.clone(),
            world_id: self.world_id.clone(),
            role: self.role,
            replication_enabled: self.replication_enabled,
            running: true,
            tick_count: 0,
            last_tick_unix_ms: Some(observed_at_ms),
            consensus: consensus.clone(),
            consensus_progress_observer_error: None,
            last_error: None,
        };
        let result = reconcile_with_authority(
            &snapshot,
            self.manifest.as_ref(),
            self.execution_world_dir.as_path(),
            self.execution_records_dir.as_path(),
            observed_at_ms,
            self.authority.as_ref(),
        );
        #[cfg(test)]
        if let Some(hook) = self.publication_result_hook.take() {
            hook(result.as_ref().err().map(|error| error.detail.clone()));
        }
        result.map_err(|error| {
            let code = if error.detail == "observer_lifecycle_commit_busy" {
                "observer_lifecycle_commit_busy"
            } else {
                error.reason
            };
            NodeConsensusProgressObserverError::coded(
                code,
                format!(
                    "publication lifecycle reconciliation failed: reason={} detail={}",
                    error.reason, error.detail
                ),
            )
        })
    }

    fn recreate_for_restart(&self) -> Option<Box<dyn NodeConsensusProgressObserver>> {
        let restarted = self.clone();
        #[cfg(test)]
        let restarted = {
            let mut restarted = restarted;
            restarted.before_final_generation_check_hook = None;
            restarted.before_native_replace_hook = None;
            restarted.publication_result_hook = None;
            restarted
        };
        Some(Box::new(restarted))
    }

    fn bind_lifecycle_authority(&mut self, authority: ObserverLifecycleAuthority) {
        #[cfg(test)]
        if let Some(hook) = self.before_final_generation_check_hook.take() {
            authority.set_before_final_generation_check_hook_for_test(hook);
        }
        #[cfg(test)]
        if let Some(hook) = self.before_native_replace_hook.take() {
            authority.set_before_native_replace_hook_for_test(hook);
        }
        self.authority = Some(authority);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PublicationHeadBinding {
    pub(super) height: u64,
    pub(super) node_block_hash: String,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) timestamp_ms: i64,
}

impl PublicationHeadBinding {
    pub(super) fn matches_record(&self, record: &PublicationExecutionRecord) -> bool {
        self.height == record.height
            && Some(self.node_block_hash.as_str()) == record.node_block_hash.as_deref()
            && self.execution_block_hash == record.execution_block_hash
            && self.execution_state_root == record.execution_state_root
            && self.timestamp_ms == record.timestamp_ms
    }

    fn from_snapshot(snapshot: &NodeSnapshot) -> Result<Self, LifecycleError> {
        Ok(Self {
            height: snapshot.consensus.committed_height,
            node_block_hash: nonempty(snapshot.consensus.last_block_hash.as_deref())
                .ok_or_else(|| LifecycleError::binding("snapshot_block_hash"))?
                .to_string(),
            execution_block_hash: nonempty(snapshot.consensus.last_execution_block_hash.as_deref())
                .ok_or_else(|| LifecycleError::binding("snapshot_execution_hash"))?
                .to_string(),
            execution_state_root: nonempty(snapshot.consensus.last_execution_state_root.as_deref())
                .ok_or_else(|| LifecycleError::binding("snapshot_state_root"))?
                .to_string(),
            timestamp_ms: snapshot
                .consensus
                .last_committed_at_ms
                .ok_or_else(|| LifecycleError::binding("snapshot_commit_timestamp"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PublicationLifecycleSnapshot {
    #[serde(default)]
    pub(super) schema_version: u32,
    #[serde(default)]
    pub(super) world_id: String,
    #[serde(default)]
    pub(super) episode: Option<PublicationHeadBinding>,
    #[serde(default)]
    pub(super) catch_up: Option<PublicationHeadBinding>,
}

impl PublicationLifecycleSnapshot {
    fn episode(world_id: &str, binding: PublicationHeadBinding) -> Self {
        Self {
            schema_version: PUBLICATION_LAG_STATE_SCHEMA_VERSION,
            world_id: world_id.to_string(),
            episode: Some(binding),
            catch_up: None,
        }
    }

    fn catch_up(world_id: &str, binding: PublicationHeadBinding) -> Self {
        Self {
            schema_version: PUBLICATION_LAG_STATE_SCHEMA_VERSION,
            world_id: world_id.to_string(),
            episode: None,
            catch_up: Some(binding),
        }
    }

    fn validate_shape(&self) -> Result<(), LifecycleError> {
        if self.schema_version != PUBLICATION_LAG_STATE_SCHEMA_VERSION
            || self.world_id.trim().is_empty()
            || self.episode.is_some() == self.catch_up.is_some()
        {
            return Err(LifecycleError::malformed("state_shape_invalid"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicationScope {
    EqualHead,
    OneBlockLag,
    Outside,
}

pub(super) fn classify_scope(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    tier: &str,
    role: &str,
) -> PublicationScope {
    if tier != "public_testnet" || role != "sequencer" {
        return PublicationScope::Outside;
    }
    let local_height = snapshot.consensus.committed_height;
    if has_complete_publication_quorum_at_height(snapshot, network_head, local_height, true) {
        return PublicationScope::EqualHead;
    }
    if local_height.checked_sub(1).is_some_and(|parent| {
        has_complete_publication_quorum_at_height(snapshot, network_head, parent, false)
    }) {
        return PublicationScope::OneBlockLag;
    }
    PublicationScope::Outside
}

pub(super) fn load_snapshot(
    execution_world_dir: &Path,
) -> Result<Option<PublicationLifecycleSnapshot>, LifecycleError> {
    let path = execution_world_dir.join(PUBLICATION_LAG_STATE_FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(LifecycleError::malformed("state_read_failed")),
    };
    let state = serde_json::from_slice::<PublicationLifecycleSnapshot>(&bytes)
        .map_err(|_| LifecycleError::malformed("state_parse_failed"))?;
    state.validate_shape()?;
    Ok(Some(state))
}

pub(super) fn validate_current_catch_up_binding(
    snapshot: &NodeSnapshot,
    execution_world_dir: &Path,
) -> Result<(), LifecycleError> {
    let state = load_snapshot(execution_world_dir)?
        .ok_or_else(|| LifecycleError::persist_pending("state_missing"))?;
    validate_world(&state, snapshot.world_id.as_str())?;
    let expected = PublicationHeadBinding::from_snapshot(snapshot)?;
    let actual = state
        .catch_up
        .as_ref()
        .ok_or_else(|| LifecycleError::binding("catch_up_missing"))?;
    if !bindings_equal(actual, &expected) {
        return Err(LifecycleError::binding("catch_up_binding_mismatch"));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn reconcile(
    snapshot: &NodeSnapshot,
    manifest: Option<&LoadedNetworkTierManifest>,
    execution_world_dir: &Path,
    execution_records_dir: &Path,
    observed_at_unix_ms: i64,
) -> Result<(), LifecycleError> {
    reconcile_with_authority(
        snapshot,
        manifest,
        execution_world_dir,
        execution_records_dir,
        observed_at_unix_ms,
        None,
    )
}

fn reconcile_with_authority(
    snapshot: &NodeSnapshot,
    manifest: Option<&LoadedNetworkTierManifest>,
    execution_world_dir: &Path,
    execution_records_dir: &Path,
    observed_at_unix_ms: i64,
    authority: Option<&ObserverLifecycleAuthority>,
) -> Result<(), LifecycleError> {
    let policy = readiness_policy(snapshot, manifest);
    let network_head = build_network_head_status(snapshot, observed_at_unix_ms, manifest);
    match classify_scope(
        snapshot,
        &network_head,
        policy.tier.as_str(),
        policy.role.as_str(),
    ) {
        PublicationScope::Outside => Ok(()),
        PublicationScope::EqualHead => reconcile_catch_up(
            snapshot,
            execution_world_dir,
            execution_records_dir,
            authority,
        ),
        PublicationScope::OneBlockLag => reconcile_episode(
            snapshot,
            &network_head,
            execution_world_dir,
            execution_records_dir,
            observed_at_unix_ms,
            authority,
        ),
    }
}

fn reconcile_catch_up(
    snapshot: &NodeSnapshot,
    execution_world_dir: &Path,
    execution_records_dir: &Path,
    authority: Option<&ObserverLifecycleAuthority>,
) -> Result<(), LifecycleError> {
    let next_binding = PublicationHeadBinding::from_snapshot(snapshot)?;
    if let Some(current) = load_snapshot(execution_world_dir)? {
        validate_world(&current, snapshot.world_id.as_str())?;
        let current_binding = current.episode.as_ref().or(current.catch_up.as_ref());
        if current_binding.is_some_and(|binding| binding.height > next_binding.height) {
            return Err(LifecycleError::binding("catch_up_height_rollback"));
        }
        if current
            .catch_up
            .as_ref()
            .is_some_and(|binding| bindings_equal(binding, &next_binding))
        {
            return Ok(());
        }
        if let Some(binding) = current_binding {
            let retained = load_record(execution_records_dir, binding.height)?;
            validate_record(&retained, binding.height, snapshot.world_id.as_str())?;
            if !binding.matches_record(&retained)
                || (binding.height == next_binding.height
                    && !bindings_equal(binding, &next_binding))
            {
                return Err(LifecycleError::binding("catch_up_binding_mismatch"));
            }
        }
    }
    save_snapshot(
        execution_world_dir,
        &PublicationLifecycleSnapshot::catch_up(snapshot.world_id.as_str(), next_binding),
        authority,
    )
}

fn reconcile_episode(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    execution_world_dir: &Path,
    execution_records_dir: &Path,
    observed_at_unix_ms: i64,
    authority: Option<&ObserverLifecycleAuthority>,
) -> Result<(), LifecycleError> {
    let current = load_snapshot(execution_world_dir)?;
    let evaluation = evaluate_publication_episode(
        snapshot,
        network_head,
        execution_records_dir,
        current.as_ref(),
        observed_at_unix_ms,
    )?;
    if evaluation.retained {
        return Ok(());
    }
    save_snapshot(
        execution_world_dir,
        &PublicationLifecycleSnapshot::episode(
            snapshot.world_id.as_str(),
            evaluation.episode_binding,
        ),
        authority,
    )
}

pub(super) fn has_complete_publication_quorum_at_height(
    snapshot: &NodeSnapshot,
    network_head: &ChainConsensusNetworkHeadStatus,
    expected_height: u64,
    bind_local: bool,
) -> bool {
    let local_height = snapshot.consensus.committed_height;
    let complete_local_boundary = snapshot.consensus.latest_height == local_height
        && snapshot.consensus.network_committed_height == local_height
        && snapshot.consensus.replication_persisted_height == local_height
        && snapshot.consensus.last_execution_height == local_height;
    let complete_bindings = [
        snapshot.consensus.last_block_hash.as_deref(),
        snapshot.consensus.last_execution_block_hash.as_deref(),
        snapshot.consensus.last_execution_state_root.as_deref(),
        network_head.block_hash.as_deref(),
        network_head.execution_block_hash.as_deref(),
        network_head.execution_state_root.as_deref(),
    ]
    .into_iter()
    .all(|value| nonempty(value).is_some());
    let every_fresh_peer_binds = network_head
        .peer_heads
        .iter()
        .filter(|peer| peer.fresh)
        .all(|peer| {
            peer.height == expected_height
                && Some(peer.block_hash.as_str()) == network_head.block_hash.as_deref()
                && peer.execution_block_hash == network_head.execution_block_hash
                && peer.execution_state_root == network_head.execution_state_root
        });
    let equal_local_binding = !bind_local
        || (network_head.block_hash == snapshot.consensus.last_block_hash
            && network_head.execution_block_hash == snapshot.consensus.last_execution_block_hash
            && network_head.execution_state_root == snapshot.consensus.last_execution_state_root);
    complete_local_boundary
        && complete_bindings
        && network_head.source == "peer_quorum"
        && network_head.decision == "ready"
        && network_head.height == Some(expected_height)
        && network_head.conflicting_peer_count == 0
        && has_authoritative_publication_stake(snapshot, network_head)
        && network_head.required_peer_count > 0
        && network_head.fresh_peer_count >= network_head.required_peer_count
        && every_fresh_peer_binds
        && equal_local_binding
}

fn save_snapshot(
    execution_world_dir: &Path,
    snapshot: &PublicationLifecycleSnapshot,
    authority: Option<&ObserverLifecycleAuthority>,
) -> Result<(), LifecycleError> {
    let mutation_guard = authority.and_then(ObserverLifecycleAuthority::acquire_durable_mutation);
    if authority.is_some() && mutation_guard.is_none() {
        return Err(LifecycleError::persist("observer_lifecycle_revoked"));
    }
    fs::create_dir_all(execution_world_dir)
        .map_err(|_| LifecycleError::persist("state_dir_create_failed"))?;
    let bytes = serde_json::to_vec_pretty(snapshot)
        .map_err(|_| LifecycleError::persist("state_serialize_failed"))?;
    let target = execution_world_dir.join(PUBLICATION_LAG_STATE_FILE);
    let (temp_path, mut temp_file) = create_unique_temp(execution_world_dir)?;
    let result = (|| {
        temp_file
            .write_all(bytes.as_slice())
            .map_err(|_| LifecycleError::persist("state_write_failed"))?;
        temp_file
            .sync_all()
            .map_err(|_| LifecycleError::persist("state_fsync_failed"))?;
        drop(temp_file);
        mutation_guard.as_ref().map_or_else(
            || {
                replace_file(temp_path.as_path(), target.as_path())?;
                sync_parent_dir(execution_world_dir)
            },
            |guard| {
                guard
                    .publish_staged_file(temp_path.as_path(), target.as_path())
                    .map_err(|error| LifecycleError::persist(error.code()))
            },
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

fn create_unique_temp(parent: &Path) -> Result<(PathBuf, File), LifecycleError> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..16_u8 {
        let path = parent.join(format!(
            ".{PUBLICATION_LAG_STATE_FILE}.{}.{}.tmp",
            std::process::id(),
            nonce + u128::from(attempt)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(LifecycleError::persist("state_temp_create_failed")),
        }
    }
    Err(LifecycleError::persist("state_temp_collision"))
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> Result<(), LifecycleError> {
    fs::rename(temp, target).map_err(|_| LifecycleError::persist("state_replace_failed"))
}

#[cfg(not(unix))]
fn replace_file(temp: &Path, target: &Path) -> Result<(), LifecycleError> {
    replace_file_with(temp, target, platform_atomic_replace)
}

#[cfg(any(not(unix), test))]
pub(super) fn replace_file_with(
    temp: &Path,
    target: &Path,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), LifecycleError> {
    replace(temp, target).map_err(|_| LifecycleError::persist("state_replace_failed"))
}

#[cfg(all(not(unix), not(windows)))]
fn platform_atomic_replace(temp: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(temp, target)
}

#[cfg(windows)]
fn platform_atomic_replace(temp: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    // SAFETY: both paths are owned, NUL-terminated UTF-16 buffers that remain
    // alive for the duration of the call.
    let replaced = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> Result<(), LifecycleError> {
    File::open(parent)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| LifecycleError::persist("state_parent_fsync_failed"))
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> Result<(), LifecycleError> {
    Ok(())
}

fn validate_world(
    state: &PublicationLifecycleSnapshot,
    world_id: &str,
) -> Result<(), LifecycleError> {
    if state.world_id != world_id {
        return Err(LifecycleError::binding("world_mismatch"));
    }
    Ok(())
}

fn bindings_equal(left: &PublicationHeadBinding, right: &PublicationHeadBinding) -> bool {
    left.height == right.height
        && left.node_block_hash == right.node_block_hash
        && left.execution_block_hash == right.execution_block_hash
        && left.execution_state_root == right.execution_state_root
        && left.timestamp_ms == right.timestamp_ms
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[derive(Clone, Debug)]
pub(super) struct LifecycleError {
    pub(super) reason: &'static str,
    pub(super) detail: String,
}

impl LifecycleError {
    fn malformed(detail: impl Into<String>) -> Self {
        Self {
            reason: "state_malformed",
            detail: detail.into(),
        }
    }

    fn binding(detail: impl Into<String>) -> Self {
        Self {
            reason: "state_binding_invalid",
            detail: detail.into(),
        }
    }

    fn persist(detail: impl Into<String>) -> Self {
        Self {
            reason: "state_persist_failed",
            detail: detail.into(),
        }
    }

    fn persist_pending(detail: impl Into<String>) -> Self {
        Self {
            reason: "state_persist_pending",
            detail: detail.into(),
        }
    }
}

impl From<PublicationProofError> for LifecycleError {
    fn from(error: PublicationProofError) -> Self {
        Self::binding(error.detail)
    }
}
