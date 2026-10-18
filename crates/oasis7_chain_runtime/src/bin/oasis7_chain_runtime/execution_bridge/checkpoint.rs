use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER, EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER,
};
use oasis7_runtime::{BlobStore, LocalCasStore};

use super::{
    EXECUTION_BRIDGE_RECORD_SCHEMA_V2, EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1, ExecutionBridgePinSet, ExecutionBridgeRecord,
    ExecutionCheckpointLatestPointer, ExecutionCheckpointManifest, write_bytes_atomic,
};

const EXECUTION_BRIDGE_PIN_SCOPE: &str = "execution_bridge_v1";
type CheckpointRetentionPolicy = (u64, usize);
#[cfg(test)]
pub(super) mod task_evidence;
#[cfg(not(test))]
mod task_evidence;
pub(super) fn refresh_live_task_read_evidence(
    records: &Path,
    store: &LocalCasStore,
    world: &oasis7_runtime::World,
    hot: u64,
) -> Result<ValidatedTaskRetention, String> {
    task_evidence::refresh(records, store, world, hot)
}

/// Opaque evidence that task retention was fully validated and synchronized
/// for this exact durable head and storage root in the current transaction.
pub(super) struct ValidatedTaskRetention {
    records: PathBuf,
    store_root: PathBuf,
    durable_record: ExecutionBridgeRecord,
    protected: BTreeSet<u64>,
}

impl ValidatedTaskRetention {
    fn new(
        records: &Path,
        store: &LocalCasStore,
        durable_record: ExecutionBridgeRecord,
        protected: BTreeSet<u64>,
    ) -> Self {
        Self {
            records: records.to_path_buf(),
            store_root: store.root().to_path_buf(),
            durable_record,
            protected,
        }
    }

    fn into_protected(
        self,
        records: &Path,
        store: &LocalCasStore,
        record: &ExecutionBridgeRecord,
    ) -> Result<BTreeSet<u64>, String> {
        if self.records.as_path() != records
            || self.store_root.as_path() != store.root()
            || self.durable_record != normalize_execution_bridge_record_for_persist(record)
        {
            return Err("validated live task retention does not match durable head".into());
        }
        let latest_height = list_execution_bridge_record_heights(records)?
            .last()
            .copied();
        if latest_height != Some(self.durable_record.height) {
            return Err("durable execution bridge head advanced after task validation".into());
        }
        let durable = load_execution_bridge_record(
            execution_bridge_record_path(records, self.durable_record.height).as_path(),
        )?;
        if durable != self.durable_record {
            return Err("durable execution bridge head changed after task validation".into());
        }
        Ok(self.protected)
    }
}

pub(super) fn execution_checkpoint_root_dir(execution_records_dir: &Path) -> std::path::PathBuf {
    execution_records_dir.join("checkpoints")
}

pub(super) fn execution_checkpoint_manifest_path(
    execution_records_dir: &Path,
    height: u64,
) -> std::path::PathBuf {
    execution_checkpoint_root_dir(execution_records_dir)
        .join(format!("{:020}", height))
        .join("manifest.json")
}

pub(super) fn execution_checkpoint_latest_path(execution_records_dir: &Path) -> std::path::PathBuf {
    execution_checkpoint_root_dir(execution_records_dir).join("latest.json")
}

pub(super) fn execution_checkpoint_manifest_rel_path(height: u64) -> String {
    format!("{:020}/manifest.json", height)
}

pub(super) fn list_execution_checkpoint_heights(
    execution_records_dir: &Path,
) -> Result<Vec<u64>, String> {
    let checkpoint_root = execution_checkpoint_root_dir(execution_records_dir);
    if !checkpoint_root.exists() {
        return Ok(Vec::new());
    }

    let mut heights = Vec::new();
    for entry in fs::read_dir(checkpoint_root.as_path()).map_err(|err| {
        format!(
            "read execution checkpoint root {} failed: {}",
            checkpoint_root.display(),
            err
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "read execution checkpoint dir entry under {} failed: {}",
                checkpoint_root.display(),
                err
            )
        })?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "read execution checkpoint dir entry type {} failed: {}",
                entry.path().display(),
                err
            )
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let Ok(height) = name.parse::<u64>() else {
            continue;
        };
        if execution_checkpoint_manifest_path(execution_records_dir, height).exists() {
            heights.push(height);
        }
    }

    heights.sort_unstable();
    heights.dedup();
    Ok(heights)
}

