//! Real H/Origin/carriers plus actual provider admission; no consensus signatures are requested.
use super::super::super::super::formal_header::{
    ActivateBftPrivateSourceV1, ActualBftPrivateSourceCreationVerifierV1,
    FreshBftPrivateSourceInputsV1, PreparedBftPrivateSourceOriginV1,
};
use super::*;

pub(super) fn inputs(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: &LocalCasStore,
) -> FreshBftPrivateSourceInputsV1 {
    FreshBftPrivateSourceInputsV1 {
        records: root.path().join("ledger"),
        store: store.clone(),
        anchor: h.anchor.clone(),
        target: h.target.clone(),
        material: h.material.clone(),
        source: h.reader.clone(),
    }
}

pub(super) fn binding(prepared: &PreparedBftPrivateSourceOriginV1) -> BftSignerBindingV3 {
    BftSignerBindingV3::new(
        prepared.handoff().clone(),
        prepared.initial_schedule().clone(),
        prepared.material_identity().into(),
        validator_id(0),
        vec![BftSignerKeyIdentityV3 {
            validator_id: validator_id(0),
            public_key_hex: hex::encode(fixtures::key(120).verifying_key().to_bytes()),
        }],
        oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 2000,
            prevote_ms: 2000,
            precommit_ms: 2000,
            round_delta_ms: 100,
        },
        "fresh-source-4975".into(),
        hex::encode(fixtures::key(117).verifying_key().to_bytes()),
    )
    .unwrap()
}

pub(super) fn role(
    binding: &BftSignerBindingV3,
    marker: &BftPrivateRetentionCreationMarkerV1,
) -> BftPrivateRetentionAttestorV1 {
    BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: hex::encode(fixtures::key(116).verifying_key().to_bytes()),
        source: marker.source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: binding.initial_schedule().height(),
        allow_authenticated_successor_origins: true,
    }
}

pub(super) fn endpoint(
    binding: &BftSignerBindingV3,
    backend: BftPrivateRetentionMemoryBackendV1,
    role: BftPrivateRetentionAttestorV1,
    verifier: ActualBftPrivateSourceCreationVerifierV1,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    let service = BftSignerServiceV3::open_with_private_retention(
        binding.clone(),
        Box::new(MemoryTestSignerKey(fixtures::key(116))),
        vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
        Box::new(backend),
        None,
        BftPrivateRetentionSupportV1::new(
            role.clone(),
            Arc::new(verifier),
            Duration::from_secs(10),
        )
        .unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        service.serve_tcp_connection(&mut stream).unwrap();
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        fixtures::key(117),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: role.provider_public_key.clone(),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap()
    .with_private_retention_attestor(role)
    .unwrap();
    (provider, server)
}

fn empty_transfer() -> Option<BftSuccessorArchiveTransferLedgerV1> {
    Some(BftSuccessorArchiveTransferLedgerV1 {
        schema_version: 1,
        reserved_response_bytes: 0,
    })
}

#[test]
fn wire3_fresh_source_real_origin_namespace_finalize_and_original_recovery() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let seed = BftPrivateRetentionFreshSeedV1::generate().unwrap();
    let recovery = seed.recovery_handle();
    let id = seed.creation_identity().to_string();
    let failure = PreparedBftPrivateSourceOriginV1::interrupt_before_h_retention_for_test(
        inputs(&h, &root, &store),
        seed,
    )
    .err()
    .unwrap();
    assert!(failure.contains("injected fresh source initialization"));
    let intent_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/init-intent.cbor");
    let original_intent = fs::read(&intent_path).unwrap();
    assert!(
        PreparedBftPrivateSourceOriginV1::prepare(
            inputs(&h, &root, &store),
            BftPrivateRetentionFreshSeedV1::generate().unwrap()
        )
        .is_err()
    );
    assert_eq!(fs::read(&intent_path).unwrap(), original_intent);
    let prepared = PreparedBftPrivateSourceOriginV1::prepare(
        inputs(&h, &root, &store),
        recovery.recover_seed(),
    )
    .unwrap();
    assert_eq!(prepared.creation_identity(), id);
    let origin_path = root
        .path()
        .join("ledger/formal-bft-header-prefix-v1/origin.cbor");
    let original_origin = fs::read(&origin_path).unwrap();
    assert_eq!(
        original_origin, original_intent,
        "recovery must publish the exact original source initialization bytes"
    );
    assert!(
        !root
            .path()
            .join("ledger/formal-bft-header-prefix-v1/node-private-v1/origin.cbor")
            .exists(),
        "prepare must not publish managed private Origin"
    );
    let binding = binding(&prepared);
    let namespace = prepared.finalize_namespace(&binding).unwrap();
    let marker = namespace.marker().clone();
    let role = role(&binding, &marker);
    let mut retained = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        role.clone(),
        empty_transfer(),
        1000,
    )
    .unwrap();
    let verifier = prepared.source_verifier();
    drop(prepared);
    let prepared = PreparedBftPrivateSourceOriginV1::prepare(
        inputs(&h, &root, &store),
        recovery.recover_seed(),
    )
    .unwrap();
    assert_eq!(fs::read(&origin_path).unwrap(), original_origin);
    assert_eq!(
        prepared.origin_digest().unwrap(),
        marker.ledger_origin_digest
    );
    let (provider, server) = endpoint(&binding, retained.clone(), role.clone(), verifier);
    let mut lease = provider.acquire(&binding).unwrap();
    let covered = lease
        .provision_private_retention(Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert!(covered.map.entries.is_empty());
    assert_eq!(
        covered.receipt.body.head.source.binding_digest,
        binding.digest().unwrap()
    );
    drop(lease);
    server.join().unwrap();
    let original = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    let mut same = BftPrivateRetentionMemoryBackendV1::create_fresh(
        prepared.finalize_namespace(&binding).unwrap(),
        role,
        empty_transfer(),
        1000,
    )
    .unwrap();
    assert_eq!(
        same.load_provisioned(&binding.digest().unwrap()).unwrap(),
        original
    );
    assert!(
        PreparedBftPrivateSourceOriginV1::prepare(
            inputs(&h, &root, &store),
            BftPrivateRetentionFreshSeedV1::generate().unwrap()
        )
        .is_err(),
        "new identity cannot replace original init WAL/Origin"
    );
    assert_eq!(fs::read(origin_path).unwrap(), original_origin);
}

