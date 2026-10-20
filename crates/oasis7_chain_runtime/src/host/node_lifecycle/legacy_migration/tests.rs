use super::{io, source, transaction, tree};
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::{Action, ChainResourceDerivationContext, LocalCasStore};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
struct Fixture {
    root: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize("/tmp").unwrap().join(format!(
            "oasis7-legacy-migration-{}",
            io::random_id().unwrap()
        ));
        io::private_dir(&root).unwrap();
        export_fixture(&root);
        Self {
            source: root.join("source.json"),
            destination: root.join("migrated"),
            root,
        }
    }
}
impl Fixture {
    fn with_mirror() -> Self {
        let root = fs::canonicalize("/tmp").unwrap().join(format!(
            "oasis7-legacy-migration-{}",
            io::random_id().unwrap()
        ));
        io::private_dir(&root).unwrap();
        export_fixture_kind(&root, true);
        Self {
            source: root.join("source.json"),
            destination: root.join("migrated"),
            root,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn assert_migrated_mirror_is_at_runtime_derived_path(launch_path: &Path) {
    let options = source::load_launch(launch_path).unwrap();
    let paths = crate::resolve_runtime_paths(&options);
    let mirror = crate::execution_bridge::simulator_world_dir_from_execution_world_dir(
        &paths.execution_world_dir,
    );
    assert!(
        mirror.join("snapshot.json").is_file(),
        "{}",
        mirror.display()
    );
    assert!(
        mirror.join("journal.json").is_file(),
        "{}",
        mirror.display()
    );
}
fn export_fixture(root: &Path) {
    export_fixture_kind(root, false);
}
fn export_fixture_kind(root: &Path, with_simulator_action: bool) {
    io::private_dir(root).unwrap();
    let runtime = root.join("old-runtime");
    let world = root.join("independent-world");
    let records = root.join("independent-records");
    let cas = root.join("independent-cas");
    let replication = root.join("independent-replication");
    for dir in [
        &runtime,
        &world,
        &records,
        &cas,
        &replication,
        &replication.join("store"),
        &replication.join("store/blobs"),
    ] {
        io::private_dir(dir).unwrap();
    }
    let key = ed25519_dalek::SigningKey::from_bytes(&[0x37; 32]);
    fs::write(
        root.join("old-config.toml"),
        format!(
            "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
            hex::encode(key.to_bytes()),
            hex::encode(key.verifying_key().to_bytes())
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        root.join("old-config.toml"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let world_id = "legacy-migration-world";
    let genesis = format!("sha256:{}", hex::encode([0x42; 32]));
    let initial = oasis7::runtime_host::world::new();
    initial
        .save_to_dir_with_chain_resource_context(
            &world,
            ChainResourceDerivationContext {
                world_id,
                chain_id: world_id,
                genesis_ref: Some(&genesis),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: 0,
            },
            hex::encode([0x11; 32]),
            hex::encode([0x22; 32]),
        )
        .unwrap();
    // Use the real execution bridge, never the binary test stub.
    let mut driver =
        crate::execution_bridge_real_tests::real_execution_bridge::NodeRuntimeExecutionDriver::new(
            runtime.join("reward-runtime-execution-bridge-state.json"),
            world.clone(),
            records.clone(),
            cas.clone(),
        )
        .unwrap();
    driver.resource_identity = Some(
        crate::execution_bridge_real_tests::real_execution_bridge::ExecutionResourceIdentity {
            chain_id: world_id.into(),
            genesis_ref: genesis.clone(),
        },
    );
    let committed_actions = if with_simulator_action {
        let payload = oasis7::consensus_action_payload::encode_consensus_action_payload(
            &oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope::from_simulator_action(
                oasis7_simulator::Action::HarvestRadiation { agent_id: "agent-0".into(), max_amount: 1 },
                oasis7_simulator::ActionSubmitter::System,
            )).unwrap();
        vec![oasis7_node::NodeConsensusAction::from_payload(1, "old-observer", payload).unwrap()]
    } else {
        Vec::new()
    };
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: world_id.into(),
            node_id: "old-observer".into(),
            proposer_id: "old-observer".into(),
            height: 1,
            slot: 0,
            epoch: 0,
            node_block_hash: "original-node-block-1".into(),
            action_root: compute_consensus_action_root(&committed_actions).unwrap(),
            committed_actions,
            committed_at_unix_ms: 1000,
        })
        .unwrap();
    drop(driver);
    // Preserve a real typed continuation world and queued action. Its marker
    // remains an old engineering continuation, never a formal certificate.
    // Ordinary commits leave the materialized cache at its previous head.
    // Restore the actual committed snapshot/journal from the record's CAS,
    // exactly as startup recovery does, before staging height two.
    let record: serde_json::Value = io::read(&records.join(format!("{:020}.json", 1))).unwrap();
    let store = LocalCasStore::new(cas.clone());
    let cas_bytes = |name: &str| store.get_verified(record[name].as_str().unwrap()).unwrap();
    let before = oasis7::runtime_host::world::from_snapshot(
        serde_cbor::from_slice(&cas_bytes("snapshot_ref")).unwrap(),
        serde_cbor::from_slice(&cas_bytes("journal_ref")).unwrap(),
    )
    .unwrap();
    assert_eq!(before.state().time, 1);
    let before_root = oasis7_runtime::blake3_hex(&serde_cbor::to_vec(&before.snapshot()).unwrap());
    assert_eq!(
        record["execution_state_root"].as_str(),
        Some(before_root.as_str())
    );
    let mut staged = before;
    staged.step().unwrap();
    staged.submit_action(Action::RegisterAgent {
        agent_id: "pending-after-cut".into(),
        pos: oasis7::geometry::GeoPos::new(1, 2, 3),
    });
    let feedback = oasis7_agent_api::FeedbackEnvelopeV1 {
        feedback_id: "retained-feedback".into(),
        feedback_seq: 1,
        agent_subject: "fixture-agent".into(),
        agent_session_id: "original-session".into(),
        agent_turn_id: "original-turn".into(),
        decision_request_id: "original-request".into(),
        candidate_action_id: Some(1),
        runtime_receipt_id: Some("original-receipt".into()),
        status: "committed".into(),
        request_digest: oasis7::simulator::h_v1("oasis7.cognition.request.v1", &"original-request"),
        reject_reason: None,
        provenance: "runtime_authoritative".into(),
    };
    staged.enqueue_runtime_feedback(feedback.clone()).unwrap();
    staged.enqueue_runtime_feedback(feedback).unwrap();
    staged.save_to_dir(&world).unwrap();
    let staged = oasis7::runtime_host::world::load_from_dir(&world).unwrap();
    let staged_root = oasis7_runtime::blake3_hex(&serde_cbor::to_vec(&staged.snapshot()).unwrap());
    io::write(&records.join("product-validation-intent.json"),&serde_json::json!({"schema_version":1,"world_id":world_id,"height":2,"journal_len":staged.journal().len(),"pre_step_execution_state_root":before_root,"staged_execution_state_root":staged_root})).unwrap();
    // Distinct replication CAS, writer/guard state and reward-prefixed data.
    io::write(
        &replication.join("replication_guard.json"),
        &oasis7_distfs::SingleWriterReplicationGuard::default(),
    )
    .unwrap();
    io::write(
        &replication.join("replication_remote_guards.json"),
        &BTreeMap::<String, oasis7_distfs::SingleWriterReplicationGuard>::new(),
    )
    .unwrap();
    io::write(
        &replication.join("replication_writer_state_old-observer.json"),
        &serde_json::json!({"writer_epoch":77,"last_sequence":0,"last_replicated_height":0}),
    )
    .unwrap();
    let blob = b"separate-replication-material";
    let hash = oasis7_runtime::blake3_hex(blob);
    fs::write(
        replication.join("store/blobs").join(format!("{hash}.blob")),
        blob,
    )
    .unwrap();
    fs::write(
        runtime.join("reward-runtime-state.json"),
        b"original-reward-state",
    )
    .unwrap();
    fs::write(
        runtime.join("reward-runtime-distfs-probe-state.json"),
        b"original-probe-state",
    )
    .unwrap();
    let argv = vec![
        "--node-id",
        "old-observer",
        "--world-id",
        world_id,
        "--node-role",
        "observer",
        "--storage-profile",
        "dev_local",
        "--node-no-auto-attest-all",
        "--reward-runtime-disable",
        "--reward-runtime-signer-node-id",
        "old-reward-signer",
        "--config",
        "old-config.toml",
        "--runtime-root",
        "old-runtime",
        "--execution-world-dir",
        "independent-world",
        "--execution-records-dir",
        "independent-records",
        "--storage-root",
        "independent-cas",
        "--replication-root",
        "independent-replication",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    io::write(
        &root.join("source.json"),
        &source::Source {
            api_version: "oasis7.legacy-launch-source/v1".into(),
            cwd: root.into(),
            runtime_binary_sha256: std::env::var("OASIS7_LEGACY_MIGRATION_RUNTIME_BIN")
                .map(|p| io::sha256(Path::new(&p)).unwrap())
                .unwrap_or_else(|_| source::runtime_digest().unwrap()),
            argv,
            declared_env: BTreeMap::from([
                ("CONFIG_PATH".into(), "old-config.toml".into()),
                ("REPLICATION_ROOT".into(), "independent-replication".into()),
                (
                    "REWARD_RUNTIME_SIGNER_NODE_ID".into(),
                    "old-reward-signer".into(),
                ),
            ]),
        },
    )
    .unwrap();
}
#[test]
fn legacy_export_real_source_fixture() {
    if let Ok(path) = std::env::var("OASIS7_LEGACY_MIGRATION_FIXTURE_OUT") {
        export_fixture(Path::new(&path));
    }
}
#[test]
fn legacy_actual_split_source_migration_preserves_identity_head_pending_cas_and_reward() {
    let fixture = Fixture::new();
    let plan = source::plan(&fixture.source, &fixture.destination, "split-source").unwrap();
    assert_ne!(plan.reward_signer_id, plan.node_id);
    assert_eq!(plan.closure.committed_height, 1);
    assert!(plan.closure.pending_actions > 0);
    assert_eq!(plan.closure.replication_cas_blobs, 1);
    assert_eq!(plan.closure.feedback_outbox_count, 1);
    let key_before = tree::hash(&fixture.root.join("old-config.toml")).unwrap();
    let reward_before =
        tree::hash(&fixture.root.join("old-runtime/reward-runtime-state.json")).unwrap();
    let receipt =
        transaction::apply(&fixture.source, &fixture.destination, "split-source").unwrap();
    assert_eq!(receipt.closure, plan.closure);
    assert!(receipt.source_preserved);
    assert!(!receipt.switched_source_launch);
    let launch = source::load_launch(&fixture.destination.join("legacy-launch.json")).unwrap();
    assert_eq!(launch.node_id, "old-observer");
    assert_eq!(
        launch.reward_runtime_signer_node_id.as_deref(),
        Some("old-reward-signer")
    );
    assert_migrated_mirror_is_at_runtime_derived_path(
        &fixture.destination.join("legacy-launch.json"),
    );
    assert_eq!(
        key_before,
        tree::hash(&fixture.root.join("old-config.toml")).unwrap()
    );
    assert_eq!(
        reward_before,
        tree::hash(&fixture.root.join("old-runtime/reward-runtime-state.json")).unwrap()
    );
    assert_eq!(
        transaction::apply(&fixture.source, &fixture.destination, "split-source")
            .unwrap()
            .plan_digest,
        receipt.plan_digest
    );
}
#[test]
fn legacy_interrupted_copy_resumes_fixed_stage_and_rejects_source_or_partial_target_drift() {
    let fixture = Fixture::new();
    let plan = source::plan(&fixture.source, &fixture.destination, "resume-source").unwrap();
    transaction::FAIL_AFTER_ENTRY.with(|n| n.set(Some(3)));
    assert!(
        transaction::apply(&fixture.source, &fixture.destination, "resume-source")
            .unwrap_err()
            .contains("injected")
    );
    transaction::FAIL_AFTER_ENTRY.with(|n| n.set(None));
    assert!(plan.stage.is_dir());
    assert!(!fixture.destination.exists());
    let entry = plan
        .files
        .iter()
        .find(|e| !e.directory && plan.stage.join(&e.target).is_file())
        .unwrap();
    let saved = fs::read(plan.stage.join(&entry.target)).unwrap();
    fs::write(plan.stage.join(&entry.target), b"corrupt partial result").unwrap();
    assert!(transaction::apply(&fixture.source, &fixture.destination, "resume-source").is_err());
    fs::write(plan.stage.join(&entry.target), saved).unwrap();
    let reward = fixture.root.join("old-runtime/reward-runtime-state.json");
    let saved = fs::read(&reward).unwrap();
    fs::write(&reward, b"changed source").unwrap();
    assert!(transaction::apply(&fixture.source, &fixture.destination, "resume-source").is_err());
    fs::write(&reward, saved).unwrap();
    assert_eq!(
        transaction::apply(&fixture.source, &fixture.destination, "resume-source")
            .unwrap()
            .closure,
        plan.closure
    );
}
#[test]
fn legacy_live_writer_missing_material_and_unknown_profile_fail_without_destination() {
    let fixture = Fixture::new();
    let plan = source::plan(&fixture.source, &fixture.destination, "reject-source").unwrap();
    let lock = crate::world_writer_lock::acquire_live_world_writer_lock(
        &plan.paths["--execution-world-dir"],
    )
    .unwrap();
    assert!(transaction::apply(&fixture.source, &fixture.destination, "reject-source").is_err());
    drop(lock);
    assert!(!fixture.destination.exists());
    let snapshot = fixture.root.join("independent-world/snapshot.json");
    let bytes = fs::read(&snapshot).unwrap();
    fs::remove_file(&snapshot).unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "reject-source").is_err());
    assert!(!snapshot.exists());
    fs::write(&snapshot, bytes).unwrap();
    let mut source: source::Source = io::read(&fixture.source).unwrap();
    source.argv.push("--node-auto-attest-all".into());
    io::write(&fixture.source, &source).unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "reject-source").is_err());
    assert!(!fixture.destination.exists());
}

#[test]
fn legacy_admitted_empty_stage_resumes_but_lost_publication_is_not_recreated() {
    let fixture = Fixture::new();
    transaction::FAIL_BEFORE_STAGE_PLAN.with(|n| n.set(true));
    assert!(
        transaction::apply(&fixture.source, &fixture.destination, "admission-crash")
            .unwrap_err()
            .contains("admission-stage")
    );
    transaction::FAIL_BEFORE_STAGE_PLAN.with(|n| n.set(false));
    assert!(
        transaction::status(&fixture.source, &fixture.destination, "admission-crash")
            .unwrap()
            .is_none()
    );
    transaction::apply(&fixture.source, &fixture.destination, "admission-crash").unwrap();
    fs::remove_dir_all(&fixture.destination).unwrap();
    assert!(
        transaction::apply(&fixture.source, &fixture.destination, "admission-crash")
            .unwrap_err()
            .contains("published destination disappeared")
    );
    assert!(!fixture.destination.exists());
}

#[test]
fn legacy_corrupt_cas_and_missing_mirror_material_are_rejected() {
    let fixture = Fixture::with_mirror();
    let record_path = fixture
        .root
        .join("independent-records/00000000000000000001.json");
    let record: serde_json::Value = io::read(&record_path).unwrap();
    let blob = fixture
        .root
        .join("independent-cas/blobs")
        .join(format!("{}.blob", record["snapshot_ref"].as_str().unwrap()));
    let original = fs::read(&blob).unwrap();
    fs::write(&blob, b"corrupt encoded CAS").unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "corrupt-cas").is_err());
    assert!(!fixture.destination.exists());
    fs::write(&blob, original).unwrap();
    let mut missing = record.clone();
    missing["simulator_mirror"]["journal_ref"] = serde_json::Value::Null;
    io::write(&record_path, &missing).unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "missing-mirror").is_err());
    assert!(!fixture.destination.exists());
    missing = record;
    missing["snapshot_ref"] = serde_json::Value::Null;
    io::write(&record_path, &missing).unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "missing-history").is_err());
}

