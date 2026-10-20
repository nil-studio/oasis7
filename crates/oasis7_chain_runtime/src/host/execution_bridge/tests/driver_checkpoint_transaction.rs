use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_latest_path,
    execution_checkpoint_manifest_path, load_execution_bridge_record,
};
use super::super::driver::{NodeRuntimeExecutionDriver, load_execution_bridge_state};
use super::super::driver_checkpoint_install::{
    CheckpointInstallFault, checkpoint_install_transaction_load_count_for_test,
    reset_checkpoint_install_transaction_load_count_for_test,
    set_checkpoint_install_fault_for_test,
};
use super::*;
use oasis7_node::{
    NodeExecutionCheckpointInstallContext, NodeExecutionCommitContext, NodeExecutionHook,
    compute_consensus_action_root,
};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupPublicationEntry {
    Directory(std::path::PathBuf),
    File(std::path::PathBuf, Vec<u8>),
    Symlink(std::path::PathBuf, std::path::PathBuf),
}

fn snapshot_startup_publication_tree(root: &std::path::Path) -> Vec<StartupPublicationEntry> {
    fn visit(
        root: &std::path::Path,
        path: &std::path::Path,
        entries: &mut Vec<StartupPublicationEntry>,
    ) {
        let relative = path
            .strip_prefix(root)
            .expect("startup publication path must remain below root")
            .to_path_buf();
        let metadata = fs::symlink_metadata(path).expect("read startup publication metadata");
        if metadata.file_type().is_symlink() {
            entries.push(StartupPublicationEntry::Symlink(
                relative,
                fs::read_link(path).expect("read startup publication symlink"),
            ));
            return;
        }
        if metadata.is_file() {
            entries.push(StartupPublicationEntry::File(
                relative,
                fs::read(path).expect("read startup publication file"),
            ));
            return;
        }
        assert!(
            metadata.is_dir(),
            "unexpected startup publication entry: {path:?}"
        );
        entries.push(StartupPublicationEntry::Directory(relative));
        let mut children = fs::read_dir(path)
            .expect("read startup publication directory")
            .map(|entry| entry.expect("read startup publication child").path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            visit(root, child.as_path(), entries);
        }
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries
}

fn startup_publication_tree_digest(entries: &[StartupPublicationEntry]) -> String {
    oasis7_runtime::blake3_hex(format!("{entries:?}").as_bytes())
}

fn syntactically_valid_checkpoint_install_marker() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "phase": "Prepared",
        "transaction_id": "marker-target",
        "height": 1,
        "previous_state": super::super::ExecutionBridgeState::default(),
        "world_was_present": true,
        "backups": [
            { "artifact": "State", "bytes": null },
            { "artifact": "Record", "bytes": null },
            { "artifact": "LatestRecord", "bytes": null },
            { "artifact": "Manifest", "bytes": null },
            { "artifact": "LatestManifest", "bytes": null }
        ]
    }))
    .expect("serialize valid checkpoint install marker")
}

