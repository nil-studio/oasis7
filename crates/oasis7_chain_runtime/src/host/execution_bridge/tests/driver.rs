use super::super::checkpoint::{execution_bridge_record_path, load_execution_bridge_record};
use super::super::driver::{
    NodeRuntimeExecutionDriver, derive_local_execution_bootstrap, load_execution_bridge_state,
    load_execution_world, load_execution_world_with_policy, persist_execution_world,
    simulator_world_dir_from_execution_world_dir,
};
use super::super::external_effect::load_execution_external_effect_materialization;
use super::*;
use oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope;
use oasis7::consensus_action_payload::encode_consensus_action_payload;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::{
    Action as RuntimeAction, DomainEvent, FROZEN_MAIN_TOKEN_INITIAL_SUPPLY, LocalCasStore,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription,
    ModuleSubscriptionStage, ReleaseSecurityPolicy, WorldEventBody,
    production_hardened_main_token_config,
};
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter};
use oasis7_wasm_abi::ModuleCallFailure;
use oasis7_wasm_executor::FixedSandbox;

#[test]
fn chain_runtime_persistence_recovery_roundtrip() {
    let dir = temp_dir("execution-world");
    let world_dir = dir.join("world");
    let mut world = oasis7::runtime_host::world::new();
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "recovery-agent".to_string(),
        pos: oasis7::geometry::GeoPos::new(1, 2, 3),
    });
    world.step().expect("commit pre-recovery action");

    persist_execution_world(world_dir.as_path(), &world).expect("persist world");
    let mut recovered = load_execution_world(world_dir.as_path()).expect("recover world");
    assert_eq!(recovered.state().time, world.state().time);
    assert_eq!(recovered.state().agents, world.state().agents);
    assert_eq!(
        recovered.tick_consensus_records(),
        world.tick_consensus_records()
    );

    recovered.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "post-recovery-agent".to_string(),
        pos: oasis7::geometry::GeoPos::new(4, 5, 6),
    });
    recovered.step().expect("continue recovered runtime");
    assert!(recovered.state().agents.contains_key("post-recovery-agent"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_execution_world_defaults_to_hardened_release_policy() {
    let dir = temp_dir("execution-world-release-policy");
    let missing_world = load_execution_world(dir.as_path()).expect("load missing world");
    assert!(
        missing_world
            .release_security_policy()
            .is_production_hardened()
    );
    assert_eq!(
        missing_world.main_token_config().initial_supply,
        FROZEN_MAIN_TOKEN_INITIAL_SUPPLY
    );
    assert!(
        missing_world.state().agents.is_empty(),
        "chain runtime empty world bootstrap should not inject viewer/demo agents"
    );
    assert_eq!(
        missing_world.journal().len(),
        0,
        "chain runtime empty world bootstrap should stay pristine"
    );

    let legacy_world_dir = dir.join("legacy");
    let legacy_world = oasis7::runtime_host::world::new();
    persist_execution_world(legacy_world_dir.as_path(), &legacy_world).expect("persist world");
    let loaded_world = load_execution_world(legacy_world_dir.as_path()).expect("load world");
    assert!(
        loaded_world
            .release_security_policy()
            .is_production_hardened()
    );
    assert_eq!(
        loaded_world.main_token_config().initial_supply,
        FROZEN_MAIN_TOKEN_INITIAL_SUPPLY
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_expected_hash_mismatch_before_persisting_record() {
    let dir = temp_dir("execution-driver-expected-hash-mismatch");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    let err = driver
        .on_commit_with_expected(
            NodeExecutionCommitContext {
                world_id: "w1".to_string(),
                node_id: "node-a".to_string(),
                proposer_id: "node-a".to_string(),
                height: 1,
                slot: 0,
                epoch: 0,
                node_block_hash: "node-h1".to_string(),
                action_root: empty_action_root,
                committed_actions: Vec::new(),
                committed_at_unix_ms: 1_000,
            },
            Some("peer-block"),
            Some("peer-state"),
        )
        .expect_err("expected hash mismatch");

    assert!(
        err.contains("peer mismatch at height 1"),
        "unexpected error: {err}"
    );
    assert!(!execution_bridge_record_path(records_dir.as_path(), 1).exists());
    assert!(!records_dir.join("latest.json").exists());
    let state = load_execution_bridge_state(state_path.as_path()).expect("load state");
    assert_eq!(state.last_applied_committed_height, 0);
    assert!(state.last_execution_block_hash.is_none());
    assert!(state.last_execution_state_root.is_none());
    let world = load_execution_world(world_dir.as_path()).expect("load world");
    assert_eq!(world.state().time, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rejected_expected_hash_does_not_persist_simulator_mirror() {
    let dir = temp_dir("execution-driver-expected-hash-mismatch-simulator");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let simulator_world_dir = simulator_world_dir_from_execution_world_dir(world_dir.as_path());
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("driver");
    let payload =
        encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_simulator_action(
            SimulatorAction::HarvestRadiation {
                agent_id: "agent-0".to_string(),
                max_amount: 1,
            },
            ActionSubmitter::System,
        ))
        .expect("encode simulator payload");
    let committed_action = oasis7_node::NodeConsensusAction::from_payload(1, "node-a", payload)
        .expect("consensus action");
    let action_root =
        compute_consensus_action_root(std::slice::from_ref(&committed_action)).expect("root");

    let err = driver
        .on_commit_with_expected(
            NodeExecutionCommitContext {
                world_id: "w1".to_string(),
                node_id: "node-a".to_string(),
                proposer_id: "node-a".to_string(),
                height: 1,
                slot: 0,
                epoch: 0,
                node_block_hash: "node-h1".to_string(),
                action_root,
                committed_actions: vec![committed_action],
                committed_at_unix_ms: 1_000,
            },
            Some("peer-block"),
            Some("peer-state"),
        )
        .expect_err("expected hash mismatch");

    assert!(
        err.contains("peer mismatch at height 1"),
        "unexpected error: {err}"
    );
    assert!(!execution_bridge_record_path(records_dir.as_path(), 1).exists());

    drop(driver);
    let simulator_snapshot_path = simulator_world_dir.join("snapshot.json");
    let simulator_journal_path = simulator_world_dir.join("journal.json");
    if simulator_snapshot_path.exists() || simulator_journal_path.exists() {
        let restored_simulator =
            oasis7::simulator::WorldKernel::load_from_dir(simulator_world_dir.as_path())
                .expect("load simulator mirror after rejected commit");
        assert_eq!(
            restored_simulator.journal().len(),
            0,
            "rejected simulator action must not advance durable simulator mirror"
        );
    }
    let restarted =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir, storage_root)
            .expect("restarted driver");
    assert_eq!(
        restarted.simulator_mirror.journal().len(),
        0,
        "restart must not observe simulator state from rejected replay"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_persists_v2_committed_tick_context() {
    let dir = temp_dir("execution-driver-v2-committed-tick-context");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path,
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 1,
            slot: 42,
            epoch: 3,
            node_block_hash: "node-h1".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000,
        })
        .expect("commit");

    let bridge_record = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load authoritative v3 bridge record");
    assert_eq!(
        bridge_record.schema_version,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3
    );
    let snapshot_ref = bridge_record
        .snapshot_ref
        .as_deref()
        .expect("authoritative v3 snapshot ref");
    let journal_ref = bridge_record
        .journal_ref
        .as_deref()
        .expect("authoritative v3 journal ref");
    let store = LocalCasStore::new(dir.join("store"));
    let snapshot_bytes = store
        .get_verified(snapshot_ref)
        .expect("load authoritative snapshot CAS blob");
    let journal_bytes = store
        .get_verified(journal_ref)
        .expect("load authoritative journal CAS blob");
    let snapshot = serde_cbor::from_slice(snapshot_bytes.as_slice()).expect("decode snapshot");
    let journal = serde_cbor::from_slice(journal_bytes.as_slice()).expect("decode journal");
    let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        .expect("reconstruct committed world");
    let record = world
        .latest_tick_consensus_record()
        .expect("latest tick consensus record");
    assert_eq!(record.block.header.schema_version, 2);
    assert_eq!(record.block.header.chain_height, Some(1));
    assert_eq!(record.block.header.chain_slot, Some(42));
    assert_eq!(record.block.header.chain_epoch, Some(3));
    assert_eq!(
        record.block.header.node_block_hash.as_deref(),
        None,
        "runtime snapshot must not include chain provenance in canonical tick records"
    );
    assert_eq!(
        record.block.header.action_root.as_deref(),
        Some(empty_action_root.as_str())
    );
    assert_eq!(record.block.header.committed_at_unix_ms, Some(1_000));
    assert_eq!(
        record.block.execution_digest.action_batch_hash,
        empty_action_root
    );
    assert_eq!(bridge_record.node_block_hash.as_deref(), Some("node-h1"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_execution_world_with_dev_local_policy_keeps_generic_supply_for_missing_world() {
    let dir = temp_dir("execution-world-dev-local-policy");
    let missing_world =
        load_execution_world_with_policy(dir.as_path(), ReleaseSecurityPolicy::default())
            .expect("load missing world with dev_local policy");

    assert_eq!(
        missing_world.release_security_policy(),
        &ReleaseSecurityPolicy::default()
    );
    assert_eq!(missing_world.main_token_config().initial_supply, 0);
    assert!(missing_world.state().agents.is_empty());
    assert_eq!(missing_world.journal().len(), 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_execution_world_with_dev_local_policy_clears_pristine_frozen_supply_from_existing_world() {
    let dir = temp_dir("execution-world-dev-local-clears-pristine-frozen");
    let world_dir = dir.join("world");
    let mut world = oasis7::runtime_host::world::new();
    world.set_main_token_config(production_hardened_main_token_config());
    persist_execution_world(world_dir.as_path(), &world).expect("persist release-like world");

    let loaded_world =
        load_execution_world_with_policy(world_dir.as_path(), ReleaseSecurityPolicy::default())
            .expect("load existing world with dev_local policy");

    assert_eq!(
        loaded_world.release_security_policy(),
        &ReleaseSecurityPolicy::default()
    );
    assert_eq!(loaded_world.main_token_config().initial_supply, 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn local_execution_bootstrap_accepts_first_successor_and_recovers_durable_head() {
    let dir = temp_dir("execution-driver-local-bootstrap");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let world_id = "world-local-bootstrap";
    let mut world = oasis7::runtime_host::world::new();
    world.step().expect("persist local setup tick one");
    world.step().expect("persist local setup tick two");
    assert_eq!(world.state().time, 2);
    persist_execution_world(world_dir.as_path(), &world).expect("persist local setup world");

    let baseline = derive_local_execution_bootstrap(
        world_dir.as_path(),
        records_dir.as_path(),
        world_id,
        2,
        "local-finality-h2",
        &ReleaseSecurityPolicy::default(),
    )
    .expect("derive persisted local execution boundary");
    let storage_profile = StorageProfileConfig::default();
    let mut driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
        baseline.clone(),
    )
    .expect("start driver at persisted local boundary");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");

    let first = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: world_id.to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 3,
            slot: 3,
            epoch: 0,
            node_block_hash: "node-h3".to_string(),
            action_root: action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 3_000,
        })
        .expect("first consensus successor should advance runtime time");
    assert_eq!(first.execution_height, 3);
    assert_eq!(
        load_execution_world_with_policy(world_dir.as_path(), ReleaseSecurityPolicy::default())
            .expect("load h3 world")
            .state()
            .time,
        3
    );
    assert!(execution_bridge_record_path(records_dir.as_path(), 3).exists());
    drop(driver);

    let mut restarted = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
        &storage_profile,
        baseline,
    )
    .expect("restore durable h3 execution head");
    let second = restarted
        .on_commit(NodeExecutionCommitContext {
            world_id: world_id.to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 4,
            slot: 4,
            epoch: 0,
            node_block_hash: "node-h4".to_string(),
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 4_000,
        })
        .expect("restarted driver should advance to h4");
    assert_eq!(second.execution_height, 4);
    assert_eq!(
        load_execution_world_with_policy(world_dir.as_path(), ReleaseSecurityPolicy::default())
            .expect("load h4 world")
            .state()
            .time,
        4
    );

    drop(restarted);
    let durable_baseline = derive_local_execution_bootstrap(
        world_dir.as_path(),
        records_dir.as_path(),
        world_id,
        4,
        "local-finality-h4",
        &ReleaseSecurityPolicy::default(),
    )
    .expect("derive durable later-height execution boundary");
    assert_eq!(durable_baseline.consensus_block_hash, "node-h4");
    let _durable_restarted = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        &storage_profile,
        durable_baseline,
    )
    .expect("restore durable later-height execution head");

    let _ = fs::remove_dir_all(dir);
}

fn tick_manifest(wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: "m.test.tick".to_string(),
        name: "Tick Test".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: oasis7_wasm_abi::ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::default(),
    }
}

#[test]
fn node_runtime_execution_driver_commit_routes_modules_via_step_with_modules() {
    let dir = temp_dir("execution-driver-modules");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");

    let wasm_bytes = b"bridge-modules-wasm".to_vec();
    let wasm_hash = {
        let mut hasher = Sha256::new();
        hasher.update(wasm_bytes.as_slice());
        hex::encode(hasher.finalize())
    };
    let manifest = tick_manifest(&wasm_hash);
    let mut world = oasis7::runtime_host::world::new();
    let signer_public_key_hex = hex::encode(
        test_module_artifact_signing_key()
            .verifying_key()
            .to_bytes(),
    );
    world
        .bind_node_identity(
            TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
            signer_public_key_hex.as_str(),
        )
        .expect("bind test module signer");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: oasis7::geometry::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register");
    world
        .set_agent_resource_balance(
            "agent-0",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            128,
        )
        .expect("seed electricity");
    world
        .set_agent_resource_balance(
            "agent-0",
            oasis7_agent_api::world_action::ResourceKind::Data,
            64,
        )
        .expect("seed data");
    world.submit_action(RuntimeAction::DeployModuleArtifact {
        publisher_agent_id: "agent-0".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes: wasm_bytes.clone(),
    });
    world.step().expect("deploy");
    world.submit_action(RuntimeAction::InstallModuleFromArtifact {
        installer_agent_id: "agent-0".to_string(),
        manifest: manifest.clone(),
        activate: true,
    });
    world.step().expect("install");

    let instance_id = {
        let event = world.journal().events.last().expect("install event");
        let WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) = &event.body
        else {
            panic!("expected module installed event: {:?}", event.body);
        };
        instance_id.clone()
    };
    let instance = world
        .state()
        .module_instances
        .get(&instance_id)
        .expect("installed module instance");
    assert!(
        instance.active,
        "installed module instance should be active"
    );
    assert!(
        world
            .snapshot()
            .module_tick_schedule
            .contains_key(&instance_id),
        "installed tick module should be scheduled"
    );

    let expected_trace = format!(
        "tick-{}-{}",
        world.state().time.saturating_add(1),
        instance_id
    );
    let sandbox = FixedSandbox::fail(ModuleCallFailure {
        module_id: manifest.module_id.clone(),
        trace_id: expected_trace.clone(),
        code: oasis7_wasm_abi::ModuleCallErrorCode::PolicyDenied,
        detail: "forced failure for routing assertion".to_string(),
    });
    let mut driver = NodeRuntimeExecutionDriver::new_with_sandbox(
        state_path,
        world_dir,
        records_dir,
        storage_root,
        ExecutionBridgeState::default(),
        world,
        Box::new(sandbox.clone()),
        EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_KEEP_LATEST,
    );

    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let err = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
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
        .expect_err("forced module failure should bubble");
    assert!(
        err.contains("world.step failed"),
        "unexpected error from commit path: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_persists_chain_records() {
    let dir = temp_dir("execution-driver");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root,
    )
    .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    let first = driver
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
        .expect("first commit");
    let second = driver
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
        .expect("second commit");

    assert_eq!(first.execution_height, 1);
    assert_eq!(second.execution_height, 2);
    assert_ne!(first.execution_block_hash, second.execution_block_hash);
    assert!(records_dir.join("00000000000000000001.json").exists());
    assert!(records_dir.join("00000000000000000002.json").exists());

    let state = load_execution_bridge_state(state_path.as_path()).expect("load state");
    assert_eq!(state.last_applied_committed_height, 2);
    assert_eq!(state.last_node_block_hash.as_deref(), Some("node-h2"));

    let store = LocalCasStore::new(dir.join("store"));
    let record_bytes = fs::read(records_dir.join("00000000000000000002.json"))
        .expect("read second execution bridge record");
    let record: ExecutionBridgeRecord =
        serde_json::from_slice(record_bytes.as_slice()).expect("parse second record");
    let external_effect_ref = record
        .external_effect_ref
        .as_deref()
        .expect("external effect ref should exist");
    let external_effect =
        load_execution_external_effect_materialization(&store, external_effect_ref)
            .expect("load external effect materialization");
    assert_eq!(external_effect.height, 2);
    assert_eq!(external_effect.slot, 1);
    assert_eq!(external_effect.epoch, 0);
    assert_eq!(
        external_effect.action_root,
        compute_consensus_action_root(&[]).expect("empty root")
    );
    assert!(external_effect.committed_actions.is_empty());
    assert!(external_effect.unresolved_inputs.is_empty());
    let proof = load_world_head_proof(&store, &record);
    assert_eq!(proof.world_id, "w1");
    assert_eq!(proof.height, 2);
    assert_eq!(proof.timestamp_ms, 2_000);
    assert_eq!(proof.execution.node_block_hash, "node-h2");
    assert_eq!(
        proof.execution.execution_block_hash,
        record.execution_block_hash
    );
    assert_eq!(
        proof.execution.execution_state_root,
        record.execution_state_root
    );
    assert_eq!(
        proof.execution.action_root,
        compute_consensus_action_root(&[]).expect("empty root")
    );
    assert_eq!(
        proof.snapshot_manifest_ref.content_hash,
        record.snapshot_ref.expect("snapshot ref")
    );
    assert_eq!(
        proof.journal_segments_ref.content_hash,
        record.journal_ref.expect("journal ref")
    );
    assert!(proof.checkpoint.is_none());
    let mut tampered_action = proof.clone();
    tampered_action.execution.action_root = "tampered-action-root".to_string();
    assert!(
        tampered_action
            .validate_contract()
            .expect_err("tampered action root should fail")
            .contains("execution action_root mismatch")
    );
    let mut tampered_timestamp = proof.clone();
    tampered_timestamp.block.timestamp_ms += 1;
    assert!(
        tampered_timestamp
            .validate_contract()
            .expect_err("tampered timestamp should fail")
            .contains("timestamp mismatch")
    );
    let mut tampered_snapshot = proof;
    tampered_snapshot.snapshot_manifest_ref.content_hash = "tampered-snapshot".to_string();
    assert!(
        tampered_snapshot
            .validate_contract()
            .expect_err("tampered snapshot should fail")
            .contains("snapshot_ref mismatch")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_non_contiguous_commit_without_predecessor_record() {
    let dir = temp_dir("execution-driver-gap");
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
        .expect("first commit");
    let err = driver
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
        .expect_err("gap commit without predecessor record should fail");
    assert!(
        err.contains("missing predecessor record"),
        "unexpected gap error: {err}"
    );
    assert!(records_dir.join("00000000000000000001.json").exists());
    assert!(!records_dir.join("00000000000000000003.json").exists());
    assert!(!records_dir.join("00000000000000000002.json").exists());

    let state = load_execution_bridge_state(state_path.as_path()).expect("load state");
    assert_eq!(state.last_applied_committed_height, 1);
    assert_eq!(state.last_node_block_hash.as_deref(), Some("node-h1"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_restores_predecessor_before_gap_commit() {
    let dir = temp_dir("execution-driver-gap-restore");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut driver =
        NodeRuntimeExecutionDriver::new(state_path.clone(), world_dir, records_dir, storage_root)
            .expect("driver");
    let empty_action_root = compute_consensus_action_root(&[]).expect("empty action root");

    let first = driver
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
        .expect("first commit");
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 2,
            slot: 1,
            epoch: 0,
            node_block_hash: "node-h2".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 2_000,
        })
        .expect("second commit");
    let third = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".to_string(),
            node_id: "node-a".to_string(),
            proposer_id: "node-a".to_string(),
            height: 3,
            slot: 2,
            epoch: 0,
            node_block_hash: "node-h3".to_string(),
            action_root: empty_action_root.clone(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 3_000,
        })
        .expect("third commit");

    let replayed_first = driver
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
        .expect("replay first commit");
    assert_eq!(replayed_first.execution_height, first.execution_height);
    assert_eq!(
        replayed_first.execution_block_hash,
        first.execution_block_hash
    );
    assert_eq!(
        replayed_first.execution_state_root,
        first.execution_state_root
    );

    let replayed_third = driver
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
        .expect("replay third commit after restoring predecessor");
    assert_eq!(replayed_third.execution_height, third.execution_height);
    assert_eq!(
        replayed_third.execution_block_hash,
        third.execution_block_hash
    );
    assert_eq!(
        replayed_third.execution_state_root,
        third.execution_state_root
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_recipe_origin_preserves_exact_consensus_context_and_runtime_id_allocation() {
    let action = RuntimeAction::ScheduleRecipe {
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(1, vec![], vec![], vec![], 0, 1),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let mut envelope = ConsensusActionPayloadEnvelope::from_runtime_action(action);
    envelope.version = 2;
    envelope.gameplay_submission_origin = Some(oasis7_runtime::GameplaySubmissionOrigin {
        verified_player_id: "browser-player".into(),
        public_key: "a".repeat(64),
        auth_nonce: 7,
        hosted_registration_nonce: None,
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
    });
    let payload = encode_consensus_action_payload(&envelope).unwrap();
    let committed =
        oasis7_node::NodeConsensusAction::from_payload(2, "node-transport", payload).unwrap();
    let root = compute_consensus_action_root(std::slice::from_ref(&committed)).unwrap();
    let context = NodeExecutionCommitContext {
        world_id: "w1".into(),
        node_id: "node-a".into(),
        proposer_id: "node-a".into(),
        height: 1,
        slot: 0,
        epoch: 0,
        node_block_hash: "node-h1".into(),
        action_root: root.clone(),
        committed_actions: vec![committed.clone()],
        committed_at_unix_ms: 1000,
    };
    let (mut decoded, _, _, _) =
        super::super::driver_replicated_input::decode_committed_actions(&context).unwrap();
    let (action, origin) = decoded.pop().unwrap();
    let origin = origin.expect("new verified metadata");
    assert_eq!(origin.consensus_action_id, 2);
    assert_eq!(origin.action_payload_hash, committed.payload_hash);
    assert_eq!(origin.action_root, root);
    assert_eq!(origin.committed_height, 1);
    assert_eq!(origin.submission.verified_player_id, "browser-player");
    assert_eq!(origin.consensus_submitter_player_id, "node-transport");
    let mut world = oasis7::runtime_host::world::new();
    for index in 0..4 {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: format!("queued-{index}"),
            pos: oasis7::geometry::GeoPos::new(0, 0, 0),
        });
    }
    assert_eq!(
        world
            .submit_recipe_action_with_origin(action, origin.clone())
            .unwrap(),
        5
    );
    let snapshot = world.snapshot();
    assert_eq!(
        snapshot.pending_actions[4].committed_recipe_origin,
        Some(origin.clone())
    );
    let restored =
        oasis7::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        restored.snapshot().pending_actions[4].committed_recipe_origin,
        Some(origin)
    );
    envelope.gameplay_submission_origin = None;
    envelope.version = 1;
    let legacy = oasis7_node::NodeConsensusAction::from_payload(
        2,
        "node-transport",
        encode_consensus_action_payload(&envelope).unwrap(),
    )
    .unwrap();
    let mut legacy_context = context;
    legacy_context.action_root =
        compute_consensus_action_root(std::slice::from_ref(&legacy)).unwrap();
    legacy_context.committed_actions = vec![legacy];
    let (decoded, _, _, _) =
        super::super::driver_replicated_input::decode_committed_actions(&legacy_context).unwrap();
    assert!(
        decoded[0].1.is_none(),
        "legacy metadata must never be retroactively synthesized"
    );
}
