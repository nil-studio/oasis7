use super::super::formal_prefix_storage::PublicationFault;
use super::super::{FormalBftPrefixExecutionV1, FormalCandidateContext, execute_formal_candidate};
use super::*;
use oasis7_formal_protocol::bft::{FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1};
use oasis7_runtime::{BlobStore, LocalCasStore};
use std::collections::BTreeSet;

fn actual_prefix(h: &QualifiedH, scratch: &Scratch) -> FormalBftCertifiedPrefixV1 {
    let execution = open_formal_read(h, scratch, h.reader.clone(), true).unwrap();
    let first = execution.build_candidate(Vec::new()).unwrap();
    let mut executed = execution.execute_candidate(&first).unwrap();
    let mut prefix = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![FormalBftCertifiedEntryV1 {
            certificate: certificate(first, execution.schedule()),
            commitment: executed.commitment.clone(),
        }],
    };
    for _ in 0..2 {
        for node in &executed.result_delta.node_writes {
            execution
                .business_reader
                .cache_content_node(&node.digest, &node.bytes)
                .unwrap();
        }
        let proof = prefix.verify(&execution.handoff).unwrap();
        let schedule = proof.next_height_schedule();
        let mut block = executed.block.clone();
        block.height = schedule.height();
        block.parent_commit_ref = schedule.parent_commit_ref().into();
        block.authority_epoch = schedule.authority_epoch();
        block.active_validator_set_hash = schedule.active_set().set_hash().into();
        block.manifest_hash = schedule.parent_manifest_hash().into();
        executed = execute_formal_candidate(
            FormalCandidateContext {
                handoff: &execution.handoff,
                schedule,
                parent_world: &executed.world,
                parent_result_tree: &executed.result_tree,
                identity: &execution.parent_record.world,
                business_reader: &execution.business_reader,
                prefix: Some(&proof),
            },
            &block,
        )
        .unwrap();
        prefix.entries.push(FormalBftCertifiedEntryV1 {
            certificate: certificate(executed.block.clone(), schedule),
            commitment: executed.commitment.clone(),
        });
    }
    prefix
}

fn open_prefix(
    h: &QualifiedH,
    scratch: &Scratch,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    initialize: bool,
) -> Result<FormalBftPrefixExecutionV1, String> {
    FormalBftPrefixExecutionV1::open_from_handoff(
        &scratch.path().join("records"),
        LocalCasStore::new(scratch.path().join("cas")),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        source,
        initialize,
    )
}

#[test]
fn formal_prefix_ledger_installs_reopens_and_recovers_the_original_three_height_world() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(ledger_case)
        .unwrap()
        .join()
        .unwrap();
}