pub(super) fn persist_execution_checkpoint_manifest(
    execution_records_dir: &Path,
    manifest: &ExecutionCheckpointManifest,
) -> Result<(), String> {
    manifest.validate()?;
    let manifest_path = execution_checkpoint_manifest_path(execution_records_dir, manifest.height);
    let manifest_parent = manifest_path.parent().ok_or_else(|| {
        format!(
            "execution checkpoint manifest path {} missing parent",
            manifest_path.display()
        )
    })?;
    fs::create_dir_all(manifest_parent).map_err(|err| {
        format!(
            "create execution checkpoint dir {} failed: {}",
            manifest_parent.display(),
            err
        )
    })?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|err| format!("serialize execution checkpoint manifest failed: {}", err))?;
    write_bytes_atomic(manifest_path.as_path(), manifest_bytes.as_slice())?;

    let root_dir = execution_checkpoint_root_dir(execution_records_dir);
    fs::create_dir_all(root_dir.as_path()).map_err(|err| {
        format!(
            "create execution checkpoint root {} failed: {}",
            root_dir.display(),
            err
        )
    })?;
    let latest = ExecutionCheckpointLatestPointer {
        schema_version: EXECUTION_CHECKPOINT_MANIFEST_SCHEMA_V1,
        checkpoint_id: manifest.checkpoint_id.clone(),
        height: manifest.height,
        manifest_hash: manifest.manifest_hash.clone(),
        manifest_rel_path: execution_checkpoint_manifest_rel_path(manifest.height),
        updated_at_ms: manifest.created_at_ms,
    };
    let latest_bytes = serde_json::to_vec_pretty(&latest).map_err(|err| {
        format!(
            "serialize execution checkpoint latest pointer failed: {}",
            err
        )
    })?;
    let latest_path = execution_checkpoint_latest_path(execution_records_dir);
    write_bytes_atomic(latest_path.as_path(), latest_bytes.as_slice())
}

pub(super) fn load_execution_checkpoint_manifest(
    path: &Path,
) -> Result<ExecutionCheckpointManifest, String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "read execution checkpoint manifest {} failed: {}",
            path.display(),
            err
        )
    })?;
    let manifest = serde_json::from_slice::<ExecutionCheckpointManifest>(bytes.as_slice())
        .map_err(|err| {
            format!(
                "parse execution checkpoint manifest {} failed: {}",
                path.display(),
                err
            )
        })?;
    manifest.validate()?;
    Ok(manifest)
}

pub(super) fn load_latest_execution_checkpoint_manifest(
    execution_records_dir: &Path,
) -> Result<Option<ExecutionCheckpointManifest>, String> {
    let latest_path = execution_checkpoint_latest_path(execution_records_dir);
    if !latest_path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(latest_path.as_path()).map_err(|err| {
        format!(
            "read execution checkpoint latest pointer {} failed: {}",
            latest_path.display(),
            err
        )
    })?;
    let latest = serde_json::from_slice::<ExecutionCheckpointLatestPointer>(bytes.as_slice())
        .map_err(|err| {
            format!(
                "parse execution checkpoint latest pointer {} failed: {}",
                latest_path.display(),
                err
            )
        })?;
    let manifest_path =
        execution_checkpoint_root_dir(execution_records_dir).join(latest.manifest_rel_path);
    let manifest = load_execution_checkpoint_manifest(manifest_path.as_path())?;
    if manifest.height != latest.height {
        return Err(format!(
            "execution checkpoint latest pointer height mismatch expected={} actual={}",
            latest.height, manifest.height
        ));
    }
    if manifest.manifest_hash != latest.manifest_hash {
        return Err(format!(
            "execution checkpoint latest pointer hash mismatch expected={} actual={}",
            latest.manifest_hash, manifest.manifest_hash
        ));
    }
    if manifest.checkpoint_id != latest.checkpoint_id {
        return Err(format!(
            "execution checkpoint latest pointer id mismatch expected={} actual={}",
            latest.checkpoint_id, manifest.checkpoint_id
        ));
    }
    Ok(Some(manifest))
}

pub(super) fn execution_bridge_record_path(
    execution_records_dir: &Path,
    height: u64,
) -> std::path::PathBuf {
    execution_records_dir.join(format!("{:020}.json", height))
}

pub(super) fn list_execution_bridge_record_heights(
    execution_records_dir: &Path,
) -> Result<Vec<u64>, String> {
    if !execution_records_dir.exists() {
        return Ok(Vec::new());
    }

    let mut heights = Vec::new();
    for entry in fs::read_dir(execution_records_dir).map_err(|err| {
        format!(
            "read execution records dir {} failed: {}",
            execution_records_dir.display(),
            err
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "read execution record dir entry under {} failed: {}",
                execution_records_dir.display(),
                err
            )
        })?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "read execution record dir entry type {} failed: {}",
                entry.path().display(),
                err
            )
        })?;
        if !file_type.is_file() {
            continue;
        }
        let Some(file_name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if file_name == "latest.json" || !file_name.ends_with(".json") {
            continue;
        }
        let Some(stem) = file_name.strip_suffix(".json") else {
            continue;
        };
        let Ok(height) = stem.parse::<u64>() else {
            continue;
        };
        heights.push(height);
    }

    heights.sort_unstable();
    heights.dedup();
    Ok(heights)
}

