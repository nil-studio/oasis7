use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_path,
    execution_checkpoint_manifest_rel_path, load_execution_bridge_record,
    load_execution_checkpoint_manifest,
};
use super::super::driver::{
    NodeRuntimeExecutionDriver, load_execution_bridge_state, persist_execution_bridge_state,
};
use super::super::driver_checkpoint_install::{
    CheckpointInstallFault, set_checkpoint_install_fault_for_test,
};
use super::*;
use oasis7_node::{
    NodeExecutionCheckpointInstallContext, NodeExecutionCommitContext, NodeExecutionHook,
    compute_consensus_action_root,
};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::LocalCasStore;

#[test]
fn node_runtime_execution_driver_exports_and_installs_checkpoint_bundle() {
    let dir = temp_dir("execution-driver-checkpoint-bundle");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");

    let first = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-bundle".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-bundle".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect("commit 2");
    assert_ne!(first.execution_state_root, second.execution_state_root);
    let bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");
    let source_store = LocalCasStore::new(source_root.join("storage"));
    let source_record = load_execution_bridge_record(
        execution_bridge_record_path(source_root.join("records").as_path(), 2).as_path(),
    )
    .expect("source record");
    let source_manifest = load_execution_checkpoint_manifest(
        execution_checkpoint_manifest_path(source_root.join("records").as_path(), 2).as_path(),
    )
    .expect("source checkpoint manifest");
    let source_proof = load_world_head_proof(&source_store, &source_record);
    let source_checkpoint = source_proof
        .checkpoint
        .as_ref()
        .expect("checkpoint proof evidence");
    assert_eq!(source_checkpoint.checkpoint_height, 2);
    assert_eq!(
        source_checkpoint.execution_block_hash,
        source_record.execution_block_hash
    );
    assert_eq!(
        source_checkpoint.execution_state_root,
        source_record.execution_state_root
    );
    assert_eq!(
        source_checkpoint.manifest_ref,
        source_record
            .checkpoint_ref
            .clone()
            .expect("checkpoint ref")
    );
    assert_eq!(
        source_checkpoint.manifest_hash,
        source_manifest.manifest_hash
    );
    assert!(
        source_checkpoint
            .pinned_refs
            .contains(&source_record.snapshot_ref.clone().expect("snapshot ref"))
    );
    assert!(
        source_checkpoint
            .pinned_refs
            .contains(&source_record.journal_ref.clone().expect("journal ref"))
    );
    let mut tampered_checkpoint = source_proof.clone();
    tampered_checkpoint
        .checkpoint
        .as_mut()
        .expect("checkpoint")
        .execution_state_root = "tampered-checkpoint-state".to_string();
    assert!(
        tampered_checkpoint
            .validate_contract()
            .expect_err("tampered checkpoint state should fail")
            .contains("checkpoint execution state mismatch")
    );
    let mut tampered_checkpoint_height = source_proof.clone();
    tampered_checkpoint_height
        .checkpoint
        .as_mut()
        .expect("checkpoint")
        .checkpoint_height += 1;
    assert!(
        tampered_checkpoint_height
            .validate_contract()
            .expect_err("tampered checkpoint height should fail")
            .contains("checkpoint height mismatch")
    );
    let mut tampered_checkpoint_execution_block = source_proof.clone();
    tampered_checkpoint_execution_block
        .checkpoint
        .as_mut()
        .expect("checkpoint")
        .execution_block_hash = "tampered-execution-block".to_string();
    assert!(
        tampered_checkpoint_execution_block
            .validate_contract()
            .expect_err("tampered checkpoint execution block should fail")
            .contains("checkpoint execution block mismatch")
    );

    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    let installed = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-bundle".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect("install checkpoint");

    assert_eq!(installed, second);
    let target_state = load_execution_bridge_state(target_root.join("bridge-state.json").as_path())
        .expect("target state");
    assert_eq!(target_state.last_applied_committed_height, 2);
    assert_eq!(
        target_state.last_execution_state_root.as_deref(),
        Some(second.execution_state_root.as_str())
    );
    let target_record = load_execution_bridge_record(
        execution_bridge_record_path(target_root.join("records").as_path(), 2).as_path(),
    )
    .expect("target record");
    assert_eq!(
        target_record.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(2).as_str())
    );
    assert_eq!(
        target_record.execution_state_root,
        second.execution_state_root
    );
    assert_eq!(target_record.world_head_proof_ref, None);
    assert_eq!(target_record.world_head_proof_hash, None);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_equal_height_replay_after_checkpoint_install() {
    let dir = temp_dir("execution-driver-checkpoint-equal-height-replay");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-equal-height-replay".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-equal-height-replay".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect("commit 2");
    let bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");

    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-equal-height-replay".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash,
                execution_state_root: second.execution_state_root,
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect("install checkpoint");

    for context in [
        NodeExecutionCommitContext {
            world_id: "world-checkpoint-equal-height-replay".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        },
        NodeExecutionCommitContext {
            world_id: "world-checkpoint-equal-height-replay".to_string(),
            node_id: "node-c".to_string(),
            proposer_id: "node-c".to_string(),
            height: 2,
            slot: 9,
            epoch: 7,
            node_block_hash: "conflicting-block".to_string(),
            action_root: "conflicting-action-root".to_string(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 99_999,
        },
    ] {
        let err = target
            .on_commit(context)
            .expect_err("checkpoint-installed head must reject equal-height consensus replay");
        assert!(
            err.contains("checkpoint-install"),
            "unexpected equal-height checkpoint replay error: {err}"
        );
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_checkpoint_install_survives_restart_and_continues() {
    let dir = temp_dir("execution-driver-checkpoint-restart-continuation");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-restart-continuation".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-restart-continuation".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect("commit 2");
    let bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");

    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-restart-continuation".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash,
                execution_state_root: second.execution_state_root,
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect("install checkpoint");
    drop(target);

    let mut restarted = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("checkpoint-installed target restarts");
    let replay_err = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-restart-continuation".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect_err("checkpoint-installed head rejects equal-height replay after restart");
    assert!(replay_err.contains("checkpoint-install"), "{replay_err}");

    let third = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-restart-continuation".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-b".to_string(),
            height: 3,
            slot: 3,
            epoch: 0,
            node_block_hash: "block-3".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_003,
        })
        .expect("continue at height N + 1 after checkpoint restart");
    assert_eq!(third.execution_height, 3);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_checkpoint_restart_reconciles_newer_published_record_when_state_is_stale()
 {
    let dir = temp_dir("execution-driver-checkpoint-restart-newer-published-record");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-stale-state".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("commit one");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-stale-state".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect("commit two");
    let bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");

    let state_path = target_root.join("bridge-state.json");
    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-stale-state".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect("install checkpoint");
    drop(target);

    persist_execution_bridge_state(
        state_path.as_path(),
        &ExecutionBridgeState {
            last_applied_committed_height: 1,
            last_execution_block_hash: Some("stale-execution-hash".to_string()),
            last_execution_state_root: Some("stale-state-root".to_string()),
            last_node_block_hash: Some("stale-node-hash".to_string()),
        },
    )
    .expect("simulate crash after checkpoint record publication");

    let mut restarted = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("checkpoint restart reconciles newer authoritative record");
    assert_eq!(restarted.state.last_applied_committed_height, 2);
    let replay_err = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-stale-state".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect_err("checkpoint-installed head rejects equal-height replay after recovery");
    assert!(replay_err.contains("checkpoint-install"), "{replay_err}");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_rolls_back_prepared_checkpoint_install_transaction() {
    let dir = temp_dir("execution-driver-checkpoint-prepared-transaction-recovery");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("source commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect("source commit 2");
    let bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");

    let state_path = target_root.join("bridge-state.json");
    let world_dir = target_root.join("world");
    let records_dir = target_root.join("records");
    let storage_root = target_root.join("storage");
    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
    )
    .expect("target driver");
    target
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("target commit 1");
    let previous_state = target.state.clone();
    let previous_world_state = target.execution_world.state().clone();
    let previous_world_journal = target.execution_world.journal().clone();
    let previous_state_bytes = fs::read(state_path.as_path()).expect("previous state bytes");
    let previous_snapshot_bytes =
        fs::read(world_dir.join("snapshot.json")).expect("previous snapshot bytes");
    let sidecar = world_dir
        .join("distfs")
        .join("module-store")
        .join("index.bin");
    fs::create_dir_all(sidecar.parent().expect("sidecar parent")).expect("create sidecar parent");
    fs::write(sidecar.as_path(), b"old world sidecar").expect("write old world sidecar");
    set_checkpoint_install_fault_for_test(Some(
        CheckpointInstallFault::AfterRecordLatestPublication,
    ));
    let err = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle.clone(),
        )
        .expect_err("record/latest fault must simulate an interrupted publication");
    assert!(err.contains("after record/latest publication"), "{err}");

    assert!(
        records_dir
            .join("checkpoint-install-transaction.json")
            .exists()
    );
    assert_eq!(
        load_execution_bridge_record(records_dir.join("latest.json").as_path())
            .expect("leaked latest record")
            .height,
        2
    );
    assert!(execution_bridge_record_path(records_dir.as_path(), 2).exists());
    assert!(execution_checkpoint_manifest_path(records_dir.as_path(), 2).exists());
    assert_eq!(
        load_execution_bridge_state(state_path.as_path())
            .expect("leaked state")
            .last_applied_committed_height,
        1,
        "record/latest fault must occur before final state persistence"
    );
    assert_ne!(
        fs::read(world_dir.join("snapshot.json")).expect("leaked snapshot bytes"),
        previous_snapshot_bytes,
        "partial publication must replace the previous world cache before restart"
    );
    assert!(
        !sidecar.exists(),
        "published world directory must not inherit an old DistFS sidecar"
    );
    drop(target);

    let mut restarted = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
        &storage_profile,
    )
    .expect("prepared checkpoint transaction must recover on startup");
    assert_eq!(restarted.state.last_applied_committed_height, 1);
    assert_eq!(restarted.state, previous_state);
    assert_eq!(restarted.execution_world.state(), &previous_world_state);
    assert_eq!(restarted.execution_world.journal(), &previous_world_journal);
    assert_eq!(
        fs::read(state_path.as_path()).expect("rolled-back state bytes"),
        previous_state_bytes
    );
    assert_eq!(
        fs::read(sidecar.as_path()).expect("rolled-back DistFS sidecar"),
        b"old world sidecar"
    );
    assert!(!execution_bridge_record_path(records_dir.as_path(), 2).exists());
    assert!(!execution_checkpoint_manifest_path(records_dir.as_path(), 2).exists());
    assert_eq!(
        load_execution_bridge_record(records_dir.join("latest.json").as_path())
            .expect("rolled-back latest record")
            .height,
        1
    );
    assert!(
        !records_dir
            .join("checkpoint-install-transaction.json")
            .exists()
    );

    restarted
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect("safe retry after prepared transaction rollback");
    assert_eq!(restarted.state.last_applied_committed_height, 2);
    let equal_height_err = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-prepared-transaction-recovery".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_002,
        })
        .expect_err("retried checkpoint install keeps equal-height rejection");
    assert!(
        equal_height_err.contains("checkpoint-install"),
        "{equal_height_err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_checkpoint_bundle_snapshot_root_mismatch() {
    let dir = temp_dir("execution-driver-checkpoint-root-mismatch");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-root-mismatch".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 11_001,
        })
        .expect("commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-root-mismatch".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 11_002,
        })
        .expect("commit 2");
    let record_1 = load_execution_bridge_record(
        execution_bridge_record_path(source_root.join("records").as_path(), 1).as_path(),
    )
    .expect("record 1");
    let mut bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");
    let malformed_manifest =
        super::super::ExecutionCheckpointManifest::new_with_predecessor_execution_block_hash(
            "world-checkpoint-root-mismatch".to_string(),
            2,
            second.execution_block_hash.clone(),
            second.execution_state_root.clone(),
            record_1.execution_block_hash.clone(),
            record_1
                .latest_state_ref
                .clone()
                .expect("record 1 latest state ref"),
            record_1.snapshot_ref.clone(),
            record_1.journal_ref.clone(),
            11_002,
        )
        .expect("malformed manifest with valid hash");
    for content_hash in &malformed_manifest.pinned_refs {
        if bundle
            .blobs
            .iter()
            .any(|blob| blob.content_hash == *content_hash)
        {
            continue;
        }
        let bytes = source
            .execution_store
            .get_verified(content_hash.as_str())
            .expect("source blob");
        bundle.blobs.push(oasis7_node::NodeExecutionCheckpointBlob {
            content_hash: content_hash.clone(),
            bytes,
        });
    }
    bundle.manifest_json = serde_json::to_vec_pretty(&malformed_manifest).expect("manifest json");

    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    let err = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-root-mismatch".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash,
                execution_state_root: second.execution_state_root,
                committed_at_unix_ms: 11_002,
            },
            bundle,
        )
        .expect_err("snapshot root mismatch should fail closed");

    assert!(err.contains("snapshot root mismatch"), "{err}");
    let target_state = load_execution_bridge_state(target_root.join("bridge-state.json").as_path())
        .expect("target state");
    assert_eq!(target_state.last_applied_committed_height, 0);
    assert!(
        !execution_bridge_record_path(target_root.join("records").as_path(), 2).exists(),
        "failed checkpoint install must not persist a height-2 execution record"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_v1_checkpoint_bundle_before_target_mutation() {
    let dir = temp_dir("execution-driver-checkpoint-v1-rejection");
    let source_root = dir.join("source");
    let target_root = dir.join("target");
    let storage_profile = StorageProfileConfig {
        execution_checkpoint_interval: 2,
        execution_checkpoint_keep: 2,
        ..StorageProfileConfig::for_profile(StorageProfile::DevLocal)
    };
    let mut source = NodeRuntimeExecutionDriver::new_with_storage_profile(
        source_root.join("bridge-state.json"),
        source_root.join("world"),
        source_root.join("records"),
        source_root.join("storage"),
        &storage_profile,
    )
    .expect("source driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-v1-rejection".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 12_001,
        })
        .expect("commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-v1-rejection".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 2,
            epoch: 0,
            node_block_hash: "block-2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 12_002,
        })
        .expect("commit 2");
    let source_record = load_execution_bridge_record(
        execution_bridge_record_path(source_root.join("records").as_path(), 2).as_path(),
    )
    .expect("source record");
    let mut bundle = source
        .export_checkpoint_bundle(2)
        .expect("export checkpoint")
        .expect("checkpoint bundle");
    let v1_manifest = super::super::ExecutionCheckpointManifest::new(
        "world-checkpoint-v1-rejection".to_string(),
        2,
        second.execution_block_hash.clone(),
        second.execution_state_root.clone(),
        source_record
            .latest_state_ref
            .clone()
            .expect("source latest state ref"),
        source_record.snapshot_ref.clone(),
        source_record.journal_ref.clone(),
        12_002,
    )
    .expect("v1 manifest");
    bundle.manifest_json = serde_json::to_vec_pretty(&v1_manifest).expect("v1 manifest json");

    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        target_root.join("bridge-state.json"),
        target_root.join("world"),
        target_root.join("records"),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    let err = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-v1-rejection".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash,
                execution_state_root: second.execution_state_root,
                committed_at_unix_ms: 12_002,
            },
            bundle,
        )
        .expect_err("v1 checkpoint bundle must be rejected before target mutation");
    assert!(err.contains("v1"), "unexpected v1 rejection error: {err}");
    assert_eq!(target.state.last_applied_committed_height, 0);
    assert!(
        !execution_bridge_record_path(target_root.join("records").as_path(), 2).exists(),
        "rejected v1 checkpoint must not persist a target execution record"
    );
    assert!(
        !target
            .execution_store
            .has(v1_manifest.latest_state_ref.as_str())
            .expect("check target store"),
        "rejected v1 checkpoint must not copy target blobs"
    );

    let _ = fs::remove_dir_all(dir);
}
