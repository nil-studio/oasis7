use super::super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_manifest_path, load_execution_bridge_record,
    maybe_persist_execution_checkpoint_for_record, persist_execution_bridge_record,
};
use super::super::driver::NodeRuntimeExecutionDriver;
use super::*;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::{
    LocalCasStore, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRole, PolicySet, World as RuntimeWorld,
};

fn install_module_fixture(world: &mut RuntimeWorld, module_id: &str, wasm_bytes: &[u8]) -> String {
    world.set_policy(PolicySet::allow_all());
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
        .expect("bind test module signer identity");
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register module fixture artifact");
    let manifest = ModuleManifest {
        module_id: module_id.to_string(),
        name: "Execution bridge recovery fixture".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits::unbounded(),
    };
    let changes = ModuleChangeSet {
        register: vec![manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: manifest.module_id.clone(),
            version: manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };
    let proposal_id = world
        .propose_manifest_update(
            Manifest {
                version: 2,
                content: serde_json::json!({"module_changes": changes}),
            },
            "execution-bridge-recovery-fixture",
        )
        .expect("propose module fixture");
    world
        .shadow_proposal(proposal_id)
        .expect("shadow module fixture proposal");
    world
        .approve_proposal(
            proposal_id,
            "execution-bridge-recovery-fixture-approver",
            oasis7_runtime::ProposalDecision::Approve,
        )
        .expect("approve module fixture proposal");
    world
        .apply_proposal(proposal_id)
        .expect("apply module fixture proposal");
    wasm_hash
}

struct CompactedNormalV3Fixture {
    state_path: std::path::PathBuf,
    world_dir: std::path::PathBuf,
    records_dir: std::path::PathBuf,
    storage_root: std::path::PathBuf,
    record: ExecutionBridgeRecord,
}

fn seed_compacted_normal_v3_fixture(
    dir: &std::path::Path,
    checkpoint_height: u64,
    latest_height: u64,
) -> CompactedNormalV3Fixture {
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
    .expect("seed driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    for height in 1..=latest_height {
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".to_string(),
                node_id: "node-a".to_string(),
                proposer_id: "node-a".to_string(),
                height,
                slot: height.saturating_sub(1),
                epoch: 0,
                node_block_hash: format!("node-h{height}"),
                action_root: action_root.clone(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: height as i64 * 1_000,
            })
            .expect("seed normal V3 commit");
    }
    drop(driver);

    let record_path = execution_bridge_record_path(records_dir.as_path(), checkpoint_height);
    let mut record = load_execution_bridge_record(record_path.as_path()).expect("load V3 record");
    let checkpoint_ref =
        maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 1, 1)
            .expect("persist retained checkpoint manifest")
            .expect("checkpoint at requested height");
    record.checkpoint_ref = Some(checkpoint_ref);
    assert!(
        record.proposer_id.is_some(),
        "fixture must remain a normal V3 record"
    );
    assert!(
        record.action_root.is_some(),
        "fixture must retain action metadata"
    );
    record.latest_state_ref = None;
    record.snapshot_ref = None;
    record.journal_ref = None;
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("persist compacted normal V3 record");

    CompactedNormalV3Fixture {
        state_path,
        world_dir,
        records_dir,
        storage_root,
        record,
    }
}

struct ModuleRecoveryFixture {
    state_path: std::path::PathBuf,
    world_dir: std::path::PathBuf,
    records_dir: std::path::PathBuf,
    storage_root: std::path::PathBuf,
    module_hash: String,
    module_registry: oasis7_runtime::ModuleRegistry,
    record: ExecutionBridgeRecord,
}