#[test]
fn wire3_fresh_source_actual_verifier_rejects_legacy_origin_with_unsigned_some() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let old = driver(&h, root.path(), &store);
    let binding = signer_binding(&old);
    {
        let mut driver = old.lock().unwrap();
        driver.configure_bft_v3_execution(&binding).unwrap();
        let context = driver
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let candidate = driver
            .build_bft_v3_candidate(NodeBftBuildContextV3 {
                execution: context.clone(),
                actions: vec![],
            })
            .unwrap();
        driver
            .retain_bft_v3_reserved_candidate(&context, &candidate)
            .unwrap();
    }
    drop(old);
    let prefix = root.path().join("ledger/formal-bft-header-prefix-v1");
    let original_origin = fs::read(prefix.join("origin.cbor")).unwrap();
    let private = prefix.join("node-private-v1");
    let before = fs::read_dir(&private)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let data = before
        .iter()
        .find(|(name, _)| {
            name.to_str().unwrap().starts_with("candidate-")
                && name.to_str().unwrap().ends_with(".cbor")
        })
        .unwrap()
        .1;
    let decoded: serde_cbor::Value = serde_cbor::from_slice(data).unwrap();
    let serde_cbor::Value::Map(fields) = decoded else {
        panic!("original Data map");
    };
    assert!(
        fields
            .get(&serde_cbor::Value::Text(
                "publication_reservation_ref".into()
            ))
            .is_some_and(|value| matches!(value, serde_cbor::Value::Text(_)))
    );
    assert!(
        PreparedBftPrivateSourceOriginV1::prepare(
            inputs(&h, &root, &store),
            BftPrivateRetentionFreshSeedV1::generate().unwrap()
        )
        .is_err()
    );
    // Deliberately create another software namespace and change the role marker:
    // this still cannot relabel the actual legacy Origin as a fresh source.
    let origin_digest = blake3::hash(&original_origin).to_hex().to_string();
    let namespace = BftPrivateRetentionFreshSeedV1::generate()
        .unwrap()
        .bind_source(&binding, origin_digest.clone())
        .unwrap();
    let role = role(&binding, namespace.marker());
    let mut retained = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        role.clone(),
        empty_transfer(),
        1000,
    )
    .unwrap();
    let initial = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    let verifier =
        ActualBftPrivateSourceCreationVerifierV1::for_original_source(inputs(&h, &root, &store))
            .unwrap();
    let (provider, server) = endpoint(&binding, retained.clone(), role, verifier);
    let mut lease = provider.acquire(&binding).unwrap();
    let deadline = || Instant::now() + Duration::from_secs(10);
    let entry = BftPrivateRetentionEntryV1 {
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: origin_digest,
            height: binding.initial_schedule().height(),
            header_id: "10".repeat(32),
        },
        fact: BftPrivateRetentionFactV1::Some {
            original_data_ref: blake3::hash(data).to_hex().to_string(),
            metadata_ref: "11".repeat(32),
            admission_digest: "12".repeat(32),
            input_ref: None,
        },
        lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
    };
    for error in [
        lease.provision_private_retention(deadline()).unwrap_err(),
        lease
            .reserve_private_retention(entry, deadline())
            .unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("actual source Origin is legacy"),
            "must exercise actual Origin verifier, not defaultUnsupported: {error}"
        );
    }
    drop(lease);
    server.join().unwrap();
    let after = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    assert_eq!(after.state, initial.state);
    assert_eq!(
        after.successor_archive_transfer,
        initial.successor_archive_transfer
    );
    assert!(
        retained
            .load_private_retention_current(&binding.digest().unwrap())
            .unwrap()
            .retention
            .is_none()
    );
    assert_eq!(
        fs::read(prefix.join("origin.cbor")).unwrap(),
        original_origin
    );
    let after = fs::read_dir(private)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_owned(),
                fs::read(path).unwrap(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        after, before,
        "all original unsigned Some material stays unchanged"
    );
}