pub(super) fn load_highest_valid_execution_bridge_record(
    execution_records_dir: &Path,
) -> Result<Option<ExecutionBridgeRecord>, String> {
    for height in list_execution_bridge_record_heights(execution_records_dir)?
        .into_iter()
        .rev()
    {
        let path = execution_bridge_record_path(execution_records_dir, height);
        let Ok(record) = load_execution_bridge_record(path.as_path()) else {
            continue;
        };
        if record.height != height || record.world_id.trim().is_empty() {
            continue;
        }
        return Ok(Some(record));
    }

    let latest_path = execution_records_dir.join("latest.json");
    if !latest_path.exists() {
        return Ok(None);
    }
    let record = load_execution_bridge_record(latest_path.as_path())?;
    if record.world_id.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(record))
}

fn maybe_insert_pin_ref(pinned_refs: &mut BTreeSet<String>, content_ref: Option<&str>) {
    if let Some(content_ref) = content_ref.filter(|content_ref| !content_ref.is_empty()) {
        pinned_refs.insert(content_ref.to_string());
    }
}

fn collect_execution_bridge_record_retained_refs(
    record: &ExecutionBridgeRecord,
    retain_archive: bool,
    retain_latest_head: bool,
    retain_hot_window: bool,
    pinned_refs: &mut BTreeSet<String>,
    best_effort_pinned_refs: &mut BTreeSet<String>,
) {
    if retain_archive {
        maybe_insert_pin_ref(pinned_refs, record.commit_log_ref.as_deref());
        // The closed package contains all historical capture bytes. Preserve it
        // with the archived record, rather than pinning mutable input sidecars.
        maybe_insert_pin_ref(pinned_refs, record.controlled_capture_ref.as_deref());
        maybe_insert_pin_ref(
            best_effort_pinned_refs,
            record.external_effect_ref.as_deref(),
        );
        maybe_insert_pin_ref(
            best_effort_pinned_refs,
            record.world_head_proof_ref.as_deref(),
        );
    }

    if retain_latest_head {
        maybe_insert_pin_ref(pinned_refs, record.latest_state_ref.as_deref());
    }
    if retain_latest_head || retain_hot_window {
        maybe_insert_pin_ref(pinned_refs, record.snapshot_ref.as_deref());
        maybe_insert_pin_ref(pinned_refs, record.journal_ref.as_deref());
        if let Some(simulator_mirror) = record.simulator_mirror.as_ref() {
            pinned_refs.insert(simulator_mirror.snapshot_ref.clone());
            pinned_refs.insert(simulator_mirror.journal_ref.clone());
        }
    }
}

fn collect_execution_checkpoint_retained_refs(
    execution_records_dir: &Path,
    pinned_refs: &mut BTreeSet<String>,
) -> Result<(), String> {
    let checkpoint_root = execution_checkpoint_root_dir(execution_records_dir);
    if !checkpoint_root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(checkpoint_root.as_path()).map_err(|err| {
        format!(
            "read execution checkpoint root {} failed: {}",
            checkpoint_root.display(),
            err
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "read execution checkpoint dir entry under {} failed: {}",
                checkpoint_root.display(),
                err
            )
        })?;
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "read execution checkpoint dir entry type {} failed: {}",
                entry.path().display(),
                err
            )
        })?;
        if !file_type.is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = load_execution_checkpoint_manifest(manifest_path.as_path())?;
        pinned_refs.extend(manifest.pinned_refs);
    }

    Ok(())
}

fn build_execution_bridge_pin_set(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    hot_window_heights: u64,
) -> Result<ExecutionBridgePinSet, String> {
    let protected = task_evidence::protected_heights(execution_records_dir, execution_store)?;
    let record_heights = list_execution_bridge_record_heights(execution_records_dir)?;
    let checkpoint_heights = list_execution_checkpoint_heights(execution_records_dir)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let Some(latest_height) = record_heights.last().copied() else {
        let mut pin_set = ExecutionBridgePinSet {
            checkpoint_heights,
            ..ExecutionBridgePinSet::default()
        };
        collect_execution_checkpoint_retained_refs(
            execution_records_dir,
            &mut pin_set.pinned_refs,
        )?;
        return Ok(pin_set);
    };

    let retained_hot_window = hot_window_heights.max(1);
    let hot_window_start_height =
        latest_height.saturating_sub(retained_hot_window.saturating_sub(1));
    let mut pin_set = ExecutionBridgePinSet {
        latest_height: Some(latest_height),
        hot_window_start_height: Some(hot_window_start_height),
        archive_window_start_height: checkpoint_heights.first().copied(),
        checkpoint_heights,
        pinned_refs: BTreeSet::new(),
        best_effort_pinned_refs: BTreeSet::new(),
    };

    for height in record_heights {
        let record = load_execution_bridge_record(
            execution_bridge_record_path(execution_records_dir, height).as_path(),
        )?;
        collect_execution_bridge_record_retained_refs(
            &record,
            record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2
                || protected.contains(&record.height)
                || pin_set
                    .archive_window_start_height
                    .is_none_or(|start_height| record.height >= start_height),
            record.height == latest_height,
            record.height >= hot_window_start_height || protected.contains(&record.height),
            &mut pin_set.pinned_refs,
            &mut pin_set.best_effort_pinned_refs,
        );
    }
    collect_execution_checkpoint_retained_refs(execution_records_dir, &mut pin_set.pinned_refs)?;

    Ok(pin_set)
}