fn ledger_case() {
    let h = qualified_handoff();
    let builder = Scratch::new();
    let prefix = actual_prefix(&h, &builder);
    let height = 5;
    let id = prefix
        .entries
        .last()
        .unwrap()
        .certificate
        .proposal
        .block
        .block_id()
        .unwrap();
    let root = Scratch::new();
    let before_h = h.world.snapshot();
    let execution = open_prefix(&h, &root, h.reader.clone(), true).unwrap();
    assert!(
        super::super::super::super::storage::lock_format(&root.path().join("records")).is_err(),
        "the root format claim must exclude another writer mode"
    );
    let cut = execution.install_prefix(&prefix, height, &id).unwrap();
    assert_eq!(cut.world().state().time, height);
    assert_eq!(cut.proof().certificate().block_id(), id);
    assert_eq!(
        cut.result_tree().root().unwrap(),
        prefix.entries[2].commitment.result_root()
    );
    let sample = h
        .reader
        .0
        .iter()
        .find(|(key, _)| cut.index_reader().read_node(key).unwrap().is_some())
        .unwrap();
    assert_eq!(
        cut.index_reader().read_node(sample.0).unwrap(),
        Some(sample.1.clone())
    );
    let snapshot = cut.world().snapshot();
    let receipt = cut.durable_record_digest().to_owned();
    assert_eq!(h.world.snapshot(), before_h);
    let records = root.path().join("records");
    assert!(
        !records.join("formal-bft-read-v1").exists(),
        "new origin does not require a legacy H+1 ledger"
    );
    assert!(
        records
            .join("formal-bft-prefix-v1/record-00000000000000000003.cbor")
            .exists()
    );
    assert!(
        records
            .join("formal-bft-prefix-v1/record-00000000000000000005.cbor")
            .exists()
    );
    assert!(
        super::super::formal_storage::FormalBftReadLedgerV1::required_material_exists(&records)
            .is_err(),
        "the unchanged old marker decoder must reject the explicit downgrade fence"
    );
    drop(cut);
    let store = LocalCasStore::new(root.path().join("cas"));
    store.put_bytes(b"unpublished orphan").unwrap();
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    let mut reopened = open_prefix(&h, &root, Arc::new(EmptyIndexReader), false).unwrap();
    let cut = reopened.verified_cut(height, &id).unwrap();
    assert_eq!(cut.world().snapshot(), snapshot);
    assert_eq!(cut.durable_record_digest(), receipt);
    let older_id = prefix.entries[1]
        .certificate
        .proposal
        .block
        .block_id()
        .unwrap();
    let older = reopened.verified_cut(4, &older_id).unwrap();
    assert_eq!(
        older.world().state().time,
        4,
        "unvisited future preload nodes are not older-cut dependencies"
    );
    let older_refs = record_business_index_refs(&records, 4);
    let latest_refs = record_business_index_refs(&records, height);
    let future_only: BTreeSet<_> = latest_refs.difference(&older_refs).cloned().collect();
    assert!(
        !future_only.is_empty(),
        "the three-height fixture must contain an index node introduced after height 4"
    );
    for digest in future_only {
        assert_eq!(
            older.index_reader().read_node(&digest).unwrap(),
            None,
            "an earlier historical cut cannot expose a node retained only by its suffix"
        );
        assert!(
            cut.index_reader().read_node(&digest).unwrap().is_some(),
            "the later cut retains its own suffix dependency"
        );
    }
    drop(older);
    drop(cut);
    for path in [
        records.join("formal-bft-prefix-v1/origin.cbor"),
        records.join("formal-bft-prefix-required-v1"),
    ] {
        let original = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            reopened.verified_cut(height, &id).is_err(),
            "live reread must detect lost durable origin/marker"
        );
        assert!(
            !path.exists(),
            "verification must not repair deleted authority metadata"
        );
        private_file(&path, &original);
    }
    drop(reopened);
    // Fault the persisted material while keeping its retention receipt intact.
    // Reexecution must not hide a missing/corrupt blob by regenerating it.
    let last_record = records.join("formal-bft-prefix-v1/record-00000000000000000005.cbor");
    let original_record = fs::read(&last_record).unwrap();
    #[derive(serde::Deserialize)]
    struct SnapshotReference {
        snapshot_ref: String,
    }
    let selected: SnapshotReference = serde_cbor::from_slice(&original_record).unwrap();
    // Test-only physical fault injection uses the CAS format's known blob path.
    let blob = store
        .root()
        .join("blobs")
        .join(format!("{}.blob", selected.snapshot_ref));
    let original_blob = fs::read(&blob).unwrap();
    let pin_ids = store.list_pin_scope_shards("formal-bft-prefix-v1").unwrap();
    for corrupt in [true, false] {
        if corrupt {
            fs::write(&blob, b"broken CAS bytes").unwrap();
        } else {
            fs::remove_file(&blob).unwrap();
        }
        assert!(open_prefix(&h, &root, h.reader.clone(), false).is_err());
        assert_eq!(
            store.list_pin_scope_shards("formal-bft-prefix-v1").unwrap(),
            pin_ids
        );
        assert_eq!(fs::read(&last_record).unwrap(), original_record);
        fs::write(&blob, &original_blob).unwrap();
    }
    fs::write(&last_record, &original_record[..original_record.len() / 2]).unwrap();
    assert!(open_prefix(&h, &root, h.reader.clone(), true).is_err());
    assert_eq!(
        fs::read(&last_record).unwrap(),
        original_record[..original_record.len() / 2]
    );
    private_file(&last_record, &original_record);
    let saved_record = root.path().join("fault-original-record");
    fs::rename(&last_record, &saved_record).unwrap();
    std::os::unix::fs::symlink(&saved_record, &last_record).unwrap();
    assert!(open_prefix(&h, &root, h.reader.clone(), true).is_err());
    assert!(
        fs::symlink_metadata(&last_record)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&last_record).unwrap();
    fs::rename(&saved_record, &last_record).unwrap();
    assert!(
        FormalBftReadExecutionV1::open_from_handoff(
            &records,
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            h.reader.clone(),
            true
        )
        .is_err()
    );

    // Removing a retention receipt must remain visible on repeated recovery;
    // no startup path may republish it using the still-present blobs.
    store.clear_pin_scope("formal-bft-prefix-v1").unwrap();
    for _ in 0..2 {
        let error = open_prefix(&h, &root, h.reader.clone(), false)
            .err()
            .unwrap();
        assert!(error.contains("retention receipt"), "{error}");
        assert!(
            store
                .list_pin_scope_shards("formal-bft-prefix-v1")
                .unwrap()
                .is_empty()
        );
    }
    for phase in [
        PublicationFault::IntentPublished,
        PublicationFault::RecordLinked,
    ] {
        let recovery = Scratch::new();
        let mut execution = open_prefix(&h, &recovery, h.reader.clone(), true).unwrap();
        execution.ledger.inject_publication_failure(4, phase);
        let error = execution
            .install_prefix(&prefix, height, &id)
            .err()
            .unwrap();
        assert!(error.contains("injected original"));
        let intent = recovery
            .path()
            .join("records/formal-bft-prefix-v1/intent-00000000000000000004.cbor");
        let original = fs::read(&intent).unwrap();
        if phase == PublicationFault::RecordLinked {
            let alias = recovery.path().join("extra-publication-alias");
            fs::hard_link(&intent, &alias).unwrap();
            assert!(open_prefix(&h, &recovery, h.reader.clone(), true).is_err());
            assert!(alias.exists() && intent.exists());
            assert_eq!(fs::read(&intent).unwrap(), original);
            fs::remove_file(alias).unwrap();
        }
        let mut resumed = open_prefix(&h, &recovery, h.reader.clone(), true).unwrap();
        assert!(
            resumed.verified_cut(height, &id).is_err(),
            "missing final height needs original input"
        );
        let mut conflict = prefix.clone();
        // Even the same valid block with a different signed round cannot
        // replace the original durable request identity.
        conflict.entries[1].certificate = certificate_at_round(
            conflict.entries[1].certificate.proposal.block.clone(),
            prefix.entries[..1].to_vec(),
            &h,
            1,
        );
        conflict
            .verify(&verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap())
            .unwrap();
        assert!(resumed.install_prefix(&conflict, height, &id).is_err());
        assert_eq!(fs::read(&intent).unwrap(), original);
        let resumed = open_prefix(&h, &recovery, Arc::new(EmptyIndexReader), true).unwrap();
        let cut = resumed.install_prefix(&prefix, height, &id).unwrap();
        assert_eq!(cut.world().snapshot(), snapshot);
        assert!(!intent.exists());
    }
    let rejected = Scratch::new();
    let mut forged = prefix.clone();
    let last = forged.entries.last_mut().unwrap();
    last.commitment.result_root = "a7".repeat(32);
    let proof_before = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: prefix.entries[..2].to_vec(),
    }
    .verify(&verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap())
    .unwrap();
    let mut false_block = last.certificate.proposal.block.clone();
    false_block.execution_block_hash = last.commitment.digest().unwrap();
    last.certificate = certificate(false_block, proof_before.next_height_schedule());
    let forged_proof = forged
        .verify(&verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap())
        .unwrap();
    let failed = open_prefix(&h, &rejected, h.reader.clone(), true)
        .unwrap()
        .install_prefix(&forged, height, forged_proof.certificate().block_id());
    assert!(failed.err().unwrap().contains("does not reproduce"));
    assert!(
        !rejected
            .path()
            .join("records/formal-bft-prefix-v1/record-00000000000000000005.cbor")
            .exists()
    );
    assert!(
        !rejected
            .path()
            .join("records/formal-bft-prefix-v1/intent-00000000000000000005.cbor")
            .exists()
    );
    let mut retained = open_prefix(&h, &rejected, Arc::new(EmptyIndexReader), false).unwrap();
    assert_eq!(
        retained
            .verified_cut(4, &older_id)
            .unwrap()
            .world()
            .state()
            .time,
        4
    );
    drop(retained);
    for relative in ["bft-format.lock", "formal-bft-prefix-v1/writer.lock"] {
        let broken = Scratch::new();
        let mut execution = open_prefix(&h, &broken, h.reader.clone(), true).unwrap();
        let path = broken.path().join("records").join(relative);
        fs::remove_file(&path).unwrap();
        assert!(execution.verified_cut(height, &id).is_err());
        drop(execution);
        assert!(
            open_prefix(&h, &broken, h.reader.clone(), true).is_err(),
            "missing stable lock is never recreated over original origin"
        );
        assert!(!path.exists());
    }
    empty_ledger_metadata_loss_cases(&h, &prefix, height, &id);
    cli_case(&h, &prefix, height, &id);
    killed_process_cases(&h, &prefix, height, &id);
}

