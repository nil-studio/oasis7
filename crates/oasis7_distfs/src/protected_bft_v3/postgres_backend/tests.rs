//! Software SQL/transport evidence only, not independent hardware qualification.
use super::*;
use crate::protected_bft_v3::rotation::tests::{binding, hash, key, public};
use BftRetainedOriginalOperationV1 as Original;
use ed25519_dalek::Signer;
use first_reservation::prepare_first_reservation as reserve;
use std::{collections::BTreeMap, net::TcpListener, sync::Arc, thread, time::Duration};

struct Key(u8);
impl BftSigningKeyHandleV3 for Key {
    fn public_key_hex(&self) -> String {
        public(self.0)
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        Ok(key(self.0).sign(bytes).to_bytes())
    }
}
fn initial(binding: &BftSignerBindingV3) -> BftSignerBackendSnapshotV3 {
    BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 17,
    }
}
fn retained(binding: &BftSignerBindingV3) -> BftOriginalRetentionTxnSnapshotV1 {
    let mut signer = initial(binding);
    let source = BftPrivateRetentionSourceV1 {
        binding_digest: binding.digest().unwrap(),
        handoff_digest: binding.handoff_record_id.clone(),
        material_source_digest: binding.material_identity.clone(),
    };
    let marker = BftPrivateRetentionCreationMarkerV1 {
        schema_version: 1,
        creation_identity: hash(81),
        source: source.clone(),
        ledger_origin_digest: hash(82),
        activation_digest: binding.activation_digest.clone(),
        initial_state_digest: signer.state.state_digest.clone(),
        initial_height: binding.initial_schedule().height(),
    };
    let attestor = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: marker.initial_height,
        allow_authenticated_successor_origins: false,
    };
    let (activation, activated) =
        BftPrivateRetentionActivationV1::prepare(&attestor, &signer, 0, &Key(33)).unwrap();
    let mut objects: Vec<_> = [
        BftPrivateRetentionInputObjectKindV1::OriginalData,
        BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, kind)| {
        let bytes = vec![0, 255, i as u8, 13];
        BftPrivateRetentionInputObjectV1 {
            kind,
            reference: BftSuccessorWindowArchiveRefV1 {
                digest: blake3::hash(&bytes).to_hex().to_string(),
                bytes: bytes.len() as u64,
            },
            bytes,
        }
    })
    .collect();
    objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    let input = Arc::new(BftPrivateRetentionAdmissionInputV1 {
        schema_version: 1,
        source: source.clone(),
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: marker.ledger_origin_digest.clone(),
            height: marker.initial_height,
            header_id: hash(83),
        },
        publication_reservation_bytes: 1024,
        objects,
    });
    let reference = input.descriptor().unwrap();
    let (reservation, state) =
        reserve(&activated, &activation, &reference, input, 1, &Key(33)).unwrap();
    signer.state = state;
    let signed_request_bytes = vec![0, 255, 13, 10, 91];
    let signed_receipt_bytes = vec![128, 0, 9, 99];
    // Opaque original byte fixture: encoding never claims signature authority.
    let operation = BftRetainedOriginalOperationV1 {
        schema_version: 1,
        operation_id: "original-1".into(),
        source,
        ledger_origin_digest: marker.ledger_origin_digest.clone(),
        original_request_digest: Original::request_digest_from_original_bytes(
            &signed_request_bytes,
        ),
        signed_request_bytes,
        before_revision: 1,
        before_state_digest: activated.state_digest,
        after_revision: 2,
        after_state_digest: signer.state.state_digest.clone(),
        original_lease_generation: 1,
        original_lease_token_digest: hash(84),
        input_refs: vec![reference],
        original_receipt_digest: Original::receipt_digest_from_original_bytes(
            &signed_receipt_bytes,
        ),
        signed_receipt_bytes,
        cumulative_reserved_response_bytes: 123,
    };
    BftOriginalRetentionTxnSnapshotV1 {
        current_retention: None,
        current: BftFirstReserveSnapshotV1 {
            current: BftFreshCoverageSnapshotV1 {
                signer,
                marker,
                original: Some(activation),
                reserved_response_bytes: 123,
            },
            reservation: Some(reservation),
        },
        original_operations: BTreeMap::from([(operation.operation_id.clone(), operation)]),
    }
}

