use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use oasis7_node::{NodeConsensusSnapshot, NodeRole, NodeSnapshot, ReplicationNetworkDebugSnapshot};
use sha2::{Digest, Sha256};

use super::execution_bridge::ExecutionCheckpointStatusEvidence;
use super::feedback_submit_api::FeedbackSubmitSigner;
use super::rebuild_status::{
    RebuildStatusResponse, build_rebuild_status_with_signer, verify_rebuild_proof,
    verify_rebuild_proof_file,
};

#[test]
fn rebuild_status_is_bounded_and_contains_rebuild_proof_fields_only() {
    let snapshot = snapshot();
    let network = ReplicationNetworkDebugSnapshot {
        local_peer_id: "12D3KooWLocal".to_string(),
        connected_peers: vec!["12D3KooWPeer".to_string()],
        peer_healths: Vec::new(),
        registered_protocols: Vec::new(),
        protocol_retry_cooldown_peers: Default::default(),
        transport_retry_cooldown_peers: Vec::new(),
        request_peer_scores: Default::default(),
        recent_errors: Vec::new(),
    };
    let response =
        build_rebuild_status_with_signer(snapshot, network, None, 1_000, None, &signer())
            .expect("bounded proof response");
    let json = serde_json::to_value(&response).expect("serialize bounded response");
    for key in [
        "schema_version",
        "ok",
        "liveness",
        "readiness",
        "heights",
        "network_head",
        "checkpoint",
        "local_peer_id",
        "connected_peers",
    ] {
        assert!(json.get(key).is_some(), "missing bounded field {key}");
    }
    assert!(json.get("consensus").is_none());
    assert!(json.get("observability").is_none());
    assert_eq!(response.local_peer_id, "12D3KooWLocal");
    assert_eq!(response.connected_peers, vec!["12D3KooWPeer"]);
    assert!(json.get("proof").is_some());
    assert!(json.get("identity").is_none());
    assert!(json.get("signature").is_none());
    verify_rebuild_proof(&response).expect("proof envelope verifies");
}

