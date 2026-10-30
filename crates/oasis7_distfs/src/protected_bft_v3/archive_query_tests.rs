//! Actual authenticated loopback TCP; backend/archive/custody remain explicit doubles.
use super::*;
use crate::protected_bft_v3::archive_query::*;
use crate::protected_bft_v3::successor_public_archive::*;
use crate::protected_bft_v3::wire::*;
use std::net::TcpStream;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn counter(snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>) -> u64 {
    snapshot
        .lock()
        .unwrap()
        .successor_archive_transfer
        .unwrap()
        .reserved_response_bytes
}
fn install(
    indexed: bool,
    ledger: bool,
) -> (
    BftSignerBindingV3,
    Arc<Mutex<BftSignerBackendSnapshotV3>>,
    Arc<Archive>,
    BftSignerStateV3,
    String,
    String,
) {
    let (binding, snapshot) = fixture_kind(true);
    if ledger {
        snapshot.lock().unwrap().successor_archive_transfer =
            Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
    }
    let original = snapshot.lock().unwrap().state.clone();
    let archive = Arc::new(Archive::default());
    let verifier = Arc::new(DriverVerifier {
        expected: original.clone(),
        reject: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), Some(verifier));
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let (mut body, mut manifest) = request(&binding, &cp);
    if indexed {
        index_whole_state_signed_results_v1(&mut manifest, &cp.state).unwrap();
        body.checkpoint.public_archive_root = manifest.digest().unwrap();
    }
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    lease.prepare_successor_window(body, manifest).unwrap();
    lease.commit_successor_window(&id, &digest).unwrap();
    drop(lease);
    server.join().unwrap();
    (binding, snapshot, archive, original, id, digest)
}
fn proposal_identity(
    state: &BftSignerStateV3,
) -> (
    BftSuccessorWindowArchiveIdentityV1,
    BftSuccessorWindowOriginalSignedResultV1,
) {
    let proposal = state
        .completed_journals
        .iter()
        .flatten()
        .flat_map(|j| &j.events)
        .find_map(|event| match event {
            BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } => Some(proposal.clone()),
            _ => None,
        })
        .unwrap();
    (
        BftSuccessorWindowArchiveIdentityV1::SigningSlot {
            public_key_hex: proposal.header.public_key_hex.clone(),
            height: proposal.header.height,
            round: proposal.header.round,
            phase: "proposal".into(),
            preimage_digest: blake3::hash(&proposal.header.signing_payload().unwrap())
                .to_hex()
                .to_string(),
        },
        BftSuccessorWindowOriginalSignedResultV1::Proposal(proposal),
    )
}
#[test]
fn archive_query_tcp_original_local_found_conflict_missing_and_persistent_charge() {
    for indexed in [false, true] {
        let (binding, snapshot, archive, original, id, digest) = install(indexed, true);
        let frozen = snapshot.lock().unwrap().state.clone();
        let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
        let mut lease = provider.acquire(&binding).unwrap();
        let state = lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap();
        assert_eq!(state.original_state(), &original);
        let charged = counter(&snapshot);
        assert!(charged > oasis7_cas::to_canonical_cbor(&original).unwrap().len() as u64);
        let (identity, expected) = proposal_identity(&original);
        assert_eq!(
            lease
                .query_successor_archive(&id, &digest, &identity, deadline())
                .unwrap(),
            Some(expected)
        );
        let mut conflict = identity.clone();
        if let BftSuccessorWindowArchiveIdentityV1::SigningSlot {
            preimage_digest, ..
        } = &mut conflict
        {
            *preimage_digest = hash(99);
        }
        assert!(
            lease
                .query_successor_archive(&id, &digest, &conflict, deadline())
                .unwrap_err()
                .0
                .contains("preimage conflict")
        );
        let mut missing = identity;
        if let BftSuccessorWindowArchiveIdentityV1::SigningSlot { round, .. } = &mut missing {
            *round += 1;
        }
        assert_eq!(
            lease
                .query_successor_archive(&id, &digest, &missing, deadline())
                .unwrap(),
            None
        );
        assert!(counter(&snapshot) > charged);
        assert_eq!(snapshot.lock().unwrap().state, frozen);
        let before_expired = counter(&snapshot);
        assert!(
            lease
                .read_successor_original_state(&id, &digest, Instant::now())
                .is_err()
        );
        assert_eq!(counter(&snapshot), before_expired);
        drop(lease);
        server.join().unwrap();
        let before_new_owner = counter(&snapshot);
        let (provider, server) = native_successor(&binding, &snapshot, archive, None);
        let mut lease = provider.acquire(&binding).unwrap();
        assert_eq!(counter(&snapshot), before_new_owner);
        assert_eq!(
            lease
                .read_successor_original_state(&id, &digest, deadline())
                .unwrap()
                .original_state(),
            &original
        );
        assert!(counter(&snapshot) > before_new_owner);
        assert_eq!(snapshot.lock().unwrap().state, frozen);
        drop(lease);
        server.join().unwrap();
    }
}
#[test]
fn archive_query_tcp_unsupported_and_missing_material_are_errors_not_absence() {
    let (binding, snapshot, archive, original, id, digest) = install(true, false);
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(
        lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap_err()
            .0
            .contains("unsupported")
    );
    assert!(
        snapshot
            .lock()
            .unwrap()
            .successor_archive_transfer
            .is_none()
    );
    drop(lease);
    server.join().unwrap();
    // Explicit namespace migration is outside this read path.
    snapshot.lock().unwrap().successor_archive_transfer =
        Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    archive.states.lock().unwrap().clear();
    assert!(
        lease
            .query_successor_archive(&id, &digest, &proposal_identity(&original).0, deadline())
            .is_err()
    );
    assert_eq!(counter(&snapshot), 0);
    drop(lease);
    server.join().unwrap();
}
fn exchange(
    stream: &mut TcpStream,
    binding: &BftSignerBindingV3,
    lease: Option<BftSignerLeaseCredentialV3>,
    command: BftSignerCommandV3,
    nonce: u64,
) -> SignedBftSignerReplyV3 {
    let request =
        SignedBftSignerRequestV3::sign(binding, &key(34), lease, command, format!("{nonce:064x}"))
            .unwrap();
    std::io::Write::write_all(stream, &encode_frame(&request).unwrap()).unwrap();
    let reply = read_frame(stream).unwrap();
    validate_reply(
        binding,
        &BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
        &request,
        &reply,
    )
    .unwrap();
    reply
}
#[test]
fn archive_query_tcp_bounded_chunks_reassemble_and_reject_changed_coordinates_without_charge() {
    let (binding, snapshot, archive, original, id, digest) = install(true, true);
    let frozen = snapshot.lock().unwrap().state.clone();
    let record = frozen
        .successor_window
        .as_ref()
        .unwrap()
        .request(&id)
        .unwrap();
    let minimum = frozen
        .successor_window
        .as_ref()
        .unwrap()
        .minimum
        .clone()
        .unwrap();
    let support = BftSuccessorWindowSupportV1::new(archive, Duration::from_secs(30)).unwrap();
    let service = BftSignerServiceV3::open_with_successor_window(
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
        Box::new(Backend {
            binding: binding.digest().unwrap(),
            snapshot: snapshot.clone(),
        }),
        support,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        service.serve_tcp_connection(&mut stream).unwrap();
    });
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let prepared = exchange(
        &mut stream,
        &binding,
        None,
        BftSignerCommandV3::PrepareAcquire {
            binding_digest: binding.digest().unwrap(),
        },
        1,
    );
    let BftSignerReplyKindV3::Prepared {
        expected_generation,
        prepare_nonce,
    } = prepared.body.result
    else {
        panic!("prepare")
    };
    let acquired = exchange(
        &mut stream,
        &binding,
        None,
        BftSignerCommandV3::Acquire {
            binding_digest: binding.digest().unwrap(),
            expected_generation,
            prepare_nonce,
        },
        2,
    );
    let credential = BftSignerLeaseCredentialV3 {
        generation: acquired.body.lease_generation,
        token: acquired.body.lease_token.unwrap(),
    };
    let mut read = BftSuccessorArchiveReadV1 {
        target: BftSuccessorArchiveReadTargetV1::GetOriginalState {
            request_id: id,
            checkpoint_digest: digest,
            segment_root: record.manifest.digest().unwrap(),
            minimum_lower_bound: Box::new(minimum),
        },
        object: record.original_public_state.clone(),
        offset: 0,
        length: 1024,
    };
    let mut nonce = 3;
    let mut bytes = Vec::new();
    let mut last_counter = 0;
    while (bytes.len() as u64) < read.object.bytes {
        read.offset = bytes.len() as u64;
        read.length = (read.object.bytes - read.offset).min(1024) as u32;
        let reply = exchange(
            &mut stream,
            &binding,
            Some(credential.clone()),
            BftSignerCommandV3::ReadSuccessorArchive {
                read: Box::new(read.clone()),
            },
            nonce,
        );
        nonce += 1;
        let BftSignerReplyKindV3::ArchiveChunk { chunk } = reply.body.result else {
            panic!("chunk rejected")
        };
        chunk.verify(&public(33)).unwrap();
        assert_eq!(chunk.body.read, read);
        assert!(chunk.body.reserved_response_bytes > last_counter);
        last_counter = chunk.body.reserved_response_bytes;
        bytes.extend_from_slice(&chunk.body.bytes);
    }
    assert!(
        nonce > 4,
        "fixture must exercise multiple actual TCP chunks"
    );
    read.object.validate_bytes(&bytes).unwrap();
    let decoded: BftSignerStateV3 = crate::protected_bft_v3::crypto::decode_bounded(
        &bytes,
        crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
    )
    .unwrap();
    assert_eq!(decoded, original);
    let charged = counter(&snapshot);
    for mutation in 0..8 {
        let mut bad = read.clone();
        match mutation {
            0 => bad.offset = bad.object.bytes,
            1 => bad.object.digest = hash(98),
            2 => {
                if let BftSuccessorArchiveReadTargetV1::GetOriginalState { segment_root, .. } =
                    &mut bad.target
                {
                    *segment_root = hash(97);
                }
            }
            3 => {
                if let BftSuccessorArchiveReadTargetV1::GetOriginalState {
                    minimum_lower_bound,
                    ..
                } = &mut bad.target
                {
                    minimum_lower_bound.height += 1;
                }
            }
            4 => bad.length = BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1 as u32 + 1,
            _ => {
                let (id, checkpoint, segment, lower) = bad.target.coordinates();
                let mut query = BftSuccessorWindowArchiveQueryV1 {
                    schema_version: 1,
                    source: lower.source.clone(),
                    request_id: id.into(),
                    checkpoint_digest: checkpoint.into(),
                    segment_root: segment.into(),
                    identity: proposal_identity(&original).0,
                    challenge: format!("{nonce:064x}"),
                    lease_generation: credential.generation,
                    lease_token_digest: crate::protected_bft_v3::crypto::digest_hex(
                        BFT_SIGNER_LEASE_DOMAIN_V3,
                        &credential.token,
                    )
                    .unwrap(),
                    minimum_lower_bound: lower.clone(),
                };
                match mutation {
                    5 => query.challenge = hash(96),
                    6 => query.lease_generation += 1,
                    _ => query.lease_token_digest = hash(95),
                }
                bad.target = BftSuccessorArchiveReadTargetV1::SigningIdentity {
                    query: Box::new(query),
                };
            }
        }
        let reply = exchange(
            &mut stream,
            &binding,
            Some(credential.clone()),
            BftSignerCommandV3::ReadSuccessorArchive {
                read: Box::new(bad),
            },
            nonce,
        );
        nonce += 1;
        assert!(matches!(
            reply.body.result,
            BftSignerReplyKindV3::Rejected { .. }
        ));
        assert_eq!(counter(&snapshot), charged);
    }
    assert_eq!(snapshot.lock().unwrap().state, frozen);
    drop(stream);
    server.join().unwrap();
}