#[test]
fn pg_whole_namespace_codec_preserves_original_input_and_signed_bytes() {
    let envelope = retained(&binding());
    let signer = envelope.current.current.signer.clone();
    let stored = codec::Stored::new("codec", signer.clone(), Some(envelope.clone())).unwrap();
    let bytes = stored.encode().unwrap();
    // Fixed from the actual schema-1 codec at main49e0a1b8 before this change.
    let state_bytes = codec::Stored::new("codec", initial(&binding()), None)
        .unwrap()
        .encode()
        .unwrap();
    for (original, length, digest) in [
        (
            &bytes,
            8709,
            "8e63363219e5610a6ff7ed72680f560a23adb36089c3f3f083f93429f9ec1dfe",
        ),
        (
            &state_bytes,
            1457,
            "f8d36280c29059199847f0dee36162cf19a21d869ccce46c5c1e4745d05937d0",
        ),
    ] {
        assert_eq!(original.len(), length);
        assert_eq!(blake3::hash(original).to_hex().as_str(), digest);
        assert_eq!(
            &codec::Stored::decode(original).unwrap().encode().unwrap(),
            original
        );
    }
    assert_eq!(codec::Stored::decode(&bytes).unwrap(), stored);
    assert!(codec::Stored::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(codec::Stored::decode(&trailing).is_err());
    assert!(codec::Stored::decode(&[0x9b, 255, 255, 255, 255, 255, 255, 255, 255]).is_err());
    let mut wrong = envelope.clone();
    let wrong_operation = wrong.original_operations.get_mut("original-1").unwrap();
    wrong_operation.signed_receipt_bytes.push(1);
    assert!(codec::Stored::new("codec", signer.clone(), Some(wrong)).is_err());
    let mut wrong = signer.clone();
    wrong.protected_time_floor_ms += 1;
    assert!(codec::Stored::new("codec", wrong, Some(envelope)).is_err());
    assert!(codec::Stored::new("invalid namespace", signer, None).is_err());
    let noncanonical = serde_cbor::value::to_value(stored).unwrap();
    assert!(codec::Stored::decode(&serde_cbor::to_vec(&noncanonical).unwrap()).is_err());
}

#[path = "../private_retention_ready_tests.rs"]
mod first_ready;

fn connect() -> Client {
    let url = std::env::var("OASIS7_POSTGRES_TEST_URL")
        .expect("explicit disposable PostgreSQL test URL is required");
    Client::connect(&url, postgres::NoTls).unwrap()
}
fn backend(name: &str, binding: &str) -> PostgresBftSignerBackendV3 {
    PostgresBftSignerBackendV3::new(name.into(), binding.into(), connect()).unwrap()
}
#[test]
#[ignore = "requires explicitly provisioned disposable PostgreSQL; missing URL fails"]
fn pg_real_sql_and_native_signer_preserve_lease_and_unknown_commit() {
    let binding = binding();
    let digest = binding.digest().unwrap();
    let original = initial(&binding);
    let mut admin = connect();
    admin.batch_execute(POSTGRES_BFT_SCHEMA_V1).unwrap();
    for name in ["native", "lost-reply", "lost-connection"] {
        let bytes =
            PostgresBftSignerBackendV3::provisioned_bytes(name, original.clone(), None).unwrap();
        admin
            .execute(
                "INSERT INTO oasis7_bft_v3_namespaces VALUES ($1, $2, $3)",
                &[&name, &digest, &bytes],
            )
            .unwrap();
    }
    assert!(
        backend("absent", &digest)
            .load_provisioned(&digest)
            .is_err()
    );
    assert!(
        backend("native", &hash(99))
            .load_provisioned(&hash(99))
            .is_err()
    );
    let service = BftSignerServiceV3::open(
        binding.clone(),
        Box::new(Key(33)),
        vec![Box::new(Key(31)), Box::new(Key(32))],
        Box::new(backend("native", &digest)),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        service.serve_connection(&mut stream).unwrap();
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(20),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: digest.clone(),
        },
    )
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(lease.checkpoint().unwrap().state, original.state);
    drop(lease);
    server.join().unwrap();
    let mut live = backend("native", &digest);
    let released = live.load_provisioned(&digest).unwrap();
    let mut expected = original.clone();
    expected.lease_generation = 3;
    assert_eq!(released, expected);
    assert!(matches!(
        live.acquire_lease(&digest, 3, &hash(91)).unwrap(),
        BftSignerAcquireOutcomeV3::Acquired(_)
    ));
    let mut contender = backend("native", &digest);
    assert!(matches!(
        contender.acquire_lease(&digest, 4, &hash(92)).unwrap(),
        BftSignerAcquireOutcomeV3::Conflict(_)
    ));
    assert!(contender.release_lease(2, &hash(92)).is_err());
    assert!(live.protected_time_ms(4, &hash(91)).is_err());
    let next = BftSignerStateV3::from_candidate(&binding, 1, original.state.candidate()).unwrap();
    let before = &original.state.state_digest;
    assert!(
        live.compare_and_swap(4, &hash(91), 0, before, Some(18), next.clone())
            .is_err()
    );
    let committed = live
        .compare_and_swap(4, &hash(91), 0, before, None, next.clone())
        .unwrap();
    assert_eq!(committed.state, next);
    assert_eq!(
        backend("native", &digest)
            .read_for_lease(4, &hash(91))
            .unwrap(),
        committed
    );
    type Fault = fn(&mut Client) -> Result<(), BftSignerErrorV3>;
    for (name, fault) in [
        ("lost-reply", lost_reply as Fault),
        ("lost-connection", terminate_connection as Fault),
    ] {
        let mut uncertain = backend(name, &digest);
        uncertain.after_commit = Some(fault);
        assert!(uncertain.acquire_lease(&digest, 1, &hash(93)).is_err());
        assert!(uncertain.load_provisioned(&digest).is_err());
        let mut cold = backend(name, &digest);
        let saved = cold.read_for_lease(2, &hash(93)).unwrap();
        let mut expected = original.clone();
        expected.lease_generation = 2;
        expected.lease_token_digest = hash(93);
        assert_eq!(saved, expected);
        assert!(matches!(
            cold.acquire_lease(&digest, 2, &hash(94)).unwrap(),
            BftSignerAcquireOutcomeV3::Conflict(_)
        ));
    }
    let envelope = retained(&binding);
    let mut stored = codec::Stored::new(
        "retained",
        envelope.current.current.signer.clone(),
        Some(envelope),
    )
    .unwrap();
    let bytes = stored.encode().unwrap();
    admin
        .execute(
            "INSERT INTO oasis7_bft_v3_namespaces VALUES ($1, $2, $3)",
            &[&"retained", &digest, &bytes],
        )
        .unwrap();
    let mut retained_backend = backend("retained", &digest);
    retained_backend
        .acquire_lease(&digest, 1, &hash(95))
        .unwrap();
    retained_backend.release_lease(2, &hash(95)).unwrap();
    stored.signer_mut().lease_generation = 3;
    let cold_bytes: Vec<u8> = admin.query_one(READ, &[&"retained"]).unwrap().get(1);
    assert_eq!(cold_bytes, stored.encode().unwrap());
    pg_native_retention(&mut admin, &binding);
    pg_ready_transaction(&mut admin);
}
fn insert_envelope(
    admin: &mut Client,
    name: &str,
    envelope: &BftOriginalRetentionTxnSnapshotV1,
) {
    let signer = &envelope.current.current.signer;
    let bytes = PostgresBftSignerBackendV3::provisioned_bytes(
        name,
        signer.clone(),
        Some(envelope.clone()),
    ).unwrap();
    admin.execute(
        "INSERT INTO oasis7_bft_v3_namespaces VALUES ($1, $2, $3)",
        &[&name, &signer.state.binding_digest, &bytes],
    ).unwrap();
}

