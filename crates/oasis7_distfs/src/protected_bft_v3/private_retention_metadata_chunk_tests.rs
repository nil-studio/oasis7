//! A real TCP proxy re-signs malicious metadata frames with the pinned provider
//! key. This proves client binding checks, not custody of the attacker's bytes.
use super::*;
use crate::protected_bft_v3::crypto::digest_hex;
use crate::protected_bft_v3::retention_proof_query::RetentionProofReadV1;
use crate::protected_bft_v3::wire::{
    BftSignerCommandV3, BftSignerReplyKindV3, SignedBftSignerReplyV3, SignedBftSignerRequestV3,
    encode_frame, read_frame, validate_reply,
};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};

#[test]
fn private_retention_metadata_client_rejects_validly_signed_malformed_chunks() {
    let (binding, old) = fixture_kind(false);
    let certificate = old.lock().unwrap().state.history[0].clone();
    let seed = BftPrivateRetentionFreshSeedV1::create(&binding).unwrap();
    let creation_identity = seed.creation_identity().to_string();
    let namespace = seed.bind_origin(hash(221)).unwrap();
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
        role.clone(),
        Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
        1000,
    )
    .unwrap();
    let mut retained = backend.clone();
    let candidate_key = BftPrivateRetentionKeyV1 {
        ledger_origin_digest: hash(221),
        height: certificate.execution_commitment.height,
        header_id: certificate.header.block_id().unwrap(),
    };
    let input =
        crate::protected_bft_v3::private_retention_tests::input_for(&role.source, &candidate_key);
    let entry = BftPrivateRetentionEntryV1 {
        key: candidate_key,
        fact: input.fact().unwrap(),
        lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
    };
    // These explicit doubles establish only transport/atomic metadata behavior.
    let support = BftPrivateRetentionSupportV1::new(
        role.clone(),
        Arc::new(MaterialDouble {
            expected: entry,
            ready: Arc::new(AtomicUsize::new(0)),
            creation_identity,
            input: input.clone(),
        }),
        Duration::from_secs(10),
    )
    .unwrap()
    .with_admission_input_reader(Arc::new(InputReaderDouble(
        input.clone(),
        Arc::new(AtomicUsize::new(0)),
    )));
    let service = BftSignerServiceV3::open_with_private_retention(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(Handle {
                key: key(31),
                disabled: None,
            }),
            Box::new(Handle {
                key: key(32),
                disabled: None,
            }),
        ],
        Box::new(backend),
        None,
        support,
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
    let mode = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let counters = Arc::new(Mutex::new(Vec::new()));
    let (address, proxy) = malicious_proxy(
        address,
        binding.clone(),
        mode.clone(),
        reads.clone(),
        counters.clone(),
    );
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
    .unwrap()
    .with_private_retention_attestor(role)
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    let deadline = || Instant::now() + Duration::from_secs(10);
    let original = lease.provision_private_retention(deadline()).unwrap();
    lease
        .reserve_private_retention_input(input.descriptor().unwrap(), deadline())
        .unwrap();
    let current = lease.checkpoint().unwrap();
    let digest = digest_hex("oasis7.bft.private-retention.v1.receipt", &original.receipt).unwrap();
    let good = lease
        .read_private_retention_operation_proof(digest.clone(), deadline())
        .unwrap();
    assert!(
        good.state.revision < current.state.revision,
        "the proof must be genuinely historical to detect cache replacement"
    );
    assert_eq!(good.ledger, original);
    let request_count = reads.lock().unwrap().len();
    assert!(
        lease
            .read_private_retention_operation_proof(digest.clone(), Instant::now())
            .is_err()
    );
    assert_eq!(
        reads.lock().unwrap().len(),
        request_count,
        "an expired absolute deadline cannot start a new frame"
    );
    for attack in 1..=5 {
        let before = retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap();
        mode.store(attack, Ordering::SeqCst);
        let fixed_deadline = deadline();
        let error = lease
            .read_private_retention_operation_proof(digest.clone(), fixed_deadline)
            .unwrap_err();
        let expected = match attack {
            1 => "hash differs",
            2 => "binding differs",
            3 => "incomplete canonical chunk",
            4 => "rejected request",
            5 => "frame header",
            _ => unreachable!(),
        };
        assert!(
            error.to_string().contains(expected),
            "signed metadata attack {attack} must be rejected by its actual consumer: {error}"
        );
        let after = retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap();
        assert_eq!(after.state, before.state);
        assert!(
            after
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes
                > before
                    .successor_archive_transfer
                    .unwrap()
                    .reserved_response_bytes,
            "the real upstream proof frame remains charged even when the receiving client rejects it"
        );
        mode.store(0, Ordering::SeqCst);
        if attack != 5 {
            let proof = lease
                .read_private_retention_operation_proof(digest.clone(), fixed_deadline)
                .unwrap();
            assert_eq!(proof, good);
            let last = reads.lock().unwrap().last().unwrap().clone();
            assert_eq!(
                last.revision, current.state.revision,
                "the next real request exposes the unchanged live cached CP"
            );
            assert_eq!(last.state_digest, current.state.state_digest);
            assert_eq!(last.offset, 0);
        }
    }
    assert!(
        counters
            .lock()
            .unwrap()
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
    // EOF prevents inspecting this old lease's cache through another request;
    // this case claims only rejection and unchanged actual backend State.
    drop(lease);
    proxy.join().unwrap();
    server.join().unwrap().unwrap();
}