fn seed_module_recovery_fixture(dir: &std::path::Path) -> ModuleRecoveryFixture {
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let mut historical_world = oasis7::runtime_host::world::new();
    let module_hash = install_module_fixture(
        &mut historical_world,
        "m.execution-bridge.historical",
        b"historical-module-bytes",
    );
    historical_world
        .save_to_dir(world_dir.as_path())
        .expect("persist historical module world");
    let module_registry = historical_world.module_registry().clone();

    let mut driver = NodeRuntimeExecutionDriver::new(
        state_path.clone(),
        world_dir.clone(),
        records_dir.clone(),
        storage_root.clone(),
    )
    .expect("seed module driver");
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    for height in 1..=2 {
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".to_string(),
                node_id: "node-a".to_string(),
                proposer_id: "node-a".to_string(),
                height,
                slot: height.saturating_sub(1),
                epoch: 0,
                node_block_hash: format!("node-h{height}"),
                action_root: action_root.clone(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: height as i64 * 1_000,
            })
            .expect("seed module commit");
    }
    drop(driver);

    // Leave a newer module-bearing world in the mutable cache. A historical
    // restore must continue to use only the snapshot CAS payload.
    let mut newer_world = oasis7::runtime_host::world::new();
    install_module_fixture(
        &mut newer_world,
        "m.execution-bridge.newer",
        b"newer-module-bytes",
    );
    newer_world
        .save_to_dir(world_dir.as_path())
        .expect("persist newer mutable module cache");

    let record_path = execution_bridge_record_path(records_dir.as_path(), 1);
    let mut record = load_execution_bridge_record(record_path.as_path()).expect("load old record");
    let checkpoint_ref =
        maybe_persist_execution_checkpoint_for_record(records_dir.as_path(), &record, 1, 1)
            .expect("persist old module checkpoint")
            .expect("old checkpoint");
    record.checkpoint_ref = Some(checkpoint_ref);
    record.latest_state_ref = None;
    record.snapshot_ref = None;
    record.journal_ref = None;
    persist_execution_bridge_record(records_dir.as_path(), &record)
        .expect("compact old module record");

    ModuleRecoveryFixture {
        state_path,
        world_dir,
        records_dir,
        storage_root,
        module_hash,
        module_registry,
        record,
    }
}