#[path = "archive_query_fault_tests.rs"]
mod fault_tests;

#[test]
fn archive_chunk_signed_malformed_summary_is_rejected() {
    let (binding, snapshot) = fixture_kind(true);
    let state = snapshot.lock().unwrap().state.clone();
    let minimum = BftSuccessorWindowMinimumV1 {
        source: BftSuccessorWindowSourceV1::from_binding(&binding).unwrap(),
        sequence: 1,
        height: binding.handoff_position + 1,
        header_id: "ab".repeat(32),
        checkpoint_digest: "cd".repeat(32),
    };
    let body = BftSuccessorArchiveChunkBodyV1 {
        schema_version: 1,
        binding_digest: binding.digest().unwrap(),
        provider_public_key: public(33).clone(),
        request_digest: "ef".repeat(32),
        challenge: "summary-test".into(),
        lease_generation: 1,
        lease_token_digest: "12".repeat(32),
        actual_revision: state.revision,
        actual_state_digest: state.state_digest,
        minimum: minimum.clone(),
        read: BftSuccessorArchiveReadV1 {
            target: BftSuccessorArchiveReadTargetV1::GetOriginalState {
                request_id: "summary".into(),
                checkpoint_digest: "cd".repeat(32),
                segment_root: "34".repeat(32),
                minimum_lower_bound: Box::new(minimum),
            },
            object: BftSuccessorWindowArchiveRefV1 {
                digest: "56".repeat(32),
                bytes: 1,
            },
            offset: 0,
            length: 1,
        },
        reserved_response_bytes: 1,
        bytes: vec![0],
    };
    let signer = Handle {
        key: key(33),
        disabled: None,
    };
    BftSuccessorArchiveChunkV1::sign(body.clone(), &signer)
        .unwrap()
        .verify(&public(33))
        .unwrap();
    for case in 0..3 {
        let mut malformed = body.clone();
        match case {
            0 => malformed.actual_state_digest = "not-a-digest".into(),
            1 => malformed.reserved_response_bytes = BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1 + 1,
            _ => malformed.minimum.checkpoint_digest = "bad".into(),
        }
        let chunk = BftSuccessorArchiveChunkV1::sign(malformed, &signer).unwrap();
        assert!(
            chunk.verify(&public(33)).is_err(),
            "signed malformed summary case {case}"
        );
    }
}

