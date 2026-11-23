//! The actual Runtime/CLI drives the native TCP protocol. Only the independent
//! transaction backend is a software fixture, as in the ordinary Node tests.
//! CLI dispatch runs in this test process; the independently recreated reader
//! and native loopback TCP service are not separate shipping binary processes.
use super::super::super::super::formal_header::ArchivedOriginalSourceReaderV1;
use super::*;
use std::os::unix::fs::PermissionsExt;

struct PinnedSource(BftPrivateRetentionCreationMarkerV1);
impl BftFreshSourceVerifierV1 for PinnedSource {
    fn verify_original_runtime_source(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(), BftSignerErrorV3> {
        if &self.0 != marker || binding.digest()? != marker.source.binding_digest {
            return Err(BftSignerErrorV3("test source pin differs".into()));
        }
        Ok(())
    }
}

struct FreshBackend {
    base: MemoryTestSignerBackend,
    marker: BftPrivateRetentionCreationMarkerV1,
    original: Option<BftPrivateRetentionActivationV1>,
    spend: u64,
    pending: Option<BftFirstReservedOriginalV1>,
}
impl BftSignerBackendV3 for FreshBackend {
    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.base.load_provisioned(binding)
    }
    fn acquire_lease(
        &mut self,
        binding: &str,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.base.acquire_lease(binding, generation, token)
    }
    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.base.release_lease(generation, token)
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.base.read_for_lease(generation, token)
    }
    fn protected_time_ms(&mut self, generation: u64, token: &str) -> Result<i64, BftSignerErrorV3> {
        self.base.protected_time_ms(generation, token)
    }
    fn compare_and_swap(
        &mut self,
        _: u64,
        _: &str,
        _: u64,
        _: &str,
        _: Option<i64>,
        _: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "ordinary signing is disabled in covered source test".into(),
        ))
    }
    fn read_fresh_coverage_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftFreshCoverageSnapshotV1, BftSignerErrorV3> {
        Ok(BftFreshCoverageSnapshotV1 {
            signer: self.base.read_for_lease(generation, token)?,
            marker: self.marker.clone(),
            original: self.original.clone(),
            reserved_response_bytes: self.spend,
        })
    }
    fn compare_and_commit_fresh_coverage(
        &mut self,
        expected: &BftFreshCoverageSnapshotV1,
        next: &BftFreshCoverageSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        let mut state = self.base.lock()?;
        MemoryTestSignerBackend::check_lease(
            &state,
            expected.signer.lease_generation,
            &expected.signer.lease_token_digest,
        )?;
        if *state != expected.signer
            || self.marker != expected.marker
            || expected.marker != next.marker
            || self.original != expected.original
            || self.spend != expected.reserved_response_bytes
        {
            return Err(BftSignerErrorV3(
                "covered source test transaction conflict".into(),
            ));
        }
        *state = next.signer.clone();
        self.original = next.original.clone();
        self.spend = next.reserved_response_bytes;
        Ok(())
    }
    fn read_first_reserve_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftFirstReserveSnapshotV1, BftSignerErrorV3> {
        Ok(BftFirstReserveSnapshotV1 {
            current: self.read_fresh_coverage_for_lease(generation, token)?,
            reservation: self.pending.clone(),
        })
    }

    fn compare_and_commit_first_reserve(
        &mut self,
        expected: &BftFirstReserveSnapshotV1,
        next: &BftFirstReserveSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        if self.pending != expected.reservation {
            return Err(BftSignerErrorV3(
                "test original Pending inventory changed".into(),
            ));
        }
        self.compare_and_commit_fresh_coverage(&expected.current, &next.current)?;
        self.pending = next.reservation.clone();
        Ok(())
    }
}

type VerifiedReader = Arc<Mutex<Option<ArchivedOriginalSourceReaderV1>>>;
struct ArchiveOriginalReader(VerifiedReader);
impl BftFirstReserveSourceV1 for ArchiveOriginalReader {
    fn read_authorized_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3> {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| BftSignerErrorV3("test reader denied".into()))?
            .read_authorized_original(reference, deadline)
    }

    fn verify_actual_first_price(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
        covered: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        deadline: Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3> {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| BftSignerErrorV3("test verifier denied".into()))?
            .verify_actual_first_price(binding, marker, covered, input, deadline)
    }
}

