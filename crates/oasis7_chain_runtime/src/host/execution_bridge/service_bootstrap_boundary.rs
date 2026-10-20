//! Durable local setup metadata, not a fabricated committed-height record.
//! The actual admitted seed CAS roots authenticate its first successor hash.
use super::driver::NodeRuntimeExecutionDriver;
use super::execution_hash::ExecutionHashPayload;
use super::{ExecutionBridgeRecord, checkpoint, to_cbor, world_service_read, write_bytes_atomic};
use oasis7::world_service::WorldIdentity;
use oasis7_node::NodeExecutionBootstrap;
use oasis7_runtime::{BlobStore, Journal, Snapshot, blake3_hex};
use serde::{Deserialize, Serialize};

const FILE: &str = "local-execution-bootstrap.json";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupBoundary {
    schema_version: u16,
    world: WorldIdentity,
    height: u64,
    consensus_block_hash: String,
    execution_block_hash: String,
    execution_state_root: String,
    snapshot_ref: String,
    journal_ref: String,
}

fn validate(
    driver: &NodeRuntimeExecutionDriver,
    boundary: &SetupBoundary,
    world_id: &str,
) -> Result<(), String> {
    let identity = world_service_read::identity(&driver.world_dir, world_id)?;
    if boundary.schema_version != 1
        || boundary.world != identity
        || boundary.height == 0
        || boundary.consensus_block_hash.trim().is_empty()
    {
        return Err("service setup boundary identity or version mismatch".into());
    }
    let snapshot_bytes = driver
        .execution_store
        .get_verified(&boundary.snapshot_ref)
        .map_err(|error| format!("service setup snapshot CAS: {error:?}"))?;
    let journal_bytes = driver
        .execution_store
        .get_verified(&boundary.journal_ref)
        .map_err(|error| format!("service setup journal CAS: {error:?}"))?;
    let snapshot: Snapshot =
        serde_cbor::from_slice(&snapshot_bytes).map_err(|error| error.to_string())?;
    let journal: Journal =
        serde_cbor::from_slice(&journal_bytes).map_err(|error| error.to_string())?;
    if blake3_hex(&snapshot_bytes) != boundary.execution_state_root
        || boundary.snapshot_ref != boundary.execution_state_root
        || snapshot.state.time != boundary.height
        || snapshot.chain_resource_manifest.world_id != identity.world_id
        || snapshot.chain_resource_manifest.genesis_ref.as_deref()
            != Some(identity.genesis_digest.as_str())
        || snapshot.journal_len != journal.len()
        || snapshot.last_event_id != journal.events.last().map_or(0, |event| event.id)
    {
        return Err("service setup boundary snapshot/journal mismatch".into());
    }
    let journal_len = journal.len();
    let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        .map_err(|error| format!("service setup world: {error:?}"))?;
    if world
        .current_cognition_runtime_binding()
        .map_err(|error| format!("service setup binding: {error:?}"))?
        .world_id
        != identity.world_id
    {
        return Err("service setup runtime identity mismatch".into());
    }
    let expected = blake3_hex(&to_cbor(ExecutionHashPayload {
        world_id,
        height: boundary.height,
        prev_execution_block_hash: "genesis",
        execution_state_root: &boundary.execution_state_root,
        journal_len,
    })?);
    if expected != boundary.execution_block_hash {
        return Err("service setup genesis execution hash mismatch".into());
    }
    Ok(())
}

/// Save only the real, already validated initial operator setup boundary.
/// Existing genuine commit records prevent recreating or replacing it later.
pub(super) fn persist_initial(
    driver: &NodeRuntimeExecutionDriver,
    baseline: &NodeExecutionBootstrap,
) -> Result<(), String> {
    if !driver
        .world_dir
        .join("world-service-identity.json")
        .exists()
        || !checkpoint::list_execution_bridge_record_heights(&driver.records_dir)?.is_empty()
        || driver.records_dir.join("latest.json").exists()
    {
        return Ok(());
    }
    let binding = driver
        .execution_world
        .current_cognition_runtime_binding()
        .map_err(|error| format!("{error:?}"))?;
    let identity = world_service_read::identity(&driver.world_dir, &binding.world_id)?;
    let snapshot_bytes = to_cbor(driver.execution_world.snapshot())?;
    let journal_bytes = to_cbor(driver.execution_world.journal())?;
    let snapshot_ref = blake3_hex(&snapshot_bytes);
    let journal_ref = blake3_hex(&journal_bytes);
    driver
        .execution_store
        .put(&snapshot_ref, &snapshot_bytes)
        .map_err(|error| format!("service setup snapshot store: {error:?}"))?;
    driver
        .execution_store
        .put(&journal_ref, &journal_bytes)
        .map_err(|error| format!("service setup journal store: {error:?}"))?;
    let boundary = SetupBoundary {
        schema_version: 1,
        world: identity,
        height: baseline.height,
        consensus_block_hash: baseline.consensus_block_hash.clone(),
        execution_block_hash: baseline.execution_block_hash.clone(),
        execution_state_root: baseline.execution_state_root.clone(),
        snapshot_ref,
        journal_ref,
    };
    validate(driver, &boundary, &binding.world_id)?;
    let path = driver.records_dir.join(FILE);
    if path.exists() {
        let existing: SetupBoundary =
            serde_json::from_slice(&std::fs::read(&path).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        validate(driver, &existing, &binding.world_id)?;
        if existing != boundary {
            return Err("service setup boundary is immutable".into());
        }
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(&boundary).map_err(|error| error.to_string())?;
    write_bytes_atomic(&path, &bytes)
}

/// Supply a predecessor only for the first genuine successor of the exact
/// persisted setup boundary. Missing or corrupt material never synthesizes it.
pub(super) fn predecessor(
    driver: &NodeRuntimeExecutionDriver,
    record: &ExecutionBridgeRecord,
) -> Result<String, String> {
    let path = driver.records_dir.join(FILE);
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("service setup boundary unavailable: {error}"))?;
    let boundary: SetupBoundary = serde_json::from_slice(&bytes)
        .map_err(|error| format!("service setup boundary decode: {error}"))?;
    validate(driver, &boundary, &record.world_id)?;
    if record.height
        != boundary
            .height
            .checked_add(1)
            .ok_or("service setup height overflow")?
        || record.prev_node_block_hash.as_deref() != Some(boundary.consensus_block_hash.as_str())
    {
        return Err("service setup boundary is not this record's predecessor".into());
    }
    Ok(boundary.execution_block_hash)
}