fn pg_ready_transaction(admin: &mut Client) {
    let (before, after) = first_ready::pg_ready_envelopes();
    let signer = &before.current.current.signer;
    let digest = &signer.state.binding_digest;
    let generation = signer.lease_generation;
    let token = &signer.lease_token_digest;
    for name in ["ready-normal", "ready-lost", "ready-disconnect"] {
        insert_envelope(admin, name, &before);
        let mut writer = backend(name, digest);
        assert_eq!(
            writer.read_original_operation_envelope_for_lease(generation, token).unwrap(),
            before
        );
        match name {
            "ready-lost" => writer.after_commit = Some(lost_reply),
            "ready-disconnect" => writer.after_commit = Some(terminate_connection),
            _ => {}
        }
        let result = writer.compare_and_commit_original_operation_envelope(&before, &after);
        assert_eq!(result.is_ok(), name == "ready-normal");
        if name != "ready-normal" {
            assert!(writer.read_for_lease(generation, token).is_err());
        }
        let mut cold = backend(name, digest);
        assert_eq!(
            cold.read_original_operation_envelope_for_lease(generation, token).unwrap(),
            after
        );
        let stored = codec::Stored::new(name, after.current.current.signer.clone(), Some(after.clone())).unwrap();
        let bytes: Vec<u8> = admin.query_one(READ, &[&name]).unwrap().get(1);
        assert_eq!(bytes, stored.encode().unwrap());
        // A legacy projection may charge a response but must retain all
        // historical signed bytes and the current Ready selection unchanged.
        let legacy = cold.read_first_reserve_for_lease(generation, token).unwrap();
        let mut charged = legacy.clone();
        charged.current.reserved_response_bytes += 17;
        cold.compare_and_commit_first_reserve(&legacy, &charged).unwrap();
        let mut charged_whole = after.clone();
        charged_whole.current = charged;
        assert_eq!(cold.read_original_operation_envelope_for_lease(generation, token).unwrap(), charged_whole);
        // The actual Noise charge primitive consumes the same whole namespace.
        let next = charged_whole.prepare_original_response_charge(generation, token, 33).unwrap();
        cold.compare_and_commit_original_operation_envelope(&charged_whole, &next).unwrap();
        assert_eq!(cold.read_original_operation_envelope_for_lease(generation, token).unwrap(), next);
        cold.release_lease(generation, token).unwrap();
        let mut reopened = backend(name, digest);
        let new_token = hash(98);
        let BftSignerAcquireOutcomeV3::Acquired(new_signer) = reopened.acquire_lease(
            digest, generation + 1, &new_token,
        ).unwrap() else { panic!("cold lease did not acquire") };
        let mut recovered = next.clone();
        recovered.current.current.signer = new_signer;
        assert_eq!(reopened.read_original_operation_envelope_for_lease(generation + 2, &new_token).unwrap(), recovered);
        assert!(backend(name, digest).read_original_operation_envelope_for_lease(generation, token).is_err());
        let mut downgraded = recovered.clone();
        downgraded.current_retention = None;
        assert!(backend(name, digest).compare_and_commit_original_operation_envelope(&recovered, &downgraded).is_err());
        let mut rollback = recovered.clone();
        rollback.current.current.reserved_response_bytes -= 1;
        assert!(backend(name, digest).compare_and_commit_original_operation_envelope(&recovered, &rollback).is_err());
        let mut altered = recovered.clone();
        let operation = altered.original_operations.values_mut().next().unwrap();
        operation.signed_request_bytes.push(5);
        operation.original_request_digest = Original::request_digest_from_original_bytes(&operation.signed_request_bytes);
        altered.current.current.reserved_response_bytes += 1;
        assert!(backend(name, digest).compare_and_commit_original_operation_envelope(&recovered, &altered).is_err());
        let mut stale = recovered.clone();
        stale.current.current.signer.protected_time_floor_ms += 1;
        assert!(backend(name, digest).compare_and_commit_original_operation_envelope(&stale, &recovered).is_err());
        assert_eq!(reopened.read_original_operation_envelope_for_lease(generation + 2, &new_token).unwrap(), recovered);
    }
}