pub(super) fn sync_execution_bridge_pin_set(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    hot_window_heights: u64,
) -> Result<ExecutionBridgePinSet, String> {
    let pin_set =
        build_execution_bridge_pin_set(execution_records_dir, execution_store, hot_window_heights)?;
    let protected = task_evidence::protected_heights(execution_records_dir, execution_store)?;
    let mut desired_pins = pin_set.pinned_refs.clone();
    for pinned_ref in pin_set
        .best_effort_pinned_refs
        .difference(&pin_set.pinned_refs)
    {
        if execution_store
            .has(pinned_ref.as_str())
            .map_err(|err| format!("check execution store ref {} failed: {:?}", pinned_ref, err))?
        {
            desired_pins.insert(pinned_ref.clone());
        }
    }
    execution_store
        .replace_pins(&desired_pins)
        .map_err(|err| format!("replace execution store pins failed: {:?}", err))?;
    // Full maintenance is the authoritative rebuild path. Publish the complete
    // legacy pin set first, then discard incremental shards so a crash can only
    // leave redundant pins, never an unprotected live ref.
    execution_store
        .clear_pin_scope(EXECUTION_BRIDGE_PIN_SCOPE)
        .map_err(|err| format!("clear execution bridge pin scope failed: {:?}", err))?;
    for height in list_execution_bridge_record_heights(execution_records_dir)? {
        let record = load_execution_bridge_record(
            execution_bridge_record_path(execution_records_dir, height).as_path(),
        )?;
        let retain_archive = record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2
            || protected.contains(&height)
            || pin_set
                .archive_window_start_height
                .is_none_or(|start_height| height >= start_height);
        let retain_latest = pin_set.latest_height == Some(height);
        let retain_hot = pin_set
            .hot_window_start_height
            .is_some_and(|start_height| height >= start_height)
            || protected.contains(&height);
        replace_execution_bridge_record_pin_shard(
            &record,
            retain_archive,
            retain_latest,
            retain_hot,
            execution_store,
        )?;
    }
    for height in &pin_set.checkpoint_heights {
        replace_execution_bridge_checkpoint_pin_shard(
            execution_records_dir,
            execution_store,
            *height,
        )?;
    }
    execution_store
        .replace_pins(&BTreeSet::new())
        .map_err(|err| {
            format!(
                "clear execution store reconciliation pins failed: {:?}",
                err
            )
        })?;

    Ok(ExecutionBridgePinSet {
        pinned_refs: desired_pins,
        ..pin_set
    })
}

fn execution_bridge_record_pin_shard_id(height: u64) -> String {
    format!("record-{height:020}")
}

fn execution_bridge_checkpoint_pin_shard_id(height: u64) -> String {
    format!("checkpoint-{height:020}")
}

fn build_execution_bridge_record_pin_shard(
    record: &ExecutionBridgeRecord,
    retain_archive: bool,
    retain_latest_head: bool,
    retain_hot_window: bool,
    execution_store: &LocalCasStore,
) -> Result<BTreeSet<String>, String> {
    let mut required_refs = BTreeSet::new();
    let mut best_effort_refs = BTreeSet::new();
    collect_execution_bridge_record_retained_refs(
        record,
        retain_archive,
        retain_latest_head,
        retain_hot_window,
        &mut required_refs,
        &mut best_effort_refs,
    );
    for content_ref in best_effort_refs {
        if execution_store.has(content_ref.as_str()).map_err(|err| {
            format!(
                "check execution store ref {} failed: {:?}",
                content_ref, err
            )
        })? {
            required_refs.insert(content_ref);
        }
    }
    Ok(required_refs)
}