fn malicious_proxy(
    address: SocketAddr,
    binding: BftSignerBindingV3,
    mode: Arc<AtomicUsize>,
    reads: Arc<Mutex<Vec<RetentionProofReadV1>>>,
    counters: Arc<Mutex<Vec<u64>>>,
) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut upstream = TcpStream::connect(address).unwrap();
        let mut continuation = false;
        while let Ok(request) = read_frame::<SignedBftSignerRequestV3>(&mut client) {
            let metadata = if let BftSignerCommandV3::ReadPrivateRetentionProof { read } =
                &request.body.command
            {
                reads.lock().unwrap().push(read.as_ref().clone());
                Some(read.as_ref())
            } else {
                None
            };
            upstream
                .write_all(&encode_frame(&request).unwrap())
                .unwrap();
            let mut reply: SignedBftSignerReplyV3 = match read_frame(&mut upstream) {
                Ok(reply) => reply,
                Err(_) => break,
            };
            let attack = mode.load(Ordering::SeqCst);
            if metadata.is_some() && continuation {
                assert!(
                    matches!(&reply.body.result, BftSignerReplyKindV3::Rejected { .. }),
                    "the genuine service must reject the attacker-selected nonexistent continuation"
                );
                assert_eq!(
                    metadata.unwrap().offset,
                    crate::protected_bft_v3::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1
                        as u64
                );
                continuation = false;
                if attack == 5 {
                    break;
                }
            } else if let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } =
                &mut reply.body.result
            {
                counters.lock().unwrap().push(chunk.reserved_response_bytes);
                match attack {
                    0 => {}
                    1 => chunk.object.digest = hash(242),
                    2 => chunk.read.offset += 1,
                    3 => {
                        chunk.bytes.pop().unwrap();
                    }
                    4 | 5 => {
                        // Deliberately malicious claimed object, not a valid
                        // enlarged State or a maximum-capacity positive proof.
                        let size = crate::protected_bft_v3::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1;
                        chunk.object.bytes = size as u64 + 1;
                        chunk.object.digest = hash(243);
                        chunk.bytes = vec![0xaa; size];
                        continuation = true;
                    }
                    _ => unreachable!(),
                }
                if attack != 0 {
                    reply = SignedBftSignerReplyV3::sign(
                        reply.body,
                        &Handle {
                            key: key(33),
                            disabled: None,
                        },
                    )
                    .unwrap();
                    validate_reply(
                        &binding,
                        &BftSignerTrustV3 {
                            schema_version: 3,
                            provider_public_key: public(33),
                            binding_digest: binding.digest().unwrap(),
                        },
                        &request,
                        &reply,
                    )
                    .unwrap();
                }
            }
            if client.write_all(&encode_frame(&reply).unwrap()).is_err() {
                break;
            }
        }
    });
    (endpoint, worker)
}
