//! The actual Runtime/CLI drives the native TCP protocol. Only the independent
//! transaction backend is a software fixture, as in the ordinary Node tests.
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

type VerifiedReader = Arc<
    Mutex<
        Option<(
            Arc<BftPrivateRetentionAdmissionInputV1>,
            BftFirstReservationPriceV1,
        )>,
    >,
>;
struct ArchiveOriginalReader(VerifiedReader);
impl BftFirstReserveSourceV1 for ArchiveOriginalReader {
    fn read_authorized_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        _deadline: Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3> {
        let pair = self.0.lock().unwrap();
        let (input, _) = pair
            .as_ref()
            .ok_or_else(|| BftSignerErrorV3("test reader denied".into()))?;
        if input.descriptor()? != *reference {
            return Err(BftSignerErrorV3(
                "test reader permitted a different original".into(),
            ));
        }
        Ok(BftAuthorizedOriginalReserveInputV1 {
            input: input.clone(),
            transport_hold: Box::new(()),
        })
    }

    fn verify_actual_first_price(
        &self,
        _: &BftSignerBindingV3,
        _: &BftPrivateRetentionCreationMarkerV1,
        _: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        _deadline: Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3> {
        let pair = self.0.lock().unwrap();
        let (verified, price) = pair
            .as_ref()
            .ok_or_else(|| BftSignerErrorV3("test price denied".into()))?;
        if verified.as_ref() != input {
            return Err(BftSignerErrorV3(
                "test Runtime verified a different exact input".into(),
            ));
        }
        Ok(BftFirstReservationPriceV1 {
            publication_bytes: price.publication_bytes,
            private_pin_bytes: price.private_pin_bytes,
            original_selection_bytes: price.original_selection_bytes,
            retained_h_dependencies: price.retained_h_dependencies.clone(),
        })
    }
}

#[test]
fn wire3_node_cli_durably_activates_original_covered_source_and_rejects_namespace_replacement() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(activation_case)
        .unwrap()
        .join()
        .unwrap();
}

fn activation_case() {
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
            "--archive-first-private-source-input",
            "--source-config",
            source_path.to_str().unwrap(),
            "--actions",
            actions.to_str().unwrap(),
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
    let (source, pin, replayed, first_bytes) = recovered_ledger
        .archive_first_original_local(&archive, &binding, &attestor, &marker, action_bytes)
        .unwrap();
    let (cold_result, price) = pin
        .verify_first_price(
            &recovered_ledger,
            &archive
                .load_original_admission_input(&pin.reference)
                .unwrap(),
            &source,
            &marker,
            &marker.digest().unwrap(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(replayed).unwrap(),
        serde_json::to_value(cold_result).unwrap()
    );
    let original = archive
        .load_original_admission_input(&pin.reference)
        .unwrap();
    assert_eq!(original.publication_reservation_bytes, first_bytes);
    let data = original
        .objects
        .iter()
        .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
        .unwrap();
    let candidate_bytes = data.bytes.clone();
    let data: serde_json::Value = serde_cbor::from_slice(&candidate_bytes).unwrap();
    assert_eq!(data["schema_version"], 9);
    assert_eq!(data["protected_source_origin_ref"], blake3_hex(&bytes));
    assert!(fs::read_dir(&dir).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("candidate")
    }));
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
    let pending_path = dir.join(format!("candidate-{:020}.cbor", original.key.height));
    drop(recovered_ledger);
    let reserve_options = serde_json::json!({
        "schema_version":1, "creation_identity":creation, "attestor":attestor,
        "archive_directory": archive_store.root(),
        "reserve_input": { "source": source, "candidate": pin },
    });
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
    *verified_reader.lock().unwrap() = Some((Arc::new(original), price));
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