fn replace_execution_bridge_record_pin_shard(
    record: &ExecutionBridgeRecord,
    retain_archive: bool,
    retain_latest_head: bool,
    retain_hot_window: bool,
    execution_store: &LocalCasStore,
) -> Result<(), String> {
    let pins = build_execution_bridge_record_pin_shard(
        record,
        retain_archive,
        retain_latest_head,
        retain_hot_window,
        execution_store,
    )?;
    if pins.is_empty() {
        return execution_store
            .remove_pin_scope_shard(
                EXECUTION_BRIDGE_PIN_SCOPE,
                execution_bridge_record_pin_shard_id(record.height).as_str(),
            )
            .map(|_| ())
            .map_err(|err| {
                format!(
                    "remove empty execution bridge record pin shard height={} failed: {:?}",
                    record.height, err
                )
            });
    }
    execution_store
        .replace_pin_scope_shard(
            EXECUTION_BRIDGE_PIN_SCOPE,
            execution_bridge_record_pin_shard_id(record.height).as_str(),
            &pins,
        )
        .map_err(|err| {
            format!(
                "replace execution bridge record pin shard height={} failed: {:?}",
                record.height, err
            )
        })
}

fn replace_execution_bridge_checkpoint_pin_shard(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    height: u64,
) -> Result<(), String> {
    let manifest = load_execution_checkpoint_manifest(
        execution_checkpoint_manifest_path(execution_records_dir, height).as_path(),
    )?;
    let pins = manifest.pinned_refs.into_iter().collect::<BTreeSet<_>>();
    execution_store
        .replace_pin_scope_shard(
            EXECUTION_BRIDGE_PIN_SCOPE,
            execution_bridge_checkpoint_pin_shard_id(height).as_str(),
            &pins,
        )
        .map_err(|err| {
            format!(
                "replace execution bridge checkpoint pin shard height={} failed: {:?}",
                height, err
            )
        })
}

fn remove_pruned_execution_bridge_checkpoint_pin_shards(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
) -> Result<(), String> {
    let retained_heights = list_execution_checkpoint_heights(execution_records_dir)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for shard in execution_store
        .list_pin_scope_shards(EXECUTION_BRIDGE_PIN_SCOPE)
        .map_err(|err| format!("list execution bridge pin shards failed: {:?}", err))?
    {
        let Some(height) = shard
            .strip_prefix("checkpoint-")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        if shard != execution_bridge_checkpoint_pin_shard_id(height) {
            continue;
        }
        if retained_heights.contains(&height) {
            continue;
        }
        execution_store
            .remove_pin_scope_shard(EXECUTION_BRIDGE_PIN_SCOPE, shard.as_str())
            .map_err(|err| {
                format!(
                    "remove pruned execution bridge checkpoint pin shard height={} failed: {:?}",
                    height, err
                )
            })?;
    }
    Ok(())
}

fn compact_execution_bridge_record_at_height(
    execution_records_dir: &Path,
    height: u64,
) -> Result<Option<ExecutionBridgeRecord>, String> {
    let path = execution_bridge_record_path(execution_records_dir, height);
    if !path.exists() {
        return Ok(None);
    }
    let mut record = load_execution_bridge_record(path.as_path())?;
    if record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2 {
        return Ok(Some(record));
    }
    record.latest_state_ref = None;
    record.snapshot_ref = None;
    record.journal_ref = None;
    record.simulator_mirror = None;
    if record
        .checkpoint_ref
        .as_ref()
        .is_some_and(|checkpoint_ref| {
            !execution_checkpoint_root_dir(execution_records_dir)
                .join(checkpoint_ref)
                .exists()
        })
    {
        record.checkpoint_ref = None;
    }
    persist_execution_bridge_record_only(execution_records_dir, &record)?;
    Ok(Some(record))
}

fn prune_execution_bridge_record_archive_at_height(
    execution_records_dir: &Path,
    height: u64,
) -> Result<(), String> {
    let path = execution_bridge_record_path(execution_records_dir, height);
    if !path.exists() {
        return Ok(());
    }
    let mut record = load_execution_bridge_record(path.as_path())?;
    if record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2 {
        return Ok(());
    }
    record.commit_log_ref = None;
    record.external_effect_ref = None;
    record.world_head_proof_ref = None;
    record.world_head_proof_hash = None;
    persist_execution_bridge_record_only(execution_records_dir, &record).map(|_| ())
}

