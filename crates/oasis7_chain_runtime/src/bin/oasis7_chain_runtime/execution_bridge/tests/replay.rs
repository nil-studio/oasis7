use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_latest_path,
    persist_execution_bridge_record, persist_execution_checkpoint_manifest,
    run_execution_bridge_retention_maintenance,
};
use super::super::external_effect::build_execution_replay_plan;
use super::*;
use oasis7_runtime::LocalCasStore;

#[test]
fn execution_replay_plan_without_checkpoint_replays_full_log() {
    let dir = temp_dir("execution-replay-plan-full-log");
    let records_dir = dir.join("records");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    persist_test_execution_record(records_dir.as_path(), 1, "exec-h1");
    persist_test_execution_record(records_dir.as_path(), 2, "exec-h2");
    persist_test_execution_record(records_dir.as_path(), 3, "exec-h3");

    let store = LocalCasStore::new(dir.join("store"));
    let plan =
        build_execution_replay_plan(records_dir.as_path(), &store, 3).expect("build replay plan");
    assert_eq!(plan.target_height, 3);
    assert_eq!(plan.start_height, 1);
    assert!(plan.checkpoint.is_none());
    assert_eq!(plan.records.len(), 3);
    assert_eq!(plan.records[0].record.height, 1);
    assert_eq!(plan.records[2].record.height, 3);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_rejects_target_outside_hot_window_without_checkpoint() {
    let dir = temp_dir("execution-replay-plan-outside-hot-window");
    let records_dir = dir.join("records");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    for height in 1..=80 {
        let mut record = persist_test_execution_record(
            records_dir.as_path(),
            height,
            &format!("exec-h{height}"),
        );
        record.latest_state_ref = None;
        record.snapshot_ref = None;
        record.journal_ref = None;
        persist_execution_bridge_record(records_dir.as_path(), &record)
            .expect("persist pruned execution record");
    }

    let store = LocalCasStore::new(dir.join("store"));
    let err = build_execution_replay_plan(records_dir.as_path(), &store, 1)
        .expect_err("target outside hot window should fail closed without checkpoint");
    assert!(
        err.contains("outside retained hot window"),
        "unexpected replay-plan hot-window error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_uses_actual_retained_hot_window_without_checkpoint() {
    let dir = temp_dir("execution-replay-plan-retained-hot-window");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    for height in 1..=65 {
        let _ =
            persist_test_execution_record_with_store_refs(records_dir.as_path(), &store, height);
    }
    run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 64)
        .expect("run retention maintenance");

    let retained_plan =
        build_execution_replay_plan(records_dir.as_path(), &store, 2).expect("build retained plan");
    assert_eq!(retained_plan.start_height, 1);
    assert_eq!(retained_plan.target_height, 2);

    let err = build_execution_replay_plan(records_dir.as_path(), &store, 1)
        .expect_err("target outside retained hot window should fail closed");
    assert!(
        err.contains("outside retained hot window"),
        "unexpected retained hot-window error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_fails_closed_when_retained_commit_record_is_missing() {
    let dir = temp_dir("execution-replay-plan-missing-commit-record");
    let records_dir = dir.join("records");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    persist_test_execution_record(records_dir.as_path(), 1, "exec-h1");
    persist_test_execution_record(records_dir.as_path(), 2, "exec-h2");
    persist_test_execution_record(records_dir.as_path(), 3, "exec-h3");
    fs::remove_file(execution_bridge_record_path(records_dir.as_path(), 3).as_path())
        .expect("remove retained commit record");

    let store = LocalCasStore::new(dir.join("store"));
    let err = build_execution_replay_plan(records_dir.as_path(), &store, 3)
        .expect_err("missing retained commit record should fail closed");
    assert!(
        err.contains("missing commit record"),
        "unexpected missing commit record error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_prefers_nearest_checkpoint_not_ahead_of_target() {
    let dir = temp_dir("execution-replay-plan-checkpoint");
    let records_dir = dir.join("records");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    for height in 1..=6 {
        persist_test_execution_record(records_dir.as_path(), height, &format!("exec-h{height}"));
    }
    let checkpoint_3 = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        3,
        "exec-h3".to_string(),
        "state-r3".to_string(),
        "cas:snapshot-3".to_string(),
        Some("cas:snapshot-3".to_string()),
        Some("cas:journal-3".to_string()),
        3_000,
    )
    .expect("checkpoint 3");
    let checkpoint_5 = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        5,
        "exec-h5".to_string(),
        "state-r5".to_string(),
        "cas:snapshot-5".to_string(),
        Some("cas:snapshot-5".to_string()),
        Some("cas:journal-5".to_string()),
        5_000,
    )
    .expect("checkpoint 5");
    persist_execution_checkpoint_manifest(records_dir.as_path(), &checkpoint_3)
        .expect("persist checkpoint 3");
    persist_execution_checkpoint_manifest(records_dir.as_path(), &checkpoint_5)
        .expect("persist checkpoint 5");

    let store = LocalCasStore::new(dir.join("store"));
    let plan =
        build_execution_replay_plan(records_dir.as_path(), &store, 6).expect("build replay plan");
    assert_eq!(plan.start_height, 6);
    assert_eq!(plan.records.len(), 1);
    assert_eq!(plan.records[0].record.height, 6);
    assert_eq!(
        plan.checkpoint.as_ref().map(|manifest| manifest.height),
        Some(5)
    );

    let earlier_plan = build_execution_replay_plan(records_dir.as_path(), &store, 4)
        .expect("build earlier replay plan");
    assert_eq!(earlier_plan.start_height, 4);
    assert_eq!(earlier_plan.records.len(), 1);
    assert_eq!(earlier_plan.records[0].record.height, 4);
    assert_eq!(
        earlier_plan
            .checkpoint
            .as_ref()
            .map(|manifest| manifest.height),
        Some(3)
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_fails_closed_when_external_effect_blob_missing() {
    let dir = temp_dir("execution-replay-plan-missing-external-effect");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    let mut record = persist_test_execution_record(records_dir.as_path(), 1, "exec-h1");
    record.external_effect_ref = Some("missing-external-effect".to_string());
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("persist updated execution record");

    let err = build_execution_replay_plan(records_dir.as_path(), &store, 1)
        .expect_err("missing external effect blob should fail closed");
    assert!(
        err.contains("execution external effect CAS get failed"),
        "unexpected replay plan error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_fails_closed_when_external_effect_mismatches_record() {
    let dir = temp_dir("execution-replay-plan-external-effect-mismatch");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    let mut record = persist_test_execution_record(records_dir.as_path(), 1, "exec-h1");
    let external_effect_ref = persist_test_external_effect(&store, "w2", 1, "node-h1");
    record.external_effect_ref = Some(external_effect_ref);
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("persist updated execution record");

    let err = build_execution_replay_plan(records_dir.as_path(), &store, 1)
        .expect_err("mismatched external effect should fail closed");
    assert!(
        err.contains("world_id mismatch"),
        "unexpected replay plan mismatch error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_replay_plan_fails_closed_when_latest_checkpoint_pointer_is_corrupted() {
    let dir = temp_dir("execution-replay-plan-corrupted-checkpoint");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    persist_test_execution_record(records_dir.as_path(), 1, "exec-h1");
    persist_test_execution_record(records_dir.as_path(), 2, "exec-h2");
    persist_test_execution_record(records_dir.as_path(), 3, "exec-h3");
    let checkpoint = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        2,
        "exec-h2".to_string(),
        "state-r2".to_string(),
        "cas:snapshot-2".to_string(),
        Some("cas:snapshot-2".to_string()),
        Some("cas:journal-2".to_string()),
        2_000,
    )
    .expect("checkpoint");
    persist_execution_checkpoint_manifest(records_dir.as_path(), &checkpoint)
        .expect("persist checkpoint");
    let latest_path = execution_checkpoint_latest_path(records_dir.as_path());
    let mut latest_json: serde_json::Value = serde_json::from_slice(
        fs::read(latest_path.as_path())
            .expect("read latest pointer")
            .as_slice(),
    )
    .expect("parse latest pointer");
    latest_json["manifest_hash"] = serde_json::Value::String("tampered".to_string());
    let latest_bytes =
        serde_json::to_vec_pretty(&latest_json).expect("serialize tampered latest pointer");
    crate::write_bytes_atomic(latest_path.as_path(), latest_bytes.as_slice())
        .expect("persist tampered latest pointer");

    let err = build_execution_replay_plan(records_dir.as_path(), &store, 3)
        .expect_err("corrupted checkpoint pointer should fail closed");
    assert!(
        err.contains("hash mismatch"),
        "unexpected checkpoint corruption error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}
