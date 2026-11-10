//! Real Node, signed replication, and real execution across a lost mirror frontier.
use super::*;
use oasis7_node::{NodeConfig, NodeRuntime};
use oasis7_proto::storage_profile::StorageProfileConfig;
use oasis7_runtime::ReleaseSecurityPolicy;
use std::time::{Duration, Instant};

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope {
    version: u8,
    world_id: String,
    node_id: String,
    record: oasis7_distfs::FileReplicationRecord,
    payload: Vec<u8>,
    public_key_hex: Option<String>,
    signature_hex: Option<String>,
}
#[derive(serde::Serialize)]
struct SigningPayload<'a> {
    version: u8,
    world_id: &'a str,
    node_id: &'a str,
    record: &'a oasis7_distfs::FileReplicationRecord,
    payload: &'a [u8],
    public_key_hex: Option<&'a str>,
}
fn authenticate(bytes: &[u8], public: &str, record: &[u8]) {
    let envelope: Envelope = serde_json::from_slice(bytes).unwrap();
    assert_eq!(envelope.public_key_hex.as_deref(), Some(public));
    assert_eq!(
        envelope.record.content_hash,
        oasis7_runtime::blake3_hex(&envelope.payload)
    );
    let signature = ed25519_dalek::Signature::from_slice(
        &hex::decode(envelope.signature_hex.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    let key: [u8; 32] = hex::decode(public).unwrap().try_into().unwrap();
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .unwrap()
        .verify_strict(
            &serde_json::to_vec(&SigningPayload {
                version: envelope.version,
                world_id: &envelope.world_id,
                node_id: &envelope.node_id,
                record: &envelope.record,
                payload: &envelope.payload,
                public_key_hex: envelope.public_key_hex.as_deref(),
            })
            .unwrap(),
            &signature,
        )
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&envelope.payload).unwrap();
    let record: serde_json::Value = serde_json::from_slice(record).unwrap();
    for (body_key, record_key) in [
        ("height", "height"),
        ("block_hash", "node_block_hash"),
        ("action_root", "action_root"),
        ("execution_block_hash", "execution_block_hash"),
        ("execution_state_root", "execution_state_root"),
    ] {
        assert!(
            body[body_key] == record[record_key],
            "signed original binding differs"
        );
    }
}

#[test]
fn node_restart_preserves_signed_retained_commit_when_replication_frontier_lags() {
    let root = temp_dir("node-retained-commit-restart");
    let world_dir = root.join("world");
    let records = root.join("records");
    let replication = root.join("replication");
    let world_id = "retained-restart-world";
    let node_id = "retained-restart-node";
    let mut world = oasis7::runtime_host::world::new();
    world.step().unwrap();
    world
        .bind_cognition_runtime(world_id, "main", 0, None, "pending", 0)
        .unwrap();
    let context_hash = super::super::execution_hash::execution_resource_context_hash(world_id);
    world
        .save_to_dir_with_chain_resource_context(
            &world_dir,
            oasis7_runtime::ChainResourceDerivationContext {
                world_id,
                chain_id: world_id,
                genesis_ref: Some("retained-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: None,
                tick: 1,
            },
            context_hash.clone(),
            context_hash,
        )
        .unwrap();
    fs::write(
        world_dir.join("world-service-identity.json"),
        serde_json::to_vec(&oasis7::world_service::WorldIdentity {
            world_id: world_id.into(),
            genesis_digest: "retained-genesis".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let baseline = super::super::local_bootstrap::derive_local_execution_bootstrap(
        &world_dir,
        &records,
        world_id,
        1,
        "retained-setup",
        &ReleaseSecurityPolicy::default(),
    )
    .unwrap();
    let key = SigningKey::from_bytes(&[67; 32]);
    let root_key = crate::node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(key.to_bytes()),
        public_key_hex: hex::encode(key.verifying_key().to_bytes()),
    };
    let signer = crate::derive_node_consensus_signer_keypair(node_id, &root_key).unwrap();
    let config = || {
        let mut config = NodeConfig::new(node_id, world_id, NodeRole::Sequencer)
            .unwrap()
            .with_tick_interval(Duration::from_millis(50))
            .unwrap();
        config.pos_config = config
            .pos_config
            .clone()
            .with_validator_signer_public_keys(
                crate::build_validator_signer_public_keys(
                    &config.pos_config.validators,
                    &root_key,
                    &std::collections::BTreeMap::new(),
                )
                .unwrap(),
            )
            .unwrap();
        config.with_replication(
            crate::build_node_replication_config(
                node_id,
                &replication,
                &root_key,
                &StorageProfileConfig::default(),
                &[],
                &[],
                false,
            )
            .unwrap(),
        )
    };
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        world_dir.clone(),
        records.clone(),
        root.join("store"),
        &StorageProfileConfig::default(),
        baseline.clone(),
    )
    .unwrap();
    let mut node = NodeRuntime::new(config())
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(baseline);
    node.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while node.snapshot().consensus.committed_height < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    node.stop().unwrap();
    assert!(
        node.snapshot().consensus.committed_height >= 3,
        "real Node fixture did not commit"
    );
    assert!(node.snapshot().last_error.is_none());
    let head = super::super::local_bootstrap::derive_service_execution_bootstrap(
        &world_dir,
        &records,
        &root.join("store"),
        world_id,
    )
    .unwrap();
    let height = head.height;
    let original_record_path = execution_bridge_record_path(&records, height);
    let original_record = fs::read(&original_record_path).unwrap();
    let mirror = replication
        .join("replication_commit_messages")
        .join(format!("{height:020}.json"));
    let original_signed = fs::read(&mirror).unwrap();
    authenticate(&original_signed, &signer.public_key_hex, &original_record);
    // Controlled loss of exactly the last hot replication mirror. Preserve its
    // authentic signed bytes outside the active frontier, and preserve Pos state,
    // execution record, proof/CAS, cache and independent service identity intact.
    let saved = root.join("original-signed-commit.json");
    fs::rename(&mirror, &saved).unwrap();
    let proof_ref = load_execution_bridge_record(&original_record_path)
        .unwrap()
        .world_head_proof_ref
        .unwrap();
    let store = LocalCasStore::new(root.join("store"));
    let original_proof = store.get(&proof_ref).unwrap();
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        world_dir,
        records,
        root.join("store"),
        &StorageProfileConfig::default(),
        head.clone(),
    )
    .unwrap();
    let mut restarted = NodeRuntime::new(config())
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(head);
    // The durable indexed signed head is an independent barrier before the
    // lower-frontier reconcile. Do not silently count any startup error as a
    // successful safety observation: require this exact bounded failure class.
    let rejected_missing_indexed_commit = matches!(
        restarted.start(),
        Err(oasis7_node::NodeError::Replication { reason })
            if reason.starts_with("latest commit head index points to missing commit world=")
    );
    assert!(
        rejected_missing_indexed_commit,
        "missing mirror did not fail at the durable indexed signed-head barrier"
    );
    let retained_unchanged = fs::read(&original_record_path).unwrap() == original_record;
    let signed_unchanged = !mirror.exists() || fs::read(&mirror).unwrap() == original_signed;
    assert!(
        retained_unchanged && signed_unchanged,
        "restart replaced an authenticated retained commit after replication frontier rollback"
    );
    assert!(
        store.get(&proof_ref).unwrap() == original_proof,
        "original proof changed"
    );
    // Restoring the exact original signed mirror must admit ordinary restart;
    // safety may block an incomplete frontier but must not reject intact history.
    fs::copy(saved, mirror).unwrap();
    let baseline = super::super::local_bootstrap::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        world_id,
    )
    .unwrap();
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
        &StorageProfileConfig::default(),
        baseline.clone(),
    )
    .unwrap();
    let mut resumed = NodeRuntime::new(config())
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(baseline);
    resumed.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while resumed.snapshot().consensus.committed_height <= height && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    resumed.stop().unwrap();
    assert!(
        resumed.snapshot().consensus.committed_height > height,
        "original recovery did not continue"
    );
    assert!(resumed.snapshot().last_error.is_none());
    fs::remove_dir_all(root).unwrap();
}