/// Advances retention by the delta introduced by one durable record.
///
/// Record/shard publication and compaction are serial within the execution
/// hook. Full CAS GC runs only at the existing checkpoint cadence, after all
/// live refs for the height are atomically pinned. This removes the per-block
/// full record/blob scans without introducing a concurrent put-vs-GC race.
fn run_execution_bridge_incremental_retention_maintenance_inner(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    record: &ExecutionBridgeRecord,
    protected: &BTreeSet<u64>,
    hot_window_heights: u64,
    checkpoint_policy: CheckpointRetentionPolicy,
    gc_allowed: bool,
) -> Result<u64, String> {
    let (checkpoint_interval_heights, checkpoint_keep_latest) = checkpoint_policy;
    replace_execution_bridge_record_pin_shard(record, true, true, true, execution_store)?;

    if record.checkpoint_ref.is_some() {
        replace_execution_bridge_checkpoint_pin_shard(
            execution_records_dir,
            execution_store,
            record.height,
        )?;
    }

    let retained_hot_window = hot_window_heights.max(1);
    if let Some(evicted_height) = record.height.checked_sub(retained_hot_window)
        && !protected.contains(&evicted_height)
        && let Some(evicted_record) =
            compact_execution_bridge_record_at_height(execution_records_dir, evicted_height)?
    {
        replace_execution_bridge_record_pin_shard(
            &evicted_record,
            true,
            false,
            false,
            execution_store,
        )?;
    }

    if gc_allowed
        && checkpoint_interval_heights > 0
        && record.height.is_multiple_of(checkpoint_interval_heights)
    {
        let retained_span =
            checkpoint_interval_heights.saturating_mul(checkpoint_keep_latest.max(1) as u64);
        if let Some(pruned_height) = record.height.checked_sub(retained_span) {
            let prune_start = pruned_height
                .saturating_sub(checkpoint_interval_heights.saturating_sub(1))
                .max(1);
            for archive_height in prune_start..=pruned_height {
                if protected.contains(&archive_height) {
                    continue;
                }
                prune_execution_bridge_record_archive_at_height(
                    execution_records_dir,
                    archive_height,
                )?;
                execution_store
                    .remove_pin_scope_shard(
                        EXECUTION_BRIDGE_PIN_SCOPE,
                        execution_bridge_record_pin_shard_id(archive_height).as_str(),
                    )
                    .map_err(|err| {
                        format!(
                            "remove execution bridge record pin shard height={} failed: {:?}",
                            archive_height, err
                        )
                    })?;
            }
        }
        remove_pruned_execution_bridge_checkpoint_pin_shards(
            execution_records_dir,
            execution_store,
        )?;
        return execution_store
            .prune_orphan_blobs()
            .map_err(|err| format!("prune execution store orphan blobs failed: {:?}", err));
    }

    Ok(0)
}

pub(super) fn run_execution_bridge_incremental_retention_maintenance(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    record: &ExecutionBridgeRecord,
    hot_window_heights: u64,
    checkpoint_interval_heights: u64,
    checkpoint_keep_latest: usize,
) -> Result<u64, String> {
    let protected = task_evidence::protected_heights(execution_records_dir, execution_store)?;
    let degraded_marker = execution_records_dir.join(EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER);
    let was_degraded = degraded_marker.exists();
    run_execution_bridge_incremental_retention_maintenance_inner(
        execution_records_dir,
        execution_store,
        record,
        &protected,
        hot_window_heights,
        (checkpoint_interval_heights, checkpoint_keep_latest),
        !was_degraded,
    )
}

/// Completes the incremental half of the same retention transaction that
/// produced `validated`. Standalone callers must use the full-validating
/// wrapper above.
pub(super) fn run_execution_bridge_incremental_retention_maintenance_after_task_refresh(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    record: &ExecutionBridgeRecord,
    validated: ValidatedTaskRetention,
    hot_window_heights: u64,
    checkpoint_interval_heights: u64,
    checkpoint_keep_latest: usize,
) -> Result<u64, String> {
    let protected = validated.into_protected(execution_records_dir, execution_store, record)?;
    let degraded_marker = execution_records_dir.join(EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER);
    let was_degraded = degraded_marker.exists();
    run_execution_bridge_incremental_retention_maintenance_inner(
        execution_records_dir,
        execution_store,
        record,
        &protected,
        hot_window_heights,
        (checkpoint_interval_heights, checkpoint_keep_latest),
        !was_degraded,
    )
}

pub(super) fn begin_execution_bridge_retention_transaction(
    execution_records_dir: &Path,
) -> Result<(), String> {
    write_bytes_atomic(
        execution_records_dir
            .join(EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER)
            .as_path(),
        b"execution retention transaction in progress\n",
    )
}

pub(super) fn complete_execution_bridge_retention_transaction(
    execution_records_dir: &Path,
) -> Result<(), String> {
    remove_retention_marker_if_present(
        execution_records_dir
            .join(EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER)
            .as_path(),
        "complete execution bridge retention transaction",
    )
}

pub(super) fn fail_execution_bridge_retention_transaction(
    execution_records_dir: &Path,
) -> Result<(), String> {
    write_bytes_atomic(
        execution_records_dir
            .join(EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER)
            .as_path(),
        b"execution retention reconciliation pending\n",
    )?;
    complete_execution_bridge_retention_transaction(execution_records_dir)
}