#[test]
fn legacy_migrated_restart_rejects_replaced_original_cut_record() {
    let fixture = Fixture::new();
    transaction::apply(&fixture.source, &fixture.destination, "restart-cut").unwrap();
    let launch = fixture.destination.join("legacy-launch.json");
    let options = source::load_launch(&launch).unwrap();
    let paths = crate::resolve_runtime_paths(&options);
    let cut = paths.execution_records_dir.join(format!("{:020}.json", 1));
    let mut record: serde_json::Value = io::read(&cut).unwrap();
    // Same execution state/hash but a different consensus record at the old cut
    // cannot silently replace the preserved migration history on restart.
    record["node_block_hash"] = serde_json::json!("different-original-node-block");
    io::write(&cut, &record).unwrap();
    assert!(
        source::load_launch(&launch)
            .unwrap_err()
            .contains("original migration cut")
    );
}

#[test]
fn legacy_migration_rejects_hash_valid_but_unrecoverable_simulator_journal() {
    let fixture = Fixture::with_mirror();
    let record_path = fixture
        .root
        .join("independent-records")
        .join(format!("{:020}.json", 1));
    let mut record: serde_json::Value = io::read(&record_path).unwrap();
    let store = LocalCasStore::new(fixture.root.join("independent-cas"));
    let malformed =
        oasis7_runtime::BlobStore::put_bytes(&store, b"not simulator journal CBOR").unwrap();
    record["simulator_mirror"]["journal_ref"] = serde_json::json!(malformed);
    io::write(&record_path, &record).unwrap();
    assert!(
        source::plan(&fixture.source, &fixture.destination, "malformed-mirror")
            .unwrap_err()
            .contains("simulator journal decode")
    );
    assert!(!fixture.destination.exists());
}