fn record_business_index_refs(records: &Path, height: u64) -> BTreeSet<String> {
    #[derive(serde::Deserialize)]
    struct References {
        business_index_refs: Vec<String>,
    }

    let path = records.join(format!("formal-bft-prefix-v1/record-{height:020}.cbor"));
    serde_cbor::from_slice::<References>(&fs::read(path).unwrap())
        .unwrap()
        .business_index_refs
        .into_iter()
        .collect()
}

fn empty_ledger_metadata_loss_cases(
    h: &QualifiedH,
    prefix: &FormalBftCertifiedPrefixV1,
    height: u64,
    block_id: &str,
) {
    for missing_relative in [
        "formal-bft-prefix-v1/origin.cbor",
        "formal-bft-prefix-required-v1",
        "formal-bft-read-required-v1",
    ] {
        let (root, config, proof) = cli_files(h, prefix, height, block_id);
        drop(open_prefix(h, &root, h.reader.clone(), true).unwrap());
        let records = root.path().join("records");
        let missing = records.join(missing_relative);
        let original = fs::read(&missing).unwrap();
        fs::remove_file(&missing).unwrap();
        let origin = records.join("formal-bft-prefix-v1/origin.cbor");
        let profile_marker = records.join("formal-bft-prefix-required-v1");
        let downgrade_marker = records.join("formal-bft-read-required-v1");
        let preserved = [
            origin.as_path(),
            profile_marker.as_path(),
            downgrade_marker.as_path(),
        ]
        .into_iter()
        .filter(|path| *path != missing.as_path())
        .map(|path| (path.to_path_buf(), fs::read(path).unwrap()))
        .collect::<Vec<_>>();

        let args = [
            "bft-prefix",
            "install",
            "--config",
            config.to_str().unwrap(),
            "--prefix",
            proof.to_str().unwrap(),
        ];
        assert!(
            crate::identity_receipt::dispatch(args.into_iter())
                .unwrap()
                .is_err(),
            "install must not reinitialize an existing empty ledger after losing {missing_relative}"
        );
        assert!(
            !missing.exists(),
            "failed install must preserve the missing metadata state"
        );
        assert!(!original.is_empty());
        for (path, expected) in preserved {
            assert_eq!(fs::read(path).unwrap(), expected);
        }
    }
}

