use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_rel_path,
    load_execution_bridge_record,
};
use super::super::driver::NodeRuntimeExecutionDriver;
use super::temp_dir;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use std::fs;

#[test]
fn node_runtime_execution_driver_uses_storage_profile_checkpoint_interval() {
    let dir = temp_dir("execution-driver-storage-profile-checkpoint");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    for height in 1..=64 {
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".to_string(),
                node_id: "node-a".to_string(),
                proposer_id: "node-a".to_string(),
                height,
                slot: height.saturating_sub(1),
                epoch: 0,
                node_block_hash: format!("node-h{height}"),
                action_root: empty_action_root.clone(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: height as i64 * 1_000,
            })
            .expect("commit with release_default profile");
    }

    let record_32 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 32).as_path(),
    )
    .expect("load record 32");
    let record_64 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 64).as_path(),
    )
    .expect("load record 64");
    assert!(record_32.checkpoint_ref.is_none());
    assert_eq!(
        record_64.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(64).as_str())
    );

    let _ = fs::remove_dir_all(dir);
}