#[test]
fn node_runtime_execution_driver_startup_restores_compacted_normal_v3_record_from_checkpoint() {
    let dir = temp_dir("execution-driver-startup-compacted-normal-v3-checkpoint");
    let fixture = seed_compacted_normal_v3_fixture(dir.as_path(), 1, 1);

    let restarted = NodeRuntimeExecutionDriver::new(
        fixture.state_path.clone(),
        fixture.world_dir,
        fixture.records_dir.clone(),
        fixture.storage_root,
    )
    .expect("startup should restore compacted normal V3 record from retained checkpoint");
    assert_eq!(restarted.state.last_applied_committed_height, 1);
    assert_eq!(
        restarted.state.last_execution_block_hash.as_deref(),
        Some(fixture.record.execution_block_hash.as_str())
    );
    assert_eq!(
        restarted.state.last_execution_state_root.as_deref(),
        Some(fixture.record.execution_state_root.as_str())
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_stale_restore_reuses_compacted_normal_v3_checkpoint() {
    let dir = temp_dir("execution-driver-stale-compacted-normal-v3-checkpoint");
    let fixture = seed_compacted_normal_v3_fixture(dir.as_path(), 1, 2);
    let mut restarted = NodeRuntimeExecutionDriver::new(
        fixture.state_path,
        fixture.world_dir,
        fixture.records_dir,
        fixture.storage_root,
    )
    .expect("latest hot record should start before stale restore");

    let result = restarted
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
        .expect("stale rollback should restore compacted checkpoint-backed record");
    assert_eq!(result.execution_height, 1);
    assert_eq!(
        result.execution_block_hash,
        fixture.record.execution_block_hash
    );
    assert_eq!(
        result.execution_state_root,
        fixture.record.execution_state_root
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_stale_restore_uses_historical_module_bytes() {
    let dir = temp_dir("execution-driver-stale-module-cas-recovery");
    let fixture = seed_module_recovery_fixture(dir.as_path());
    let mut restarted = NodeRuntimeExecutionDriver::new(
        fixture.state_path,
        fixture.world_dir,
        fixture.records_dir,
        fixture.storage_root,
    )
    .expect("latest record should start before stale module restore");

    let result = restarted
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
        .expect("stale restore should use the compacted historical checkpoint");
    assert_eq!(result.execution_height, 1);
    assert_eq!(
        result.execution_block_hash,
        fixture.record.execution_block_hash
    );
    assert_eq!(
        restarted.execution_world.module_registry(),
        &fixture.module_registry,
    );
    restarted
        .execution_world
        .load_module(fixture.module_hash.as_str())
        .expect("historical module artifact bytes must be available after stale restore");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_startup_recovers_modules_when_cache_is_removed() {
    let dir = temp_dir("execution-driver-module-cas-cache-removed");
    let fixture = seed_module_recovery_fixture(dir.as_path());
    let module_registry_path = fixture.world_dir.join("module_registry.json");
    let modules_dir = fixture.world_dir.join("modules");
    fs::remove_file(module_registry_path).expect("remove module registry cache");
    fs::remove_dir_all(modules_dir).expect("remove module artifact cache");

    let restarted = NodeRuntimeExecutionDriver::new(
        fixture.state_path,
        fixture.world_dir,
        fixture.records_dir,
        fixture.storage_root,
    )
    .expect("CAS snapshot should recover after module cache removal");
    assert!(
        restarted
            .execution_world
            .module_registry()
            .records
            .values()
            .any(|record| record.manifest.wasm_hash == fixture.module_hash)
    );
    let mut world = restarted.execution_world;
    world
        .load_module(fixture.module_hash.as_str())
        .expect("CAS snapshot should carry module artifact bytes");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_tampered_compacted_checkpoint_manifest() {
    let dir = temp_dir("execution-driver-tampered-compacted-checkpoint-manifest");
    let fixture = seed_compacted_normal_v3_fixture(dir.as_path(), 1, 1);
    let manifest_path = execution_checkpoint_manifest_path(fixture.records_dir.as_path(), 1);
    let mut manifest_json: serde_json::Value = serde_json::from_slice(
        fs::read(manifest_path.as_path())
            .expect("read manifest")
            .as_slice(),
    )
    .expect("parse manifest");
    manifest_json["execution_state_root"] =
        serde_json::Value::String("tampered-state-root".to_string());
    crate::write_bytes_atomic(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&manifest_json)
            .expect("serialize tampered manifest")
            .as_slice(),
    )
    .expect("persist tampered manifest");

    let err = match NodeRuntimeExecutionDriver::new(
        fixture.state_path,
        fixture.world_dir,
        fixture.records_dir,
        fixture.storage_root,
    ) {
        Ok(_) => panic!("tampered checkpoint manifest must fail closed"),
        Err(err) => err,
    };
    assert!(
        err.contains("hash mismatch") || err.contains("checkpoint"),
        "unexpected tampered manifest error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn node_runtime_execution_driver_rejects_missing_compacted_checkpoint_blob() {
    let dir = temp_dir("execution-driver-missing-compacted-checkpoint-blob");
    let fixture = seed_compacted_normal_v3_fixture(dir.as_path(), 1, 1);
    let manifest_path = execution_checkpoint_manifest_path(fixture.records_dir.as_path(), 1);
    let manifest: ExecutionCheckpointManifest = serde_json::from_slice(
        fs::read(manifest_path.as_path())
            .expect("read manifest")
            .as_slice(),
    )
    .expect("parse manifest");
    let store = LocalCasStore::new(fixture.storage_root.clone());
    fs::remove_file(
        store
            .blobs_dir()
            .join(format!("{}.blob", manifest.latest_state_ref)),
    )
    .expect("remove checkpoint snapshot blob");

    let err = match NodeRuntimeExecutionDriver::new(
        fixture.state_path,
        fixture.world_dir,
        fixture.records_dir,
        fixture.storage_root,
    ) {
        Ok(_) => panic!("missing checkpoint blob must fail closed"),
        Err(err) => err,
    };
    assert!(
        err.contains("CAS snapshot") || err.contains("checkpoint"),
        "unexpected missing checkpoint blob error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}