#[test]
fn wire3_fresh_source_real_empty_activation_and_covered_cold_reopen() {
    actual_empty_activation(false);
}
#[test]
fn wire3_fresh_source_original_activation_reservation_recovers_without_new_identity() {
    actual_empty_activation(true);
}

fn actual_empty_activation(interrupt: bool) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let request = inputs(&h, &root, &store);
    let prepared = Box::new(
        PreparedBftPrivateSourceOriginV1::prepare(
            request.clone(),
            BftPrivateRetentionFreshSeedV1::generate().unwrap(),
        )
        .unwrap(),
    );
    let binding = binding(&prepared);
    let namespace = prepared.finalize_namespace(&binding).unwrap();
    let marker = namespace.marker().clone();
    let fixed = role(&binding, &marker);
    let mut backend = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        fixed.clone(),
        empty_transfer(),
        1000,
    )
    .unwrap();
    let (provider, server) = endpoint(
        &binding,
        backend.clone(),
        fixed.clone(),
        prepared.source_verifier(),
    );
    let mut lease = provider.acquire(&binding).unwrap();
    lease
        .provision_private_retention(Instant::now() + Duration::from_secs(30))
        .unwrap();
    let before = lease.checkpoint().unwrap();
    let private_root = request
        .records
        .join("formal-bft-header-prefix-v1/node-private-v1");
    let origin_path = private_root.join("origin.cbor");
    let activation = ActivateBftPrivateSourceV1 {
        binding: &binding,
        fixed_attestor: &fixed,
        original_marker: &marker,
        lease: lease.as_mut(),
        deadline: Instant::now() + Duration::from_secs(120),
    };
    let mut driver = if interrupt {
        let error = prepared
            .activate_before_capsule_for_test(activation)
            .err()
            .unwrap();
        assert!(
            error.contains("injected original source activation before capsule"),
            "{error}"
        );
        let reservation = fs::read(private_root.join("origin.reservation.cbor")).unwrap();
        assert!(!origin_path.exists());
        let recovered = PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
            request.clone(),
            ActivateBftPrivateSourceV1 {
                binding: &binding,
                fixed_attestor: &fixed,
                original_marker: &marker,
                lease: lease.as_mut(),
                deadline: Instant::now() + Duration::from_secs(120),
            },
        )
        .unwrap();
        assert_eq!(
            fs::read(&origin_path).unwrap(),
            reservation,
            "recovery publishes the exact original private Data identity"
        );
        recovered
    } else {
        prepared.activate(activation).unwrap()
    };
    assert!(origin_path.exists());
    assert_eq!(
        lease.checkpoint().unwrap().state,
        before.state,
        "private activation never invents a provider State transition"
    );
    assert!(
        driver
            .configure_bft_v3_execution(&binding)
            .unwrap()
            .is_none()
    );
    let context = driver
        .current_bft_v3_execution_context(binding.initial_schedule())
        .unwrap();
    let candidate = driver
        .build_bft_v3_candidate(NodeBftBuildContextV3 {
            execution: context.clone(),
            actions: vec![],
        })
        .unwrap();
    assert!(
        driver
            .retain_bft_v3_private_candidate(&context, &candidate)
            .unwrap_err()
            .contains("atomic protected input")
    );
    assert!(
        driver
            .retain_bft_v3_reserved_candidate(&context, &candidate)
            .unwrap_err()
            .contains("atomic protected input")
    );
    let original_data = fs::read(&origin_path).unwrap();
    #[derive(serde::Deserialize)]
    struct Capsule {
        source_activation: CapsuleRef,
    }
    #[derive(serde::Deserialize)]
    struct CapsuleRef {
        reference: String,
        encoded_bytes: u64,
    }
    let capsule: Capsule = serde_cbor::from_slice(&original_data).unwrap();
    let capsule_path = store
        .blobs_dir()
        .join(format!("{}.blob", capsule.source_activation.reference));
    let original_capsule = fs::read(&capsule_path).unwrap();
    let decoded_capsule = store
        .get_verified(&capsule.source_activation.reference)
        .unwrap();
    assert_eq!(
        decoded_capsule.len() as u64,
        capsule.source_activation.encoded_bytes
    );
    assert_eq!(
        blake3_hex(&decoded_capsule),
        capsule.source_activation.reference
    );
    drop(driver);
    // Current timers/journal may advance after original activation. Cold recovery
    // consumes the new authenticated observation, never the old receipt as State.
    let current = lease.checkpoint().unwrap();
    let timed = lease.start_height_timer(current.state.revision).unwrap();
    drop(lease);
    server.join().unwrap();
    let verifier =
        ActualBftPrivateSourceCreationVerifierV1::for_original_source(request.clone()).unwrap();
    let (provider, server) = endpoint(&binding, backend.clone(), fixed.clone(), verifier);
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(lease.checkpoint().unwrap().state, timed.state);
    let mut cold = PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
        request.clone(),
        ActivateBftPrivateSourceV1 {
            binding: &binding,
            fixed_attestor: &fixed,
            original_marker: &marker,
            lease: lease.as_mut(),
            deadline: Instant::now() + Duration::from_secs(120),
        },
    )
    .unwrap();
    assert!(cold.configure_bft_v3_execution(&binding).unwrap().is_none());
    assert_eq!(
        cold.current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap(),
        context
    );
    drop(cold);
    assert_eq!(fs::read(&origin_path).unwrap(), original_data);
    assert_eq!(fs::read(&capsule_path).unwrap(), original_capsule);
    let mut foreign = fixed.clone();
    foreign.creation_marker_digest = "f1".repeat(32);
    assert!(
        PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
            request.clone(),
            ActivateBftPrivateSourceV1 {
                binding: &binding,
                fixed_attestor: &foreign,
                original_marker: &marker,
                lease: lease.as_mut(),
                deadline: Instant::now() + Duration::from_secs(120)
            }
        )
        .is_err()
    );
    fs::remove_file(&capsule_path).unwrap();
    assert!(
        PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
            request.clone(),
            ActivateBftPrivateSourceV1 {
                binding: &binding,
                fixed_attestor: &fixed,
                original_marker: &marker,
                lease: lease.as_mut(),
                deadline: Instant::now() + Duration::from_secs(120)
            }
        )
        .is_err(),
        "missing retained original capsule must not become fresh activation"
    );
    assert!(!capsule_path.exists());
    fs::write(&capsule_path, &original_capsule).unwrap();
    // Preserving every child inode does not preserve the original private
    // directory. The schema8 carrier and live descriptor both bind its identity.
    let moved_private = private_root.with_file_name("node-private-moved");
    fs::rename(&private_root, &moved_private).unwrap();
    fs::create_dir(&private_root).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for entry in fs::read_dir(&moved_private).unwrap() {
        let entry = entry.unwrap();
        fs::hard_link(entry.path(), private_root.join(entry.file_name())).unwrap();
        fs::remove_file(entry.path()).unwrap();
    }
    assert!(
        PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
            request.clone(),
            ActivateBftPrivateSourceV1 {
                binding: &binding,
                fixed_attestor: &fixed,
                original_marker: &marker,
                lease: lease.as_mut(),
                deadline: Instant::now() + Duration::from_secs(120)
            }
        )
        .is_err(),
        "replaced private parent cannot borrow original file inodes"
    );
    for entry in fs::read_dir(&private_root).unwrap() {
        let entry = entry.unwrap();
        fs::hard_link(entry.path(), moved_private.join(entry.file_name())).unwrap();
        fs::remove_file(entry.path()).unwrap();
    }
    fs::remove_dir(&private_root).unwrap();
    fs::rename(&moved_private, &private_root).unwrap();
    let restored = PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
        request,
        ActivateBftPrivateSourceV1 {
            binding: &binding,
            fixed_attestor: &fixed,
            original_marker: &marker,
            lease: lease.as_mut(),
            deadline: Instant::now() + Duration::from_secs(120),
        },
    )
    .unwrap();
    drop(restored);
    assert_eq!(lease.checkpoint().unwrap().state, timed.state);
    drop(lease);
    server.join().unwrap();
    let current = backend
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    assert_eq!(current.state, timed.state);
    assert!(
        current
            .successor_archive_transfer
            .unwrap()
            .reserved_response_bytes
            > 0
    );
}

#[path = "driver_bft_v3_source_role_pin_tests.rs"]
mod role_pin;