pub(super) fn promote_interrupted_execution_bridge_retention(
    execution_records_dir: &Path,
) -> Result<bool, String> {
    let in_progress = execution_records_dir.join(EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER);
    if !in_progress.exists() {
        return Ok(false);
    }
    fail_execution_bridge_retention_transaction(execution_records_dir)?;
    Ok(true)
}

fn remove_retention_marker_if_present(path: &Path, operation: &str) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("{operation} {} failed: {err}", path.display())),
    }
}

fn list_execution_bridge_pre_v2_heights(execution_records_dir: &Path) -> Result<Vec<u64>, String> {
    let mut pre_v2_heights = Vec::new();
    for height in list_execution_bridge_record_heights(execution_records_dir)? {
        let record = load_execution_bridge_record(
            execution_bridge_record_path(execution_records_dir, height).as_path(),
        )?;
        if record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2 {
            pre_v2_heights.push(height);
        }
    }
    Ok(pre_v2_heights)
}

fn update_execution_bridge_record_checkpoint_ref(
    execution_records_dir: &Path,
    height: u64,
    checkpoint_ref: Option<String>,
) -> Result<(), String> {
    let path = execution_bridge_record_path(execution_records_dir, height);
    if !path.exists() {
        return Ok(());
    }
    let mut record = load_execution_bridge_record(path.as_path())?;
    record.checkpoint_ref = checkpoint_ref;
    persist_execution_bridge_record_only(execution_records_dir, &record)?;
    Ok(())
}

fn prune_execution_checkpoint_manifests(
    execution_records_dir: &Path,
    checkpoint_keep_latest: usize,
) -> Result<Vec<u64>, String> {
    let checkpoint_heights = list_execution_checkpoint_heights(execution_records_dir)?;
    let retained_count = checkpoint_keep_latest.max(1);
    if checkpoint_heights.len() <= retained_count {
        return Ok(Vec::new());
    }

    let prune_heights = checkpoint_heights[..checkpoint_heights.len() - retained_count].to_vec();
    for height in &prune_heights {
        let manifest_path = execution_checkpoint_manifest_path(execution_records_dir, *height);
        let manifest_dir = manifest_path.parent().ok_or_else(|| {
            format!(
                "execution checkpoint manifest path {} missing parent",
                manifest_path.display()
            )
        })?;
        if manifest_dir.exists() {
            fs::remove_dir_all(manifest_dir).map_err(|err| {
                format!(
                    "remove execution checkpoint dir {} failed: {}",
                    manifest_dir.display(),
                    err
                )
            })?;
        }
        update_execution_bridge_record_checkpoint_ref(execution_records_dir, *height, None)?;
    }

    Ok(prune_heights)
}

pub(super) fn maybe_persist_execution_checkpoint_for_record(
    execution_records_dir: &Path,
    record: &ExecutionBridgeRecord,
    checkpoint_interval_heights: u64,
    checkpoint_keep_latest: usize,
) -> Result<Option<String>, String> {
    if checkpoint_interval_heights == 0
        || record.height == 0
        || !record.height.is_multiple_of(checkpoint_interval_heights)
    {
        return Ok(None);
    }

    let latest_state_ref = record.latest_state_ref.clone().ok_or_else(|| {
        format!(
            "execution checkpoint height {} missing latest_state_ref",
            record.height
        )
    })?;
    let predecessor_execution_block_hash = if record.height == 1 {
        "genesis".to_string()
    } else {
        let predecessor = load_execution_bridge_record(
            execution_bridge_record_path(execution_records_dir, record.height - 1).as_path(),
        )?;
        if predecessor.height != record.height - 1 || predecessor.world_id != record.world_id {
            return Err(format!(
                "execution checkpoint predecessor record mismatch at height {}",
                record.height - 1
            ));
        }
        predecessor.execution_block_hash
    };
    let manifest = ExecutionCheckpointManifest::new_with_predecessor_execution_block_hash(
        record.world_id.clone(),
        record.height,
        record.execution_block_hash.clone(),
        record.execution_state_root.clone(),
        predecessor_execution_block_hash,
        latest_state_ref,
        record.snapshot_ref.clone(),
        record.journal_ref.clone(),
        record.timestamp_ms,
    )?;
    persist_execution_checkpoint_manifest(execution_records_dir, &manifest)?;
    let checkpoint_ref = execution_checkpoint_manifest_rel_path(record.height);
    prune_execution_checkpoint_manifests(execution_records_dir, checkpoint_keep_latest)?;
    Ok(Some(checkpoint_ref))
}

