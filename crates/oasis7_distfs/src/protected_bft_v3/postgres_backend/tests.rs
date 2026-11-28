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
