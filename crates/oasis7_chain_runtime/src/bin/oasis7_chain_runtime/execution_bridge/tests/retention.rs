use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_rel_path,
    list_execution_checkpoint_heights, load_execution_bridge_record,
    load_latest_execution_checkpoint_manifest, maybe_persist_execution_checkpoint_for_record,
    persist_execution_bridge_record, persist_execution_checkpoint_manifest,
    run_execution_bridge_incremental_retention_maintenance,
    run_execution_bridge_retention_maintenance, sync_execution_bridge_pin_set,
};
use super::super::external_effect::build_execution_replay_plan;
use super::super::{bridge_committed_heights, driver::NodeRuntimeExecutionDriver};
use super::*;
use std::collections::BTreeSet;

use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_proto::storage_profile::{StorageProfile, StorageProfileConfig};
use oasis7_runtime::BlobStore;
use oasis7_runtime::LocalCasStore;
use oasis7_wasm_abi::ModuleOutput;
use oasis7_wasm_executor::FixedSandbox;

fn remove_test_store_blob(store: &LocalCasStore, content_ref: &str) {
    fs::remove_file(store.blobs_dir().join(format!("{content_ref}.blob")))
        .expect("remove test store blob");
}

#[test]
fn execution_checkpoint_cadence_trims_old_manifests_and_clears_record_refs() {
    let dir = temp_dir("execution-checkpoint-cadence-trim");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    for height in 1..=6 {
        let mut record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 2)
                .expect("maybe persist checkpoint");
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist checkpointed record");
    }

    assert_eq!(
        list_execution_checkpoint_heights(records_dir.as_path()).expect("list checkpoint heights"),
        vec![4, 6]
    );
    let record_2 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 2).as_path(),
    )
    .expect("load record 2");
    let record_4 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 4).as_path(),
    )
    .expect("load record 4");
    let record_6 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 6).as_path(),
    )
    .expect("load record 6");
    assert!(record_2.checkpoint_ref.is_none());
    assert_eq!(
        record_4.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(4).as_str())
    );
    assert_eq!(
        record_6.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(6).as_str())
    );
    let latest = load_latest_execution_checkpoint_manifest(records_dir.as_path())
        .expect("load latest checkpoint")
        .expect("latest checkpoint exists");
    assert_eq!(latest.height, 6);

    let plan = build_execution_replay_plan(records_dir.as_path(), &store, 5)
        .expect("build replay plan from sparse checkpoint");
    assert_eq!(
        plan.checkpoint.as_ref().map(|manifest| manifest.height),
        Some(4)
    );
    assert_eq!(plan.start_height, 5);
    assert_eq!(plan.records.len(), 1);
    assert_eq!(plan.records[0].record.height, 5);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn bridge_committed_heights_persists_sparse_checkpoint_at_default_interval() {
    let dir = temp_dir("execution-bridge-default-checkpoint");
    let store = LocalCasStore::new(dir.join("store"));
    let mut world = oasis7::runtime_host::world::new();
    let mut sandbox = FixedSandbox::succeed(ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    });
    let mut state = ExecutionBridgeState::default();
    let records_dir = dir.join("records");
    let snapshot = sample_snapshot(
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
        Some("node-h32"),
    );

    let records = bridge_committed_heights(
        &snapshot,
        1_000,
        &mut world,
        &mut sandbox,
        &store,
        records_dir.as_path(),
        &mut state,
    )
    .expect("bridge committed heights");

    assert_eq!(
        records.len() as u64,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS
    );
    let latest_record = records.last().expect("latest record");
    assert_eq!(
        latest_record.checkpoint_ref.as_deref(),
        Some(
            execution_checkpoint_manifest_rel_path(
                EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
            )
            .as_str()
        )
    );
    let latest_checkpoint = load_latest_execution_checkpoint_manifest(records_dir.as_path())
        .expect("load latest checkpoint")
        .expect("latest checkpoint exists");
    assert_eq!(
        latest_checkpoint.height,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS
    );

    let plan = build_execution_replay_plan(
        records_dir.as_path(),
        &store,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
    )
    .expect("build replay plan");
    assert_eq!(
        plan.checkpoint.as_ref().map(|manifest| manifest.height),
        Some(EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS)
    );
    assert!(plan.records.is_empty());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_retention_maintenance_clears_archive_refs_and_prunes_orphans() {
    let dir = temp_dir("execution-bridge-retention-maintenance");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    let mut records = Vec::new();
    for height in 1..=6 {
        let mut record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 2)
                .expect("maybe persist checkpoint");
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist checkpointed record");
        records.push(record);
    }

    let freed_bytes = run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 2)
        .expect("run retention maintenance");
    assert!(freed_bytes > 0, "expected orphan sweep to free bytes");

    let record_1 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load record 1");
    let record_4 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 4).as_path(),
    )
    .expect("load record 4");
    let record_5 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 5).as_path(),
    )
    .expect("load record 5");
    let record_6 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 6).as_path(),
    )
    .expect("load record 6");

    assert!(record_1.latest_state_ref.is_none());
    assert!(record_1.snapshot_ref.is_none());
    assert!(record_1.journal_ref.is_none());
    assert!(record_1.simulator_mirror.is_none());
    assert_eq!(
        record_4.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(4).as_str())
    );
    assert!(record_4.snapshot_ref.is_none());
    assert!(record_4.journal_ref.is_none());
    assert!(record_4.simulator_mirror.is_none());
    assert!(record_5.snapshot_ref.is_some());
    assert!(record_5.journal_ref.is_some());
    assert!(record_6.snapshot_ref.is_some());
    assert!(record_6.journal_ref.is_some());

    assert!(
        !store
            .has(
                records[0]
                    .snapshot_ref
                    .as_deref()
                    .expect("record1 snapshot ref")
            )
            .expect("check archive snapshot")
    );
    assert!(
        store
            .has(
                records[3]
                    .snapshot_ref
                    .as_deref()
                    .expect("record4 snapshot ref")
            )
            .expect("check checkpoint snapshot")
    );
    assert!(
        store
            .has(
                records[4]
                    .journal_ref
                    .as_deref()
                    .expect("record5 journal ref")
            )
            .expect("check hot journal")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_retention_maintenance_tolerates_missing_archive_external_effect_ref() {
    let dir = temp_dir("execution-bridge-retention-missing-external-effect");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    let mut records = Vec::new();
    for height in 1..=6 {
        let mut record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 2)
                .expect("maybe persist checkpoint");
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist checkpointed record");
        records.push(record);
    }

    let missing_external_effect_ref = records[0]
        .external_effect_ref
        .as_deref()
        .expect("record1 external effect ref")
        .to_string();
    remove_test_store_blob(&store, missing_external_effect_ref.as_str());

    let freed_bytes = run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 2)
        .expect("run retention maintenance with missing archive external effect");
    assert!(freed_bytes > 0, "expected orphan sweep to free bytes");

    let record_1 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load record 1");
    assert!(record_1.snapshot_ref.is_none());
    assert!(record_1.journal_ref.is_none());
    assert!(
        !store
            .is_pinned(missing_external_effect_ref.as_str())
            .expect("check missing external effect pin")
    );
    assert!(
        !store
            .has(missing_external_effect_ref.as_str())
            .expect("check missing external effect blob")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_retention_maintenance_fails_for_missing_required_refs() {
    let dir = temp_dir("execution-bridge-retention-missing-required-ref");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    let mut records = Vec::new();
    for height in 1..=6 {
        let mut record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 2)
                .expect("maybe persist checkpoint");
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist checkpointed record");
        records.push(record);
    }

    let missing_latest_state_ref = records[5]
        .latest_state_ref
        .as_deref()
        .expect("record6 latest state ref")
        .to_string();
    remove_test_store_blob(&store, missing_latest_state_ref.as_str());

    let err = run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 2)
        .expect_err("missing latest state ref should fail retention maintenance");
    assert!(
        err.contains(missing_latest_state_ref.as_str()),
        "expected missing ref in error, got {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn bridge_committed_heights_sweeps_archive_refs_outside_default_hot_window() {
    let dir = temp_dir("execution-bridge-default-retention-sweep");
    let store = LocalCasStore::new(dir.join("store"));
    let mut world = oasis7::runtime_host::world::new();
    let mut sandbox = FixedSandbox::succeed(ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    });
    let mut state = ExecutionBridgeState::default();
    let records_dir = dir.join("records");
    let target_height = EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS
        + EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS;
    let snapshot = sample_snapshot(target_height, Some("node-h64"));

    let records = bridge_committed_heights(
        &snapshot,
        1_000,
        &mut world,
        &mut sandbox,
        &store,
        records_dir.as_path(),
        &mut state,
    )
    .expect("bridge committed heights");

    let record_1 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load record 1");
    let checkpoint_height = EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS;
    let record_checkpoint = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), checkpoint_height).as_path(),
    )
    .expect("load checkpoint record");
    let record_hot = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), checkpoint_height + 1).as_path(),
    )
    .expect("load hot record");

    assert!(record_1.snapshot_ref.is_none());
    assert!(record_1.journal_ref.is_none());
    assert_eq!(
        record_checkpoint.checkpoint_ref.as_deref(),
        Some(execution_checkpoint_manifest_rel_path(checkpoint_height).as_str())
    );
    assert!(record_checkpoint.snapshot_ref.is_none());
    assert!(record_checkpoint.journal_ref.is_none());
    assert!(record_hot.snapshot_ref.is_some());
    assert!(record_hot.journal_ref.is_some());

    assert!(
        !store
            .has(
                records[0]
                    .snapshot_ref
                    .as_deref()
                    .expect("record1 snapshot ref")
            )
            .expect("check archive snapshot")
    );
    let checkpoint_index = checkpoint_height.saturating_sub(1) as usize;
    assert!(
        store
            .has(
                records[checkpoint_index]
                    .snapshot_ref
                    .as_deref()
                    .expect("checkpoint snapshot ref"),
            )
            .expect("check checkpoint snapshot")
    );
    assert!(
        store
            .has(
                records[checkpoint_index + 1]
                    .journal_ref
                    .as_deref()
                    .expect("hot journal ref"),
            )
            .expect("check hot journal")
    );

    let plan = build_execution_replay_plan(records_dir.as_path(), &store, checkpoint_height + 8)
        .expect("build replay plan from sparse checkpoint");
    assert_eq!(
        plan.checkpoint.as_ref().map(|manifest| manifest.height),
        Some(checkpoint_height)
    );
    assert_eq!(plan.start_height, checkpoint_height + 1);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_uses_storage_profile_hot_window_budget() {
    let dir = temp_dir("execution-driver-storage-profile-hot-window");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root,
        &storage_profile,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    for height in 1..=65 {
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

    let record_1 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load record 1");
    let record_2 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 2).as_path(),
    )
    .expect("load record 2");
    let record_65 = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 65).as_path(),
    )
    .expect("load record 65");

    assert!(record_1.snapshot_ref.is_none());
    assert!(record_1.journal_ref.is_none());
    assert!(record_2.snapshot_ref.is_some());
    assert!(record_2.journal_ref.is_some());
    assert!(record_65.latest_state_ref.is_some());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn steady_state_commit_does_not_reparse_far_history_but_explicit_rebuild_does() {
    let dir = temp_dir("execution-driver-steady-state-history-tripwire");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    for height in 1..=65 {
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
            .expect("seed release_default history");
    }

    let store = LocalCasStore::new(storage_root);
    let tripwire_ref = store
        .put_bytes(b"qa-far-history-tripwire")
        .expect("store far-history tripwire");
    let record_1_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut record_1_json: serde_json::Value = serde_json::from_slice(
        fs::read(record_1_path.as_path())
            .expect("read archived record 1")
            .as_slice(),
    )
    .expect("parse archived record 1");
    record_1_json["latest_state_ref"] = serde_json::Value::String(tripwire_ref.clone());
    record_1_json["snapshot_ref"] = serde_json::Value::String(tripwire_ref.clone());
    record_1_json["qa_history_sentinel"] = serde_json::Value::Bool(true);
    fs::write(
        record_1_path.as_path(),
        serde_json::to_vec_pretty(&record_1_json).expect("serialize tripwire record"),
    )
    .expect("write tripwire record");

    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 66,
            slot: 65,
            epoch: 0,
            node_block_hash: "node-h66".to_string(),
            action_root: empty_action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 66_000,
        })
        .expect("steady-state commit");

    let untouched_record_1: serde_json::Value = serde_json::from_slice(
        fs::read(record_1_path.as_path())
            .expect("read record 1 after steady-state commit")
            .as_slice(),
    )
    .expect("parse record 1 after steady-state commit");
    assert_eq!(
        untouched_record_1
            .get("qa_history_sentinel")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "steady-state commit reparsed and rewrote a far-history record"
    );
    assert!(
        store.has(tripwire_ref.as_str()).expect("check tripwire"),
        "steady-state commit ran full orphan GC"
    );

    run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 64)
        .expect("explicit retention rebuild");
    let rebuilt_record_1: serde_json::Value = serde_json::from_slice(
        fs::read(record_1_path.as_path())
            .expect("read rebuilt record 1")
            .as_slice(),
    )
    .expect("parse rebuilt record 1");
    assert!(rebuilt_record_1.get("qa_history_sentinel").is_none());
    assert!(!store.has(tripwire_ref.as_str()).expect("check rebuilt GC"));

    let plan = build_execution_replay_plan(records_dir.as_path(), &store, 66)
        .expect("build replay plan after explicit retention rebuild");
    assert_eq!(
        plan.checkpoint.as_ref().map(|manifest| manifest.height),
        Some(64)
    );
    assert_eq!(plan.start_height, 65);
    assert_eq!(plan.records.len(), 2);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn steady_state_commits_do_not_run_full_blob_gc_every_block() {
    let dir = temp_dir("execution-driver-steady-state-gc-cadence");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let commit = |driver: &mut NodeRuntimeExecutionDriver, height: u64| {
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
            .expect("steady-state commit");
    };

    commit(&mut driver, 1);
    let store = LocalCasStore::new(storage_root);
    let orphan_after_height_1 = store
        .put_bytes(b"qa-orphan-after-height-1")
        .expect("store first orphan");
    commit(&mut driver, 2);
    let orphan_after_height_2 = store
        .put_bytes(b"qa-orphan-after-height-2")
        .expect("store second orphan");
    commit(&mut driver, 3);

    let first_survived = store
        .has(orphan_after_height_1.as_str())
        .expect("check first orphan");
    let second_survived = store
        .has(orphan_after_height_2.as_str())
        .expect("check second orphan");
    assert!(
        first_survived,
        "full orphan GC ran before checkpoint cadence"
    );
    assert!(
        second_survived,
        "full orphan GC ran before checkpoint cadence"
    );

    run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 64)
        .expect("explicit retention rebuild");
    assert!(
        !store
            .has(orphan_after_height_1.as_str())
            .expect("check first orphan after rebuild")
    );
    assert!(
        !store
            .has(orphan_after_height_2.as_str())
            .expect("check second orphan after rebuild")
    );
    let plan = build_execution_replay_plan(records_dir.as_path(), &store, 3)
        .expect("build replay plan after explicit GC");
    assert_eq!(plan.start_height, 1);
    assert_eq!(plan.records.len(), 3);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn steady_state_retention_removes_evicted_record_pin_shards() {
    let dir = temp_dir("execution-driver-record-pin-shard-window");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut storage_profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    storage_profile.execution_hot_head_heights = 2;
    storage_profile.execution_checkpoint_interval = 2;
    storage_profile.execution_checkpoint_keep = 2;
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir,
        storage_root.clone(),
        &storage_profile,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    for height in 1..=6 {
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
            .expect("steady-state commit");
    }

    let scope_dir = storage_root.join("pin_scopes").join("execution_bridge_v1");
    assert!(!scope_dir.join("record-00000000000000000001.json").exists());
    assert!(!scope_dir.join("record-00000000000000000002.json").exists());
    assert!(scope_dir.join("record-00000000000000000003.json").exists());
    assert!(scope_dir.join("record-00000000000000000006.json").exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn checkpoint_gc_removes_every_pin_shard_pruned_after_keep_reduction() {
    let dir = temp_dir("execution-checkpoint-pin-shard-keep-reduction");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    for height in [2, 4, 6] {
        let predecessor = persist_test_execution_record_with_store_refs(
            records_dir.as_path(),
            &store,
            height - 1,
        );
        persist_execution_bridge_record(records_dir.as_path(), &predecessor)
            .expect("persist checkpoint predecessor record");
        let mut record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        record.checkpoint_ref =
            maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 3)
                .expect("persist checkpoint with larger keep");
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist checkpointed record");
        run_execution_bridge_incremental_retention_maintenance(
            records_dir.as_path(),
            &store,
            &record,
            64,
            2,
            3,
        )
        .expect("publish checkpoint pin shard");
    }

    let noncanonical_ref = store
        .put_bytes(b"noncanonical checkpoint shard")
        .expect("persist noncanonical shard blob");
    store
        .replace_pin_scope_shard(
            "execution_bridge_v1",
            "checkpoint-1",
            &BTreeSet::from([noncanonical_ref.clone()]),
        )
        .expect("publish noncanonical checkpoint-like shard");
    let unknown_ref = store
        .put_bytes(b"unknown external shard")
        .expect("persist unknown shard blob");
    store
        .replace_pin_scope_shard(
            "execution_bridge_v1",
            "external-owner",
            &BTreeSet::from([unknown_ref.clone()]),
        )
        .expect("publish unknown shard");

    let predecessor =
        persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, 7);
    persist_execution_bridge_record(records_dir.as_path(), &predecessor)
        .expect("persist retained checkpoint predecessor record");
    let mut record =
        persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, 8);
    record.checkpoint_ref =
        maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 2, 1)
            .expect("persist checkpoint after keep reduction");
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("persist retained checkpoint record");
    run_execution_bridge_incremental_retention_maintenance(
        records_dir.as_path(),
        &store,
        &record,
        64,
        2,
        1,
    )
    .expect("prune stale checkpoint pin shards");

    let scope_dir = store.root().join("pin_scopes").join("execution_bridge_v1");
    for height in [2, 4, 6] {
        assert!(
            !scope_dir
                .join(format!("checkpoint-{height:020}.json"))
                .exists(),
            "checkpoint shard at height {height} survived keep reduction"
        );
    }
    assert!(
        scope_dir
            .join("checkpoint-00000000000000000008.json")
            .exists()
    );
    for (shard, content_ref) in [
        ("checkpoint-1", noncanonical_ref),
        ("external-owner", unknown_ref),
    ] {
        assert!(
            scope_dir.join(format!("{shard}.json")).exists(),
            "unknown shard {shard} was removed"
        );
        assert!(
            store
                .has(content_ref.as_str())
                .expect("check retained blob"),
            "blob pinned by unknown shard {shard} was pruned"
        );
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn incremental_retention_pin_shard_count_is_bounded_at_ten_thousand_heights() {
    let dir = temp_dir("execution-retention-pin-shard-scale");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    let mut record =
        persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, 1);
    const HOT_WINDOW: u64 = 64;

    for height in 1..=10_000 {
        record.height = height;
        persist_execution_bridge_record(records_dir.as_path(), &record).expect("persist record");
        run_execution_bridge_incremental_retention_maintenance(
            records_dir.as_path(),
            &store,
            &record,
            HOT_WINDOW,
            64,
            8,
        )
        .expect("incremental retention");
    }

    let scope_dir = store.root().join("pin_scopes").join("execution_bridge_v1");
    let shard_count = fs::read_dir(scope_dir).expect("read pin scope").count();
    assert!(
        shard_count <= 64 * 8 + 63,
        "record pin shards exceeded the retained checkpoint span plus one cadence: {shard_count}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn retention_failure_schedules_in_process_full_reconciliation() {
    let dir = temp_dir("execution-driver-retention-reconcile-retry");
    let storage_root = dir.join("store");
    fs::create_dir_all(storage_root.as_path()).expect("create storage root");
    fs::write(
        storage_root.join("pin_scopes"),
        b"block pin scope directory",
    )
    .expect("block pin scope directory");
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        storage_root.clone(),
        &StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault),
    )
    .expect("driver");
    driver.checkpoint_interval_heights = 2;
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let commit = |driver: &mut NodeRuntimeExecutionDriver, height: u64| {
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
            .expect("commit remains available when retention fails");
    };

    commit(&mut driver, 1);
    assert!(driver.retention_reconcile_pending);
    assert_eq!(driver.retention_reconcile_next_height, Some(3));

    commit(&mut driver, 2);
    assert!(driver.retention_reconcile_pending);
    assert_eq!(
        driver.retention_reconcile_next_height,
        Some(3),
        "bounded incremental failure must not postpone the scheduled full reconciliation"
    );

    commit(&mut driver, 3);
    assert!(driver.retention_reconcile_pending);
    assert_eq!(
        driver.retention_reconcile_next_height,
        Some(5),
        "failed full reconciliation must back off by one checkpoint cadence"
    );

    fs::remove_file(storage_root.join("pin_scopes")).expect("unblock pin scope directory");
    commit(&mut driver, 4);
    assert!(driver.retention_reconcile_pending);
    assert_eq!(driver.retention_reconcile_next_height, Some(5));

    commit(&mut driver, 5);
    assert!(!driver.retention_reconcile_pending);
    assert_eq!(driver.retention_reconcile_next_height, None);
    assert!(!dir.join("records/retention-degraded").exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn retention_reconciliation_is_marker_driven_across_restart() {
    let dir = temp_dir("execution-driver-retention-marker-restart");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    let state = ExecutionBridgeState {
        last_applied_committed_height: 10_000,
        ..ExecutionBridgeState::default()
    };

    let healthy = NodeRuntimeExecutionDriver::new_with_sandbox(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        state.clone(),
        oasis7::runtime_host::world::new(),
        Box::new(FixedSandbox::succeed(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })),
        32,
        64,
        8,
    );
    assert!(!healthy.retention_reconcile_pending);
    assert_eq!(healthy.retention_reconcile_next_height, None);

    fs::write(
        records_dir.join("retention-degraded"),
        b"interrupted retention\n",
    )
    .expect("write degraded marker");
    let degraded = NodeRuntimeExecutionDriver::new_with_sandbox(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        state,
        oasis7::runtime_host::world::new(),
        Box::new(FixedSandbox::succeed(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })),
        32,
        64,
        8,
    );
    assert!(degraded.retention_reconcile_pending);
    assert_eq!(degraded.retention_reconcile_next_height, Some(10_001));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn interrupted_retention_transaction_is_promoted_on_production_startup() {
    let dir = temp_dir("execution-driver-retention-interrupted-startup");
    let records_dir = dir.join("records");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let storage_root = dir.join("store");
    let profile = StorageProfileConfig::for_profile(StorageProfile::ReleaseDefault);
    let mut driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        &profile,
    )
    .expect("initial production driver startup");
    fs::create_dir_all(execution_bridge_record_path(records_dir.as_path(), 1))
        .expect("block record publication with directory");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: compute_consensus_action_root(&[]).expect("empty action root"),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect_err("record publication failure must abort commit");
    assert!(records_dir.join("retention-in-progress").exists());
    drop(driver);
    fs::remove_dir_all(execution_bridge_record_path(records_dir.as_path(), 1))
        .expect("remove record publication blocker");

    let driver = NodeRuntimeExecutionDriver::new_with_storage_profile(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root,
        &profile,
    )
    .expect("restart production driver");

    assert!(driver.retention_reconcile_pending);
    assert!(records_dir.join("retention-degraded").exists());
    assert!(!records_dir.join("retention-in-progress").exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_pin_set_keeps_latest_head_and_hot_window_refs() {
    let dir = temp_dir("execution-bridge-pin-set-hot-window");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    let mut all_refs = Vec::new();
    let mut records = Vec::new();
    for height in 1..=4 {
        let record =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
        all_refs.extend(record.snapshot_ref.iter().cloned());
        all_refs.extend(record.journal_ref.iter().cloned());
        all_refs.extend(record.latest_state_ref.iter().cloned());
        all_refs.extend(record.external_effect_ref.iter().cloned());
        if let Some(simulator_mirror) = record.simulator_mirror.as_ref() {
            all_refs.push(simulator_mirror.snapshot_ref.clone());
            all_refs.push(simulator_mirror.journal_ref.clone());
        }
        records.push(record);
    }
    all_refs.sort();
    all_refs.dedup();

    for content_ref in &all_refs {
        store.pin(content_ref.as_str()).expect("pre-pin record ref");
    }

    let pin_set =
        sync_execution_bridge_pin_set(records_dir.as_path(), &store, 2).expect("sync pin set");
    assert_eq!(pin_set.latest_height, Some(4));
    assert_eq!(pin_set.hot_window_start_height, Some(3));

    let actual_pins = store
        .list_effective_pins()
        .expect("list pins")
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert!(
        store.list_pins().expect("list legacy pins").is_empty(),
        "full reconciliation left permanent legacy over-pins"
    );
    let mut expected_pins = BTreeSet::new();
    for record in &records {
        expected_pins.extend(record.external_effect_ref.iter().cloned());
        if record.height >= 3 {
            expected_pins.extend(record.snapshot_ref.iter().cloned());
            expected_pins.extend(record.journal_ref.iter().cloned());
            if let Some(simulator_mirror) = record.simulator_mirror.as_ref() {
                expected_pins.insert(simulator_mirror.snapshot_ref.clone());
                expected_pins.insert(simulator_mirror.journal_ref.clone());
            }
        }
        if record.height == 4 {
            expected_pins.extend(record.latest_state_ref.iter().cloned());
        }
    }
    assert_eq!(actual_pins, expected_pins);
    assert!(
        !records[0]
            .snapshot_ref
            .as_ref()
            .is_some_and(|snapshot_ref| actual_pins.contains(snapshot_ref))
    );
    assert!(
        !records[1]
            .journal_ref
            .as_ref()
            .is_some_and(|journal_ref| actual_pins.contains(journal_ref))
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_pin_set_keeps_checkpoint_refs_outside_hot_window() {
    let dir = temp_dir("execution-bridge-pin-set-checkpoint");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    for height in 1..=3 {
        let _ =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
    }

    let checkpoint_latest_state_ref = store
        .put_bytes(b"checkpoint-latest-state")
        .expect("store checkpoint latest state");
    let checkpoint_snapshot_ref = store
        .put_bytes(b"checkpoint-snapshot")
        .expect("store checkpoint snapshot");
    let checkpoint_journal_ref = store
        .put_bytes(b"checkpoint-journal")
        .expect("store checkpoint journal");
    let checkpoint = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        1,
        "exec-h1".to_string(),
        "state-root-1".to_string(),
        checkpoint_latest_state_ref.clone(),
        Some(checkpoint_snapshot_ref.clone()),
        Some(checkpoint_journal_ref.clone()),
        1_000,
    )
    .expect("checkpoint");
    persist_execution_checkpoint_manifest(records_dir.as_path(), &checkpoint)
        .expect("persist checkpoint");

    let pin_set =
        sync_execution_bridge_pin_set(records_dir.as_path(), &store, 1).expect("sync pin set");
    let actual_pins = pin_set.pinned_refs;
    assert!(actual_pins.contains(&checkpoint_latest_state_ref));
    assert!(actual_pins.contains(&checkpoint_snapshot_ref));
    assert!(actual_pins.contains(&checkpoint_journal_ref));

    let _ = fs::remove_dir_all(dir);
}
