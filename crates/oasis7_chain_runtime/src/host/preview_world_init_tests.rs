//! Owned real Runtime/NodeRuntime tests; no fixture capabilities or provider calls.
use super::*;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::{
    fs,
    sync::{Arc, Barrier},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let path = parent.join(format!("oasis7-preview-init-test-{}", hex::encode(nonce)));
        files::private_dir(&path).unwrap();
        Self(path)
    }
    fn options(&self) -> Options {
        let keys = self.0.join("keys");
        files::private_dir(&keys).unwrap();
        super::super::node_keypair_config::ensure_node_keypair_in_secure_config_dir(&keys).unwrap();
        Options {
            root: self.0.join("preview"),
            key_dir: keys,
            world_id: "preview-world".into(),
            node_id: "preview-node".into(),
            genesis_nonce: "aa".repeat(32),
            status_bind: "127.0.0.1:5121".parse().unwrap(),
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn canonical_artifacts_ignore_object_insertion_order_but_bind_scope() {
    assert_eq!(
        encode(json!({"z":{"b":2,"a":1},"a":0})).unwrap(),
        encode(json!({"a":0,"z":{"a":1,"b":2}})).unwrap()
    );
    assert_ne!(
        blake3_hex(&encode(json!({"world":"one"})).unwrap()),
        blake3_hex(&encode(json!({"world":"two"})).unwrap())
    );
}
#[test]
fn arguments_require_explicit_exact_private_preview_inputs() {
    assert!(parse(["--world-id", "world"].into_iter()).is_err());
    let root = Directory::new();
    let o = root.options();
    let values = vec![
        "--root",
        o.root.to_str().unwrap(),
        "--key-dir",
        o.key_dir.to_str().unwrap(),
        "--world-id",
        "preview-world",
        "--node-id",
        "preview-node",
        "--genesis-nonce",
        &o.genesis_nonce,
        "--status-bind",
        "127.0.0.1:5121",
    ];
    assert!(parse(values.clone().into_iter()).is_ok());
    let mut duplicate = values.clone();
    duplicate.extend(["--world-id", "other"]);
    assert!(parse(duplicate.into_iter()).is_err());
    let mut remote = values.clone();
    *remote.last_mut().unwrap() = "0.0.0.0:5121";
    assert!(parse(remote.into_iter()).is_err());
    let mut wrong = values;
    wrong[5] = "world with space";
    assert!(parse(wrong.into_iter()).is_err());
}
#[test]
fn genuine_commit_cas_publication_and_restart_keep_initial_receipt() {
    let root = Directory::new();
    let o = root.options();
    let receipt = initialize(&o).unwrap();
    assert!(receipt.published_commit["position"].as_u64().unwrap() > receipt.initial_height);
    let artifact = fs::read(o.root.join("genesis/initial-artifact.json")).unwrap();
    assert_eq!(
        receipt.world.genesis_digest,
        format!("blake3:{}", blake3_hex(&artifact))
    );
    let original = fs::read(o.root.join("initialization-receipt.json")).unwrap();
    let identity =
        bridge::world_service_read::identity(&o.root.join("world"), &o.world_id).unwrap();
    let pinned = bridge::world_service_read::pin(
        &o.root.join("records"),
        &o.root.join("store"),
        &identity,
        None,
    )
    .unwrap();
    let binding = pinned.world.current_cognition_runtime_binding().unwrap();
    assert_eq!(binding.finality_status, "pending");
    assert!(binding.finality_block_hash.is_none());
    let bootstrap = bridge::derive_service_execution_bootstrap(
        &o.root.join("world"),
        &o.root.join("records"),
        &o.root.join("store"),
        &o.world_id,
    )
    .unwrap();
    let initial = bootstrap.height;
    let driver = bridge::NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        o.root.join("state.json"),
        o.root.join("world"),
        o.root.join("records"),
        o.root.join("store"),
        &StorageProfileConfig::default(),
        bootstrap.clone(),
    )
    .unwrap();
    let config = NodeConfig::new(&o.node_id, &o.world_id, NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(Duration::from_millis(50))
        .unwrap();
    let mut node = NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    node.start().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while node.snapshot().consensus.committed_height <= initial && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    node.stop().unwrap();
    assert!(node.snapshot().consensus.committed_height > initial);
    let again = initialize(&o).unwrap();
    assert_eq!(again.world, receipt.world);
    assert_eq!(
        fs::read(o.root.join("initialization-receipt.json")).unwrap(),
        original
    );
    assert!(!String::from_utf8(original).unwrap().contains("PRIVATE_KEY"));

    let original_height = receipt.published_commit["position"].as_u64().unwrap();
    let original_record_path = o
        .root
        .join("records")
        .join(format!("{original_height:020}.json"));
    let original_record: Value =
        serde_json::from_slice(&fs::read(original_record_path).unwrap()).unwrap();
    let current_identity =
        bridge::world_service_read::identity(&o.root.join("world"), &o.world_id).unwrap();
    let current = bridge::world_service_read::pin(
        &o.root.join("records"),
        &o.root.join("store"),
        &current_identity,
        None,
    )
    .unwrap();
    let current_record: Value = serde_json::from_slice(
        &fs::read(
            o.root
                .join("records")
                .join(format!("{:020}.json", current.commit.position)),
        )
        .unwrap(),
    )
    .unwrap();
    let live_refs = [
        current_record["snapshot_ref"].as_str().unwrap(),
        current_record["journal_ref"].as_str().unwrap(),
        current_record["world_head_proof_ref"].as_str().unwrap(),
    ];
    let historical_refs = [
        original_record["snapshot_ref"].as_str().unwrap(),
        original_record["journal_ref"].as_str().unwrap(),
        original_record["world_head_proof_ref"].as_str().unwrap(),
    ];
    let store = o.root.join("store");
    for hash in historical_refs {
        if live_refs.contains(&hash) {
            continue;
        }
        let object = find_cas_object(&store, hash).expect("retained historical CAS object");
        fs::remove_file(object).unwrap();
    }
    assert_eq!(
        initialize(&o).unwrap().world,
        receipt.world,
        "archived original proof must survive historical CAS collection"
    );
    assert!(
        bridge::world_service_read::pin(
            &o.root.join("records"),
            &o.root.join("store"),
            &current_identity,
            None,
        )
        .is_ok(),
        "current head remains validated from live CAS"
    );
}

fn find_cas_object(path: &Path, hash: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_dir() {
            if let Some(found) = find_cas_object(&entry.path(), hash) {
                return Some(found);
            }
        } else if entry.file_name().to_string_lossy().contains(hash) {
            return Some(entry.path());
        }
    }
    None
}

#[test]
fn missing_or_tampered_initial_publication_archive_requires_verified_backup() {
    let root = Directory::new();
    let o = root.options();
    let receipt = initialize(&o).unwrap();
    let archive = o.root.join("genesis/initial-publication");
    let manifest = archive.join("manifest.json");
    let original_manifest = fs::read(&manifest).unwrap();

    fs::write(&manifest, b"{}").unwrap();
    let error = initialize(&o).err().unwrap();
    assert!(error.contains("archive hash mismatch"), "{error}");
    fs::write(&manifest, &original_manifest).unwrap();

    let proof_path = archive.join("world-head-proof.cbor");
    let original_proof = fs::read(&proof_path).unwrap();
    fs::write(&proof_path, b"tampered").unwrap();
    let error = initialize(&o).err().unwrap();
    assert!(
        error.contains("archived world head proof hash mismatch"),
        "{error}"
    );
    fs::write(&proof_path, original_proof).unwrap();

    fs::remove_file(&manifest).unwrap();
    let error = initialize(&o).err().unwrap();
    assert!(error.contains("verified backup"), "{error}");
    assert_eq!(receipt.world.world_id, o.world_id);
}

#[test]
fn legacy_receipt_without_archive_fails_closed_without_rewriting_world() {
    let root = Directory::new();
    let o = root.options();
    initialize(&o).unwrap();
    let receipt_path = o.root.join("initialization-receipt.json");
    let before = fs::read(&receipt_path).unwrap();
    let mut legacy: Receipt = serde_json::from_slice(&before).unwrap();
    legacy.initial_publication_proof_hash = None;
    legacy.receipt_schema_hash = blake3_hex(LEGACY_RECEIPT_SCHEMA.as_bytes());
    legacy.initializer_source_hash = "older-initializer-source".into();
    legacy.relative_paths = json!({
        "world":"world",
        "records":"records",
        "store":"store",
        "execution_state":"state.json",
        "initial_artifact":"genesis/initial-artifact.json",
        "viewer_public":"viewer.public.json",
        "viewer_environment":"private/viewer.env"
    });
    let (node_key, _) =
        super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &o.key_dir,
        )
        .unwrap();
    let service = super::super::build_feedback_submit_signer(&o.node_id, &node_key).unwrap();
    let private: [u8; 32] = hex::decode(service.private_key_hex)
        .unwrap()
        .try_into()
        .unwrap();
    legacy.node_scoped_signature = hex::encode(
        SigningKey::from_bytes(&private)
            .sign(&receipt_payload(&legacy).unwrap())
            .to_bytes(),
    );
    fs::write(&receipt_path, encode(json!(legacy)).unwrap()).unwrap();
    fs::remove_dir_all(o.root.join("genesis/initial-publication")).unwrap();
    let legacy_receipt = fs::read(&receipt_path).unwrap();

    let error = initialize(&o).err().unwrap();
    assert!(error.contains("verified backup"), "{error}");
    assert_eq!(fs::read(&receipt_path).unwrap(), legacy_receipt);
    assert!(o.root.join("genesis/initial-artifact.json").exists());
}
#[test]
fn partial_existing_root_and_mismatched_receipt_are_never_repaired() {
    let root = Directory::new();
    let o = root.options();
    files::private_dir(&o.root).unwrap();
    files::write_new(&o.root.join("keep"), b"existing world").unwrap();
    assert!(initialize(&o).is_err());
    assert_eq!(fs::read(o.root.join("keep")).unwrap(), b"existing world");
    fs::remove_dir_all(&o.root).unwrap();
    initialize(&o).unwrap();
    let original = fs::read(o.root.join("initialization-receipt.json")).unwrap();
    let mut other = o.clone();
    other.genesis_nonce = "bb".repeat(32);
    assert!(initialize(&other).is_err());
    assert_eq!(
        fs::read(o.root.join("initialization-receipt.json")).unwrap(),
        original
    );
}
#[test]
fn original_cas_genesis_and_private_consumer_corruption_fail_closed() {
    let root = Directory::new();
    let o = root.options();
    let receipt = initialize(&o).unwrap();
    let p = o.root.join("genesis/initial-artifact.json");
    let original = fs::read(&p).unwrap();
    fs::write(&p, b"{}").unwrap();
    assert!(initialize(&o).is_err());
    fs::write(&p, original).unwrap();
    let private = o.root.join("private/viewer.env");
    let original = fs::read(&private).unwrap();
    fs::write(&private, b"OASIS7_WORLD_SERVICE_SCOPE=agent:other\n").unwrap();
    assert!(initialize(&o).is_err());
    fs::write(&private, original).unwrap();
    let identity = receipt.world;
    let commit =
        serde_json::from_value::<oasis7::world_service::CommitRef>(receipt.published_commit)
            .unwrap();
    let pin = bridge::world_service_read::pin(
        &o.root.join("records"),
        &o.root.join("store"),
        &identity,
        Some(&commit),
    )
    .unwrap();
    assert!(pin.commit.position > 0);
    // Corrupt a real admitted snapshot CAS object, not mutable world JSON.
    let refs: Value = serde_json::from_slice(
        &fs::read(
            o.root
                .join("records")
                .join(format!("{:020}.json", commit.position)),
        )
        .unwrap(),
    )
    .unwrap();
    let hash = refs["snapshot_ref"].as_str().unwrap();
    let store = o.root.join("store");
    fn find(path: &Path, hash: &str) -> Option<PathBuf> {
        for e in fs::read_dir(path).ok()? {
            let e = e.ok()?;
            if e.file_type().ok()?.is_dir() {
                if let Some(p) = find(&e.path(), hash) {
                    return Some(p);
                }
            } else if e.file_name().to_string_lossy().contains(hash) {
                return Some(e.path());
            }
        }
        None
    }
    let object = find(&store, hash).expect("real snapshot CAS object");
    fs::write(object, b"corrupt").unwrap();
    assert!(initialize(&o).is_err());
}
#[test]
fn concurrent_initializers_publish_only_one_complete_root() {
    let root = Directory::new();
    let o = root.options();
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let o = o.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                initialize(&o).is_ok()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert!(results.iter().any(|r| *r));
    initialize(&o).unwrap();
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 2);
}
#[cfg(unix)]
#[test]
fn kernel_exclusive_publish_preserves_out_of_band_empty_dir_and_symlink() {
    let root = Directory::new();
    let target = root.0.join("target");
    let stage = files::Staging::new(&target).unwrap();
    fs::DirBuilder::new().mode(0o700).create(&target).unwrap();
    assert!(stage.publish().is_err());
    assert!(target.is_dir());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    fs::remove_dir(&target).unwrap();
    let stage = files::Staging::new(&target).unwrap();
    let victim = root.0.join("victim");
    files::write_new(&victim, b"keep").unwrap();
    std::os::unix::fs::symlink(&victim, &target).unwrap();
    assert!(stage.publish().is_err());
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(victim).unwrap(), b"keep");
}
#[cfg(unix)]
#[test]
fn weak_identity_and_private_reader_permissions_or_symlinks_rejected() {
    let root = Directory::new();
    let o = root.options();
    let key = o.key_dir.join("node-keypair.toml");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(initialize(&o).is_err());
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    initialize(&o).unwrap();
    let reader = o.root.join("private/public-reader.key");
    fs::set_permissions(&reader, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(initialize(&o).is_err());
    fs::set_permissions(&reader, fs::Permissions::from_mode(0o600)).unwrap();
    let bytes = fs::read(&reader).unwrap();
    fs::remove_file(&reader).unwrap();
    let redirected = root.0.join("redirected-reader");
    files::write_new(&redirected, &bytes).unwrap();
    std::os::unix::fs::symlink(&redirected, &reader).unwrap();
    assert!(initialize(&o).is_err());
}

#[test]
fn stored_receipt_source_schema_and_signature_mismatch_fail_closed() {
    let root = Directory::new();
    let o = root.options();
    initialize(&o).unwrap();
    let path = o.root.join("initialization-receipt.json");
    let original = fs::read(&path).unwrap();
    for field in [
        "initializer_source_hash",
        "receipt_schema_hash",
        "node_scoped_signature",
    ] {
        let mut receipt: Value = serde_json::from_slice(&original).unwrap();
        receipt[field] = json!("00".repeat(if field == "node_scoped_signature" {
            64
        } else {
            32
        }));
        fs::write(&path, encode(receipt).unwrap()).unwrap();
        assert!(initialize(&o).is_err(), "accepted corrupt {field}");
        fs::write(&path, &original).unwrap();
    }
    initialize(&o).unwrap();
}

#[test]
fn initial_commit_validator_binding_uses_provisioned_node_identity() {
    let root = Directory::new();
    let o = root.options();
    let (key, _) =
        super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
            &o.key_dir,
        )
        .unwrap();
    let expected = super::super::derive_node_consensus_signer_keypair(&o.node_id, &key).unwrap();
    let config = preview_node_config(&o, &key, &root.0.join("replication")).unwrap();
    assert_eq!(config.pos_config.validator_signer_public_keys.len(), 1);
    assert_eq!(
        config.pos_config.validator_signer_public_keys[&o.node_id],
        expected.public_key_hex
    );
    let receipt = initialize(&o).unwrap();
    let height = receipt.published_commit["position"].as_u64().unwrap();
    let envelope: Value = serde_json::from_slice(
        &fs::read(
            o.root
                .join("replication/replication_commit_messages")
                .join(format!("{height:020}.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(envelope["public_key_hex"], expected.public_key_hex);
    let record = fs::read(o.root.join("records").join(format!("{height:020}.json"))).unwrap();
    let envelope_bytes = fs::read(
        o.root
            .join("replication/replication_commit_messages")
            .join(format!("{height:020}.json")),
    )
    .unwrap();
    verify_signed_initial_commit(&o, height, &record, &envelope_bytes).unwrap();
}

#[test]
fn tampered_archived_initial_consensus_signature_fails_closed_before_rerun() {
    let root = Directory::new();
    let o = root.options();
    initialize(&o).unwrap();
    let path = o
        .root
        .join("genesis/initial-publication/consensus-envelope.json");
    let mut envelope: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope["signature_hex"] = json!("00".repeat(64));
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(
        initialize(&o)
            .err()
            .unwrap()
            .contains("archived signed envelope hash mismatch")
    );
}

#[test]
fn valid_same_signer_commit_from_other_initialization_cannot_replace_pin() {
    let root = Directory::new();
    let o = root.options();
    let receipt = initialize(&o).unwrap();
    let other_root = Directory::new();
    let mut other = o.clone();
    other.root = other_root.0.join("world-root");
    other.genesis_nonce = "44".repeat(32);
    let other_receipt = initialize(&other).unwrap();
    let height = receipt.published_commit["position"].as_u64().unwrap();
    let other_height = other_receipt.published_commit["position"].as_u64().unwrap();
    assert_eq!(height, other_height);
    let archived_envelope = "genesis/initial-publication/consensus-envelope.json";
    fs::copy(
        other.root.join(archived_envelope),
        o.root.join(archived_envelope),
    )
    .unwrap();
    assert!(initialize(&o).is_err());
}
