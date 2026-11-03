//! A software namespace marker cannot silently become legacy signing authority.
//! Actual TCP and Memory backend; this does not prove Runtime material custody.
use super::*;
use crate::protected_bft_v3::private_retention::*;

#[test]
fn private_retention_tcp_fresh_no_role_cannot_start_timer_or_append_qc() {
    // Separate sessions ensure a prior poisoned operation cannot explain the
    // second rejection. Both requests are otherwise valid legacy operations.
    for append_qc in [false, true] {
        fresh_no_role(append_qc);
    }
}

fn fresh_no_role(append_qc: bool) {
    let (binding, old) = fixture_kind(!append_qc);
    let certificate = old.lock().unwrap().state.history[0].clone();
    let namespace = BftPrivateRetentionFreshSeedV1::create(&binding)
        .unwrap()
        .bind_origin(hash(91))
        .unwrap();
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: BftPrivateRetentionSourceV1 {
            binding_digest: binding.digest().unwrap(),
            handoff_digest: binding.handoff_record_id.clone(),
            material_source_digest: binding.material_identity.clone(),
        },
        creation_marker_digest: namespace.marker().digest().unwrap(),
        coverage_start_height: binding.initial_schedule().height(),
        allow_authenticated_successor_origins: true,
    };
    let backend = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        role,
        Some(BftSuccessorArchiveTransferLedgerV1 {
            schema_version: 1,
            reserved_response_bytes: 0,
        }),
        1000,
    )
    .unwrap();
    let mut retained = backend.clone();
    let validator_calls = Arc::new(AtomicUsize::new(0));
    // Intentionally omit retention support from both ends. The original marker
    // must still fence the backend before an uncovered generic State mutation.
    let service = BftSignerServiceV3::open(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(CountHandle {
                key: key(31),
                calls: validator_calls.clone(),
            }),
            Box::new(CountHandle {
                key: key(32),
                calls: validator_calls.clone(),
            }),
        ],
        Box::new(backend),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        service.serve_tcp_connection(&mut stream)
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    let checkpoint = lease.checkpoint().unwrap();
    let mut before = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    let rejected = if append_qc {
        lease.append_committed_certificate(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            certificate,
        )
    } else {
        lease.start_height_timer(checkpoint.state.revision)
    }
    .unwrap_err();
    assert!(
        rejected.to_string().contains("generic CAS mismatch"),
        "valid legacy operation must reach the fresh-source fence: {rejected}"
    );
    let after = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    // Reading protected time may durably advance its anti-rollback floor even
    // though the subsequent generic State CAS is refused. It cannot regress.
    assert!(after.protected_time_floor_ms >= before.protected_time_floor_ms);
    before.protected_time_floor_ms = after.protected_time_floor_ms;
    assert_eq!(
        after, before,
        "rejection preserves State, current lease and transfer ledger"
    );
    assert_eq!(validator_calls.load(Ordering::SeqCst), 0);
    drop(lease);
    assert_eq!(
        server.join().unwrap(),
        Err(BftSignerErrorV3("BFT V3 provider is poisoned".into()))
    );
}