#[test]
fn legacy_migrated_restart_requires_private_destination_and_original_runtime() {
    let fixture = Fixture::new();
    transaction::apply(&fixture.source, &fixture.destination, "restart-runtime").unwrap();
    let launch_path = fixture.destination.join("legacy-launch.json");
    source::load_launch(&launch_path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fixture.destination, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(source::load_launch(&launch_path).is_err());
        fs::set_permissions(&fixture.destination, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let plan_path = fixture.destination.join("migration-plan.json");
    let mut plan: source::Plan = io::read(&plan_path).unwrap();
    let mut launch: source::Launch = io::read(&launch_path).unwrap();
    plan.runtime_binary_sha256 = "00".repeat(32);
    launch.runtime_binary_sha256 = plan.runtime_binary_sha256.clone();
    launch.plan_digest = io::digest(&plan).unwrap();
    io::write(&plan_path, &plan).unwrap();
    io::write(&launch_path, &launch).unwrap();
    assert!(
        source::load_launch(&launch_path)
            .unwrap_err()
            .contains("executable changed")
    );
}

#[test]
fn legacy_migration_bounds_compressed_cas_before_decoding() {
    let fixture = Fixture::new();
    let record_path = fixture
        .root
        .join("independent-records")
        .join(format!("{:020}.json", 1));
    let record: serde_json::Value = io::read(&record_path).unwrap();
    let blob = fixture
        .root
        .join("independent-cas/blobs")
        .join(format!("{}.blob", record["snapshot_ref"].as_str().unwrap()));
    let mut encoded = fs::read(&blob).unwrap();
    assert!(encoded.starts_with(b"O7CBLOB1"));
    encoded[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    fs::write(blob, encoded).unwrap();
    assert!(
        source::plan(&fixture.source, &fixture.destination, "cas-budget")
            .unwrap_err()
            .contains("ResourceLimited")
    );
    assert!(!fixture.destination.exists());
}

#[test]
fn legacy_actual_some_mirror_is_preserved() {
    let fixture = Fixture::with_mirror();
    let plan = source::plan(&fixture.source, &fixture.destination, "actual-mirror").unwrap();
    let receipt =
        transaction::apply(&fixture.source, &fixture.destination, "actual-mirror").unwrap();
    assert_eq!(receipt.closure, plan.closure);
    assert_migrated_mirror_is_at_runtime_derived_path(
        &fixture.destination.join("legacy-launch.json"),
    );
}

#[test]
fn legacy_never_recorded_mirror_requires_original_genesis_kernel() {
    let fixture = Fixture::new();
    source::plan(&fixture.source, &fixture.destination, "original-genesis").unwrap();
    let snapshot_path = fixture
        .root
        .join("independent-world-simulator-mirror/snapshot.json");
    let mut snapshot: serde_json::Value = io::read(&snapshot_path).unwrap();
    snapshot["time"] = serde_json::json!(7);
    io::write(&snapshot_path, &snapshot).unwrap();
    assert!(source::plan(&fixture.source, &fixture.destination, "changed-genesis").is_err());
    assert!(!fixture.destination.exists());
}
