use super::status_payload_tests::{
    build_minimal_status_payload, build_minimal_status_payload_with_storage_root,
};
use oasis7_proto::distributed::{
    BlobRef, ExecutionBindingEvidenceV1, HeadConsensusEvidenceV1, WIRE_ENCODING_CBOR,
    WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1, WORLD_HEAD_PROOF_HASH_DOMAIN_V1,
    WORLD_HEAD_PROOF_V1_SCHEMA, WorldBlock, WorldHeadAnnounce, WorldHeadProofV1,
};
use oasis7_runtime::{BlobStore, LocalCasStore};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-status-payload-{prefix}-{unique}"))
}

fn write_chain_proof_fixture(root: &Path) -> (PathBuf, String) {
    let records_dir = root.join("records");
    let storage_root = root.join("store");
    let proof_ref = write_chain_proof_fixture_at(records_dir.as_path(), storage_root.as_path());
    (records_dir, proof_ref)
}

fn write_chain_proof_fixture_at(records_dir: &Path, storage_root: &Path) -> String {
    fs::create_dir_all(records_dir).expect("create records dir");
    let store = LocalCasStore::new(storage_root);
    let timestamp_ms = 1_700_000_000_000;
    let block = WorldBlock {
        world_id: "live-a".to_string(),
        height: 42,
        prev_block_hash: "node-block-41".to_string(),
        action_root: "action-root-42".to_string(),
        event_root: String::new(),
        state_root: "exec-state-42".to_string(),
        journal_ref: "journal-ref-42".to_string(),
        snapshot_ref: "snapshot-ref-42".to_string(),
        receipts_root: "exec-block-42".to_string(),
        proposer_id: "node-a".to_string(),
        timestamp_ms,
        signature: "runtime_bridge_evidence_only_v1".to_string(),
    };
    let block_hash = oasis7_runtime::blake3_hex(
        serde_cbor::to_vec(&block)
            .expect("encode proof block")
            .as_slice(),
    );
    let proof = WorldHeadProofV1 {
        schema_version: WORLD_HEAD_PROOF_V1_SCHEMA,
        world_id: "live-a".to_string(),
        height: 42,
        timestamp_ms,
        head: WorldHeadAnnounce {
            world_id: "live-a".to_string(),
            height: 42,
            block_hash,
            state_root: "exec-state-42".to_string(),
            timestamp_ms,
            signature: "runtime_bridge_evidence_only_v1".to_string(),
        },
        block,
        snapshot_manifest_ref: BlobRef {
            content_hash: "snapshot-ref-42".to_string(),
            size_bytes: 0,
            codec: WIRE_ENCODING_CBOR.to_string(),
            links: Vec::new(),
        },
        journal_segments_ref: BlobRef {
            content_hash: "journal-ref-42".to_string(),
            size_bytes: 0,
            codec: WIRE_ENCODING_CBOR.to_string(),
            links: Vec::new(),
        },
        consensus: HeadConsensusEvidenceV1 {
            consensus_status: "committed".to_string(),
            proposer_id: "node-a".to_string(),
            quorum_threshold: 0,
            validator_count: 0,
            vote_count: 0,
            approver_ids: Vec::new(),
            evidence_hash: "node-block-42".to_string(),
        },
        execution: ExecutionBindingEvidenceV1 {
            execution_height: 42,
            node_block_hash: "node-block-42".to_string(),
            execution_block_hash: "exec-block-42".to_string(),
            execution_state_root: "exec-state-42".to_string(),
            action_root: "action-root-42".to_string(),
        },
        checkpoint: None,
        claim_boundary: WORLD_HEAD_PROOF_CLAIM_BOUNDARY_V1.to_string(),
    };
    proof.validate_contract().expect("validate proof fixture");
    let proof_bytes = serde_cbor::to_vec(&proof).expect("encode proof fixture");
    let proof_ref = store
        .put_bytes(proof_bytes.as_slice())
        .expect("persist world head proof fixture");
    let proof_hash = oasis7_runtime::blake3_hex(
        serde_cbor::to_vec(&(WORLD_HEAD_PROOF_HASH_DOMAIN_V1, &proof))
            .expect("encode proof hash payload")
            .as_slice(),
    );
    fs::write(
        records_dir.join("latest.json"),
        serde_json::json!({
            "schema_version": 3,
            "world_id": "live-a",
            "height": 42,
            "node_block_hash": "node-block-42",
            "action_root": "action-root-42",
            "execution_block_hash": "exec-block-42",
            "execution_state_root": "exec-state-42",
            "world_head_proof_ref": proof_ref,
            "world_head_proof_hash": proof_hash,
        })
        .to_string(),
    )
    .expect("write latest execution record");
    let latest: serde_json::Value = serde_json::from_slice(
        fs::read(records_dir.join("latest.json"))
            .expect("read latest execution record")
            .as_slice(),
    )
    .expect("parse latest execution record");
    latest["world_head_proof_ref"]
        .as_str()
        .expect("fixture world head proof ref")
        .to_string()
}
#[test]
fn status_payload_reports_chain_proof_unavailable_without_records_dir() {
    let payload = build_minimal_status_payload(None);

    assert_eq!(payload.chain_proof.status, "unavailable");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert_eq!(
        payload.chain_proof.load_error.as_deref(),
        Some("execution_records_dir_unconfigured")
    );
    assert!(
        payload
            .chain_proof
            .does_not_claim
            .contains(&"ready_for_live_candidate".to_string())
    );
    assert_ne!(payload.readiness.status, "ready_for_live_candidate");
}

