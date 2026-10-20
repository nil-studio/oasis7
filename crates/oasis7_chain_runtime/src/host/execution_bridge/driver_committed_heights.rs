use std::fs;
use std::path::Path;

use oasis7_node::NodeSnapshot;
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, LocalCasStore, World as RuntimeWorld, blake3_hex,
};
use oasis7_wasm_abi::ModuleSandbox;

use super::checkpoint::{
    execution_checkpoint_root_dir, load_execution_checkpoint_manifest,
    maybe_persist_execution_checkpoint_for_record, persist_execution_bridge_record,
    run_execution_bridge_incremental_retention_maintenance,
};
use super::execution_hash::{
    ExecutionHashPayload, execution_resource_commit_hash, execution_resource_context_hash,
    execution_resource_created_at_height,
};
use super::{
    EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
    EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_KEEP_LATEST, EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS,
    ExecutionBridgeRecord, ExecutionBridgeState, persist_world_head_proof_for_record,
};

pub(crate) fn bridge_committed_heights(
    snapshot: &NodeSnapshot,
    observed_at_unix_ms: i64,
    execution_world: &mut RuntimeWorld,
    execution_sandbox: &mut dyn ModuleSandbox,
    execution_store: &LocalCasStore,
    execution_records_dir: &Path,
    state: &mut ExecutionBridgeState,
) -> Result<Vec<ExecutionBridgeRecord>, String> {
    bridge_committed_heights_with_policy(
        snapshot,
        observed_at_unix_ms,
        execution_world,
        execution_sandbox,
        execution_store,
        execution_records_dir,
        state,
        EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_KEEP_LATEST,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
fn bridge_committed_heights_with_policy(
    snapshot: &NodeSnapshot,
    observed_at_unix_ms: i64,
    execution_world: &mut RuntimeWorld,
    execution_sandbox: &mut dyn ModuleSandbox,
    execution_store: &LocalCasStore,
    execution_records_dir: &Path,
    state: &mut ExecutionBridgeState,
    hot_window_heights: u64,
    checkpoint_interval_heights: u64,
    checkpoint_keep_latest: usize,
) -> Result<Vec<ExecutionBridgeRecord>, String> {
    let target_height = snapshot.consensus.committed_height;
    if target_height <= state.last_applied_committed_height {
        return Ok(Vec::new());
    }

    fs::create_dir_all(execution_records_dir).map_err(|err| {
        format!(
            "create execution records dir {} failed: {}",
            execution_records_dir.display(),
            err
        )
    })?;

    let mut records = Vec::new();
    for height in (state.last_applied_committed_height + 1)..=target_height {
        execution_world
            .step_with_modules(execution_sandbox)
            .map_err(|err| {
                format!(
                    "execution bridge world.step failed at height {}: {:?}",
                    height, err
                )
            })?;

        let prev_node_block_hash = state.last_node_block_hash.clone();
        let node_block_hash = if height == target_height {
            snapshot.consensus.last_block_hash.clone()
        } else {
            None
        };
        let runtime_resource_commit_hash =
            execution_resource_commit_hash(&snapshot.world_id, height);
        let runtime_resource_context = ChainResourceDerivationContext {
            world_id: snapshot.world_id.as_str(),
            chain_id: snapshot.world_id.as_str(),
            genesis_ref: None,
            created_at_height: execution_resource_created_at_height(height),
            manifest_height: height,
            commit_block_hash: Some(runtime_resource_commit_hash.as_str()),
            tick: execution_world.state().time,
        };
        let runtime_resource_context_hash = execution_resource_context_hash(&snapshot.world_id);
        let snapshot_value = execution_world.snapshot_with_chain_resource_context(
            runtime_resource_context,
            runtime_resource_context_hash.clone(),
            runtime_resource_context_hash.clone(),
        );
        let journal_value = execution_world.journal().clone();
        let snapshot_bytes = super::to_cbor(snapshot_value)?;
        let journal_bytes = super::to_cbor(journal_value)?;

        let snapshot_ref = execution_store
            .put_bytes(snapshot_bytes.as_slice())
            .map_err(|err| format!("execution bridge CAS snapshot put failed: {:?}", err))?;
        let journal_ref = execution_store
            .put_bytes(journal_bytes.as_slice())
            .map_err(|err| format!("execution bridge CAS journal put failed: {:?}", err))?;

        let execution_state_root = blake3_hex(snapshot_bytes.as_slice());
        let prev_execution_block_hash = state
            .last_execution_block_hash
            .clone()
            .unwrap_or_else(|| "genesis".to_string());
        let hash_payload = ExecutionHashPayload {
            world_id: snapshot.world_id.as_str(),
            height,
            prev_execution_block_hash: prev_execution_block_hash.as_str(),
            execution_state_root: execution_state_root.as_str(),
            journal_len: execution_world.journal().len(),
        };
        let execution_block_hash = blake3_hex(super::to_cbor(hash_payload)?.as_slice());
        let mut record = ExecutionBridgeRecord::new_v3(
            snapshot.world_id.clone(),
            height,
            node_block_hash.clone(),
            prev_node_block_hash,
            "snapshot-bridge".to_string(),
            "snapshot-bridge".to_string(),
            execution_block_hash.clone(),
            execution_state_root.clone(),
            execution_world.journal().len(),
            snapshot_ref,
            journal_ref,
            None,
            None,
            observed_at_unix_ms,
        );
        record.checkpoint_ref = maybe_persist_execution_checkpoint_for_record(
            execution_records_dir,
            &record,
            checkpoint_interval_heights,
            checkpoint_keep_latest,
        )?;
        if record.node_block_hash.is_some() {
            let checkpoint_manifest = record
                .checkpoint_ref
                .as_deref()
                .map(|checkpoint_ref| {
                    load_execution_checkpoint_manifest(
                        execution_checkpoint_root_dir(execution_records_dir)
                            .join(checkpoint_ref)
                            .as_path(),
                    )
                })
                .transpose()?;
            persist_world_head_proof_for_record(
                execution_store,
                &mut record,
                checkpoint_manifest.as_ref(),
            )?;
        }
        persist_execution_bridge_record(execution_records_dir, &record)?;

        if let Err(err) =
            super::checkpoint::begin_execution_bridge_retention_transaction(execution_records_dir)
                .and_then(|_| {
                    super::checkpoint::refresh_live_task_read_evidence(
                        execution_records_dir,
                        execution_store,
                        execution_world,
                        hot_window_heights,
                    )
                })
                .and_then(|_| {
                    run_execution_bridge_incremental_retention_maintenance(
                        execution_records_dir,
                        execution_store,
                        &record,
                        hot_window_heights,
                        checkpoint_interval_heights,
                        checkpoint_keep_latest,
                    )
                })
                .and_then(|_| {
                    super::checkpoint::complete_execution_bridge_retention_transaction(
                        execution_records_dir,
                    )
                })
        {
            oasis7::observability::emit_stderr_or_event(
                tracing::Level::WARN,
                format!(
                    "execution bridge incremental retention failed after replay height {}: {}",
                    height, err
                )
                .as_str(),
                "execution bridge incremental retention failed after replay",
            );
        }

        state.last_applied_committed_height = height;
        state.last_execution_block_hash = Some(execution_block_hash);
        state.last_execution_state_root = Some(execution_state_root);
        state.last_node_block_hash = node_block_hash;
        records.push(record);
    }

    Ok(records)
}
