use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7::simulator::WorldSnapshot as SimulatorSnapshot;
use oasis7_node::NodeSnapshot;
use oasis7_runtime::{
    CHAIN_RESOURCE_DELTA_SCHEMA_V1, CHAIN_RESOURCE_MANIFEST_SCHEMA_V1, CHUNK_GENERATION_SCHEMA_V1,
    ChainChunkResourceStatus, ChainResourceDelta, ChainResourceDeltaEntry, ChainResourceManifest,
};
use oasis7_runtime::{LocalCasStore, Snapshot as RuntimeSnapshot};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize)]
pub(crate) struct ChainWorldResourceStatus {
    pub(crate) schema_version: String,
    pub(crate) delta_schema_version: String,
    pub(crate) world_id: String,
    pub(crate) chain_id: String,
    pub(crate) world_seed: u64,
    pub(crate) chunk_generation_schema_version: String,
    pub(crate) seed_manifest_hash: String,
    pub(crate) starter_chunk_manifest_hash: Option<String>,
    pub(crate) latest_resource_commit_height: u64,
    pub(crate) latest_resource_commit_hash: Option<String>,
    pub(crate) committed_chunk_count: u64,
    pub(crate) provisional_chunk_count: u64,
    pub(crate) pending_delta_count: u64,
    pub(crate) last_delta_id: Option<String>,
    pub(crate) last_delta_commit_height: Option<u64>,
    pub(crate) readiness_status: String,
    pub(crate) failed_gates: Vec<String>,
}