#[test]
fn status_payload_reports_latest_chain_proof_metadata_from_execution_record() {
    let dir = temp_dir("chain-proof-available");
    let (records_dir, proof_ref) = write_chain_proof_fixture(dir.as_path());

    let payload = build_minimal_status_payload(Some(records_dir.as_path()));

    assert_eq!(payload.chain_proof.status, "available");
    assert_eq!(
        payload.chain_proof.schema_version,
        "oasis7.chain_proof_status.v1"
    );
    assert_eq!(payload.chain_proof.proof_contract, "WorldHeadProofV1");
    assert_eq!(
        payload.chain_proof.claim_boundary,
        "head_execution_checkpoint_evidence_only_not_light_client_or_mainnet_readiness"
    );
    let expected_source_record_path = records_dir.join("latest.json").display().to_string();
    assert_eq!(
        payload.chain_proof.source_record_path.as_deref(),
        Some(expected_source_record_path.as_str())
    );
    assert!(payload.chain_proof.load_error.is_none());
    let proof = payload
        .chain_proof
        .latest_world_head_proof
        .as_ref()
        .expect("latest proof metadata");
    assert_eq!(proof.schema_version, 1);
    assert_eq!(proof.world_id, "live-a");
    assert_eq!(proof.height, 42);
    assert_eq!(proof.execution_block_hash, "exec-block-42");
    assert_eq!(proof.execution_state_root, "exec-state-42");
    assert_eq!(proof.node_block_hash, "node-block-42");
    assert_eq!(proof.action_root, "action-root-42");
    assert_eq!(proof.world_head_proof_ref, proof_ref);
    assert_eq!(proof.checkpoint_ref, None);
    assert_ne!(payload.readiness.status, "ready_for_live_candidate");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_validates_chain_proof_from_configured_independent_storage_root() {
    let dir = temp_dir("chain-proof-independent-storage-root");
    let records_dir = dir.join("custom-records");
    let storage_root = dir.join("custom-cas");
    write_chain_proof_fixture_at(records_dir.as_path(), storage_root.as_path());

    let payload = build_minimal_status_payload_with_storage_root(
        Some(records_dir.as_path()),
        Some(storage_root.as_path()),
    );

    assert_eq!(payload.chain_proof.status, "available");
    assert!(payload.chain_proof.latest_world_head_proof.is_some());
    assert!(payload.chain_proof.load_error.is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_rejects_chain_proof_when_configured_storage_root_is_wrong() {
    let dir = temp_dir("chain-proof-wrong-storage-root");
    let (records_dir, _proof_ref) = write_chain_proof_fixture(dir.as_path());
    let wrong_storage_root = dir.join("wrong-store");

    let payload = build_minimal_status_payload_with_storage_root(
        Some(records_dir.as_path()),
        Some(wrong_storage_root.as_path()),
    );

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(payload.chain_proof.load_error.is_some());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_when_cas_blob_is_missing() {
    let dir = temp_dir("chain-proof-missing-blob");
    let (records_dir, _proof_ref) = write_chain_proof_fixture(dir.as_path());
    fs::remove_dir_all(dir.join("store")).expect("remove proof CAS store");

    let payload = build_minimal_status_payload(Some(records_dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("world head proof")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_when_cas_blob_is_corrupt() {
    let dir = temp_dir("chain-proof-corrupt-blob");
    let (records_dir, _proof_ref) = write_chain_proof_fixture(dir.as_path());
    let store = LocalCasStore::new(dir.join("store"));
    let corrupt_ref = store
        .put_bytes(b"not-a-world-head-proof")
        .expect("persist corrupt proof blob");
    let latest_path = records_dir.join("latest.json");
    let mut latest: serde_json::Value = serde_json::from_slice(
        fs::read(latest_path.as_path())
            .expect("read latest record")
            .as_slice(),
    )
    .expect("parse latest record");
    latest["world_head_proof_ref"] = serde_json::Value::String(corrupt_ref);
    fs::write(
        latest_path,
        serde_json::to_vec_pretty(&latest).expect("serialize corrupt latest record"),
    )
    .expect("write corrupt latest record");

    let payload = build_minimal_status_payload(Some(records_dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("decode WorldHeadProofV1")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_when_proof_hash_mismatches_record() {
    let dir = temp_dir("chain-proof-hash-mismatch");
    let (records_dir, _proof_ref) = write_chain_proof_fixture(dir.as_path());
    let latest_path = records_dir.join("latest.json");
    let mut latest: serde_json::Value = serde_json::from_slice(
        fs::read(latest_path.as_path())
            .expect("read latest record")
            .as_slice(),
    )
    .expect("parse latest record");
    latest["world_head_proof_hash"] = serde_json::Value::String("wrong-proof-hash".to_string());
    fs::write(
        latest_path,
        serde_json::to_vec_pretty(&latest).expect("serialize mismatched latest record"),
    )
    .expect("write mismatched latest record");

    let payload = build_minimal_status_payload(Some(records_dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("proof hash mismatch")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_when_proof_binding_mismatches_record() {
    let dir = temp_dir("chain-proof-binding-mismatch");
    let (records_dir, _proof_ref) = write_chain_proof_fixture(dir.as_path());
    let latest_path = records_dir.join("latest.json");
    let mut latest: serde_json::Value = serde_json::from_slice(
        fs::read(latest_path.as_path())
            .expect("read latest record")
            .as_slice(),
    )
    .expect("parse latest record");
    latest["execution_state_root"] = serde_json::Value::String("wrong-state-root".to_string());
    fs::write(
        latest_path,
        serde_json::to_vec_pretty(&latest).expect("serialize mismatched latest record"),
    )
    .expect("write mismatched latest record");

    let payload = build_minimal_status_payload(Some(records_dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("execution state root mismatch")
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_when_pointer_missing() {
    let dir = temp_dir("chain-proof-missing-pointer");
    fs::create_dir_all(dir.as_path()).expect("create records dir");
    fs::write(
        dir.join("latest.json"),
        br#"{
          "schema_version": 3,
          "world_id": "live-a",
          "height": 42,
          "node_block_hash": "node-block-42",
          "action_root": "action-root-42",
          "execution_block_hash": "exec-block-42",
          "execution_state_root": "exec-state-42",
          "world_head_proof_hash": "proof-hash-42"
        }"#,
    )
    .expect("write latest record");

    let payload = build_minimal_status_payload(Some(dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("world_head_proof_ref")
    );
    assert_ne!(payload.readiness.status, "ready_for_live_candidate");

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn status_payload_marks_chain_proof_stale_for_malformed_latest_record() {
    let dir = temp_dir("chain-proof-malformed");
    fs::create_dir_all(dir.as_path()).expect("create records dir");
    fs::write(dir.join("latest.json"), b"{not-json").expect("write malformed latest record");

    let payload = build_minimal_status_payload(Some(dir.as_path()));

    assert_eq!(payload.chain_proof.status, "stale_or_invalid");
    assert!(payload.chain_proof.latest_world_head_proof.is_none());
    assert!(
        payload
            .chain_proof
            .load_error
            .as_deref()
            .unwrap_or_default()
            .contains("parse latest execution record failed")
    );
    assert_ne!(payload.readiness.status, "ready_for_live_candidate");

    let _ = fs::remove_dir_all(dir);
}