#[test]
fn wire3_node_cli_durably_activates_original_covered_source_and_rejects_namespace_replacement() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| activation_case(false))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn wire3_node_original_received_archive_cold_verifier_and_reserve_wal_recovery() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| activation_case(true))
        .unwrap()
        .join()
        .unwrap();
}

fn activation_case(received: bool) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    for (digest, bytes) in &h.reader.0 {
        assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
    }
    let records = root.path().join("ledger");
    let replication = root.path().join("replication");
    for dir in [&records, &replication] {
        fs::create_dir(dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let write = |name: &str, bytes: &[u8]| {
        let path = root.path().join(name);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    };

    let trust = write("trust.json", &serde_json::to_vec(&h.trust).unwrap());
    let material = write(
        "H.cbor",
        &oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
    );
    let requester = fixtures::key(117);
    let requester_path = write(
        "requester.hex",
        hex::encode(requester.to_bytes()).as_bytes(),
    );
    let creation = "19".repeat(32);
    let source_path = write(
        "source.json",
        &serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "creation_identity":creation,
        }))
        .unwrap(),
    );
    let validator = validator_id(0);
    let validator_key = hex::encode(fixtures::key(120).verifying_key().to_bytes());
    let mut node_value = serde_json::json!({
        "schema_version":1, "trust_config_path":trust, "target":h.target,
        "handoff_material_path":material, "cas_directory":store.root(),
        "records_directory":records, "replication_directory":replication,
        "validator_id":validator, "player_id":"wire3-world-private",
        "operator_instance_id":"original-covered-source-test",
        "provisioned_signing_keys":[{"validator_id":validator,"public_key_hex":validator_key}],
        "timeouts":{"propose_ms":2000,"prevote_ms":2000,"precommit_ms":2000,"round_delta_ms":100},
        "provider_address":"127.0.0.1:1", "requester_signing_key_path":requester_path,
        "provider_timeout_ms":30000, "completion_timeout_ms":120000,
    });
    let node_path = write("node.json", &serde_json::to_vec(&node_value).unwrap());
    let run = |action: &str| {
        crate::identity_receipt::dispatch(
            [
                "bft-header",
                "node",
                "--config",
                node_path.to_str().unwrap(),
                action,
                "--source-config",
                source_path.to_str().unwrap(),
            ]
            .into_iter(),
        )
        .unwrap()
    };
    run("--prepare-private-source").unwrap();
    let dir = records
        .join("formal-bft-header-prefix-v1")
        .join("node-private-v1");
    assert!(!dir.exists());

    let ledger = super::ledger_tests::open(&h, root.path(), &store, false).unwrap();
    let (handoff, schedule, identity) = ledger.covered_source_binding_parts(&creation).unwrap();
    let binding = BftSignerBindingV3::new(
        handoff,
        schedule,
        identity,
        validator.clone(),
        vec![BftSignerKeyIdentityV3 {
            validator_id: validator,
            public_key_hex: validator_key,
        }],
        serde_json::from_value(node_value["timeouts"].clone()).unwrap(),
        "original-covered-source-test".into(),
        hex::encode(requester.verifying_key().to_bytes()),
    )
    .unwrap();
    let marker = ledger.covered_source_marker(&binding, &creation).unwrap();
    drop(ledger);
    let provider_key = fixtures::key(116);
    let provider_public_key = hex::encode(provider_key.verifying_key().to_bytes());
    let attestor = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: provider_public_key.clone(),
        source: marker.source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: marker.initial_height,
        allow_authenticated_successor_origins: false,
    };
    write(
        "source.json",
        &serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "creation_identity":creation, "attestor":attestor,
        }))
        .unwrap(),
    );
    node_value["provider_trust"] = serde_json::json!({
        "schema_version":3, "binding_digest":binding.digest().unwrap(), "provider_public_key":provider_public_key,
    });
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 1_000,
    }));
    let verified_reader = Arc::new(Mutex::new(None));
    let service = Arc::new(
        BftSignerServiceV3::open_with_fresh_coverage(
            binding.clone(),
            Box::new(MemoryTestSignerKey(provider_key)),
            vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
            Box::new(FreshBackend {
                base: MemoryTestSignerBackend {
                    binding_digest: binding.digest().unwrap(),
                    fail_confirm_after_commit: None,
                    snapshot: snapshot.clone(),
                },
                marker: marker.clone(),
                original: None,
                spend: 0,
                pending: None,
            }),
            BftFreshCoverageSupportV1 {
                attestor: attestor.clone(),
                marker: marker.clone(),
                verifier: Box::new(PinnedSource(marker.clone())),
            },
        )
        .unwrap()
        .with_first_reservation(
            Box::new(ArchiveOriginalReader(verified_reader.clone())),
            Duration::from_secs(25),
        )
        .unwrap(),
    );

    let session = |action: &str, node_value: &mut serde_json::Value| {
        let (address, join) = tcp(service.clone());
        node_value["provider_address"] = serde_json::json!(address);
        write("node.json", &serde_json::to_vec(node_value).unwrap());
        let result = run(action);
        join.join().unwrap().unwrap();
        result
    };
    FormalBftHeaderLedgerExecutionV3::inject_covered_source_reservation_cut();
    let interrupted = session("--activate-private-source", &mut node_value).unwrap_err();
    assert!(interrupted.contains("injected cut after covered original reservation"));
    assert!(dir.join("origin.reservation.cbor").is_file());
    assert!(!dir.join("origin.cbor").exists());
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    session("--activate-private-source", &mut node_value).unwrap();
    let bytes = fs::read(dir.join("origin.cbor")).unwrap();
    assert_eq!(
        store
            .get_range(&blake3_hex(&bytes), 0, 4 * 1024 * 1024)
            .unwrap(),
        (bytes.clone(), true)
    );
    let origin: serde_json::Value = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(origin["schema_version"], 8);
    assert_eq!(origin["instance_id"], creation);
    let activation_hash = origin["source_activation"]["reference"].as_str().unwrap();
    let (capsule, complete) = store.get_range(activation_hash, 0, 64 * 1024).unwrap();
    assert!(complete);
    let decoded: serde_json::Value = serde_cbor::from_slice(&capsule).unwrap();
    assert_eq!(
        decoded["domain"],
        "oasis7.formal.bft.private-source.activation.v1"
    );
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    session("--activate-private-source", &mut node_value).unwrap();
    assert_eq!(fs::read(dir.join("origin.cbor")).unwrap(), bytes);
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    let action_bytes = super::super::execution_tests::private_actions();
    let actions = write("first-actions.cbor", &action_bytes);
    let package = received.then(|| super::node_multivalue_tests::package(&h, 41));
    let package_path = package.as_ref().map(|package| {
        write(
            "received-package.cbor",
            &oasis7_node::encode_bft_v3_private_candidate_package(package).unwrap(),
        )
    });
    let archive_root = Scratch::new();
    let archive_store = cas(archive_root.path());
    let archive = LocalBftSuccessorArchiveStoreV1::new(archive_store.clone()).unwrap();
    write(
        "source.json",
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "creation_identity":creation, "attestor":attestor,
            "archive_directory": archive_store.root(),
        }))
        .unwrap(),
    );
    crate::identity_receipt::dispatch(
        [
            "bft-header",
            "node",
            "--config",
            node_path.to_str().unwrap(),
            if received {
                "--archive-first-received-private-source-input"
            } else {
                "--archive-first-private-source-input"
            },
            "--source-config",
            source_path.to_str().unwrap(),
            if received { "--package" } else { "--actions" },
            package_path.as_ref().unwrap_or(&actions).to_str().unwrap(),
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    let recovered_ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &records,
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
        false,
    )
    .unwrap();
    let (source, pin, replayed, first_bytes) = if let Some(package) = &package {
        let mut bad = package.clone();
        bad.dependency_nodes.push(("ee".repeat(32), vec![0]));
        assert!(
            recovered_ledger
                .archive_first_original_received(&archive, &binding, &attestor, &marker, &bad)
                .is_err()
        );
        let mut missing = package.clone();
        assert!(missing.dependency_nodes.pop().is_some());
        let missing = oasis7_node::decode_bft_v3_private_candidate_package(
            &oasis7_node::encode_bft_v3_private_candidate_package(&missing).unwrap(),
        )
        .unwrap();
        assert_eq!(
            recovered_ledger
                .archive_first_original_received(&archive, &binding, &attestor, &marker, &missing,)
                .err()
                .unwrap(),
            "original Received dependency closure differs from actual transition"
        );
        let mut lied = package.clone();
        lied.candidate.public_value.execution_commitment.result_root = "ad".repeat(32);
        lied.candidate.public_value.header.execution_commitment_hash = lied
            .candidate
            .public_value
            .execution_commitment
            .digest()
            .unwrap();
        lied.candidate.public_value.validate_binding().unwrap();
        let lied = oasis7_node::decode_bft_v3_private_candidate_package(
            &oasis7_node::encode_bft_v3_private_candidate_package(&lied).unwrap(),
        )
        .unwrap();
        assert!(
            recovered_ledger
                .archive_first_original_received(&archive, &binding, &attestor, &marker, &lied,)
                .err()
                .unwrap()
                .contains("actual qualified H")
        );
        recovered_ledger
            .archive_first_original_received(&archive, &binding, &attestor, &marker, package)
    } else {
        recovered_ledger.archive_first_original_local(
            &archive,
            &binding,
            &attestor,
            &marker,
            action_bytes,
        )
    }
    .unwrap();
    let original_source = recovered_ledger
        .archive_first_original_source(&archive, &binding, &attestor, &marker, &source)
        .unwrap();
    let cold_reader = || {
        ArchivedOriginalSourceReaderV1::new(
            Arc::new(
                LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(archive_store.root()))
                    .unwrap(),
            ),
            original_source.clone(),
            h.anchor.clone(),
            h.target.clone(),
            binding.clone(),
            attestor.clone(),
            pin.clone(),
        )
    };
    let reader = cold_reader();
    let deadline = Instant::now() + Duration::from_secs(120);
    assert!(
        reader
            .read_authorized_original(&pin.reference, Instant::now())
            .is_err()
    );
    let mut wrong_ref = pin.reference.clone();
    wrong_ref.key.header_id = "cc".repeat(32);
    assert!(
        reader
            .read_authorized_original(&wrong_ref, deadline)
            .is_err()
    );
    let held = reader
        .read_authorized_original(&pin.reference, deadline)
        .unwrap();
    assert!(
        reader
            .read_authorized_original(&pin.reference, deadline)
            .is_err()
    );
    let (cold_result, price) = reader.verify(&held.input).unwrap();
    let mut stale = snapshot.lock().unwrap().state.clone();
    stale.revision += 1;
    assert!(
        reader
            .verify_actual_first_price(&binding, &marker, &stale, &held.input, deadline)
            .is_err()
    );
    assert_eq!(price.original_selection_bytes > 0, received);
    assert_eq!(
        serde_json::to_value(replayed).unwrap(),
        serde_json::to_value(cold_result).unwrap()
    );
    let original = archive
        .load_original_admission_input(&pin.reference)
        .unwrap();
    drop(held);
    assert!(
        reader
            .verify(&original)
            .err()
            .unwrap()
            .contains("hold was released")
    );
    let held = reader
        .read_authorized_original(&pin.reference, deadline)
        .unwrap();
    let manifest: serde_json::Value = serde_cbor::from_slice(
        &archive
            .load_object(&original_source.manifest, 1024 * 1024)
            .unwrap(),
    )
    .unwrap();
    let mut objects = vec![original_source.manifest.digest.clone()];
    for role in [
        "h_material",
        "ledger_origin",
        "private_origin",
        "activation_capsule",
    ] {
        objects.push(manifest[role]["digest"].as_str().unwrap().to_owned());
    }
    let dependencies = manifest["dependencies"].as_array().unwrap();
    assert!(!dependencies.is_empty());
    let cold_dependency = dependencies.iter().find(|node| {
        let digest = node["digest"].as_str().unwrap();
        !original
            .objects
            .iter()
            .any(|object| object.reference.digest == digest)
    });
    assert!(
        cold_dependency.is_some(),
        "both candidate kinds must leave a cold H branch for the source archive"
    );
    let cold_digest = cold_dependency.unwrap()["digest"].as_str().unwrap();
    if let Some(package) = &package {
        let mut extra = package.clone();
        let reference: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1 =
            serde_json::from_value(cold_dependency.unwrap().clone()).unwrap();
        extra.dependency_nodes.push((
            cold_digest.to_owned(),
            archive
                .load_object(&reference, reference.bytes as usize)
                .unwrap(),
        ));
        extra.dependency_nodes.sort_by(|a, b| a.0.cmp(&b.0));
        let extra = oasis7_node::decode_bft_v3_private_candidate_package(
            &oasis7_node::encode_bft_v3_private_candidate_package(&extra).unwrap(),
        )
        .unwrap();
        assert_eq!(
            recovered_ledger
                .archive_first_original_received(&archive, &binding, &attestor, &marker, &extra)
                .err()
                .unwrap(),
            "original Received dependency closure differs from actual transition"
        );
    }
    objects.push(cold_digest.to_owned());
    for digest in objects {
        let path = archive_store.blobs_dir().join(format!("{digest}.blob"));
        let exact = fs::read(&path).unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        fs::remove_file(&path).unwrap();
        assert!(
            reader.verify(&held.input).is_err(),
            "missing original object {digest}"
        );
        fs::write(&path, b"corrupt original bytes").unwrap();
        assert!(
            reader.verify(&held.input).is_err(),
            "corrupt original object {digest}"
        );
        fs::write(&path, exact).unwrap();
        fs::set_permissions(&path, permissions).unwrap();
    }
    reader.verify(&held.input).unwrap();
    drop(held);
    let original_path = archive_store
        .blobs_dir()
        .join(format!("{}.blob", pin.reference.object.digest));
    let original_disk = fs::read(&original_path).unwrap();
    fs::write(&original_path, b"damaged original admission input").unwrap();
    assert!(
        reader
            .read_authorized_original(&pin.reference, deadline)
            .is_err()
    );
    fs::write(&original_path, original_disk).unwrap();
    let recovered_hold = reader
        .read_authorized_original(&pin.reference, deadline)
        .unwrap();
    reader.verify(&recovered_hold.input).unwrap();
    drop(recovered_hold);
    let mut repriced = original.clone();
    repriced.publication_reservation_bytes += 1;
    let mut repriced_pin = pin.clone();
    repriced_pin.reference = repriced.descriptor().unwrap();
    archive
        .persist_original_admission_input(&repriced_pin.reference, &repriced)
        .unwrap();
    let repriced_reader = ArchivedOriginalSourceReaderV1::new(
        Arc::new(
            LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(archive_store.root())).unwrap(),
        ),
        original_source.clone(),
        h.anchor.clone(),
        h.target.clone(),
        binding.clone(),
        attestor.clone(),
        repriced_pin.clone(),
    );
    let repriced_hold = repriced_reader
        .read_authorized_original(&repriced_pin.reference, deadline)
        .unwrap();
    assert!(repriced_reader.verify(&repriced_hold.input).is_err());
    assert_eq!(original.publication_reservation_bytes, first_bytes);
    let data = original
        .objects
        .iter()
        .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
        .unwrap();
    let candidate_bytes = data.bytes.clone();
    let data: serde_json::Value = serde_cbor::from_slice(&candidate_bytes).unwrap();
    assert_eq!(data["schema_version"], if received { 10 } else { 9 });
    assert_eq!(data["protected_source_origin_ref"], blake3_hex(&bytes));
    assert!(fs::read_dir(&dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("candidate")
    }));
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    let pending_path = if received {
        dir.join(format!(
            "candidate-{:020}-{}.cbor",
            original.key.height, original.key.header_id
        ))
    } else {
        dir.join(format!("candidate-{:020}.cbor", original.key.height))
    };
    drop(recovered_ledger);
    let reserve_options = serde_json::json!({
        "schema_version":1, "creation_identity":creation, "attestor":attestor,
        "archive_directory": archive_store.root(),
        "reserve_input": { "source": source, "candidate": pin },
        "original_source": original_source,
    });
    write(
        "source.json",
        &serde_json::to_vec(&reserve_options).unwrap(),
    );
    let mut cold_config = node_value.clone();
    for field in [
        "handoff_material_path",
        "cas_directory",
        "records_directory",
        "replication_directory",
    ] {
        cold_config[field] = serde_json::json!(root.path().join(format!("absent-{field}")));
    }
    write("node.json", &serde_json::to_vec(&cold_config).unwrap());
    run("--verify-first-private-source-archive").unwrap();
    let mut unreachable = cold_config.clone();
    for field in [
        "handoff_material_path",
        "cas_directory",
        "records_directory",
        "replication_directory",
    ] {
        let path = PathBuf::from("/dev/null").join(format!("old-{field}"));
        assert_eq!(
            fs::canonicalize(&path).unwrap_err().kind(),
            std::io::ErrorKind::NotADirectory
        );
        unreachable[field] = serde_json::json!(path);
    }
    write("node.json", &serde_json::to_vec(&unreachable).unwrap());
    run("--verify-first-private-source-archive").unwrap();
    let mut aliased_archive = cold_config.clone();
    aliased_archive["cas_directory"] =
        serde_json::json!(archive_store.root().join("..").join("cas"));
    write("node.json", &serde_json::to_vec(&aliased_archive).unwrap());
    assert!(
        run("--verify-first-private-source-archive")
            .unwrap_err()
            .contains("disjoint")
    );
    write("node.json", &serde_json::to_vec(&cold_config).unwrap());
    let mut denied = reserve_options.clone();
    denied["attestor"]["provider_public_key"] = serde_json::json!("ff".repeat(32));
    write("source.json", &serde_json::to_vec(&denied).unwrap());
    assert!(run("--verify-first-private-source-archive").is_err());
    write("node.json", &serde_json::to_vec(&node_value).unwrap());
    let mut wrong_pin = reserve_options.clone();
    wrong_pin["reserve_input"]["candidate"]["original_private_origin_digest"] =
        serde_json::json!("f1".repeat(32));
    write("source.json", &serde_json::to_vec(&wrong_pin).unwrap());
    assert!(
        run("--reserve-first-private-source-input")
            .unwrap_err()
            .contains("does not bind the original private Origin")
    );
    assert!(!pending_path.exists());
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    write(
        "source.json",
        &serde_json::to_vec(&reserve_options).unwrap(),
    );
    assert!(session("--reserve-first-private-source-input", &mut node_value).is_err());
    assert!(!pending_path.exists());
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    *verified_reader.lock().unwrap() = Some(cold_reader());
    FormalBftHeaderLedgerExecutionV3::inject_covered_source_reservation_cut();
    let cut = session("--reserve-first-private-source-input", &mut node_value).unwrap_err();
    assert!(cut.contains("injected cut after covered original reservation"));
    assert!(pending_path.with_extension("reservation.cbor").is_file());
    assert!(!pending_path.exists());
    assert_eq!(snapshot.lock().unwrap().state.revision, 2);
    *verified_reader.lock().unwrap() = None;
    session("--reserve-first-private-source-input", &mut node_value).unwrap();
    assert_eq!(fs::read(&pending_path).unwrap(), candidate_bytes);
    assert_eq!(snapshot.lock().unwrap().state.revision, 2);

    let ordinary = crate::identity_receipt::dispatch(
        [
            "bft-header",
            "node",
            "--config",
            node_path.to_str().unwrap(),
            "--print-binding",
        ]
        .into_iter(),
    )
    .unwrap();
    assert!(
        ordinary
            .unwrap_err()
            .contains("recovery_stage=private_store")
    );

    let saved = records.join("original-covered");
    fs::rename(&dir, &saved).unwrap();
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["origin.cbor", "ready"] {
        fs::copy(saved.join(name), dir.join(name)).unwrap();
    }
    assert!(
        run("--reserve-first-private-source-input")
            .unwrap_err()
            .contains("namespace identity changed")
    );
}