fn compact_execution_bridge_records(
    execution_records_dir: &Path,
    pin_set: &ExecutionBridgePinSet,
    protected: &BTreeSet<u64>,
) -> Result<usize, String> {
    let record_heights = list_execution_bridge_record_heights(execution_records_dir)?;
    let latest_height = pin_set.latest_height;
    let hot_window_start_height = pin_set.hot_window_start_height.unwrap_or(u64::MAX);
    let mut rewritten_records = 0_usize;

    for height in record_heights {
        let path = execution_bridge_record_path(execution_records_dir, height);
        let mut record = load_execution_bridge_record(path.as_path())?;
        let original_record = record.clone();
        let retain_latest_head = latest_height == Some(height);
        let retain_hot_window = height >= hot_window_start_height;
        let retain_checkpoint = pin_set.checkpoint_heights.contains(&height);

        if !retain_latest_head && !retain_hot_window && !protected.contains(&height) {
            record.latest_state_ref = None;
            record.snapshot_ref = None;
            record.journal_ref = None;
            record.simulator_mirror = None;
            if !retain_checkpoint {
                record.checkpoint_ref = None;
            }
        }
        let retain_archive = record.schema_version < EXECUTION_BRIDGE_RECORD_SCHEMA_V2
            || protected.contains(&height)
            || pin_set
                .archive_window_start_height
                .is_none_or(|start_height| height >= start_height);
        if !retain_archive {
            record.commit_log_ref = None;
            record.external_effect_ref = None;
            record.world_head_proof_ref = None;
            record.world_head_proof_hash = None;
        }

        if record != original_record {
            if retain_latest_head {
                persist_execution_bridge_record(execution_records_dir, &record)?;
            } else {
                persist_execution_bridge_record_only(execution_records_dir, &record)?;
            }
            rewritten_records = rewritten_records.saturating_add(1);
        }
    }

    Ok(rewritten_records)
}

pub(super) fn run_execution_bridge_retention_maintenance(
    execution_records_dir: &Path,
    execution_store: &LocalCasStore,
    hot_window_heights: u64,
) -> Result<u64, String> {
    let pre_v2_heights = list_execution_bridge_pre_v2_heights(execution_records_dir)?;
    let freed_bytes = if !pre_v2_heights.is_empty() {
        sync_execution_bridge_pin_set(execution_records_dir, execution_store, hot_window_heights)?;
        0
    } else {
        let pin_set = build_execution_bridge_pin_set(
            execution_records_dir,
            execution_store,
            hot_window_heights,
        )?;
        let protected = task_evidence::protected_heights(execution_records_dir, execution_store)?;
        compact_execution_bridge_records(execution_records_dir, &pin_set, &protected)?;
        sync_execution_bridge_pin_set(execution_records_dir, execution_store, hot_window_heights)?;
        execution_store
            .prune_orphan_blobs()
            .map_err(|err| format!("prune execution store orphan blobs failed: {:?}", err))?
    };
    remove_retention_marker_if_present(
        execution_records_dir
            .join(EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER)
            .as_path(),
        "clear execution bridge retention degraded marker",
    )?;
    Ok(freed_bytes)
}

pub(super) fn load_execution_bridge_record(path: &Path) -> Result<ExecutionBridgeRecord, String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "read execution bridge record {} failed: {}",
            path.display(),
            err
        )
    })?;
    serde_json::from_slice::<ExecutionBridgeRecord>(bytes.as_slice()).map_err(|err| {
        format!(
            "parse execution bridge record {} failed: {}",
            path.display(),
            err
        )
    })
}

fn normalize_execution_bridge_record_for_persist(
    record: &ExecutionBridgeRecord,
) -> ExecutionBridgeRecord {
    let mut normalized = record.clone();
    normalized.schema_version = EXECUTION_BRIDGE_RECORD_SCHEMA_V3;
    if normalized.latest_state_ref.is_none() {
        normalized.latest_state_ref = normalized.snapshot_ref.clone();
    }
    normalized
}

pub(super) fn persist_execution_bridge_record_only(
    execution_records_dir: &Path,
    record: &ExecutionBridgeRecord,
) -> Result<Vec<u8>, String> {
    let normalized = normalize_execution_bridge_record_for_persist(record);
    let bytes = serde_json::to_vec_pretty(&normalized)
        .map_err(|err| format!("serialize execution bridge record failed: {}", err))?;
    let path = execution_bridge_record_path(execution_records_dir, normalized.height);
    if normalized.controlled_capture_ref.is_some() {
        super::durable_transaction::write_file_durable(&path, &bytes)?;
    } else {
        write_bytes_atomic(path.as_path(), bytes.as_slice())?;
    }
    Ok(bytes)
}

pub(super) fn persist_execution_bridge_record(
    execution_records_dir: &Path,
    record: &ExecutionBridgeRecord,
) -> Result<(), String> {
    let bytes = persist_execution_bridge_record_only(execution_records_dir, record)?;
    let latest_path = execution_records_dir.join("latest.json");
    if record.controlled_capture_ref.is_some() {
        super::durable_transaction::write_file_durable(&latest_path, &bytes)
    } else {
        write_bytes_atomic(latest_path.as_path(), bytes.as_slice())
    }
}