struct FreshSource(BftPrivateRetentionCreationMarkerV1);
impl BftFreshSourceVerifierV1 for FreshSource {
    fn verify_original_runtime_source(
        &self, binding: &BftSignerBindingV3, marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(), BftSignerErrorV3> {
        if &self.0 != marker || binding.digest()? != marker.source.binding_digest {
            return Err(invalid("PG test source differs"));
        }
        Ok(())
    }
}
struct ReserveSource(Arc<BftPrivateRetentionAdmissionInputV1>);
fn pg_price() -> BftFirstReservationPriceV1 {
    BftFirstReservationPriceV1 {
        publication_bytes: 512,
        private_pin_bytes: 17,
        original_selection_bytes: 22,
        retained_h_dependencies: Default::default(),
    }
}
impl BftFirstReserveSourceV1 for ReserveSource {
    fn read_authorized_original(
        &self, reference: &BftPrivateRetentionAdmissionInputRefV1, _: std::time::Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3> {
        if self.0.descriptor()? != *reference {
            return Err(invalid("PG test input reference differs"));
        }
        Ok(BftAuthorizedOriginalReserveInputV1 {
            input: self.0.clone(),
            transport_hold: Box::new(()),
        })
    }
    fn verify_actual_first_price(
        &self, _: &BftSignerBindingV3, _: &BftPrivateRetentionCreationMarkerV1,
        _: &BftSignerStateV3, input: &BftPrivateRetentionAdmissionInputV1, _: std::time::Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3> {
        if self.0.as_ref() != input {
            return Err(invalid("PG test original input differs"));
        }
        Ok(pg_price())
    }
}
fn pg_native_retention(admin: &mut Client, binding: &BftSignerBindingV3) {
    let digest = binding.digest().unwrap();
    let sample = retained(binding);
    let marker = sample.current.current.marker.clone();
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: marker.source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: marker.initial_height,
        allow_authenticated_successor_origins: false,
    };
    let mut input = (*sample.current.reservation.unwrap().input).clone();
    input.publication_reservation_bytes = pg_price().required_input_bytes(&input).unwrap();
    let input = Arc::new(input);
    let initial = BftOriginalRetentionTxnSnapshotV1 {
        current: BftFirstReserveSnapshotV1 {
            current: BftFreshCoverageSnapshotV1 {
                signer: initial(binding),
                marker: marker.clone(),
                original: None,
                reserved_response_bytes: 0,
            },
            reservation: None,
        },
        current_retention: None,
        original_operations: BTreeMap::new(),
    };
    for recorded in [false, true] {
        let name = if recorded { "pg-recorded" } else { "pg-legacy" };
        insert_envelope(admin, name, &initial);
        let service = BftSignerServiceV3::open_with_fresh_coverage(
            binding.clone(),
            Box::new(Key(33)),
            vec![Box::new(Key(31)), Box::new(Key(32))],
            Box::new(backend(name, &digest)),
            BftFreshCoverageSupportV1 {
                attestor: role.clone(),
                marker: marker.clone(),
                verifier: Box::new(FreshSource(marker.clone())),
            },
        ).unwrap();
        let source = Box::new(ReserveSource(input.clone()));
        let timeout = Duration::from_secs(20);
        let service = if recorded {
            service.with_recorded_first_reservation(source, timeout)
        } else {
            service.with_first_reservation(source, timeout)
        }.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(timeout)).unwrap();
            service.serve_connection(&mut stream).unwrap();
        });
        let provider = RemoteBftSignerProviderV3::new(
            address, timeout, binding.clone(), key(34),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: public(33),
                binding_digest: digest.clone(),
            },
        ).unwrap();
        let mut lease = provider.acquire(binding).unwrap();
        let activation = lease.provision_private_retention(&role, &marker).unwrap();
        let ledger = lease.reserve_first_private_retention(&role, &marker, &activation, &input).unwrap();
        let checkpoint = lease.checkpoint().unwrap();
        assert_eq!(checkpoint.state.revision, 2);
        assert_eq!(lease.reserve_first_private_retention(&role, &marker, &activation, &input).unwrap(), ledger);
        drop(lease);
        server.join().unwrap();
        let mut cold = backend(name, &digest);
        let token = hash(96);
        let BftSignerAcquireOutcomeV3::Acquired(signer) = cold.acquire_lease(&digest, checkpoint.lease_generation + 1, &token).unwrap()
            else { panic!("PG cold lease did not acquire") };
        let envelope = cold.read_original_operation_envelope_for_lease(signer.lease_generation, &token).unwrap();
        assert_eq!(envelope.current.current.original, Some(activation));
        let original = envelope.current.reservation.unwrap();
        assert_eq!(original.ledger, ledger);
        assert_eq!(original.input, input);
        assert_eq!(envelope.original_operations.len(), usize::from(recorded));
        assert!(envelope.current.current.reserved_response_bytes > 0);
    }
}

fn lost_reply(_: &mut Client) -> Result<(), BftSignerErrorV3> {
    // Real SQL COMMIT succeeded; intentionally drop confirmation at test boundary.
    Err(invalid("test: lost successful commit confirmation"))
}
fn terminate_connection(client: &mut Client) -> Result<(), BftSignerErrorV3> {
    // Real server terminates this TCP connection after COMMIT, before readback.
    assert!(
        client
            .simple_query("SELECT pg_terminate_backend(pg_backend_pid())")
            .is_err()
    );
    Ok(())
}