fn certificate_at_round(
    block: BftBlockV2,
    entries: Vec<FormalBftCertifiedEntryV1>,
    h: &QualifiedH,
    round: u64,
) -> BftCommitCertificateV2 {
    let handoff = verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap();
    let proof = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries,
    }
    .verify(&handoff)
    .unwrap();
    let schedule = proof.next_height_schedule();
    let key = fixtures::key(101);
    let proposer = schedule.expected_proposer_for_round(round).unwrap();
    let header = BftProposalHeaderV2::sign(
        block.scope.clone(),
        block.height,
        round,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        proposer.clone(),
        block.block_id().unwrap(),
        None,
        &key,
    )
    .unwrap();
    let vote = BftVoteV2::sign(
        block.scope.clone(),
        block.height,
        round,
        BftVotePhaseV2::Precommit,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        BftVoteValueV2::Block {
            block_id: block.block_id().unwrap(),
        },
        proposer,
        &key,
    )
    .unwrap();
    BftCommitCertificateV2 {
        proposal: Box::new(BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        }),
        precommits: vec![vote],
    }
}

fn private_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn cli_files(
    h: &QualifiedH,
    prefix: &FormalBftCertifiedPrefixV1,
    height: u64,
    block_id: &str,
) -> (Scratch, PathBuf, PathBuf) {
    let root = Scratch::new();
    let cas = root.path().join("cas");
    let records = root.path().join("records");
    for path in [&cas, &records] {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = LocalCasStore::new(&cas);
    for (digest, bytes) in &h.reader.0 {
        assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
    }
    let trust = root.path().join("trust.json");
    let material = root.path().join("H.cbor");
    let config = root.path().join("prefix.json");
    let proof = root.path().join("prefix.cbor");
    private_file(&trust, &serde_json::to_vec(&h.trust).unwrap());
    private_file(
        &material,
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    private_file(&proof, &prefix.canonical_bytes().unwrap());
    private_file(&config, &serde_json::to_vec(&serde_json::json!({
        "schema_version": 1, "trust_config_path": trust, "target": h.target,
        "exact_target": {"height": height, "block_id": block_id}, "handoff_material_path": material,
        "cas_directory": cas, "ledger_directory": records,
    })).unwrap());
    (root, config, proof)
}

fn cli_case(h: &QualifiedH, prefix: &FormalBftCertifiedPrefixV1, height: u64, block_id: &str) {
    let (root, config, proof) = cli_files(h, prefix, height, block_id);
    let records = root.path().join("records");
    let args = [
        "bft-prefix",
        "install",
        "--config",
        config.to_str().unwrap(),
        "--prefix",
        proof.to_str().unwrap(),
    ];
    crate::identity_receipt::dispatch(args.into_iter())
        .unwrap()
        .unwrap();
    let args = ["bft-prefix", "verify", "--config", config.to_str().unwrap()];
    crate::identity_receipt::dispatch(args.into_iter())
        .unwrap()
        .unwrap();
    assert!(
        records
            .join("formal-bft-prefix-v1/record-00000000000000000005.cbor")
            .exists()
    );
    assert!(!records.join("formal-bft-read-v1").exists());
}

fn killed_process_cases(
    h: &QualifiedH,
    prefix: &FormalBftCertifiedPrefixV1,
    height: u64,
    block_id: &str,
) {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::process::ExitStatusExt;
    for phase in ["intent", "link", "pins", "reservation-stage", "init-stage"] {
        let (root, config, proof) = cli_files(h, prefix, height, block_id);
        let child_name = concat!(
            "execution_bridge_real_tests::real_execution_bridge::driver::bft_execution::v2::formal_read::tests::prefix_ledger_tests::",
            "formal_prefix_publication_child"
        );
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", child_name, "--nocapture"])
            .env("OASIS7_TEST_FORMAL_PREFIX_KILL_CONFIG", &config)
            .env("OASIS7_TEST_FORMAL_PREFIX_KILL_PHASE", phase)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let _control = child.stdin.take().unwrap();
        let ready = root.path().join("records/prefix-test-publication-ready");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !ready.exists() {
            if child.try_wait().unwrap().is_some() {
                let output = child.wait_with_output().unwrap();
                panic!(
                    "publication child exited before the fault: {:?} {} {}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "publication child did not reach the requested durable seam: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let busy = FormalBftReadExecutionV1::open_from_handoff(
            &root.path().join("records"),
            LocalCasStore::new(root.path().join("cas")),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            h.reader.clone(),
            true,
        )
        .err()
        .unwrap();
        assert!(
            busy.contains("already open"),
            "cross-format claim must use the live OS lock: {busy}"
        );
        child.kill().unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.signal(),
            Some(libc::SIGKILL),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let directory = root.path().join("records/formal-bft-prefix-v1");
        let intent = directory.join("intent-00000000000000000004.cbor");
        let reservation = directory.join("reservation-00000000000000000004.cbor");
        let record = directory.join("record-00000000000000000004.cbor");
        let (protected_path, protected_bytes) = match phase {
            "intent" | "link" => {
                assert!(!reservation.exists());
                assert_eq!(
                    fs::metadata(&intent).unwrap().nlink(),
                    if phase == "link" { 2 } else { 1 }
                );
                (intent.clone(), fs::read(&intent).unwrap())
            }
            "pins" => {
                assert!(reservation.exists());
                assert!(!intent.exists() && !record.exists());
                let shards = LocalCasStore::new(root.path().join("cas"))
                    .list_pin_scope_shards("formal-bft-prefix-v1")
                    .unwrap();
                assert!(
                    shards
                        .iter()
                        .any(|shard| shard.ends_with("height-00000000000000000004")),
                    "PinsPublished must leave the original height reservation and retention pin"
                );
                (reservation.clone(), fs::read(&reservation).unwrap())
            }
            "reservation-stage" => {
                assert!(!reservation.exists() && !intent.exists() && !record.exists());
                assert!(directory.join("origin.cbor").exists());
                let stages =
                    publication_stages(&directory, "reservation-00000000000000000004.cbor");
                assert_eq!(
                    stages.len(),
                    1,
                    "pre-rename reservation stage must be retained"
                );
                (stages[0].clone(), fs::read(&stages[0]).unwrap())
            }
            "init-stage" => {
                let origin = directory.join("origin.cbor");
                let init_intent = directory.join("init-intent.cbor");
                assert!(!origin.exists() && !init_intent.exists());
                assert!(!reservation.exists() && !intent.exists() && !record.exists());
                assert!(
                    !root
                        .path()
                        .join("records/formal-bft-prefix-required-v1")
                        .exists()
                );
                assert!(
                    !root
                        .path()
                        .join("records/formal-bft-read-required-v1")
                        .exists()
                );
                let stages = publication_stages(&directory, "init-intent.cbor");
                assert_eq!(
                    stages.len(),
                    1,
                    "pre-rename initialization intent must be retained"
                );
                (stages[0].clone(), fs::read(&stages[0]).unwrap())
            }
            _ => unreachable!(),
        };
        let records = root.path().join("records");
        let lock_inode = fs::metadata(records.join("bft-format.lock")).unwrap().ino();
        let store = LocalCasStore::new(root.path().join("cas"));
        if matches!(phase, "intent" | "link" | "pins") {
            store.prune_orphan_blobs().unwrap();
        }

        if phase == "init-stage" {
            let alternate_h = qualified_handoff_with_controller_seed(103);
            assert!(open_prefix(&alternate_h, &root, alternate_h.reader.clone(), true).is_err());
            assert_eq!(fs::read(&protected_path).unwrap(), protected_bytes);
            assert!(!directory.join("origin.cbor").exists());
        }
        if phase != "init-stage" {
            let mut conflict = prefix.clone();
            // Same block, different valid round is a different original QC
            // and must not replace a durable reservation or stage.
            conflict.entries[1].certificate = certificate_at_round(
                conflict.entries[1].certificate.proposal.block.clone(),
                prefix.entries[..1].to_vec(),
                h,
                1,
            );
            conflict
                .verify(&verify_formal_bft_handoff_v1(&h.anchor, h.target.clone()).unwrap())
                .unwrap();
            let mut resumed = open_prefix(h, &root, Arc::new(EmptyIndexReader), true).unwrap();
            assert!(resumed.verified_cut(height, block_id).is_err());
            assert!(resumed.install_prefix(&conflict, height, block_id).is_err());
            assert_eq!(fs::read(&protected_path).unwrap(), protected_bytes);
        }

        let args = [
            "bft-prefix",
            "install",
            "--config",
            config.to_str().unwrap(),
            "--prefix",
            proof.to_str().unwrap(),
        ];
        crate::identity_receipt::dispatch(args.into_iter())
            .unwrap()
            .unwrap();
        assert!(!intent.exists());
        assert!(!reservation.exists());
        assert!(record.exists());
        assert!(publication_stages(&directory, "init-intent.cbor").is_empty());
        assert!(publication_stages(&directory, "reservation-00000000000000000004.cbor").is_empty());
        assert!(publication_stages(&directory, "intent-00000000000000000004.cbor").is_empty());
        if phase == "init-stage" {
            assert!(directory.join("origin.cbor").exists());
        }
        assert_eq!(
            fs::metadata(records.join("bft-format.lock")).unwrap().ino(),
            lock_inode,
            "recovery reuses the stable OS carrier instead of replacing a dead PID lock"
        );
        let mut execution = open_prefix(h, &root, Arc::new(EmptyIndexReader), false).unwrap();
        assert_eq!(
            execution
                .verified_cut(height, block_id)
                .unwrap()
                .world()
                .state()
                .time,
            height
        );
    }
}

fn publication_stages(directory: &Path, target: &str) -> Vec<PathBuf> {
    let prefix = format!(".{target}.checkpoint-install-");
    let mut stages = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".tmp"))
        })
        .collect::<Vec<_>>();
    stages.sort();
    stages
}

#[test]
fn formal_prefix_publication_child() {
    let Some(config) = std::env::var_os("OASIS7_TEST_FORMAL_PREFIX_KILL_CONFIG") else {
        return;
    };
    let config = PathBuf::from(config);
    let root = config.parent().unwrap().to_path_buf();
    assert_eq!(
        root.parent().unwrap(),
        std::env::temp_dir().canonicalize().unwrap()
    );
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("oasis7-formal-bft-cut-")
    );
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
            let path = |key: &str| {
                let path = PathBuf::from(value[key].as_str().unwrap());
                assert_eq!(path.parent().unwrap(), root);
                path
            };
            let target: BftProfileTargetV1 =
                serde_json::from_value(value["target"].clone()).unwrap();
            let phase = std::env::var("OASIS7_TEST_FORMAL_PREFIX_KILL_PHASE").unwrap();
            let ledger_directory = path("ledger_directory");
            let ready = root.join("records/prefix-test-publication-ready");
            if phase == "init-stage" {
                super::super::super::super::super::super::durable_transaction::set_private_publication_pause(
                    ledger_directory.join("formal-bft-prefix-v1/init-intent.cbor"),
                    ready.clone(),
                );
            }
            let trusted = crate::formal_bft_read_cli::load_qualified_handoff(
                &path("trust_config_path"),
                &target,
                &path("handoff_material_path"),
            )
            .unwrap();
            let store = LocalCasStore::new(path("cas_directory"));
            let source = Arc::new(crate::formal_bft_read_cli::CasBusinessIndexReader {
                store: store.clone(),
            });
            let mut execution = FormalBftPrefixExecutionV1::open_from_handoff(
                &path("ledger_directory"),
                store,
                trusted.anchor,
                target,
                trusted.handoff_material,
                source,
                true,
            )
            .unwrap();
            let phase_fault = match phase.as_str() {
                "init-stage" => None,
                "reservation-stage" => {
                    super::super::super::super::super::super::durable_transaction::set_private_publication_pause(
                        ledger_directory.join("formal-bft-prefix-v1/reservation-00000000000000000004.cbor"),
                        ready,
                    );
                    None
                }
                "pins" => Some(PublicationFault::PinsPublished),
                "intent" => Some(PublicationFault::IntentPublished),
                "link" => Some(PublicationFault::RecordLinked),
                _ => panic!("unknown self-kill phase"),
            };
            if let Some(phase) = phase_fault {
                execution.ledger.inject_process_kill(4, phase);
            }
            let prefix = FormalBftCertifiedPrefixV1::decode_canonical(
                &fs::read(root.join("prefix.cbor")).unwrap(),
            )
            .unwrap();
            execution
                .install_prefix(
                    &prefix,
                    value["exact_target"]["height"].as_u64().unwrap(),
                    value["exact_target"]["block_id"].as_str().unwrap(),
                )
                .unwrap();
            panic!("test child reached the cut instead of its original publication fault");
        })
        .unwrap()
        .join()
        .unwrap();
}