#[cfg(test)]
pub(super) fn build_world_resource_status(
    snapshot: &NodeSnapshot,
    execution_world_dir: &Path,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> ChainWorldResourceStatus {
    build_world_resource_status_with_authoritative_execution(
        snapshot,
        execution_world_dir,
        None,
        None,
        loaded_network_tier_manifest,
    )
}

pub(super) fn build_world_resource_status_with_authoritative_execution(
    snapshot: &NodeSnapshot,
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    execution_storage_root: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> ChainWorldResourceStatus {
    let chain_id = loaded_network_tier_manifest
        .map(|loaded| loaded.manifest.chain_id.clone())
        .unwrap_or_else(|| snapshot.world_id.clone());
    let (manifest, latest_delta, snapshot_load_error) = load_latest_world_resource_snapshot(
        execution_world_dir,
        execution_records_dir,
        execution_storage_root,
        snapshot.world_id.as_str(),
        chain_id.as_str(),
    );
    let mut failed_gates = Vec::new();
    if snapshot.world_id.trim().is_empty() {
        failed_gates.push("world_resource_world_id_missing".to_string());
    }
    if chain_id.trim().is_empty() {
        failed_gates.push("world_resource_chain_id_missing".to_string());
    }
    if let Some(error) = snapshot_load_error {
        failed_gates.push(format!("world_resource_snapshot_unavailable:{error}"));
    }
    let seed_manifest_hash = manifest
        .as_ref()
        .map(|manifest| manifest.manifest_hash.clone())
        .unwrap_or_default();
    let world_seed = manifest
        .as_ref()
        .map(|manifest| manifest.world_seed)
        .unwrap_or(0);
    if world_seed == 0 {
        failed_gates.push("world_resource_seed_missing".to_string());
    }
    if manifest
        .as_ref()
        .is_none_or(|manifest| manifest.world_id != snapshot.world_id)
    {
        failed_gates.push("world_resource_world_id_mismatch".to_string());
    }
    if manifest
        .as_ref()
        .is_none_or(|manifest| manifest.chain_id != chain_id)
    {
        failed_gates.push("world_resource_chain_id_mismatch".to_string());
    }
    if manifest
        .as_ref()
        .is_none_or(|manifest| !manifest.is_schema_current())
    {
        failed_gates.push("world_resource_manifest_hash_mismatch".to_string());
    }
    let starter_chunk_manifest_hash = starter_chunk_manifest_hash(manifest.as_ref());
    if starter_chunk_manifest_hash.is_none() {
        failed_gates.push("world_resource_starter_chunk_missing".to_string());
    }
    let committed_chunk_count = manifest
        .as_ref()
        .map(committed_chunk_count)
        .unwrap_or_default();
    let provisional_chunk_count = manifest
        .as_ref()
        .map(provisional_chunk_count)
        .unwrap_or_default();
    let pending_delta_count = latest_delta
        .as_ref()
        .map(|delta| u64::from(!delta.is_schema_current()))
        .unwrap_or(1);
    if committed_chunk_count == 0 {
        failed_gates.push("world_resource_committed_chunk_missing".to_string());
    }
    if provisional_chunk_count > 0 {
        failed_gates.push("world_resource_provisional_chunk_present".to_string());
    }
    if pending_delta_count > 0 {
        failed_gates.push("world_resource_pending_delta_present".to_string());
    }
    if latest_delta
        .as_ref()
        .is_some_and(|delta| delta.resulting_manifest_hash != seed_manifest_hash)
    {
        failed_gates.push("world_resource_delta_manifest_mismatch".to_string());
    }
    if latest_delta
        .as_ref()
        .is_none_or(|delta| delta.commit_block_hash.as_deref().is_none_or(str::is_empty))
    {
        failed_gates.push("world_resource_delta_commit_hash_missing".to_string());
    }
    if latest_delta.as_ref().is_some_and(|delta| {
        delta.block_height != snapshot.consensus.committed_height
            || delta.ordering_key.height != snapshot.consensus.committed_height
    }) {
        failed_gates.push("world_resource_delta_height_mismatch".to_string());
    }
    if committed_chunk_count == 0
        && latest_delta
            .as_ref()
            .is_none_or(|delta| !delta_has_nonzero_resource_effect(delta))
    {
        failed_gates.push("world_resource_delta_zero_effect".to_string());
    }
    ChainWorldResourceStatus {
        schema_version: CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.to_string(),
        delta_schema_version: CHAIN_RESOURCE_DELTA_SCHEMA_V1.to_string(),
        world_id: snapshot.world_id.clone(),
        chain_id,
        world_seed,
        chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_V1.to_string(),
        seed_manifest_hash,
        starter_chunk_manifest_hash,
        latest_resource_commit_height: snapshot.consensus.committed_height,
        latest_resource_commit_hash: snapshot.consensus.last_block_hash.clone(),
        committed_chunk_count,
        provisional_chunk_count,
        pending_delta_count,
        last_delta_id: latest_delta.as_ref().map(|delta| delta.delta_id.clone()),
        last_delta_commit_height: latest_delta.as_ref().map(|delta| delta.block_height),
        readiness_status: if failed_gates.is_empty() {
            "ready".to_string()
        } else {
            "not_ready".to_string()
        },
        failed_gates,
    }
}

fn load_latest_world_resource_snapshot(
    execution_world_dir: &Path,
    execution_records_dir: Option<&Path>,
    execution_storage_root: Option<&Path>,
    world_id: &str,
    chain_id: &str,
) -> (
    Option<ChainResourceManifest>,
    Option<ChainResourceDelta>,
    Option<String>,
) {
    let mut load_errors = Vec::new();
    let mut fallback = None;

    if let (Some(execution_records_dir), Some(execution_storage_root)) =
        (execution_records_dir, execution_storage_root)
    {
        match load_latest_execution_record_resource_snapshot(
            execution_records_dir,
            execution_storage_root,
            world_id,
            chain_id,
        ) {
            Ok(Some(candidate)) => {
                return validate_world_resource_snapshot(
                    candidate.0,
                    candidate.1,
                    world_id,
                    chain_id,
                );
            }
            Ok(None) => {}
            Err(err) => return (None, None, Some(format!("execution_record:{err}"))),
        }
    }

    let runtime_snapshot_path = execution_world_dir.join("snapshot.json");
    if runtime_snapshot_path.exists() {
        match RuntimeSnapshot::load_json(runtime_snapshot_path.as_path()) {
            Ok(snapshot) if !snapshot.chain_resource_manifest.generated_chunks.is_empty() => {
                let candidate = (
                    snapshot.chain_resource_manifest,
                    snapshot.latest_chain_resource_delta,
                );
                if resource_snapshot_matches(&candidate.0, world_id, chain_id) {
                    return validate_world_resource_snapshot(
                        candidate.0,
                        candidate.1,
                        world_id,
                        chain_id,
                    );
                }
                fallback.get_or_insert(candidate);
            }
            Ok(_) => load_errors.push("runtime_snapshot_empty_resource_manifest".to_string()),
            Err(err) => load_errors.push(format!("runtime_snapshot:{err:?}")),
        }
    } else {
        load_errors.push("runtime_snapshot_missing".to_string());
    }

    let simulator_snapshot_path =
        simulator_world_dir_from_execution_world_dir(execution_world_dir).join("snapshot.json");
    if simulator_snapshot_path.exists() {
        match SimulatorSnapshot::load_json(simulator_snapshot_path.as_path()) {
            Ok(snapshot) if !snapshot.chain_resource_manifest.generated_chunks.is_empty() => {
                let candidate = (
                    snapshot.chain_resource_manifest,
                    Some(snapshot.latest_chain_resource_delta),
                );
                if resource_snapshot_matches(&candidate.0, world_id, chain_id) {
                    return validate_world_resource_snapshot(
                        candidate.0,
                        candidate.1,
                        world_id,
                        chain_id,
                    );
                }
                fallback.get_or_insert(candidate);
            }
            Ok(_) => load_errors.push("simulator_snapshot_empty_resource_manifest".to_string()),
            Err(err) => load_errors.push(format!("simulator_snapshot:{err:?}")),
        }
    } else {
        load_errors.push("simulator_snapshot_missing".to_string());
    }

    if let Some((manifest, latest_delta)) = fallback {
        return validate_world_resource_snapshot(manifest, latest_delta, world_id, chain_id);
    }

    (None, None, Some(load_errors.join(",")))
}

#[derive(Deserialize)]
struct ExecutionResourceSnapshotRecord {
    world_id: String,
    height: u64,
    #[serde(default)]
    latest_state_ref: Option<String>,
    #[serde(default)]
    snapshot_ref: Option<String>,
}

fn load_latest_execution_record_resource_snapshot(
    execution_records_dir: &Path,
    execution_storage_root: &Path,
    world_id: &str,
    chain_id: &str,
) -> Result<Option<(ChainResourceManifest, Option<ChainResourceDelta>)>, String> {
    if !execution_records_dir.exists() {
        return Ok(None);
    }
    let mut latest_record_path = None;
    let mut latest_height = None;
    for entry in std::fs::read_dir(execution_records_dir).map_err(|err| {
        format!(
            "read execution records dir {} failed: {err}",
            execution_records_dir.display()
        )
    })? {
        let entry = entry.map_err(|err| format!("read execution record entry failed: {err}"))?;
        if !entry
            .file_type()
            .map_err(|err| format!("read execution record entry type failed: {err}"))?
            .is_file()
        {
            continue;
        }
        let Some(file_name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".json") else {
            continue;
        };
        let Ok(height) = stem.parse::<u64>() else {
            continue;
        };
        if latest_height.is_none_or(|latest| height > latest) {
            latest_height = Some(height);
            latest_record_path = Some(entry.path());
        }
    }
    let Some(record_path) = latest_record_path else {
        return Ok(None);
    };
    let record_bytes = std::fs::read(record_path.as_path()).map_err(|err| {
        format!(
            "read execution record {} failed: {err}",
            record_path.display()
        )
    })?;
    let record: ExecutionResourceSnapshotRecord = serde_json::from_slice(record_bytes.as_slice())
        .map_err(|err| {
        format!(
            "parse execution record {} failed: {err}",
            record_path.display()
        )
    })?;
    if record.height != latest_height.unwrap_or_default() {
        return Err(format!(
            "latest execution record height mismatch path={} filename_height={} record_height={}",
            record_path.display(),
            latest_height.unwrap_or_default(),
            record.height
        ));
    }
    if record.world_id != world_id {
        return Err(format!(
            "latest execution record world_id mismatch expected={world_id} actual={}",
            record.world_id
        ));
    }
    let snapshot_ref = record
        .latest_state_ref
        .or(record.snapshot_ref)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "latest execution record missing snapshot reference".to_string())?;
    let store = LocalCasStore::new(execution_storage_root);
    let snapshot_bytes = store
        .get_verified(snapshot_ref.as_str())
        .map_err(|err| format!("load execution snapshot CAS ref {snapshot_ref} failed: {err:?}"))?;
    let snapshot: RuntimeSnapshot = serde_cbor::from_slice(snapshot_bytes.as_slice())
        .map_err(|err| format!("parse execution snapshot CAS ref {snapshot_ref} failed: {err}"))?;
    if snapshot.chain_resource_manifest.generated_chunks.is_empty() {
        return Err("latest execution snapshot has empty resource manifest".to_string());
    }
    if !resource_snapshot_matches(&snapshot.chain_resource_manifest, world_id, chain_id) {
        return Err("latest execution snapshot resource identity mismatch".to_string());
    }
    let latest_delta = snapshot
        .latest_chain_resource_delta
        .as_ref()
        .ok_or_else(|| "latest execution snapshot missing resource delta".to_string())?;
    if snapshot.chain_resource_manifest.manifest_height != record.height
        || latest_delta.block_height != record.height
        || latest_delta.ordering_key.height != record.height
    {
        return Err(format!(
            "latest execution snapshot height mismatch record_height={} manifest_height={} delta_height={} ordering_height={}",
            record.height,
            snapshot.chain_resource_manifest.manifest_height,
            latest_delta.block_height,
            latest_delta.ordering_key.height,
        ));
    }
    Ok(Some((
        snapshot.chain_resource_manifest,
        snapshot.latest_chain_resource_delta,
    )))
}

fn resource_snapshot_matches(
    manifest: &ChainResourceManifest,
    world_id: &str,
    chain_id: &str,
) -> bool {
    manifest.world_id == world_id && manifest.chain_id == chain_id
}

fn validate_world_resource_snapshot(
    manifest: ChainResourceManifest,
    latest_delta: Option<ChainResourceDelta>,
    _world_id: &str,
    _chain_id: &str,
) -> (
    Option<ChainResourceManifest>,
    Option<ChainResourceDelta>,
    Option<String>,
) {
    (Some(manifest), latest_delta, None)
}

fn simulator_world_dir_from_execution_world_dir(world_dir: &Path) -> std::path::PathBuf {
    match world_dir.file_name().and_then(|name| name.to_str()) {
        Some(name) if !name.is_empty() => {
            world_dir.with_file_name(format!("{name}-simulator-mirror"))
        }
        _ => world_dir.join("simulator-mirror"),
    }
}

fn starter_chunk_manifest_hash(manifest: Option<&ChainResourceManifest>) -> Option<String> {
    manifest
        .and_then(|manifest| manifest.generated_chunks.values().next())
        .map(|entry| entry.manifest_hash.clone())
        .filter(|hash| !hash.trim().is_empty())
}

fn committed_chunk_count(manifest: &ChainResourceManifest) -> u64 {
    manifest
        .generated_chunks
        .values()
        .filter(|entry| entry.chunk_status == ChainChunkResourceStatus::Committed)
        .count() as u64
}

fn provisional_chunk_count(manifest: &ChainResourceManifest) -> u64 {
    manifest
        .generated_chunks
        .values()
        .filter(|entry| {
            matches!(
                entry.chunk_status,
                ChainChunkResourceStatus::ChainPending | ChainChunkResourceStatus::Provisional
            )
        })
        .count() as u64
}

fn delta_has_nonzero_resource_effect(delta: &ChainResourceDelta) -> bool {
    delta.entries.iter().any(|entry| match entry {
        ChainResourceDeltaEntry::RuntimeResource { delta, .. } => *delta != 0,
        ChainResourceDeltaEntry::MaterialLedger { delta, .. } => *delta != 0,
        ChainResourceDeltaEntry::ChunkResource {
            total_delta_g,
            remaining_delta_g,
            resulting_remaining_g,
            ..
        } => *total_delta_g != 0 || *remaining_delta_g != 0 || *resulting_remaining_g != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7::geometry::GeoPos;
    use oasis7::simulator::WorldKernel;
    use oasis7_node::{NodeConsensusSnapshot, NodeRole};
    use oasis7_runtime::{Action, BlobStore, ChainResourceDerivationContext, LocalCasStore};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-status-world-resource-{prefix}-{unique}"))
    }

    #[test]
    fn status_uses_runtime_execution_snapshot_when_simulator_mirror_is_empty() {
        let dir = temp_dir("runtime-fallback");
        let mut world = oasis7::runtime_host::world::new();
        world.submit_action(Action::RegisterAgent {
            agent_id: "starter-agent-0".to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
        world.step().expect("register agent");
        world
            .save_to_dir(dir.as_path())
            .expect("save runtime world");

        let snapshot = NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: false,
            running: true,
            tick_count: 1,
            last_tick_unix_ms: None,
            consensus: NodeConsensusSnapshot {
                committed_height: 1,
                last_block_hash: Some("block-a".to_string()),
                ..NodeConsensusSnapshot::default()
            },
            consensus_progress_observer_error: None,
            last_error: None,
        };

        let status = build_world_resource_status(&snapshot, dir.as_path(), None);

        assert_eq!(status.world_id, "world-a");
        assert_eq!(status.chain_id, "world-a");
        assert_eq!(status.readiness_status, "not_ready");
        assert_eq!(status.committed_chunk_count, 1);
        assert_eq!(status.pending_delta_count, 0);
        assert!(
            status
                .failed_gates
                .contains(&"world_resource_delta_commit_hash_missing".to_string()),
            "{:?}",
            status.failed_gates
        );
        assert!(
            status
                .failed_gates
                .contains(&"world_resource_world_id_mismatch".to_string()),
            "{:?}",
            status.failed_gates
        );
        assert!(
            status
                .failed_gates
                .contains(&"world_resource_chain_id_mismatch".to_string()),
            "{:?}",
            status.failed_gates
        );
        assert!(status.starter_chunk_manifest_hash.is_some());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_ready_for_empty_runtime_snapshot_with_chain_resource_context() {
        let dir = temp_dir("empty-runtime-context-ready");
        let records_dir = dir.join("records");
        let store_dir = dir.join("store");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 3,
                    commit_block_hash: Some("block-h3"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save runtime world with chain context");

        let snapshot = NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: false,
            running: true,
            tick_count: 3,
            last_tick_unix_ms: None,
            consensus: NodeConsensusSnapshot {
                committed_height: 3,
                last_block_hash: Some("block-h3".to_string()),
                ..NodeConsensusSnapshot::default()
            },
            consensus_progress_observer_error: None,
            last_error: None,
        };

        let status = build_world_resource_status_with_authoritative_execution(
            &snapshot,
            dir.as_path(),
            Some(records_dir.as_path()),
            Some(store_dir.as_path()),
            None,
        );

        assert_eq!(status.readiness_status, "ready");
        assert!(status.failed_gates.is_empty(), "{:?}", status.failed_gates);
        assert_eq!(status.world_id, "world-a");
        assert_eq!(status.chain_id, "world-a");
        assert_ne!(status.world_seed, 0);
        assert_eq!(status.committed_chunk_count, 1);
        assert_eq!(status.provisional_chunk_count, 0);
        assert_eq!(status.pending_delta_count, 0);
        assert_eq!(status.last_delta_commit_height, Some(3));
        assert!(status.starter_chunk_manifest_hash.is_some());
        assert_eq!(
            status.latest_resource_commit_hash.as_deref(),
            Some("block-h3")
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_prefers_matching_runtime_snapshot_over_stale_simulator_mirror() {
        let dir = temp_dir("runtime-over-simulator-mirror");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 3,
                    commit_block_hash: Some("block-h3"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save runtime world with chain context");

        let mirror_dir = simulator_world_dir_from_execution_world_dir(dir.as_path());
        WorldKernel::new()
            .save_to_dir_with_chain_resource_context(
                mirror_dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "simulator-world",
                    chain_id: "simulator-chain",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 0,
                    commit_block_hash: Some("simulator-genesis"),
                    tick: 0,
                },
            )
            .expect("save stale simulator mirror");

        let snapshot = NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: false,
            running: true,
            tick_count: 3,
            last_tick_unix_ms: None,
            consensus: NodeConsensusSnapshot {
                committed_height: 3,
                last_block_hash: Some("block-h3".to_string()),
                ..NodeConsensusSnapshot::default()
            },
            consensus_progress_observer_error: None,
            last_error: None,
        };

        let status = build_world_resource_status(&snapshot, dir.as_path(), None);

        assert_eq!(status.readiness_status, "ready");
        assert!(status.failed_gates.is_empty(), "{:?}", status.failed_gates);
        assert_eq!(status.world_id, "world-a");
        assert_eq!(status.chain_id, "world-a");
        assert_eq!(status.last_delta_commit_height, Some(3));

        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(mirror_dir);
    }

    #[test]
    fn status_prefers_latest_execution_record_cas_snapshot_over_stale_execution_world_json() {
        let dir = temp_dir("authoritative-execution-record");
        let records_dir = dir.join("records");
        let store_dir = dir.join("store");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 0,
                    commit_block_hash: Some("block-genesis"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save stale execution-world json");
        fs::create_dir_all(records_dir.as_path()).expect("create execution records");

        let authoritative_snapshot = world.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext {
                world_id: "world-a",
                chain_id: "world-a",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 3,
                commit_block_hash: Some("block-h3"),
                tick: world.state().time,
            },
            "world-a-config",
            "world-a-generation",
        );
        let store = LocalCasStore::new(store_dir.as_path());
        let snapshot_ref = store
            .put_bytes(
                serde_cbor::to_vec(&authoritative_snapshot)
                    .expect("serialize authoritative snapshot")
                    .as_slice(),
            )
            .expect("store authoritative snapshot");
        fs::write(
            records_dir.join("00000000000000000003.json"),
            format!(r#"{{"world_id":"world-a","height":3,"latest_state_ref":"{snapshot_ref}"}}"#),
        )
        .expect("write latest execution record");

        let snapshot = NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: false,
            running: true,
            tick_count: 3,
            last_tick_unix_ms: None,
            consensus: NodeConsensusSnapshot {
                committed_height: 3,
                last_block_hash: Some("block-h3".to_string()),
                ..NodeConsensusSnapshot::default()
            },
            consensus_progress_observer_error: None,
            last_error: None,
        };

        let status = build_world_resource_status_with_authoritative_execution(
            &snapshot,
            dir.as_path(),
            Some(records_dir.as_path()),
            Some(store_dir.as_path()),
            None,
        );

        assert_eq!(status.last_delta_commit_height, Some(3));
        assert!(
            !status
                .failed_gates
                .contains(&"world_resource_delta_height_mismatch".to_string()),
            "{:?}",
            status.failed_gates
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_rejects_tampered_authoritative_execution_record_cas_snapshot() {
        let dir = temp_dir("tampered-authoritative-execution-record");
        let records_dir = dir.join("records");
        let store_dir = dir.join("store");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 3,
                    commit_block_hash: Some("block-h3"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save ready legacy execution-world json");
        fs::create_dir_all(records_dir.as_path()).expect("create execution records");

        let authoritative_snapshot = world.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext {
                world_id: "world-a",
                chain_id: "world-a",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 3,
                commit_block_hash: Some("block-h3"),
                tick: world.state().time,
            },
            "world-a-config",
            "world-a-generation",
        );
        let store = LocalCasStore::new(store_dir.as_path());
        let snapshot_ref = store
            .put_bytes(
                serde_cbor::to_vec(&authoritative_snapshot)
                    .expect("serialize authoritative snapshot")
                    .as_slice(),
            )
            .expect("store authoritative snapshot");
        let mut tampered_snapshot = authoritative_snapshot;
        tampered_snapshot.journal_len = tampered_snapshot.journal_len.saturating_add(1);
        fs::write(
            store.blobs_dir().join(format!("{snapshot_ref}.blob")),
            serde_cbor::to_vec(&tampered_snapshot).expect("serialize tampered snapshot"),
        )
        .expect("tamper authoritative snapshot bytes");
        fs::write(
            records_dir.join("00000000000000000003.json"),
            format!(r#"{{"world_id":"world-a","height":3,"latest_state_ref":"{snapshot_ref}"}}"#),
        )
        .expect("write latest execution record");

        let status = build_world_resource_status_with_authoritative_execution(
            &ready_node_snapshot(),
            dir.as_path(),
            Some(records_dir.as_path()),
            Some(store_dir.as_path()),
            None,
        );

        assert_authoritative_execution_record_failure(&status);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_rejects_invalid_latest_execution_record_over_ready_legacy_json() {
        let dir = temp_dir("invalid-authoritative-execution-record");
        let records_dir = dir.join("records");
        let store_dir = dir.join("store");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 3,
                    commit_block_hash: Some("block-h3"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save ready legacy execution-world json");
        fs::create_dir_all(records_dir.as_path()).expect("create execution records");
        fs::write(records_dir.join("00000000000000000003.json"), b"{")
            .expect("write invalid latest execution record");

        let status = build_world_resource_status_with_authoritative_execution(
            &ready_node_snapshot(),
            dir.as_path(),
            Some(records_dir.as_path()),
            Some(store_dir.as_path()),
            None,
        );

        assert_authoritative_execution_record_failure(&status);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_rejects_newer_execution_record_with_stale_verified_snapshot_height() {
        let dir = temp_dir("stale-authoritative-execution-snapshot-height");
        let records_dir = dir.join("records");
        let store_dir = dir.join("store");
        let world = oasis7::runtime_host::world::new();
        world
            .save_to_dir_with_chain_resource_context(
                dir.as_path(),
                ChainResourceDerivationContext {
                    world_id: "world-a",
                    chain_id: "world-a",
                    genesis_ref: None,
                    created_at_height: 0,
                    manifest_height: 3,
                    commit_block_hash: Some("block-h3"),
                    tick: world.state().time,
                },
                "world-a-config",
                "world-a-generation",
            )
            .expect("save ready legacy execution-world json");
        fs::create_dir_all(records_dir.as_path()).expect("create execution records");

        let stale_snapshot = world.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext {
                world_id: "world-a",
                chain_id: "world-a",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 2,
                commit_block_hash: Some("block-h2"),
                tick: world.state().time,
            },
            "world-a-config",
            "world-a-generation",
        );
        let store = LocalCasStore::new(store_dir.as_path());
        let snapshot_ref = store
            .put_bytes(
                serde_cbor::to_vec(&stale_snapshot)
                    .expect("serialize stale authoritative snapshot")
                    .as_slice(),
            )
            .expect("store stale authoritative snapshot");
        fs::write(
            records_dir.join("00000000000000000003.json"),
            format!(r#"{{"world_id":"world-a","height":3,"latest_state_ref":"{snapshot_ref}"}}"#),
        )
        .expect("write newer execution record");

        let status = build_world_resource_status_with_authoritative_execution(
            &ready_node_snapshot(),
            dir.as_path(),
            Some(records_dir.as_path()),
            Some(store_dir.as_path()),
            None,
        );

        assert_authoritative_execution_record_failure(&status);

        let _ = fs::remove_dir_all(dir);
    }

    fn ready_node_snapshot() -> NodeSnapshot {
        NodeSnapshot {
            node_id: "node-a".to_string(),
            player_id: "player-a".to_string(),
            world_id: "world-a".to_string(),
            role: NodeRole::Sequencer,
            replication_enabled: false,
            running: true,
            tick_count: 3,
            last_tick_unix_ms: None,
            consensus: NodeConsensusSnapshot {
                committed_height: 3,
                last_block_hash: Some("block-h3".to_string()),
                ..NodeConsensusSnapshot::default()
            },
            consensus_progress_observer_error: None,
            last_error: None,
        }
    }

    fn assert_authoritative_execution_record_failure(status: &ChainWorldResourceStatus) {
        assert_eq!(status.readiness_status, "not_ready");
        assert!(
            status
                .failed_gates
                .iter()
                .any(|gate| gate
                    .starts_with("world_resource_snapshot_unavailable:execution_record:")),
            "{:?}",
            status.failed_gates
        );
    }
}
