use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::checkpoint::{
    execution_bridge_record_path, execution_checkpoint_latest_path,
    execution_checkpoint_manifest_path, load_execution_bridge_record,
    load_execution_checkpoint_manifest, load_latest_execution_checkpoint_manifest,
    persist_execution_bridge_record, persist_execution_bridge_record_only,
    persist_execution_checkpoint_manifest, run_execution_bridge_retention_maintenance,
};
use super::driver::{
    NodeRuntimeExecutionDriver, load_execution_bridge_state, persist_execution_bridge_state,
    persist_execution_world,
};
use super::external_effect::{
    build_execution_external_effect_materialization_with_pre_step_root,
    execution_committed_actions_hash, execution_module_anchor_hash, execution_world_snapshot_root,
    persist_execution_external_effect_materialization,
    validate_execution_external_effect_for_context,
};
use super::product_validation_intent::{
    ProductValidationIntentMarkerV1, load_product_validation_intent,
    persist_product_validation_intent, world_is_staged_for_product_validation_intent,
};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_node::{
    NodeConsensusSnapshot, NodeExecutionCommitContext, NodeRole, NodeSnapshot,
    compute_consensus_action_root,
};
use oasis7_runtime::{BlobStore, LocalCasStore, ModuleArtifactIdentity, World as RuntimeWorld};
use oasis7_wasm_abi::ModuleOutput;
use oasis7_wasm_executor::FixedSandbox;
use sha2::{Digest, Sha256};

mod determinism;
mod driver;
mod driver_authoritative_recovery;
mod driver_checkpoint;
mod driver_checkpoint_profile;
mod driver_checkpoint_transaction;
mod driver_compacted_checkpoint_recovery;
mod driver_observability;
mod driver_storage_profile;
// The fixture helpers live behind `test_tier_required` in the external
// `oasis7` library. Keep this binary-test module on the same feature boundary;
// `cfg(test)` on the binary does not propagate to its library dependency.
#[cfg(feature = "test_tier_required")]
mod provider_bootstrap;
mod qa_conformance;
mod replay;
mod retention;
#[path = "qa_conformance/scenario_resource.rs"]
mod scenario_resource;
mod simulator_driver;
pub(super) mod world_service;

const TEST_MODULE_ARTIFACT_SIGNER_NODE_ID: &str = "test.module.release.signer";

fn signed_test_artifact_identity(wasm_hash: &str) -> ModuleArtifactIdentity {
    let source_hash = sha256_hex(format!("test-src:{wasm_hash}").as_bytes());
    let build_manifest_hash = sha256_hex(b"test-build-manifest-v1");
    let payload = ModuleArtifactIdentity::signing_payload_v1(
        wasm_hash,
        source_hash.as_str(),
        build_manifest_hash.as_str(),
        TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
    );
    let signing_key = test_module_artifact_signing_key();
    let signature = signing_key.sign(payload.as_slice());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
        signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
        artifact_signature: format!(
            "{}{}",
            ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(signature.to_bytes())
        ),
    }
}

fn load_world_head_proof(
    store: &LocalCasStore,
    record: &ExecutionBridgeRecord,
) -> WorldHeadProofV1 {
    let proof_ref = record
        .world_head_proof_ref
        .as_deref()
        .expect("record should include world head proof ref");
    let bytes = store.get_verified(proof_ref).expect("load proof blob");
    let proof: WorldHeadProofV1 = serde_cbor::from_slice(bytes.as_slice()).expect("decode proof");
    proof.validate_contract().expect("validate proof");
    let proof_hash = proof.proof_hash().expect("proof hash");
    assert_eq!(
        record.world_head_proof_hash.as_deref(),
        Some(proof_hash.as_str())
    );
    proof
}

fn test_module_artifact_signing_key() -> SigningKey {
    let seed_bytes = sha256_bytes(b"oasis7-test-module-artifact-signer-v1");
    SigningKey::from_bytes(&seed_bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha256_bytes(bytes))
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    static NEXT_PATH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT_PATH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-execution-{prefix}-{}-{unique}-{sequence}",
        std::process::id()
    ))
}

