use super::super::EXECUTION_BRIDGE_RECORD_SCHEMA_V3;
use super::super::checkpoint::{execution_bridge_record_path, load_execution_bridge_record};
use super::super::driver::NodeRuntimeExecutionDriver;
use super::temp_dir;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::{LocalCasStore, Snapshot as RuntimeSnapshot};
use std::fs;

fn commit_context(
    node_id: &str,
    node_block_hash: &str,
    action_root: String,
) -> NodeExecutionCommitContext {
    NodeExecutionCommitContext {
        world_id: "w1".to_string(),
        node_id: node_id.to_string(),
        proposer_id: node_id.to_string(),
        height: 1,
        slot: 0,
        epoch: 0,
        node_block_hash: node_block_hash.to_string(),
        action_root,
        committed_actions: Vec::new(),
        committed_at_unix_ms: 1_000,
    }
}

fn authoritative_delta_commit_hash(
    records_dir: std::path::PathBuf,
    storage_root: std::path::PathBuf,
) -> String {
    let record = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load authoritative v3 bridge record");
    assert_eq!(record.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V3);
    let snapshot_ref = record
        .snapshot_ref
        .as_deref()
        .expect("authoritative v3 snapshot ref");
    let snapshot_bytes = LocalCasStore::new(storage_root)
        .get_verified(snapshot_ref)
        .expect("load authoritative snapshot CAS blob");
    serde_cbor::from_slice::<RuntimeSnapshot>(snapshot_bytes.as_slice())
        .expect("decode authoritative snapshot")
        .latest_chain_resource_delta
        .and_then(|delta| delta.commit_block_hash)
        .expect("resource delta commit hash")
}

#[test]
fn node_runtime_execution_driver_keeps_execution_hash_deterministic_across_node_provenance() {
    let dir = temp_dir("execution-driver-deterministic-provenance");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let mut driver_a = NodeRuntimeExecutionDriver::new(
        dir.join("a-state.json"),
        dir.join("a-world"),
        dir.join("a-records"),
        dir.join("a-store"),
    )
    .expect("driver a");
    let mut driver_b = NodeRuntimeExecutionDriver::new(
        dir.join("b-state.json"),
        dir.join("b-world"),
        dir.join("b-records"),
        dir.join("b-store"),
    )
    .expect("driver b");

    let result_a = driver_a
        .on_commit(commit_context(
            "sequencer",
            "node-h1-sequencer",
            empty_action_root.clone(),
        ))
        .expect("commit a");
    let result_b = driver_b
        .on_commit(commit_context(
            "storage",
            "node-h1-storage",
            empty_action_root,
        ))
        .expect("commit b");

    assert_eq!(result_a.execution_state_root, result_b.execution_state_root);
    assert_eq!(result_a.execution_block_hash, result_b.execution_block_hash);
    assert_eq!(
        load_execution_bridge_record(
            execution_bridge_record_path(dir.join("a-records").as_path(), 1).as_path(),
        )
        .expect("record a")
        .node_block_hash
        .as_deref(),
        Some("node-h1-sequencer")
    );
    assert_eq!(
        load_execution_bridge_record(
            execution_bridge_record_path(dir.join("b-records").as_path(), 1).as_path(),
        )
        .expect("record b")
        .node_block_hash
        .as_deref(),
        Some("node-h1-storage")
    );
    let delta_commit_hash_a =
        authoritative_delta_commit_hash(dir.join("a-records"), dir.join("a-store"));
    assert!(!delta_commit_hash_a.is_empty());
    assert_eq!(
        delta_commit_hash_a,
        authoritative_delta_commit_hash(dir.join("b-records"), dir.join("b-store"))
    );

    let _ = fs::remove_dir_all(dir);
}