#[test]
fn rebuild_status_marks_stopped_runtime_not_ready_without_full_status_builder() {
    let response = build_rebuild_status_with_signer(
        snapshot(),
        ReplicationNetworkDebugSnapshot {
            local_peer_id: "local".to_string(),
            connected_peers: Vec::new(),
            peer_healths: Vec::new(),
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: Default::default(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: Default::default(),
            recent_errors: Vec::new(),
        },
        Some(ExecutionCheckpointStatusEvidence {
            schema_version: 2,
            checkpoint_id: "checkpoint-1".to_string(),
            world_id: "world".to_string(),
            height: 1,
            execution_block_hash: "exec-1".to_string(),
            execution_state_root: "state-1".to_string(),
            manifest_hash: "manifest".to_string(),
        }),
        1_000,
        None,
        &signer(),
    )
    .expect("bounded proof response");
    assert!(!response.ok);
    assert!(!response.liveness.running);
    assert_eq!(response.readiness.status, "not_ready");
    assert_eq!(response.checkpoint.unwrap().height, 1);
}

#[allow(dead_code)]
fn _type_check(_: RebuildStatusResponse) {}

#[test]
fn rebuild_status_rejects_checkpoint_head_mismatch_and_signs_not_ready_proof() {
    let mut snapshot = snapshot();
    snapshot.running = true;
    snapshot.world_id = "world-a".to_string();
    snapshot.consensus.committed_height = 42;
    snapshot.consensus.network_committed_height = 42;
    snapshot.consensus.last_execution_height = 42;
    snapshot.consensus.last_execution_block_hash = Some("exec-42".to_string());
    snapshot.consensus.last_execution_state_root = Some("state-42".to_string());
    let checkpoint = ExecutionCheckpointStatusEvidence {
        schema_version: 1,
        checkpoint_id: "checkpoint-42".to_string(),
        world_id: "world-a".to_string(),
        height: 42,
        execution_block_hash: "tampered-exec-42".to_string(),
        execution_state_root: "state-42".to_string(),
        manifest_hash: "manifest-42".to_string(),
    };
    let response = build_rebuild_status_with_signer(
        snapshot,
        network(),
        Some(checkpoint),
        1_000,
        None,
        &signer(),
    )
    .expect("proof response");
    assert!(!response.ok);
    assert_eq!(response.readiness.status, "not_ready");
    assert!(
        response
            .readiness
            .failed_gates
            .iter()
            .any(|gate| gate == "checkpoint_head_mismatch")
    );
    verify_rebuild_proof(&response).expect("not-ready proof still verifies");
}

#[test]
fn rebuild_status_accepts_retained_checkpoint_below_current_execution_head() {
    let mut snapshot = snapshot();
    snapshot.running = true;
    snapshot.world_id = "world-a".to_string();
    snapshot.consensus.committed_height = 64;
    snapshot.consensus.network_committed_height = 64;
    snapshot.consensus.last_execution_height = 64;
    snapshot.consensus.last_execution_block_hash = Some("exec-64".to_string());
    snapshot.consensus.last_execution_state_root = Some("state-64".to_string());
    let checkpoint = ExecutionCheckpointStatusEvidence {
        schema_version: 1,
        checkpoint_id: "checkpoint-42".to_string(),
        world_id: "world-a".to_string(),
        height: 42,
        execution_block_hash: "exec-42".to_string(),
        execution_state_root: "state-42".to_string(),
        manifest_hash: "manifest-42".to_string(),
    };
    let response = build_rebuild_status_with_signer(
        snapshot,
        network(),
        Some(checkpoint),
        1_000,
        None,
        &signer(),
    )
    .expect("proof response");
    assert!(
        response.ok,
        "retained checkpoint should be valid below head"
    );
    assert_eq!(
        response.checkpoint.as_ref().map(|item| item.height),
        Some(42)
    );
    verify_rebuild_proof(&response).expect("retained-boundary proof verifies");
}

#[test]
fn rebuild_proof_file_verifier_binds_expected_signer_and_public_key() {
    let response =
        build_rebuild_status_with_signer(snapshot(), network(), None, 1_000, None, &signer())
            .expect("bounded proof response");
    let path = temp_proof_path();
    fs::write(
        path.as_path(),
        serde_json::to_vec(&response).expect("serialize proof response"),
    )
    .expect("write proof response");
    verify_rebuild_proof_file(
        path.as_path(),
        response.proof.signer_id.as_str(),
        response.proof.signer_public_key_hex.as_str(),
    )
    .expect("trusted proof file verifies");
    let err = verify_rebuild_proof_file(
        path.as_path(),
        "unexpected-signer",
        response.proof.signer_public_key_hex.as_str(),
    )
    .expect_err("unexpected signer must fail closed");
    assert!(err.contains("trusted signer id mismatch"), "{err}");
    let mut tampered = serde_json::to_value(&response).expect("serialize tampered proof");
    tampered["proof"]["signature_hex"] = serde_json::Value::String("00".repeat(64));
    fs::write(
        path.as_path(),
        serde_json::to_vec(&tampered).expect("serialize tampered proof value"),
    )
    .expect("write tampered proof");
    let err = verify_rebuild_proof_file(
        path.as_path(),
        response.proof.signer_id.as_str(),
        response.proof.signer_public_key_hex.as_str(),
    )
    .expect_err("tampered signature must fail closed");
    assert!(err.contains("signature verification failed"), "{err}");
}

#[test]
fn rebuild_proof_verification_receipt_binds_peer_and_exact_raw_proof_digest() {
    let mut snapshot = snapshot();
    snapshot.node_id = "sequencer-node".to_string();
    let response =
        build_rebuild_status_with_signer(snapshot, network(), None, 1_000, None, &signer())
            .expect("bounded proof response");
    let path = temp_proof_path();
    let bytes = serde_json::to_vec(&response).expect("serialize proof response");
    fs::write(path.as_path(), bytes.as_slice()).expect("write proof response");
    let receipt = verify_rebuild_proof_file(
        path.as_path(),
        response.proof.signer_id.as_str(),
        response.proof.signer_public_key_hex.as_str(),
    )
    .expect("trusted proof file verifies");
    assert_eq!(receipt.local_peer_id, response.local_peer_id);
    assert_eq!(
        receipt.proof_sha256,
        hex::encode(Sha256::digest(bytes.as_slice()))
    );
    assert_ne!(
        receipt.proof_sha256,
        hex::encode(Sha256::digest(b"different-proof"))
    );
}

#[test]
fn rebuild_proof_verification_receipt_rejects_tampered_or_wrong_raw_proof_binding() {
    let response_a =
        build_rebuild_status_with_signer(snapshot(), network(), None, 1_000, None, &signer())
            .expect("first bounded proof response");
    let response_b =
        build_rebuild_status_with_signer(snapshot(), network(), None, 2_000, None, &signer())
            .expect("second bounded proof response");
    let path_a = temp_proof_path();
    let path_b = temp_proof_path();
    let bytes_a = serde_json::to_vec(&response_a).expect("serialize first proof");
    let bytes_b = serde_json::to_vec(&response_b).expect("serialize second proof");
    fs::write(path_a.as_path(), bytes_a.as_slice()).expect("write first proof");
    fs::write(path_b.as_path(), bytes_b.as_slice()).expect("write second proof");
    let receipt_a = verify_rebuild_proof_file(
        path_a.as_path(),
        response_a.proof.signer_id.as_str(),
        response_a.proof.signer_public_key_hex.as_str(),
    )
    .expect("first proof verifies");
    let receipt_b = verify_rebuild_proof_file(
        path_b.as_path(),
        response_b.proof.signer_id.as_str(),
        response_b.proof.signer_public_key_hex.as_str(),
    )
    .expect("second proof verifies");
    assert_ne!(receipt_a.proof_sha256, receipt_b.proof_sha256);
    let tampered = String::from_utf8(bytes_b)
        .expect("proof JSON is UTF-8")
        .replace(
            "\"observed_at_unix_ms\":2000",
            "\"observed_at_unix_ms\":2001",
        )
        .into_bytes();
    fs::write(path_b.as_path(), tampered).expect("write tampered second proof");
    assert!(
        verify_rebuild_proof_file(
            path_b.as_path(),
            response_b.proof.signer_id.as_str(),
            response_b.proof.signer_public_key_hex.as_str(),
        )
        .is_err()
    );
}

static TEMP_PROOF_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempProofPath {
    path: PathBuf,
}

impl TempProofPath {
    fn as_path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TempProofPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn temp_proof_path() -> TempProofPath {
    loop {
        let sequence = TEMP_PROOF_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-rebuild-proof-{}-{sequence}.json",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return TempProofPath { path },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!(
                "create unique temporary proof path {}: {error}",
                path.display()
            ),
        }
    }
}