#[test]
fn archive_snapshot_validation_is_once_per_observation_and_fresh_next_request() {
    let (binding, snapshot, archive, original, id, digest) = install(true, true);
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    // One fresh complete validation of the immutable snapshot, then one fresh
    // selected payload load. No prior request or opening result is reused.
    let before = archive.loads.load(Ordering::SeqCst);
    assert_eq!(
        lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap()
            .original_state(),
        &original,
    );
    assert_eq!(archive.loads.load(Ordering::SeqCst) - before, 2);
    let charged = counter(&snapshot);
    let before = archive.loads.load(Ordering::SeqCst);
    assert_eq!(
        lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap()
            .original_state(),
        &original,
    );
    assert_eq!(archive.loads.load(Ordering::SeqCst) - before, 2);
    assert!(counter(&snapshot) > charged);
    // A later independent observation must detect missing material even though
    // this exact snapshot and original S were previously verified successfully.
    let before_missing = counter(&snapshot);
    archive.states.lock().unwrap().clear();
    assert!(
        lease
            .read_successor_original_state(&id, &digest, deadline())
            .is_err()
    );
    assert_eq!(counter(&snapshot), before_missing);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn combined_observation_still_binds_reserved_original_before_service_open() {
    let (binding, snapshot, archive, original, _, _) = install(true, true);
    let record = snapshot
        .lock()
        .unwrap()
        .state
        .successor_window
        .as_ref()
        .unwrap()
        .requests[0]
        .clone();
    let mut extension = BftSuccessorWindowStateV1::empty();
    extension.usage = record.request.intent.body.checkpoint.usage;
    extension.active_request_id = Some(record.request.intent.body.checkpoint.request_id.clone());
    extension.reservation = Some(
        crate::protected_bft_v3::successor_state::BftSuccessorWindowReservationV1 {
            body: record.request.intent.body.clone(),
            manifest: record.manifest.clone(),
            original_public_state: record.original_public_state.clone(),
            previous: None,
        },
    );
    let reserved = original
        .with_successor_window_context(&binding, extension, None, false)
        .unwrap();
    let open = |state: BftSignerStateV3| {
        let mut backend = snapshot.lock().unwrap().clone();
        backend.state = state;
        BftSignerServiceV3::open_with_successor_window(
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
            Box::new(Backend {
                binding: binding.digest().unwrap(),
                snapshot: Arc::new(Mutex::new(backend)),
            }),
            BftSuccessorWindowSupportV1::new(archive.clone(), Duration::from_secs(30)).unwrap(),
        )
    };
    assert!(open(reserved.clone()).is_ok());
    let mut bad = reserved;
    let reservation = bad
        .successor_window
        .as_mut()
        .unwrap()
        .reservation
        .as_mut()
        .unwrap();
    // A coherent, well-shaped cut still must bind the reconstructed S's exact
    // applied driver receipt; structural extension validation alone is insufficient.
    reservation.body.checkpoint.cut.driver_install_digest = hash(99);
    reservation.body.driver_evidence_digest = hash(99);
    bad.state_digest = crate::protected_bft_v3::state::test_state_digest(&bad);
    assert!(validate_state(&binding, Some(&public(33)), &bad).is_ok());
    let error = open(bad)
        .err()
        .expect("reserved cut must bind authenticated original S");
    assert!(
        error.0.contains("actual applied provider cut/schedule"),
        "{error:?}"
    );
}
