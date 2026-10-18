use super::super::checkpoint::{
    execution_bridge_record_path, load_execution_bridge_record, persist_execution_bridge_record,
};
use super::super::driver::{
    NodeRuntimeExecutionDriver, load_execution_bridge_state, persist_execution_bridge_state,
};
use super::super::external_effect::{
    execution_committed_actions_hash, load_execution_external_effect_materialization,
    persist_execution_external_effect_materialization,
};
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_node::{
    NodeConsensusAction, NodeExecutionCommitContext, NodeExecutionHook,
    compute_consensus_action_root,
};
use oasis7_runtime::{LocalCasStore, blake3_hex};
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter};

fn simulator_committed_action(action_id: u64, max_amount: i64) -> NodeConsensusAction {
    let payload =
        encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_simulator_action(
            SimulatorAction::HarvestRadiation {
                agent_id: "agent-0".to_string(),
                max_amount,
            },
            ActionSubmitter::System,
        ))
        .expect("encode simulator action");
    NodeConsensusAction::from_payload(action_id, "node-a", payload).expect("consensus action")
}

fn replay_context(
    proposer_id: &str,
    node_block_hash: &str,
    committed_actions: Vec<NodeConsensusAction>,
) -> NodeExecutionCommitContext {
    let action_root =
        compute_consensus_action_root(committed_actions.as_slice()).expect("action root");
    NodeExecutionCommitContext {
        world_id: "w1".to_string(),
        node_id: "node-a".to_string(),
        proposer_id: proposer_id.to_string(),
        height: 1,
        slot: 0,
        epoch: 0,
        node_block_hash: node_block_hash.to_string(),
        action_root,
        committed_actions,
        committed_at_unix_ms: 1_000,
    }
}

