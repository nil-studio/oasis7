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
        "validator_id":validator, "player_id":"covered-source-cli-player",
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
            }),
            BftFreshCoverageSupportV1 {
                attestor,
                marker: marker.clone(),
                verifier: Box::new(PinnedSource(marker)),
            },
        )
        .unwrap(),
    );

    let activate = |node_value: &mut serde_json::Value| {
        let (address, join) = tcp(service.clone());
        node_value["provider_address"] = serde_json::json!(address);
        write("node.json", &serde_json::to_vec(node_value).unwrap());
        let result = run("--activate-private-source");
        join.join().unwrap().unwrap();
        result.unwrap();
    };
    activate(&mut node_value);
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
    activate(&mut node_value);
    assert_eq!(fs::read(dir.join("origin.cbor")).unwrap(), bytes);
    assert_eq!(snapshot.lock().unwrap().state.revision, 1);
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
        run("--activate-private-source")
            .unwrap_err()
            .contains("namespace identity changed")
    );
}