#[test]
fn temp_proof_paths_are_isolated_under_parallel_allocation() {
    const PATH_COUNT: usize = 64;
    let paths: Vec<TempProofPath> = std::thread::scope(|scope| {
        let handles = (0..PATH_COUNT)
            .map(|_| {
                scope.spawn(|| {
                    let path = temp_proof_path();
                    fs::write(path.as_path(), b"isolated-proof").expect("write isolated proof");
                    path
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("parallel proof path allocation"))
            .collect()
    });
    let mut unique_paths = paths
        .iter()
        .map(|path| path.as_path().to_path_buf())
        .collect::<Vec<_>>();
    unique_paths.sort();
    unique_paths.dedup();
    assert_eq!(unique_paths.len(), PATH_COUNT);
    for path in paths {
        assert_eq!(
            fs::read(path.as_path()).expect("read isolated proof"),
            b"isolated-proof"
        );
    }
}

fn snapshot() -> NodeSnapshot {
    NodeSnapshot {
        node_id: "node".to_string(),
        player_id: "player".to_string(),
        world_id: "world".to_string(),
        role: NodeRole::Sequencer,
        replication_enabled: false,
        running: false,
        tick_count: 0,
        last_tick_unix_ms: None,
        consensus: NodeConsensusSnapshot::default(),
        consensus_progress_observer_error: None,
        last_error: None,
    }
}

fn network() -> ReplicationNetworkDebugSnapshot {
    ReplicationNetworkDebugSnapshot {
        local_peer_id: "local".to_string(),
        connected_peers: Vec::new(),
        peer_healths: Vec::new(),
        registered_protocols: Vec::new(),
        protocol_retry_cooldown_peers: Default::default(),
        transport_retry_cooldown_peers: Vec::new(),
        request_peer_scores: Default::default(),
        recent_errors: Vec::new(),
    }
}

fn signer() -> FeedbackSubmitSigner {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[7_u8; 32]);
    FeedbackSubmitSigner {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    }
}