#[test]
fn node_runtime_execution_driver_rolls_back_in_memory_worlds_after_state_persist_failure() {
    let dir = temp_dir("execution-driver-state-persist-retry");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir,
        records_dir.clone(),
        storage_root,
    )
    .expect("driver");
    let original_execution_state = driver.execution_world.state().clone();
    let original_execution_journal = driver.execution_world.journal().clone();
    let original_simulator_mirror = driver.simulator_mirror.clone();
    let original_driver_state = driver.state.clone();
    let context = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);

    let state_path_directory = dir.join("state-path-directory");
    fs::create_dir_all(state_path_directory.as_path()).expect("create failing state path");
    driver.state_path = state_path_directory;
    let err = driver
        .on_commit(context.clone())
        .expect_err("state persistence failure must reject commit");
    assert!(
        err.contains("state"),
        "state persistence failure should remain visible: {err}"
    );
    assert!(
        execution_bridge_record_path(records_dir.as_path(), 1).exists(),
        "the failure is after durable record persistence"
    );
    assert_eq!(driver.execution_world.state(), &original_execution_state);
    assert_eq!(
        driver.execution_world.journal(),
        &original_execution_journal
    );
    assert_eq!(driver.simulator_mirror, original_simulator_mirror);
    assert_eq!(driver.state, original_driver_state);

    driver.state_path = state_path.clone();
    let retried = driver
        .on_commit(context)
        .expect("same-process retry after repaired state path");
    assert_eq!(retried.execution_height, 1);
    let persisted = load_execution_bridge_state(state_path.as_path()).expect("persisted state");
    assert_eq!(persisted.last_applied_committed_height, 1);
    assert_eq!(persisted.last_node_block_hash.as_deref(), Some("node-h1"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_recovers_when_bridge_state_metadata_is_lost() {
    let dir = temp_dir("execution-driver-missing-state-recovery");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let context = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    driver.on_commit(context).expect("commit");
    assert_eq!(driver.state.last_applied_committed_height, 1);

    fs::remove_file(state_path.as_path()).expect("simulate lost bridge state metadata");
    let restarted =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("startup must recover from the authoritative execution record");

    assert_eq!(restarted.state.last_applied_committed_height, 1);
    assert_eq!(
        restarted.state.last_node_block_hash.as_deref(),
        Some("node-h1")
    );
    let record = load_execution_bridge_record(
        execution_bridge_record_path(
            restarted.records_dir.as_path(),
            restarted.state.last_applied_committed_height,
        )
        .as_path(),
    )
    .expect("authoritative record");
    let snapshot_ref = record.snapshot_ref.as_deref().expect("snapshot ref");
    let snapshot_bytes = LocalCasStore::new(dir.join("store"))
        .get_verified(snapshot_ref)
        .expect("recovered snapshot");
    assert_eq!(
        blake3_hex(snapshot_bytes.as_slice()),
        record.execution_state_root
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_conflicting_equal_height_v3_record_identity() {
    let dir = temp_dir("execution-driver-equal-height-v3-record-identity");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("driver");
    let committed_action = simulator_committed_action(1, 1);
    let accepted = replay_context("node-a", "node-h1", vec![committed_action]);
    driver.on_commit(accepted).expect("seed commit");

    for (field, conflicting) in [
        (
            "node_block_hash",
            replay_context(
                "node-a",
                "node-h1-conflict",
                vec![simulator_committed_action(1, 1)],
            ),
        ),
        (
            "proposer_id",
            replay_context("node-b", "node-h1", vec![simulator_committed_action(1, 1)]),
        ),
        (
            "action_root",
            replay_context("node-a", "node-h1", vec![simulator_committed_action(2, 2)]),
        ),
    ] {
        let err = driver
            .on_commit(conflicting)
            .expect_err("conflicting equal-height V3 replay must fail closed");
        assert!(
            err.contains(field),
            "error must identify conflicting {field}: {err}"
        );
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_conflicting_equal_height_v3_effect_identity_fields() {
    let dir = temp_dir("execution-driver-equal-height-v3-effect-identity");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("driver");
    let accepted = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    driver.on_commit(accepted.clone()).expect("seed commit");

    let mut conflicting_world_id = accepted.clone();
    conflicting_world_id.world_id = "w2".to_string();
    let mut conflicting_node_id = accepted.clone();
    conflicting_node_id.node_id = "node-b".to_string();
    let mut conflicting_slot = accepted.clone();
    conflicting_slot.slot = 9;
    let mut conflicting_epoch = accepted.clone();
    conflicting_epoch.epoch = 7;
    let mut conflicting_committed_at = accepted;
    conflicting_committed_at.committed_at_unix_ms = 9_999;

    for (field, conflicting) in [
        ("world_id", conflicting_world_id),
        ("node_id", conflicting_node_id),
        ("slot", conflicting_slot),
        ("epoch", conflicting_epoch),
        ("committed_at_unix_ms", conflicting_committed_at),
    ] {
        let err = driver
            .on_commit(conflicting)
            .expect_err("conflicting equal-height V3 replay must fail closed");
        assert!(
            err.contains(field),
            "error must identify conflicting {field}: {err}"
        );
    }

    let _ = fs::remove_dir_all(dir);
}

fn assert_tampered_authoritative_effect_identity_rejected(
    field: &str,
    mutate: fn(&mut ExecutionExternalEffectMaterialization),
) {
    let dir = temp_dir(format!("execution-driver-v3-effect-{field}").as_str());
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let accepted = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    driver.on_commit(accepted.clone()).expect("seed commit");

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut record = load_execution_bridge_record(record_path.as_path()).expect("load record");
    let effect_ref = record
        .external_effect_ref
        .as_deref()
        .expect("V3 record has authoritative external effect");
    let store = LocalCasStore::new(storage_root);
    let mut effect = load_execution_external_effect_materialization(&store, effect_ref)
        .expect("load authoritative external effect");
    mutate(&mut effect);
    record.external_effect_ref = Some(
        persist_execution_external_effect_materialization(&store, &effect)
            .expect("persist conflicting authoritative external effect"),
    );
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("replace authoritative record effect reference");

    let err = driver
        .on_commit(accepted)
        .expect_err("tampered authoritative V3 effect identity must fail closed");
    assert!(
        err.contains(field),
        "error must identify conflicting authoritative effect {field}: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_tampered_authoritative_v3_effect_identity_fields() {
    for (field, mutate) in [
        (
            "world_id",
            (|effect: &mut ExecutionExternalEffectMaterialization| {
                effect.world_id = "w2".to_string();
            }) as fn(&mut ExecutionExternalEffectMaterialization),
        ),
        ("node_id", |effect| effect.node_id = "node-b".to_string()),
        ("height", |effect| effect.height = 2),
        ("slot", |effect| effect.slot = 9),
        ("epoch", |effect| effect.epoch = 7),
        ("node_block_hash", |effect| {
            effect.node_block_hash = "node-h1-conflict".to_string();
        }),
        ("action_root", |effect| {
            effect.action_root = "conflicting-action-root".to_string();
        }),
        ("committed_at_unix_ms", |effect| {
            effect.committed_at_unix_ms = 9_999;
        }),
    ] {
        assert_tampered_authoritative_effect_identity_rejected(field, mutate);
    }
}

#[test]
fn node_runtime_execution_driver_rejects_conflicting_equal_height_v3_record_timestamp() {
    let dir = temp_dir("execution-driver-equal-height-v3-record-timestamp");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir.clone(), storage_root)
            .expect("driver");
    let accepted = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    driver.on_commit(accepted.clone()).expect("seed commit");

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut record = load_execution_bridge_record(record_path.as_path()).expect("load record");
    record.timestamp_ms = 9_999;
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("persist conflicting record timestamp");

    let err = driver
        .on_commit(accepted)
        .expect_err("conflicting equal-height V3 record timestamp must fail closed");
    assert!(
        err.contains("committed_at_unix_ms"),
        "error must identify conflicting record timestamp: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_equal_height_replay_when_authoritative_effect_actions_conflict()
 {
    let dir = temp_dir("execution-driver-equal-height-v3-effect-actions");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path,
        world_dir,
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let accepted = replay_context("node-a", "node-h1", vec![simulator_committed_action(1, 1)]);
    driver.on_commit(accepted.clone()).expect("seed commit");

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut record = load_execution_bridge_record(record_path.as_path()).expect("load record");
    let effect_ref = record
        .external_effect_ref
        .as_deref()
        .expect("V3 record has authoritative external effect");
    let store = LocalCasStore::new(storage_root);
    let mut effect = load_execution_external_effect_materialization(&store, effect_ref)
        .expect("load authoritative external effect");
    effect.committed_actions[0].payload_hash = "conflicting-action-payload".to_string();
    effect.committed_actions_hash =
        execution_committed_actions_hash(effect.committed_actions.as_slice())
            .expect("rehash conflicting authoritative actions");
    record.external_effect_ref = Some(
        persist_execution_external_effect_materialization(&store, &effect)
            .expect("persist conflicting authoritative external effect"),
    );
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("replace authoritative record effect reference");

    let err = driver
        .on_commit(accepted)
        .expect_err("equal-height replay must reject conflicting authoritative effect actions");
    assert!(
        err.contains("committed_actions"),
        "error must identify conflicting committed actions: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_restart_recovers_latest_head_after_retention() {
    let dir = temp_dir("execution-driver-restart-recovery");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let mut latest_result = None;
    for height in 1..=33 {
        latest_result = Some(
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
                .expect("commit before restart"),
        );
    }
    let latest_result = latest_result.expect("latest result before restart");
    drop(driver);

    let mut restarted = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
    )
    .expect("restarted driver");
    let replayed_latest = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 33,
            slot: 32,
            epoch: 0,
            node_block_hash: "node-h33".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 33_000,
        })
        .expect("replay latest commit after restart");
    assert_eq!(
        replayed_latest.execution_height,
        latest_result.execution_height
    );
    assert_eq!(
        replayed_latest.execution_block_hash,
        latest_result.execution_block_hash
    );
    assert_eq!(
        replayed_latest.execution_state_root,
        latest_result.execution_state_root
    );

    let next = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 34,
            slot: 33,
            epoch: 0,
            node_block_hash: "node-h34".to_string(),
            action_root: empty_action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 34_000,
        })
        .expect("next commit after restart");
    assert_eq!(next.execution_height, 34);

    let state = load_execution_bridge_state(state_path.as_path()).expect("load state");
    assert_eq!(state.last_applied_committed_height, 34);
    assert_eq!(state.last_node_block_hash.as_deref(), Some("node-h34"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_restart_recovers_authoritative_cas_when_world_cache_is_removed() {
    let dir = temp_dir("execution-driver-restart-cas-recovery-without-cache");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit");
    let committed_time = driver.execution_world.state().time;
    let committed_journal_len = driver.execution_world.journal().len();
    drop(driver);
    fs::remove_dir_all(world_dir.as_path()).expect("remove non-authoritative world cache");

    let restarted =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("restart recovers authoritative CAS state");
    assert_eq!(restarted.execution_world.state().time, committed_time);
    assert_eq!(
        restarted.execution_world.journal().len(),
        committed_journal_len
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_restart_fails_closed_when_authoritative_cas_is_missing() {
    let dir = temp_dir("execution-driver-restart-missing-authoritative-cas");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit");
    drop(driver);
    fs::remove_dir_all(world_dir.as_path()).expect("remove non-authoritative world cache");
    fs::remove_dir_all(storage_root.as_path()).expect("remove authoritative CAS blobs");

    let err =
        match NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root) {
            Ok(_) => panic!("restart must fail closed without authoritative CAS data"),
            Err(err) => err,
        };
    assert!(
        err.contains("authoritative") || err.contains("CAS") || err.contains("record"),
        "restart failure must identify authoritative recovery data: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_restores_lower_authoritative_head_when_state_head_lacks_exact_record()
 {
    let dir = temp_dir("execution-driver-startup-stale-state-head");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let authoritative_height_one = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("seed authoritative record");
    drop(driver);

    let authoritative_latest =
        load_execution_bridge_record(records_dir.join("latest.json").as_path())
            .expect("load latest authoritative record");
    assert_eq!(authoritative_latest.height, 1);
    assert_eq!(
        authoritative_latest.execution_block_hash.as_str(),
        authoritative_height_one.execution_block_hash.as_str()
    );
    assert_eq!(
        authoritative_latest.execution_state_root.as_str(),
        authoritative_height_one.execution_state_root.as_str()
    );

    persist_execution_bridge_state(
        state_path.as_path(),
        &ExecutionBridgeState {
            last_applied_committed_height: 2,
            last_execution_block_hash: Some("stale-execution-hash".to_string()),
            last_execution_state_root: Some("stale-state-root".to_string()),
            last_node_block_hash: Some("stale-node-hash".to_string()),
        },
    )
    .expect("persist stale state head");

    let restarted =
        NodeRuntimeExecutionDriver::new(state_path.clone(), world_dir, records_dir, storage_root)
            .expect("startup restores the lower authoritative head");
    let restored_state =
        load_execution_bridge_state(state_path.as_path()).expect("load restored state");

    assert_eq!(restored_state.last_applied_committed_height, 1);
    assert_eq!(
        restored_state.last_execution_block_hash.as_deref(),
        Some(authoritative_height_one.execution_block_hash.as_str())
    );
    assert_eq!(
        restored_state.last_execution_state_root.as_deref(),
        Some(authoritative_height_one.execution_state_root.as_str())
    );
    assert_eq!(
        restored_state.last_node_block_hash.as_deref(),
        Some("node-h1")
    );
    assert_eq!(restarted.state.last_applied_committed_height, 1);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_uses_higher_durable_record_when_latest_pointer_is_stale() {
    let dir = temp_dir("execution-driver-startup-stale-latest-pointer");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit height one");
    let height_two = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 1,
            epoch: 0,
            node_block_hash: "node-h2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2_000,
        })
        .expect("commit height two");
    drop(driver);

    let height_two_path = execution_bridge_record_path(records_dir.as_path(), 2);
    let height_two_bytes = fs::read(height_two_path.as_path()).expect("read durable height two");
    let stale_latest = fs::read(execution_bridge_record_path(records_dir.as_path(), 1))
        .expect("read durable height one");
    fs::write(records_dir.join("latest.json"), stale_latest).expect("install stale latest pointer");
    fs::remove_file(state_path.as_path()).expect("remove bridge state cache");

    let restarted = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir,
        records_dir.clone(),
        storage_root,
    )
    .expect("startup must recover the highest valid durable record");
    assert_eq!(restarted.state.last_applied_committed_height, 2);
    assert_eq!(
        restarted.state.last_execution_block_hash.as_deref(),
        Some(height_two.execution_block_hash.as_str())
    );
    assert_eq!(
        restarted.state.last_execution_state_root.as_deref(),
        Some(height_two.execution_state_root.as_str())
    );
    assert_eq!(
        restarted.state.last_node_block_hash.as_deref(),
        Some("node-h2")
    );
    assert_eq!(
        fs::read(height_two_path).expect("read height two after startup"),
        height_two_bytes
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_rejects_lower_v3_record_missing_journal_ref() {
    let dir = temp_dir("execution-driver-startup-malformed-lower-v3-record");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
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
        .expect("seed authoritative record");
    drop(driver);

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut malformed_record =
        load_execution_bridge_record(record_path.as_path()).expect("load authoritative record");
    assert_eq!(
        malformed_record.schema_version,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3
    );
    malformed_record.journal_ref = None;
    let malformed_bytes = serde_json::to_vec_pretty(&malformed_record)
        .expect("serialize malformed authoritative record");
    fs::write(record_path, malformed_bytes.as_slice()).expect("write malformed height record");
    fs::write(records_dir.join("latest.json"), malformed_bytes)
        .expect("write malformed latest record");
    persist_execution_bridge_state(
        state_path.as_path(),
        &ExecutionBridgeState {
            last_applied_committed_height: 2,
            last_execution_block_hash: Some("stale-execution-hash".to_string()),
            last_execution_state_root: Some("stale-state-root".to_string()),
            last_node_block_hash: Some("stale-node-hash".to_string()),
        },
    )
    .expect("persist ahead state head");

    let err =
        match NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root) {
            Ok(_) => panic!("startup must reject malformed lower V3 authority"),
            Err(err) => err,
        };
    assert!(
        err.contains("authoritative v3 record missing exact CAS refs"),
        "unexpected startup error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_reconciles_stale_state_from_exact_record() {
    let dir = temp_dir("execution-driver-stale-state-reconcile");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let mut last_result = None;
    for height in 1..=3 {
        last_result = Some(
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
                .expect("seed commit"),
        );
    }
    let height_three = last_result.expect("height three result");
    drop(driver);

    let stale_state = ExecutionBridgeState {
        last_applied_committed_height: 4,
        last_execution_block_hash: Some("stale-execution-hash".to_string()),
        last_execution_state_root: Some("stale-state-root".to_string()),
        last_node_block_hash: Some("stale-node-hash".to_string()),
    };
    persist_execution_bridge_state(state_path.as_path(), &stale_state)
        .expect("persist stale state");
    let mut restarted = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
    )
    .expect("restarted driver");
    let reconciled = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 3,
            slot: 2,
            epoch: 0,
            node_block_hash: "node-h3".to_string(),
            action_root: empty_action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 3_000,
        })
        .expect("reconcile stale state from record");

    assert_eq!(reconciled.execution_height, 3);
    assert_eq!(
        reconciled.execution_block_hash,
        height_three.execution_block_hash
    );
    assert_eq!(
        reconciled.execution_state_root,
        height_three.execution_state_root
    );

    let state = load_execution_bridge_state(state_path.as_path()).expect("load reconciled state");
    assert_eq!(state.last_applied_committed_height, 3);
    assert_eq!(
        state.last_execution_block_hash.as_deref(),
        Some(height_three.execution_block_hash.as_str())
    );
    assert_eq!(
        state.last_execution_state_root.as_deref(),
        Some(height_three.execution_state_root.as_str())
    );
    assert_eq!(state.last_node_block_hash.as_deref(), Some("node-h3"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_restart_reconciles_newer_published_record_when_state_is_stale() {
    let dir = temp_dir("execution-driver-restart-newer-published-record");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let first = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit height one");
    let second = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 1,
            epoch: 0,
            node_block_hash: "node-h2".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2_000,
        })
        .expect("commit height two");
    drop(driver);

    persist_execution_bridge_state(
        state_path.as_path(),
        &ExecutionBridgeState {
            last_applied_committed_height: 1,
            last_execution_block_hash: Some(first.execution_block_hash.clone()),
            last_execution_state_root: Some(first.execution_state_root.clone()),
            last_node_block_hash: Some("node-h1".to_string()),
        },
    )
    .expect("simulate crash after height-two record publication");

    let mut restarted =
        NodeRuntimeExecutionDriver::new(state_path.clone(), world_dir, records_dir, storage_root)
            .expect("restart reconciles newer authoritative record");
    assert_eq!(restarted.state.last_applied_committed_height, 2);
    let replayed = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 1,
            epoch: 0,
            node_block_hash: "node-h2".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2_000,
        })
        .expect("equal-height delivery is idempotent after restart");
    assert_eq!(replayed.execution_block_hash, second.execution_block_hash);
    assert_eq!(replayed.execution_state_root, second.execution_state_root);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_recovers_malformed_v2_record_from_state_root_and_local_journal() {
    let dir = temp_dir("execution-driver-malformed-v2-recovery");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let mut commit_results = Vec::new();
    for height in 1..=3 {
        let result = driver
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
            .expect("seed commit");
        commit_results.push(result);
    }
    drop(driver);

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let record_bytes = fs::read(record_path.as_path()).expect("read original record");
    let mut record_json: serde_json::Value =
        serde_json::from_slice(record_bytes.as_slice()).expect("parse original record");
    record_json["schema_version"] = serde_json::json!(EXECUTION_BRIDGE_RECORD_SCHEMA_V2);
    record_json
        .as_object_mut()
        .expect("record json object")
        .remove("latest_state_ref");
    record_json
        .as_object_mut()
        .expect("record json object")
        .remove("snapshot_ref");
    record_json
        .as_object_mut()
        .expect("record json object")
        .remove("journal_ref");
    let malformed_bytes =
        serde_json::to_vec_pretty(&record_json).expect("serialize malformed record");
    crate::write_bytes_atomic(record_path.as_path(), malformed_bytes.as_slice())
        .expect("persist malformed record");

    let stale_state = ExecutionBridgeState {
        last_applied_committed_height: 4,
        last_execution_block_hash: Some("stale-execution-hash".to_string()),
        last_execution_state_root: Some("stale-state-root".to_string()),
        last_node_block_hash: Some("stale-node-hash".to_string()),
    };
    persist_execution_bridge_state(state_path.as_path(), &stale_state)
        .expect("persist stale state");
    fs::write(
        records_dir.join("retention-degraded"),
        b"interrupted incremental retention\n",
    )
    .expect("persist interrupted retention marker");

    let mut restarted = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir,
        records_dir.clone(),
        storage_root,
    )
    .expect("restarted driver");
    let recovered_height_one = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("recover malformed height-1 record");
    assert_eq!(recovered_height_one.execution_height, 1);
    assert_eq!(
        recovered_height_one.execution_block_hash,
        commit_results[0].execution_block_hash
    );
    assert_eq!(
        recovered_height_one.execution_state_root,
        commit_results[0].execution_state_root
    );
    assert!(
        records_dir.join("retention-degraded").exists(),
        "stale-height recovery must precede destructive full reconciliation"
    );

    let continued_height_two = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 1,
            epoch: 0,
            node_block_hash: "node-h2".to_string(),
            action_root: empty_action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2_000,
        })
        .expect("continue after malformed-record recovery");
    assert_eq!(continued_height_two.execution_height, 2);
    assert_eq!(
        continued_height_two.execution_block_hash,
        commit_results[1].execution_block_hash
    );
    assert_eq!(
        continued_height_two.execution_state_root,
        commit_results[1].execution_state_root
    );

    let repaired_record =
        load_execution_bridge_record(record_path.as_path()).expect("load repaired height-1 record");
    assert_eq!(
        repaired_record.latest_state_ref.as_deref(),
        Some(repaired_record.execution_state_root.as_str())
    );
    assert_eq!(
        repaired_record.snapshot_ref.as_deref(),
        Some(repaired_record.execution_state_root.as_str())
    );
    assert!(
        repaired_record
            .journal_ref
            .as_deref()
            .is_some_and(|journal_ref| !journal_ref.is_empty())
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_stale_restore_from_other_world() {
    let dir = temp_dir("execution-driver-stale-state-world-mismatch");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("seed commit");
    drop(driver);

    let stale_state = ExecutionBridgeState {
        last_applied_committed_height: 2,
        last_execution_block_hash: Some("stale-execution-hash".to_string()),
        last_execution_state_root: Some("stale-state-root".to_string()),
        last_node_block_hash: Some("stale-node-hash".to_string()),
    };
    persist_execution_bridge_state(state_path.as_path(), &stale_state)
        .expect("persist stale state");

    let mut restarted =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("restarted driver");
    let err = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: "w2".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "node-h1".to_string(),
            action_root: empty_action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect_err("world mismatch should fail closed");
    assert!(
        err.contains("stale-height restore world_id mismatch"),
        "unexpected mismatch error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}