#[test]
fn concurrent_same_prefix_temp_paths_are_unique_without_creating_directories() {
    const WORKERS: usize = 8;
    const PATHS_PER_WORKER: usize = 1024;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(WORKERS));
    let workers: Vec<_> = (0..WORKERS)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (0..PATHS_PER_WORKER)
                    .map(|_| temp_dir("concurrent-path-uniqueness"))
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let paths: Vec<_> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("path allocation worker"))
        .collect();
    let unique: std::collections::BTreeSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), WORKERS * PATHS_PER_WORKER);
    assert!(paths.iter().all(|path| !path.exists()));
}

fn sample_snapshot(committed_height: u64, block_hash: Option<&str>) -> NodeSnapshot {
    NodeSnapshot {
        node_id: "node-a".to_string(),
        player_id: "node-a".to_string(),
        world_id: "w1".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: true,
        tick_count: 10,
        last_tick_unix_ms: Some(10),
        consensus: NodeConsensusSnapshot {
            committed_height,
            last_block_hash: block_hash.map(ToOwned::to_owned),
            ..NodeConsensusSnapshot::default()
        },
        consensus_progress_observer_error: None,
        last_error: None,
    }
}

#[test]
fn product_validation_intent_marker_first_crash_window_reconciles_predecessor() {
    let dir = temp_dir("product-validation-intent-marker-first");
    let state_path = dir.join("state.json");
    let world_dir = dir.join("world");
    let records_dir = dir.join("records");
    let storage_root = dir.join("store");
    let world = oasis7::runtime_host::world::new();
    persist_execution_world(world_dir.as_path(), &world).expect("persist predecessor world");
    let action_root = "action-root-1".to_string();
    let context = NodeExecutionCommitContext {
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
    };
    let effect =
        build_execution_external_effect_materialization_with_pre_step_root(&world, &context, None)
            .expect("build pre-step effect");
    let marker = ProductValidationIntentMarkerV1 {
        schema_version: super::product_validation_intent::PRODUCT_VALIDATION_INTENT_SCHEMA_V1,
        world_id: "w1".to_string(),
        height: 1,
        action_root,
        // The staged world would contain the attempt journal entry. It is
        // intentionally absent here: this models a crash after marker write
        // and before world publication.
        journal_len: 1,
        pre_step_execution_state_root: execution_world_snapshot_root(&world)
            .expect("predecessor root"),
        controlled_preparation_ref: None,
        pre_step_external_effect: Some(effect.clone()),
        staged_execution_state_root: String::new(),
        previous_staged_execution_state_root: None,
        previous_staged_journal_len: None,
    };
    persist_product_validation_intent(records_dir.as_path(), &marker)
        .expect("persist marker before staged world");
    let loaded_marker = load_product_validation_intent(records_dir.as_path())
        .expect("load marker")
        .expect("marker exists");
    assert_eq!(loaded_marker.pre_step_external_effect, Some(effect));

    let driver =
        NodeRuntimeExecutionDriver::new(state_path, world_dir, records_dir.clone(), storage_root)
            .expect("restart reconciles marker-only crash window");
    assert!(driver.pending_product_validation_intent.is_none());
    assert!(!records_dir.join("product-validation-intent.json").exists());
    assert_eq!(driver.execution_world.state().time, 0);
    assert_eq!(driver.execution_world.journal().len(), 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn product_validation_intent_roundtrip_preserves_external_effect_cas_bytes() {
    let dir = temp_dir("product-validation-intent-evidence");
    let world = oasis7::runtime_host::world::new();
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let context = NodeExecutionCommitContext {
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
    };
    let uninterrupted =
        build_execution_external_effect_materialization_with_pre_step_root(&world, &context, None)
            .expect("build uninterrupted effect");
    validate_execution_external_effect_for_context(&uninterrupted, &context)
        .expect("validate uninterrupted effect");

    let records_dir = dir.join("records");
    let marker = ProductValidationIntentMarkerV1 {
        schema_version: super::product_validation_intent::PRODUCT_VALIDATION_INTENT_SCHEMA_V1,
        world_id: context.world_id.clone(),
        height: context.height,
        action_root: context.action_root.clone(),
        journal_len: 1,
        pre_step_execution_state_root: uninterrupted.pre_step_execution_state_root.clone(),
        controlled_preparation_ref: None,
        pre_step_external_effect: Some(uninterrupted.clone()),
        staged_execution_state_root: String::new(),
        previous_staged_execution_state_root: None,
        previous_staged_journal_len: None,
    };
    persist_product_validation_intent(records_dir.as_path(), &marker)
        .expect("persist complete intent");
    let recovered = load_product_validation_intent(records_dir.as_path())
        .expect("load complete intent")
        .expect("complete marker")
        .pre_step_external_effect
        .expect("pre-step effect");
    validate_execution_external_effect_for_context(&recovered, &context)
        .expect("validate recovered effect");
    assert_eq!(recovered, uninterrupted);

    let uninterrupted_store = LocalCasStore::new(dir.join("uninterrupted-store"));
    let recovered_store = LocalCasStore::new(dir.join("recovered-store"));
    let uninterrupted_ref =
        persist_execution_external_effect_materialization(&uninterrupted_store, &uninterrupted)
            .expect("persist uninterrupted effect");
    let recovered_ref =
        persist_execution_external_effect_materialization(&recovered_store, &recovered)
            .expect("persist recovered effect");
    assert_eq!(recovered_ref, uninterrupted_ref);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn product_validation_intent_recognizes_previous_same_height_generation() {
    let predecessor = oasis7::runtime_host::world::new();
    let mut previous_generation = predecessor.clone();
    previous_generation
        .step()
        .expect("stage first validation generation");
    let previous_root = execution_world_snapshot_root(&previous_generation)
        .expect("hash previous same-height generation");
    let marker = ProductValidationIntentMarkerV1 {
        schema_version: super::product_validation_intent::PRODUCT_VALIDATION_INTENT_SCHEMA_V1,
        world_id: "w1".to_string(),
        height: 1,
        action_root: "action-root-1".to_string(),
        journal_len: previous_generation.journal().len().saturating_add(1),
        pre_step_execution_state_root: execution_world_snapshot_root(&predecessor)
            .expect("predecessor root"),
        controlled_preparation_ref: None,
        pre_step_external_effect: None,
        staged_execution_state_root: "newer-generation-root".to_string(),
        previous_staged_execution_state_root: Some(previous_root),
        previous_staged_journal_len: Some(previous_generation.journal().len()),
    };

    assert!(
        world_is_staged_for_product_validation_intent(&previous_generation, &marker, 0)
            .expect("recognize prior same-height generation")
    );
}

#[test]
fn bridge_committed_heights_persists_records_and_state() {
    let dir = temp_dir("execution-bridge");
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

    let snapshot = sample_snapshot(2, Some("node-h2"));
    let records = bridge_committed_heights(
        &snapshot,
        1_000,
        &mut world,
        &mut sandbox,
        &store,
        records_dir.as_path(),
        &mut state,
    )
    .expect("bridge");

    assert_eq!(records.len(), 2);
    assert_eq!(state.last_applied_committed_height, 2);
    assert_eq!(state.last_node_block_hash.as_deref(), Some("node-h2"));
    assert!(records_dir.join("00000000000000000001.json").exists());
    assert!(records_dir.join("00000000000000000002.json").exists());
    assert!(records_dir.join("latest.json").exists());

    let latest_bytes = fs::read(records_dir.join("latest.json")).expect("read latest record");
    let latest_record: ExecutionBridgeRecord =
        serde_json::from_slice(latest_bytes.as_slice()).expect("parse latest record");
    assert_eq!(
        latest_record.schema_version,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3
    );
    assert_eq!(
        latest_record.latest_state_ref.as_deref(),
        latest_record.snapshot_ref.as_deref()
    );
    assert!(latest_record.commit_log_ref.is_none());
    assert!(latest_record.checkpoint_ref.is_none());
    assert!(latest_record.external_effect_ref.is_none());

    let latest_json: serde_json::Value =
        serde_json::from_slice(latest_bytes.as_slice()).expect("parse latest json");
    assert!(latest_json.get("schema_version").is_some());
    assert!(latest_json.get("latest_state_ref").is_some());
    assert!(latest_json.get("commit_log_ref").is_none());
    assert!(latest_json.get("checkpoint_ref").is_none());
    assert!(latest_json.get("external_effect_ref").is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn bridge_committed_heights_is_noop_when_height_not_advanced() {
    let dir = temp_dir("execution-bridge-noop");
    let store = LocalCasStore::new(dir.join("store"));
    let mut world = oasis7::runtime_host::world::new();
    let mut sandbox = FixedSandbox::succeed(ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    });
    let mut state = ExecutionBridgeState {
        last_applied_committed_height: 3,
        last_execution_block_hash: Some("h3".to_string()),
        last_execution_state_root: Some("s3".to_string()),
        last_node_block_hash: Some("node-h3".to_string()),
    };

    let snapshot = sample_snapshot(3, Some("node-h3"));
    let records = bridge_committed_heights(
        &snapshot,
        1_100,
        &mut world,
        &mut sandbox,
        &store,
        dir.join("records").as_path(),
        &mut state,
    )
    .expect("bridge");

    assert!(records.is_empty());
    assert_eq!(state.last_applied_committed_height, 3);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_state_roundtrip() {
    let dir = temp_dir("execution-bridge-state");
    let state_path = dir.join("state.json");
    let state = ExecutionBridgeState {
        last_applied_committed_height: 9,
        last_execution_block_hash: Some("exec-h9".to_string()),
        last_execution_state_root: Some("exec-s9".to_string()),
        last_node_block_hash: Some("node-h9".to_string()),
    };

    persist_execution_bridge_state(state_path.as_path(), &state).expect("persist");
    let loaded = load_execution_bridge_state(state_path.as_path()).expect("load");
    assert_eq!(loaded, state);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_record_legacy_payload_defaults_latest_state_ref() {
    let legacy = serde_json::json!({
        "world_id": "w1",
        "height": 7,
        "node_block_hash": "node-h7",
        "execution_block_hash": "exec-h7",
        "execution_state_root": "state-r7",
        "journal_len": 3,
        "snapshot_ref": "cas:snapshot-7",
        "journal_ref": "cas:journal-7",
        "timestamp_ms": 7000
    });
    let record: ExecutionBridgeRecord =
        serde_json::from_value(legacy).expect("parse legacy execution bridge record");

    assert_eq!(record.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V1);
    assert_eq!(record.latest_state_ref.as_deref(), Some("cas:snapshot-7"));
    assert_eq!(record.snapshot_ref.as_deref(), Some("cas:snapshot-7"));
    assert_eq!(record.journal_ref.as_deref(), Some("cas:journal-7"));
    assert!(record.commit_log_ref.is_none());
    assert!(record.checkpoint_ref.is_none());
    assert!(record.external_effect_ref.is_none());
}

#[test]
fn execution_bridge_record_recovery_snapshot_ref_falls_back_to_execution_state_root() {
    let malformed_v2 = serde_json::json!({
        "schema_version": 2,
        "world_id": "w1",
        "height": 1,
        "node_block_hash": "node-h1",
        "execution_block_hash": "exec-h1",
        "execution_state_root": "state-r1",
        "journal_len": 1,
        "external_effect_ref": "cas:effect-1",
        "timestamp_ms": 1000
    });
    let record: ExecutionBridgeRecord =
        serde_json::from_value(malformed_v2).expect("parse malformed v2 execution bridge record");

    assert!(record.latest_state_ref.is_none());
    assert!(record.snapshot_ref.is_none());
    assert!(record.journal_ref.is_none());
    assert_eq!(record.recovery_snapshot_ref(), Some("state-r1"));
}

#[test]
fn persist_execution_bridge_record_only_migrates_legacy_record_to_v3() {
    let dir = temp_dir("execution-bridge-legacy-migrate");
    let records_dir = dir.join("records");
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");
    let legacy = serde_json::json!({
        "world_id": "w1",
        "height": 7,
        "node_block_hash": "node-h7",
        "execution_block_hash": "exec-h7",
        "execution_state_root": "state-r7",
        "journal_len": 7,
        "snapshot_ref": "cas:snapshot-7",
        "journal_ref": "cas:journal-7",
        "timestamp_ms": 7000
    });
    let legacy_bytes = serde_json::to_vec_pretty(&legacy).expect("serialize legacy record");
    crate::write_bytes_atomic(
        execution_bridge_record_path(records_dir.as_path(), 7).as_path(),
        legacy_bytes.as_slice(),
    )
    .expect("persist legacy record");

    let record = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 7).as_path(),
    )
    .expect("load legacy record");
    assert_eq!(record.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V1);
    persist_execution_bridge_record_only(records_dir.as_path(), &record)
        .expect("rewrite legacy record as v3");

    let migrated = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 7).as_path(),
    )
    .expect("load migrated record");
    assert_eq!(migrated.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V3);
    assert_eq!(migrated.latest_state_ref.as_deref(), Some("cas:snapshot-7"));
    assert_eq!(migrated.snapshot_ref.as_deref(), Some("cas:snapshot-7"));
    assert_eq!(migrated.journal_ref.as_deref(), Some("cas:journal-7"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_bridge_retention_maintenance_skips_aggressive_sweep_for_legacy_records() {
    let dir = temp_dir("execution-bridge-legacy-safe-mode");
    let records_dir = dir.join("records");
    let store = LocalCasStore::new(dir.join("store"));
    fs::create_dir_all(records_dir.as_path()).expect("create records dir");

    let snapshot_ref = store
        .put_bytes(b"legacy-snapshot")
        .expect("store legacy snapshot");
    let journal_ref = store
        .put_bytes(b"legacy-journal")
        .expect("store legacy journal");
    let legacy = serde_json::json!({
        "world_id": "w1",
        "height": 1,
        "node_block_hash": "node-h1",
        "execution_block_hash": "exec-h1",
        "execution_state_root": "state-r1",
        "journal_len": 1,
        "snapshot_ref": snapshot_ref,
        "journal_ref": journal_ref,
        "timestamp_ms": 1000
    });
    let legacy_bytes = serde_json::to_vec_pretty(&legacy).expect("serialize legacy record");
    crate::write_bytes_atomic(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
        legacy_bytes.as_slice(),
    )
    .expect("persist legacy record");
    crate::write_bytes_atomic(
        records_dir.join("latest.json").as_path(),
        legacy_bytes.as_slice(),
    )
    .expect("persist legacy latest pointer");
    crate::write_bytes_atomic(
        records_dir
            .join(crate::EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER)
            .as_path(),
        b"interrupted legacy retention\n",
    )
    .expect("persist retention degraded marker");

    let freed_bytes = run_execution_bridge_retention_maintenance(records_dir.as_path(), &store, 1)
        .expect("run retention maintenance");
    assert_eq!(freed_bytes, 0);
    let record = load_execution_bridge_record(
        execution_bridge_record_path(records_dir.as_path(), 1).as_path(),
    )
    .expect("load legacy record after maintenance");
    assert_eq!(record.schema_version, EXECUTION_BRIDGE_RECORD_SCHEMA_V1);
    assert!(
        !records_dir
            .join(crate::EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER)
            .exists(),
        "successful legacy-safe reconciliation must clear the durable degraded marker"
    );
    assert!(record.snapshot_ref.is_some());
    assert!(record.journal_ref.is_some());
    assert!(
        store
            .has(record.snapshot_ref.as_deref().expect("legacy snapshot ref"))
            .expect("legacy snapshot still exists")
    );
    assert!(
        store
            .has(record.journal_ref.as_deref().expect("legacy journal ref"))
            .expect("legacy journal still exists")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_checkpoint_manifest_roundtrip_updates_latest_pointer() {
    let dir = temp_dir("execution-checkpoint-manifest");
    let records_dir = dir.join("records");
    let manifest = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        12,
        "exec-h12".to_string(),
        "state-r12".to_string(),
        "cas:snapshot-12".to_string(),
        Some("cas:snapshot-12".to_string()),
        Some("cas:journal-12".to_string()),
        12_000,
    )
    .expect("build manifest");

    persist_execution_checkpoint_manifest(records_dir.as_path(), &manifest)
        .expect("persist manifest");
    let loaded = load_execution_checkpoint_manifest(
        execution_checkpoint_manifest_path(records_dir.as_path(), 12).as_path(),
    )
    .expect("load manifest");
    let latest = load_latest_execution_checkpoint_manifest(records_dir.as_path())
        .expect("load latest manifest")
        .expect("latest manifest should exist");

    assert_eq!(loaded, manifest);
    assert_eq!(latest, manifest);
    assert_eq!(
        latest.pinned_refs,
        vec!["cas:journal-12".to_string(), "cas:snapshot-12".to_string()]
    );
    assert!(execution_checkpoint_latest_path(records_dir.as_path()).exists());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn execution_checkpoint_manifest_rejects_tampered_latest_pointer() {
    let dir = temp_dir("execution-checkpoint-manifest-tamper");
    let records_dir = dir.join("records");
    let manifest = ExecutionCheckpointManifest::new(
        "w1".to_string(),
        4,
        "exec-h4".to_string(),
        "state-r4".to_string(),
        "cas:snapshot-4".to_string(),
        Some("cas:snapshot-4".to_string()),
        Some("cas:journal-4".to_string()),
        4_000,
    )
    .expect("build manifest");
    persist_execution_checkpoint_manifest(records_dir.as_path(), &manifest)
        .expect("persist manifest");

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

    let err = load_latest_execution_checkpoint_manifest(records_dir.as_path())
        .expect_err("tampered latest pointer should fail");
    assert!(
        err.contains("hash mismatch"),
        "unexpected latest pointer error: {err}"
    );

    let _ = fs::remove_dir_all(dir);
}

fn persist_test_execution_record(
    records_dir: &Path,
    height: u64,
    block_hash: &str,
) -> ExecutionBridgeRecord {
    let record = ExecutionBridgeRecord::new_v2(
        "w1".to_string(),
        height,
        Some(format!("node-h{height}")),
        block_hash.to_string(),
        format!("state-r{height}"),
        height as usize,
        format!("cas:snapshot-{height}"),
        format!("cas:journal-{height}"),
        None,
        None,
        height as i64 * 1_000,
    );
    persist_execution_bridge_record(records_dir, &record).expect("persist test execution record");
    record
}

fn persist_test_execution_record_with_store_refs(
    records_dir: &Path,
    store: &LocalCasStore,
    height: u64,
) -> ExecutionBridgeRecord {
    let snapshot_ref = store
        .put_bytes(format!("record-snapshot-{height}").as_bytes())
        .expect("store snapshot");
    let journal_ref = store
        .put_bytes(format!("record-journal-{height}").as_bytes())
        .expect("store journal");
    let simulator_snapshot_ref = store
        .put_bytes(format!("simulator-snapshot-{height}").as_bytes())
        .expect("store simulator snapshot");
    let simulator_journal_ref = store
        .put_bytes(format!("simulator-journal-{height}").as_bytes())
        .expect("store simulator journal");
    let external_effect_ref =
        persist_test_external_effect(store, "w1", height, format!("node-h{height}").as_str());
    let record = ExecutionBridgeRecord {
        latest_state_ref: Some(snapshot_ref.clone()),
        snapshot_ref: Some(snapshot_ref),
        journal_ref: Some(journal_ref),
        external_effect_ref: Some(external_effect_ref),
        simulator_mirror: Some(ExecutionSimulatorMirrorRecord {
            action_count: height as usize,
            rejected_action_count: 0,
            journal_len: height as usize,
            snapshot_ref: simulator_snapshot_ref,
            journal_ref: simulator_journal_ref,
            state_root: format!("simulator-state-{height}"),
        }),
        ..ExecutionBridgeRecord::new_v2(
            "w1".to_string(),
            height,
            Some(format!("node-h{height}")),
            format!("exec-h{height}"),
            format!("state-root-{height}"),
            height as usize,
            "placeholder-snapshot".to_string(),
            "placeholder-journal".to_string(),
            None,
            None,
            height as i64 * 1_000,
        )
    };
    persist_execution_bridge_record(records_dir, &record)
        .expect("persist test execution record with store refs");
    record
}

fn persist_test_external_effect(
    store: &LocalCasStore,
    world_id: &str,
    height: u64,
    node_block_hash: &str,
) -> String {
    let materialization = ExecutionExternalEffectMaterialization {
        schema_version: EXECUTION_EXTERNAL_EFFECT_SCHEMA_V1,
        contract: EXECUTION_EXTERNAL_EFFECT_CONTRACT_CLOSED_WORLD_V1.to_string(),
        world_id: world_id.to_string(),
        node_id: "node-a".to_string(),
        height,
        slot: height.saturating_sub(1),
        epoch: 0,
        node_block_hash: node_block_hash.to_string(),
        action_root: format!("action-root-{height}"),
        committed_at_unix_ms: height as i64 * 1_000,
        pre_step_execution_state_root: format!("pre-step-state-{height}"),
        world_manifest_hash: format!("manifest-hash-{height}"),
        active_modules_hash: execution_module_anchor_hash(&[]).expect("empty module hash"),
        committed_actions_hash: execution_committed_actions_hash(&[]).expect("empty actions hash"),
        active_modules: Vec::new(),
        committed_actions: Vec::new(),
        unresolved_inputs: Vec::new(),
    };
    persist_execution_external_effect_materialization(store, &materialization)
        .expect("persist test external effect")
}

mod controlled_history;