fn assert_checkpoint_install_marker_startup_failure_preserves_publication<F>(
    prefix: &str,
    expected_error: &str,
    install_marker: F,
) where
    F: FnOnce(&std::path::Path),
{
    let dir = temp_dir(prefix);
    let state_path = dir.join("bridge-state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::DevLocal);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        dir.join("storage"),
        &storage_profile,
    )
    .expect("driver");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-install-marker-startup".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: compute_consensus_action_root(&[]).expect("empty action root"),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 10_001,
        })
        .expect("commit");
    drop(driver);

    let marker = records_dir.join("checkpoint-install-transaction.json");
    install_marker(marker.as_path());
    fs::remove_file(world_dir.join("journal.json"))
        .expect("create partial world persistence before startup");
    let before = snapshot_startup_publication_tree(dir.as_path());
    let before_digest = startup_publication_tree_digest(before.as_slice());

    reset_checkpoint_install_transaction_load_count_for_test();
    let err = match NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        dir.join("storage"),
        &storage_profile,
    ) {
        Ok(_) => panic!("startup must reject the checkpoint install marker"),
        Err(err) => err,
    };
    assert!(
        err.contains(expected_error),
        "unexpected startup error: {err}"
    );
    assert_eq!(
        checkpoint_install_transaction_load_count_for_test(),
        1,
        "startup must load and validate the transaction marker exactly once"
    );
    let after = snapshot_startup_publication_tree(dir.as_path());
    assert_eq!(
        startup_publication_tree_digest(after.as_slice()),
        before_digest
    );
    assert_eq!(
        after, before,
        "startup must not mutate the publication tree"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_rejects_malformed_checkpoint_install_marker_without_mutation()
 {
    assert_checkpoint_install_marker_startup_failure_preserves_publication(
        "execution-driver-malformed-checkpoint-install-marker",
        "parse checkpoint install transaction failed",
        |marker| fs::write(marker, b"not-json").expect("write malformed marker"),
    );
}

#[test]
fn node_runtime_execution_driver_startup_rejects_directory_checkpoint_install_marker_without_mutation()
 {
    assert_checkpoint_install_marker_startup_failure_preserves_publication(
        "execution-driver-directory-checkpoint-install-marker",
        "checkpoint install transaction marker is not a regular file",
        |marker| fs::create_dir(marker).expect("create directory marker"),
    );
}

#[cfg(unix)]
#[test]
fn node_runtime_execution_driver_startup_rejects_checkpoint_install_marker_symlinks_without_mutation()
 {
    use std::os::unix::fs::symlink;

    for (name, target, expected_target) in [
        (
            "execution-driver-valid-checkpoint-install-marker-symlink",
            Some(syntactically_valid_checkpoint_install_marker()),
            "marker-target.json",
        ),
        (
            "execution-driver-dangling-checkpoint-install-marker-symlink",
            None,
            "missing-marker-target.json",
        ),
    ] {
        assert_checkpoint_install_marker_startup_failure_preserves_publication(
            name,
            "symbolic link",
            |marker| {
                let target_path = marker.with_file_name(expected_target);
                if let Some(target) = target.as_deref() {
                    fs::write(target_path.as_path(), target).expect("write marker target");
                }
                symlink(target_path.as_path(), marker).expect("create marker symlink");
                assert!(
                    fs::symlink_metadata(marker)
                        .expect("marker metadata")
                        .file_type()
                        .is_symlink(),
                    "marker must remain a symlink"
                );
            },
        );
    }
}

#[test]
fn node_runtime_execution_driver_checkpoint_install_startup_rolls_back_after_final_state_persist_fault()
 {
    let dir = temp_dir("execution-driver-checkpoint-final-state-persist-rollback");
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
            world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
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
            world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
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
    let mut target = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("target driver");
    target
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
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
    let original_state = target.state.clone();
    let original_world_state = target.execution_world.state().clone();

    set_checkpoint_install_fault_for_test(Some(CheckpointInstallFault::AfterFinalStatePersist));
    let err = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle.clone(),
        )
        .expect_err("final execution bridge state persistence fault must simulate a crash");
    assert!(
        err.contains("after final state persistence"),
        "unexpected injected fault error: {err}"
    );
    assert_eq!(target.state.last_applied_committed_height, 2);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            fs::read(records_dir.join("checkpoint-install-transaction.json"))
                .expect("prepared marker bytes")
                .as_slice(),
        )
        .expect("prepared marker JSON")["phase"],
        "Prepared",
        "the marker remains Prepared until the final durable state publication"
    );

    drop(target);
    let restart_state_path = state_path.clone();
    let restart_world_dir = world_dir.clone();
    let restart_records_dir = records_dir.clone();
    reset_checkpoint_install_transaction_load_count_for_test();
    let mut restarted = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir.clone(),
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("restart must roll back final-state-persist checkpoint publication");
    assert_eq!(checkpoint_install_transaction_load_count_for_test(), 1);
    assert_eq!(restarted.state.last_applied_committed_height, 1);
    assert_eq!(restarted.state, original_state);
    assert_eq!(restarted.execution_world.state(), &original_world_state);
    assert!(
        !execution_bridge_record_path(records_dir.as_path(), 2).exists(),
        "restart rollback must remove the height-2 record"
    );
    assert!(
        !execution_checkpoint_manifest_path(records_dir.as_path(), 2).exists(),
        "restart rollback must remove the height-2 checkpoint manifest"
    );
    assert!(
        !records_dir
            .join("checkpoint-install-transaction.json")
            .exists(),
        "restart rollback must durably clean up the Prepared marker"
    );

    set_checkpoint_install_fault_for_test(Some(
        CheckpointInstallFault::AfterCommittedMarkerPersist,
    ));
    let committed_err = restarted
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 10_002,
            },
            bundle,
        )
        .expect_err("Committed marker fault must simulate a crash after authoritative publication");
    assert!(committed_err.contains("after committed marker persistence"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            fs::read(records_dir.join("checkpoint-install-transaction.json"))
                .expect("Committed marker bytes")
                .as_slice(),
        )
        .expect("Committed marker JSON")["phase"],
        "Committed"
    );
    drop(restarted);
    reset_checkpoint_install_transaction_load_count_for_test();
    let mut restarted = NodeRuntimeExecutionDriver::new_with_storage_profile(
        restart_state_path,
        restart_world_dir,
        restart_records_dir,
        target_root.join("storage"),
        &storage_profile,
    )
    .expect("restart must finalize a durable Committed marker");
    assert_eq!(checkpoint_install_transaction_load_count_for_test(), 1);
    assert_eq!(restarted.state.last_applied_committed_height, 2);
    assert!(
        !records_dir
            .join("checkpoint-install-transaction.json")
            .exists(),
        "Committed marker restart finalization must clean up the marker"
    );
    let replay_err = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-final-state-persist-rollback".to_string(),
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
        .expect_err("retried checkpoint-installed head rejects equal-height replay");
    assert!(replay_err.contains("checkpoint-install"), "{replay_err}");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rolls_back_final_state_persist_io_failure_in_process() {
    let dir = temp_dir("execution-driver-checkpoint-final-state-io-rollback");
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
            world_id: "world-checkpoint-final-state-io-rollback".to_string(),
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
        .expect("source commit 1");
    let second = source
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-final-state-io-rollback".to_string(),
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
        storage_root,
        &storage_profile,
    )
    .expect("target driver");
    target
        .on_commit(NodeExecutionCommitContext {
            world_id: "world-checkpoint-final-state-io-rollback".to_string(),
            node_id: "node-b".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 1,
            epoch: 0,
            node_block_hash: "block-1".to_string(),
            action_root: compute_consensus_action_root(&[]).expect("empty action root"),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 12_001,
        })
        .expect("target commit 1");

    let previous_state = target.state.clone();
    let previous_world_state = target.execution_world.state().clone();
    let previous_world_journal = target.execution_world.journal().clone();
    let previous_state_bytes = fs::read(state_path.as_path()).expect("previous state bytes");
    let previous_snapshot_bytes =
        fs::read(world_dir.join("snapshot.json")).expect("previous snapshot bytes");
    let previous_journal_bytes =
        fs::read(world_dir.join("journal.json")).expect("previous journal bytes");
    let previous_latest_bytes =
        fs::read(records_dir.join("latest.json")).expect("previous latest bytes");
    let sidecar = world_dir
        .join("distfs")
        .join("module-store")
        .join("index.bin");
    fs::create_dir_all(sidecar.parent().expect("sidecar parent")).expect("create sidecar parent");
    fs::write(sidecar.as_path(), b"previous sidecar").expect("write previous sidecar");

    set_checkpoint_install_fault_for_test(Some(CheckpointInstallFault::FinalStatePersistFailure));
    let err = target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-final-state-io-rollback".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash.clone(),
                execution_state_root: second.execution_state_root.clone(),
                committed_at_unix_ms: 12_002,
            },
            bundle.clone(),
        )
        .expect_err("final state persistence I/O failure must roll back in process");
    assert!(err.contains("final state persistence failure"), "{err}");

    assert_eq!(target.state, previous_state);
    assert_eq!(target.execution_world.state(), &previous_world_state);
    assert_eq!(target.execution_world.journal(), &previous_world_journal);
    assert_eq!(
        fs::read(state_path.as_path()).expect("restored state bytes"),
        previous_state_bytes
    );
    assert_eq!(
        fs::read(world_dir.join("snapshot.json")).expect("restored snapshot bytes"),
        previous_snapshot_bytes
    );
    assert_eq!(
        fs::read(world_dir.join("journal.json")).expect("restored journal bytes"),
        previous_journal_bytes
    );
    assert_eq!(
        fs::read(records_dir.join("latest.json")).expect("restored latest bytes"),
        previous_latest_bytes
    );
    assert_eq!(
        load_execution_bridge_state(state_path.as_path()).expect("restored state"),
        previous_state
    );
    assert_eq!(
        load_execution_bridge_record(records_dir.join("latest.json").as_path())
            .expect("restored latest record")
            .height,
        1
    );
    assert!(!execution_bridge_record_path(records_dir.as_path(), 2).exists());
    assert!(!execution_checkpoint_manifest_path(records_dir.as_path(), 2).exists());
    assert!(!execution_checkpoint_latest_path(records_dir.as_path()).exists());
    assert_eq!(
        fs::read(sidecar.as_path()).expect("restored sidecar"),
        b"previous sidecar"
    );
    assert!(
        !records_dir
            .join("checkpoint-install-transaction.json")
            .exists()
    );
    let leaked_world_transaction_paths: Vec<_> = fs::read_dir(target_root.as_path())
        .expect("read target root")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".world.checkpoint-install-")
        })
        .collect();
    assert!(
        leaked_world_transaction_paths.is_empty(),
        "staging/backup paths leaked: {leaked_world_transaction_paths:?}"
    );

    target
        .install_checkpoint_bundle(
            NodeExecutionCheckpointInstallContext {
                world_id: "world-checkpoint-final-state-io-rollback".to_string(),
                node_id: "node-b".to_string(),
                height: 2,
                node_block_hash: "block-2".to_string(),
                execution_block_hash: second.execution_block_hash,
                execution_state_root: second.execution_state_root,
                committed_at_unix_ms: 12_002,
            },
            bundle,
        )
        .expect("retry after in-process rollback");
    assert_eq!(target.state.last_applied_committed_height, 2);

    let _ = fs::remove_dir_all(dir);
}
