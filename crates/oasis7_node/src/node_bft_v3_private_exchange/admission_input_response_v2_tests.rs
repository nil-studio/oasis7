use super::super::admission_input_request_v2::{
    AdmissionBatchOperationV2, tests::fixture_with_original,
};
use super::*;
use ed25519_dalek::Signer;

fn fixture(
    operation: AdmissionBatchOperationV2,
    original: &[u8],
) -> (AdmissionBatchRequestV2, ed25519_dalek::SigningKey) {
    let digest = blake3::hash(original).to_hex().to_string();
    let (request, _, signer) = fixture_with_original(operation, original.len() as u64, digest);
    (request, signer)
}

fn signed_operation(
    request: &AdmissionBatchRequestV2,
    signer: &ed25519_dalek::SigningKey,
    operation: AdmissionBatchOperationV2,
) -> AdmissionBatchRequestV2 {
    let mut updated = request.clone();
    updated.read.operation = operation;
    updated.authorization.signature_hex = hex::encode(
        signer
            .sign(
                &updated
                    .read
                    .signing_bytes(&updated.authorization.grant)
                    .unwrap(),
            )
            .to_bytes(),
    );
    updated.validate().unwrap();
    updated
}

fn response(
    request: &AdmissionBatchRequestV2,
    original: &[u8],
) -> OriginalAdmissionBatchResponseV2 {
    let ranges = request.read.ranges().unwrap();
    let offset = match request.read.operation {
        AdmissionBatchOperationV2::Manifest => 0,
        AdmissionBatchOperationV2::Batch { offset_bytes, .. } => offset_bytes,
    };
    OriginalAdmissionBatchResponseV2 {
        domain: RESPONSE_DOMAIN.to_owned(),
        request_digest: request.request_digest().unwrap(),
        reference: request.read.reference.clone(),
        offset_bytes: offset,
        parts: ranges
            .iter()
            .map(|range| {
                let from = range.offset as usize;
                let to = from + range.length as usize;
                OriginalAdmissionPartV2 {
                    index: range.index,
                    bytes: OriginalAdmissionPartBytesV2::new(original[from..to].to_vec()).unwrap(),
                }
            })
            .collect(),
    }
}

#[test]
fn original_manifest_and_two_batch_frames_roundtrip_into_exact_staging() {
    let original: Vec<u8> = (0..(4 * CHUNK as usize + 7))
        .map(|value| (value % 251) as u8)
        .collect();
    let (manifest, signer) = fixture(AdmissionBatchOperationV2::Manifest, &original);
    let metadata = response(&manifest, &original);
    assert!(metadata.parts.is_empty());
    let encoded_manifest = metadata.encode_for(&manifest).unwrap();
    assert_eq!(
        OriginalAdmissionBatchResponseV2::decode_for(&encoded_manifest, &manifest).unwrap(),
        metadata
    );

    let first = signed_operation(
        &manifest,
        &signer,
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let next = signed_operation(
        &manifest,
        &signer,
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 3 * u64::from(CHUNK),
            part_count: 2,
        },
    );
    let bytes1 = response(&first, &original).encode_for(&first).unwrap();
    let bytes2 = response(&next, &original).encode_for(&next).unwrap();
    assert!(bytes1.len() < 64 * 1024);
    assert!(bytes2.len() < 64 * 1024);
    let mut stage =
        BftAdmissionInputV2StreamStage::new(&manifest.read.reference, Vec::new()).unwrap();
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes1, &first, &mut stage).unwrap();
    assert_eq!(stage.next_index(), 1);
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes2, &next, &mut stage).unwrap();
    assert_eq!(stage.next_index(), 2);
    assert_eq!(stage.finish().unwrap(), original);

    // Even if every authenticated RPC already returned, digest verification
    // and the sink's final flush remain inside the SAME attempt deadline.
    // A deterministic clock proves expiry immediately after successful finish
    // without a flaky 120-second timeout or a fresh retry/grant.
    let mut late =
        BftAdmissionInputV2StreamStage::new(&manifest.read.reference, Vec::new()).unwrap();
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes1, &first, &mut late).unwrap();
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes2, &next, &mut late).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let checked = std::cell::Cell::new(0);
    let result = super::super::admission_input_client_v2::finish_before_attempt_deadline(
        late,
        deadline,
        || {
            let previous = checked.get();
            checked.set(previous + 1);
            if previous == 0 {
                deadline - std::time::Duration::from_millis(1)
            } else {
                deadline
            }
        },
    );
    assert!(result.unwrap_err().contains("deadline exhausted"));
    assert_eq!(checked.get(), 2);
}

#[test]
fn malformed_original_binary_response_never_appends_to_staging() {
    let original = vec![0x82; 4 * CHUNK as usize + 7];
    let (manifest, signer) = fixture(AdmissionBatchOperationV2::Manifest, &original);
    let first = signed_operation(
        &manifest,
        &signer,
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let valid = response(&first, &original);
    for mode in 0..8 {
        let mut broken = valid.clone();
        match mode {
            0 => broken.domain = "oasis7.bft.private-candidate.exchange.v2".into(),
            1 => broken.request_digest = "ee".repeat(32),
            2 => broken.reference.object.digest = "ef".repeat(32),
            3 => broken.offset_bytes = u64::from(CHUNK),
            4 => broken.parts.swap(0, 1),
            5 => broken.parts[1].index = 0,
            6 => {
                broken.parts[2].bytes =
                    OriginalAdmissionPartBytesV2::new(vec![4; CHUNK as usize - 1]).unwrap()
            }
            7 => {
                broken.parts.pop();
            }
            _ => unreachable!(),
        }
        let encoded = super::super::wire::encode(&broken).unwrap();
        let mut stage =
            BftAdmissionInputV2StreamStage::new(&manifest.read.reference, Vec::new()).unwrap();
        assert!(
            OriginalAdmissionBatchResponseV2::decode_and_stage(&encoded, &first, &mut stage)
                .is_err(),
            "invalid mode {mode}"
        );
        assert_eq!(stage.next_index(), 0);
    }
    let mut bad = valid.clone();
    bad.parts.push(bad.parts[0].clone());
    bad.parts.iter_mut().for_each(|part| {
        part.bytes = OriginalAdmissionPartBytesV2::new(vec![7]).unwrap();
    });
    assert!(
        OriginalAdmissionBatchResponseV2::decode_for(
            &super::super::wire::encode(&bad).unwrap(),
            &first
        )
        .is_err()
    );
}

#[test]
fn part_visitor_enforces_binary_nonempty_16k_and_canonical_frame() {
    let raw = OriginalAdmissionPartBytesV2::new(vec![1, 2, 3]).unwrap();
    let encoded = super::super::wire::encode(&raw).unwrap();
    assert_eq!(encoded, vec![0x43, 1, 2, 3]);
    assert_eq!(
        super::super::wire::decode_frame::<OriginalAdmissionPartBytesV2>(&encoded).unwrap(),
        raw
    );
    assert!(
        super::super::wire::decode_frame::<OriginalAdmissionPartBytesV2>(
            &super::super::wire::encode(&vec![1u8, 2, 3]).unwrap()
        )
        .is_err()
    );
    assert!(OriginalAdmissionPartBytesV2::new(vec![]).is_err());
    assert!(OriginalAdmissionPartBytesV2::new(vec![1; CHUNK as usize + 1]).is_err());
    let mut oversize = vec![0x59, 0x40, 0x01];
    oversize.extend(vec![7; CHUNK as usize + 1]);
    assert!(super::super::wire::decode_frame::<OriginalAdmissionPartBytesV2>(&oversize).is_err());
    let original = vec![0xa5; 4 * CHUNK as usize + 7];
    let (manifest, signer) = fixture(AdmissionBatchOperationV2::Manifest, &original);
    let first = signed_operation(
        &manifest,
        &signer,
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let bytes = response(&first, &original).encode_for(&first).unwrap();
    let mut malformed = vec![0xa0];
    malformed.extend_from_slice(&bytes);
    assert!(OriginalAdmissionBatchResponseV2::decode_for(&malformed, &first).is_err());
    assert!(
        OriginalAdmissionBatchResponseV2::decode_for(&vec![0xff; 64 * 1024 + 1], &first,).is_err()
    );
}

#[test]
fn real_outer_network_response_charge_can_exceed_inner_64k_limit() {
    use libp2p::request_response::Codec;
    use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
    let protocol =
        libp2p::StreamProtocol::new(super::super::admission_input_request_v2::PROTOCOL_V2);
    let original = vec![0xff; 4 * CHUNK as usize + 7];
    let (manifest, signer) = fixture(AdmissionBatchOperationV2::Manifest, &original);
    let first = signed_operation(
        &manifest,
        &signer,
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let frame = response(&first, &original).encode_for(&first).unwrap();
    assert!(frame.len() <= 64 * 1024);
    let mut codec =
        libp2p::request_response::cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default();
    let mut writer = futures::io::Cursor::new(Vec::new());
    futures::executor::block_on(codec.write_response(
        &protocol,
        &mut writer,
        NetworkResponse {
            payload: frame.clone(),
        },
    ))
    .unwrap();
    let outer = writer.into_inner();
    assert!(outer.len() > frame.len());
    assert!(outer.len() > 64 * 1024);
    let mut reader = futures::io::Cursor::new(outer.clone());
    let decoded = futures::executor::block_on(codec.read_response(&protocol, &mut reader)).unwrap();
    assert_eq!(decoded.payload, frame);
    real_original_admission_noise_is_bound_and_charged();
}

fn real_original_admission_noise_is_bound_and_charged() {
    use super::super::admission_input_live_v2::{
        NodeBftOriginalAdmissionNoiseV2, ProtectedOriginalAdmissionAuthorityV2,
        ProtectedOriginalAdmissionContextV2, ProtectedOriginalAdmissionPartsV2,
    };
    use super::super::admission_input_request_v2::{
        AdmissionInputBoundContextV2, PROTOCOL_V2, VerifiedAdmissionBatchSubjectV2,
    };
    use oasis7_distfs::protected_bft_v3::{
        BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionAdmissionReadGrantV1,
        BftPrivateRetentionInputObjectKindV1, BftPrivateRetentionInputObjectV1,
        BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSignerErrorV3,
        BftSigningKeyHandleV3, BftSuccessorWindowArchiveRefV1, LocalBftSuccessorArchiveStoreV1,
    };
    use oasis7_net::{Libp2pNetwork, Libp2pNetworkConfig};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};

    struct Issuer(ed25519_dalek::SigningKey);
    impl BftSigningKeyHandleV3 for Issuer {
        fn public_key_hex(&self) -> String {
            hex::encode(self.0.verifying_key().to_bytes())
        }
        fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
            Ok(self.0.sign(bytes).to_bytes())
        }
    }
    struct SourceHold(Arc<AtomicUsize>);
    impl Drop for SourceHold {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    struct Authority {
        bound: AdmissionInputBoundContextV2,
        bytes: Vec<u8>,
        generation: Arc<AtomicU64>,
        permitted: Arc<AtomicBool>,
        reads: AtomicUsize,
        holds: Arc<AtomicUsize>,
        charges: AtomicU64,
        deny_charge: AtomicBool,
    }
    impl ProtectedOriginalAdmissionAuthorityV2 for Authority {
        fn current_context(
            &self,
            _: &AdmissionBatchRequestV2,
        ) -> Result<ProtectedOriginalAdmissionContextV2, String> {
            Ok(ProtectedOriginalAdmissionContextV2 {
                bound: self.bound.clone(),
                original_attempt_deadline: Instant::now() + Duration::from_secs(30),
                current_generation: self.generation.clone(),
                expected_generation: 7,
                reply_permitted: self.permitted.clone(),
            })
        }
        fn read_authorized_parts(
            &self,
            subject: &VerifiedAdmissionBatchSubjectV2,
            deadline: Instant,
        ) -> Result<ProtectedOriginalAdmissionPartsV2, String> {
            if deadline > Instant::now() + Duration::from_secs(10) {
                return Err("original admission RPC exceeds ten-second limit".into());
            }
            if subject.read().reference.object.digest
                != blake3::hash(&self.bytes).to_hex().to_string()
            {
                return Err("original source mismatch".into());
            }
            self.reads.fetch_add(1, Ordering::SeqCst);
            let parts = subject
                .read()
                .ranges()?
                .iter()
                .map(|r| {
                    self.bytes[r.offset as usize..r.offset as usize + r.length as usize].to_vec()
                })
                .collect();
            self.holds.fetch_add(1, Ordering::SeqCst);
            Ok(ProtectedOriginalAdmissionPartsV2 {
                parts,
                transport_hold: Box::new(SourceHold(self.holds.clone())), // Drop probe, not custody.
            })
        }
        fn reserve_encoded_response_and_readback(
            &self,
            _: &VerifiedAdmissionBatchSubjectV2,
            outer: u64,
        ) -> Result<(), String> {
            assert!(self.holds.load(Ordering::SeqCst) > 0);
            if outer == 0 || self.deny_charge.load(Ordering::SeqCst) {
                return Err("protected original charge/readback refused".into());
            }
            self.charges.fetch_add(outer, Ordering::SeqCst);
            Ok(())
        }
    }
    fn wait_connected(mut condition: impl FnMut() -> bool) {
        let limit = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < limit, "Noise peer connection deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // The real two-peer Noise path now carries the exact canonical BFT input,
    // rather than only an arbitrary byte stream that cannot enter the CAS.
    let object = |kind, bytes: Vec<u8>| BftPrivateRetentionInputObjectV1 {
        kind,
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            bytes: bytes.len() as u64,
        },
        bytes,
    };
    let mut original_input = BftPrivateRetentionAdmissionInputV1 {
        schema_version: 1,
        source: BftPrivateRetentionSourceV1 {
            binding_digest: "11".repeat(32),
            handoff_digest: "22".repeat(32),
            material_source_digest: "33".repeat(32),
        },
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: "44".repeat(32),
            height: 32,
            header_id: "55".repeat(32),
        },
        publication_reservation_bytes: 4 * CHUNK as u64 + 128,
        objects: vec![
            object(
                BftPrivateRetentionInputObjectKindV1::OriginalData,
                vec![0x5a; 4 * CHUNK as usize + 7],
            ),
            object(
                BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
                vec![0x19; 48],
            ),
        ],
    };
    original_input
        .objects
        .sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    let (original, pinned) = original_input.canonical_with_descriptor().unwrap();
    let server = Arc::new(Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        request_response_timeout: Duration::from_secs(3),
        ..Default::default()
    }));
    wait_connected(|| !server.listening_addrs().is_empty());
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![
            server.listening_addrs()[0]
                .clone()
                .with(libp2p::multiaddr::Protocol::P2p(server.peer_id())),
        ],
        request_response_timeout: Duration::from_secs(3),
        ..Default::default()
    });
    wait_connected(|| client.connected_peers().contains(&server.peer_id()));
    let (mut request, mut bound, subject) = fixture_with_original(
        AdmissionBatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
        original.len() as u64,
        blake3::hash(&original).to_hex().to_string(),
    );
    let unbound_request = request.clone();
    request.read.reference = pinned.clone();
    request.read.requester_peer_id = client.peer_id().to_string();
    request.read.serving_peer_id = server.peer_id().to_string();
    let issuer = Issuer(ed25519_dalek::SigningKey::from_bytes(&[71; 32]));
    let mut body = request.authorization.grant.body.clone();
    body.reference = pinned.clone();
    body.requester_peer_id = request.read.requester_peer_id.clone();
    body.serving_peer_id = request.read.serving_peer_id.clone();
    request.authorization.grant =
        BftPrivateRetentionAdmissionReadGrantV1::sign(body, &issuer).unwrap();
    request.authorization.signature_hex = hex::encode(
        subject
            .sign(
                &request
                    .read
                    .signing_bytes(&request.authorization.grant)
                    .unwrap(),
            )
            .to_bytes(),
    );
    bound.authenticated_requester_peer_id = client.peer_id().to_string();
    bound.authenticated_serving_peer_id = server.peer_id().to_string();
    let authority = Arc::new(Authority {
        bound,
        bytes: original.clone(),
        generation: Arc::new(AtomicU64::new(7)),
        permitted: Arc::new(AtomicBool::new(true)),
        reads: AtomicUsize::new(0),
        holds: Arc::new(AtomicUsize::new(0)),
        charges: AtomicU64::new(0),
        deny_charge: AtomicBool::new(false),
    });
    let _service = NodeBftOriginalAdmissionNoiseV2::new(
        server.clone(),
        vec![client.peer_id().to_string()],
        authority.clone(),
    )
    .unwrap();
    assert!(
        client
            .request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &unbound_request.encode().unwrap(),
                server.peer_id(),
                Duration::from_secs(3),
            )
            .is_err()
    );
    assert_eq!(authority.reads.load(Ordering::SeqCst), 0);
    let answer = client
        .request_authenticated_to_peer_with_timeout(
            PROTOCOL_V2,
            &request.encode().unwrap(),
            server.peer_id(),
            Duration::from_secs(3),
        )
        .unwrap();
    let verified = OriginalAdmissionBatchResponseV2::decode_for(&answer, &request).unwrap();
    assert_eq!(verified.parts.len(), 3);
    assert_eq!(
        verified.parts[0].bytes.as_slice(),
        &original[..CHUNK as usize]
    );
    assert_eq!(authority.reads.load(Ordering::SeqCst), 1);
    assert!(authority.charges.load(Ordering::SeqCst) > answer.len() as u64);
    wait_connected(|| authority.holds.load(Ordering::SeqCst) == 0);

    let previous_charge = authority.charges.load(Ordering::SeqCst);
    let mut tampered = request.clone();
    tampered.authorization.signature_hex = "00".repeat(64);
    assert!(
        client
            .request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &tampered.encode().unwrap(),
                server.peer_id(),
                Duration::from_secs(3),
            )
            .is_err()
    );
    authority.generation.store(8, Ordering::SeqCst);
    assert!(
        client
            .request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &request.encode().unwrap(),
                server.peer_id(),
                Duration::from_secs(3),
            )
            .is_err()
    );
    assert_eq!(authority.reads.load(Ordering::SeqCst), 1);
    assert_eq!(authority.charges.load(Ordering::SeqCst), previous_charge);
    // A protected charge/readback failure must never deliver confidential
    // material, even when signature, Noise identity and generation match.
    authority.generation.store(7, Ordering::SeqCst);
    authority.deny_charge.store(true, Ordering::SeqCst);
    assert!(
        client
            .request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &request.encode().unwrap(),
                server.peer_id(),
                Duration::from_secs(3),
            )
            .is_err()
    );
    assert_eq!(authority.charges.load(Ordering::SeqCst), previous_charge);

    // The same real two-peer Noise route is now consumed by a bounded client,
    // not by a manually issued one-off Batch. Keep the *same* issuer grant,
    // nonce and subject identity for Manifest + both exact original batches.
    authority.deny_charge.store(false, Ordering::SeqCst);
    wait_connected(|| authority.holds.load(Ordering::SeqCst) == 0);
    let staged = super::super::admission_input_client_v2::fetch_original_admission_input_v2(
        &client,
        server.peer_id(),
        &request,
        &authority.bound,
        |preimage| Ok(hex::encode(subject.sign(preimage).to_bytes())),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(staged, original);
    assert!(authority.charges.load(Ordering::SeqCst) > previous_charge);
    let archive_root = std::env::temp_dir().join(format!(
        "o7-original-noise-cas-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir(&archive_root).unwrap();
    struct ArchiveRoot(std::path::PathBuf);
    impl Drop for ArchiveRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let archive_root = ArchiveRoot(archive_root);
    let archive = LocalBftSuccessorArchiveStoreV1::new(oasis7_distfs::LocalCasStore::new(
        archive_root.0.join("cas"),
    ))
    .unwrap();
    let verified_cold =
        super::super::admission_input_client_v2::fetch_original_admission_input_v2_into_cas(
            &client,
            server.peer_id(),
            &request,
            &authority.bound,
            |preimage| Ok(hex::encode(subject.sign(preimage).to_bytes())),
            &archive,
        )
        .unwrap();
    assert_eq!(verified_cold, original_input);
    assert_eq!(
        archive.load_original_admission_input(&pinned).unwrap(),
        original_input
    );
    let reads_after_success = authority.reads.load(Ordering::SeqCst);
    wait_connected(|| authority.holds.load(Ordering::SeqCst) == 0);

    // Wrong actual peer and wrong grant cannot initiate any part read.
    let mut wrong_peer = request.clone();
    wrong_peer.read.serving_peer_id = libp2p::PeerId::random().to_string();
    assert!(
        super::super::admission_input_client_v2::fetch_original_admission_input_v2(
            &client,
            server.peer_id(),
            &wrong_peer,
            &authority.bound,
            |preimage| Ok(hex::encode(subject.sign(preimage).to_bytes())),
            Vec::new(),
        )
        .is_err()
    );
    assert_eq!(authority.reads.load(Ordering::SeqCst), reads_after_success);
    check_one_protected_original_response_cas_and_uncertain_commit(&request, &authority.bound);
}

// Called from the existing selected two-peer Noise test. This in-memory spy
// represents ONLY the backend interface transaction, never HSM/DB custody.
fn check_one_protected_original_response_cas_and_uncertain_commit(
    request: &AdmissionBatchRequestV2,
    context: &super::super::admission_input_request_v2::AdmissionInputBoundContextV2,
) {
    use super::super::admission_input_live_v2::{
        ProtectedOriginalAdmissionAuthorityV2, ProtectedOriginalAdmissionContextV2,
        ProtectedOriginalAdmissionPartsV2, ProtectedOriginalAdmissionRetainedAuthorityV2,
        ProtectedOriginalAdmissionSourceV2, ProtectedOriginalResponseChargeV2,
    };
    use super::super::admission_input_request_v2::VerifiedAdmissionBatchSubjectV2;
    use oasis7_distfs::protected_bft_v3::{
        BftFirstReserveSnapshotV1, BftFreshCoverageSnapshotV1, BftOriginalRetentionTxnSnapshotV1,
        BftPrivateRetentionCreationMarkerV1, BftPrivateRetentionHeadV1, BftSignerAcquireOutcomeV3,
        BftSignerBackendSnapshotV3, BftSignerBackendV3, BftSignerErrorV3, BftSignerStateV3,
    };
    use std::{
        collections::BTreeMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    struct ProtectedSpy {
        state: Arc<Mutex<BftOriginalRetentionTxnSnapshotV1>>,
        lost_commit_reply: Arc<AtomicBool>,
    }
    impl BftSignerBackendV3 for ProtectedSpy {
        fn load_provisioned(
            &mut self,
            _: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            Err(BftSignerErrorV3("fixture does not provision signer".into()))
        }
        fn acquire_lease(
            &mut self,
            _: &str,
            _: u64,
            _: &str,
        ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
            Err(BftSignerErrorV3(
                "fixture has no physical lease acquire".into(),
            ))
        }
        fn release_lease(
            &mut self,
            _: u64,
            _: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            Err(BftSignerErrorV3(
                "fixture cannot release a physical lease".into(),
            ))
        }
        fn read_for_lease(
            &mut self,
            _: u64,
            _: &str,
        ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
            Err(BftSignerErrorV3(
                "fixture has no separate signer snapshot".into(),
            ))
        }
        fn protected_time_ms(&mut self, _: u64, _: &str) -> Result<i64, BftSignerErrorV3> {
            Err(BftSignerErrorV3(
                "fixture cannot sample protected time".into(),
            ))
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
            Err(BftSignerErrorV3("separate State CAS forbidden".into()))
        }
        fn read_original_operation_envelope_for_lease(
            &mut self,
            generation: u64,
            token_digest: &str,
        ) -> Result<BftOriginalRetentionTxnSnapshotV1, BftSignerErrorV3> {
            let snapshot = self.state.lock().unwrap().clone();
            if snapshot.current.current.signer.lease_generation != generation
                || snapshot.current.current.signer.lease_token_digest != token_digest
            {
                return Err(BftSignerErrorV3("stale original backend lease".into()));
            }
            Ok(snapshot)
        }
        fn compare_and_commit_original_operation_envelope(
            &mut self,
            expected: &BftOriginalRetentionTxnSnapshotV1,
            next: &BftOriginalRetentionTxnSnapshotV1,
        ) -> Result<(), BftSignerErrorV3> {
            let mut state = self.state.lock().unwrap();
            if *state != *expected {
                return Err(BftSignerErrorV3("stale protected full tuple".into()));
            }
            *state = next.clone();
            // A persisted commit with a lost acknowledgement remains charged.
            if self.lost_commit_reply.swap(false, Ordering::SeqCst) {
                return Err(BftSignerErrorV3("unknown protected commit response".into()));
            }
            Ok(())
        }
    }

    // The live adapter is deliberately source-only here; no second reserve
    // method or fallback cost ledger may be supplied by its issuer fixture.
    struct SourceSpy {
        bound: super::super::admission_input_request_v2::AdmissionInputBoundContextV2,
    }
    impl ProtectedOriginalAdmissionSourceV2 for SourceSpy {
        fn current_context(
            &self,
            request: &AdmissionBatchRequestV2,
        ) -> Result<ProtectedOriginalAdmissionContextV2, String> {
            request.verify_bound_subject(&self.bound)?;
            Ok(ProtectedOriginalAdmissionContextV2 {
                bound: self.bound.clone(),
                original_attempt_deadline: Instant::now() + Duration::from_secs(30),
                current_generation: Arc::new(AtomicU64::new(7)),
                expected_generation: 7,
                reply_permitted: Arc::new(AtomicBool::new(true)),
            })
        }
        fn read_authorized_parts(
            &self,
            subject: &VerifiedAdmissionBatchSubjectV2,
            _deadline: Instant,
        ) -> Result<ProtectedOriginalAdmissionPartsV2, String> {
            let parts = subject
                .read()
                .ranges()?
                .iter()
                .map(|part| vec![0x21; part.length as usize])
                .collect();
            Ok(ProtectedOriginalAdmissionPartsV2 {
                parts,
                transport_hold: Box::new(()), // test-only, NOT protected memory.
            })
        }
    }

    let reference = &request.read.reference;
    let mut signer_state: BftSignerStateV3 = serde_json::from_value(serde_json::json!({
        "schema_version": 3,
        "binding_digest": reference.source.binding_digest,
        "revision": 1,
        "history": [],
        "journal": null,
        "completed_journals": [],
        "pending_driver_apply": null,
        "applied_installs": [],
        "state_digest": "ca".repeat(32),
    }))
    .unwrap();
    signer_state.private_retention = Some(BftPrivateRetentionHeadV1 {
        schema_version: 2,
        source: reference.source.clone(),
        capability_digest: "cd".repeat(32),
        sequence: 1,
        map_digest: "ce".repeat(32),
        map_byte_hash: "cf".repeat(32),
        entry_count: 0,
        previous_receipt_digest: None,
        retained_material_bytes: Some(128 * 1024 * 1024),
    });
    let token = "da".repeat(32);
    let snapshot = BftOriginalRetentionTxnSnapshotV1 {
        current: BftFirstReserveSnapshotV1 {
            current: BftFreshCoverageSnapshotV1 {
                signer: BftSignerBackendSnapshotV3 {
                    state: signer_state,
                    lease_generation: 7,
                    lease_token_digest: token.clone(),
                    protected_time_floor_ms: 123,
                },
                marker: BftPrivateRetentionCreationMarkerV1 {
                    schema_version: 1,
                    creation_identity: "d0".repeat(32),
                    source: reference.source.clone(),
                    ledger_origin_digest: reference.key.ledger_origin_digest.clone(),
                    activation_digest: "d1".repeat(32),
                    initial_state_digest: "d2".repeat(32),
                    initial_height: reference.key.height,
                },
                original: None,
                reserved_response_bytes: 0,
            },
            reservation: None,
        },
        original_operations: BTreeMap::new(),
    };
    let stored = Arc::new(Mutex::new(snapshot));
    let lost_commit_reply = Arc::new(AtomicBool::new(false));
    let spy = ProtectedSpy {
        state: Arc::clone(&stored),
        lost_commit_reply: Arc::clone(&lost_commit_reply),
    };
    let charge = ProtectedOriginalResponseChargeV2::new(Box::new(spy), 7, token).unwrap();
    let authority = ProtectedOriginalAdmissionRetainedAuthorityV2::new(
        Arc::new(SourceSpy {
            bound: context.clone(),
        }),
        charge,
    );
    let subject = request.verify_bound_subject(context).unwrap();
    let verified_context = authority.current_context(request).unwrap();
    assert_eq!(
        verified_context.bound.schedule_digest,
        context.schedule_digest
    );
    let source_parts = authority
        .read_authorized_parts(&subject, Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(source_parts.parts.len(), 3);
    authority
        .reserve_encoded_response_and_readback(&subject, 65_540)
        .unwrap();
    assert_eq!(
        stored
            .lock()
            .unwrap()
            .current
            .current
            .reserved_response_bytes,
        65_540
    );
    lost_commit_reply.store(true, Ordering::SeqCst);
    assert!(
        authority
            .reserve_encoded_response_and_readback(&subject, 80)
            .is_err()
    );
    assert_eq!(
        stored
            .lock()
            .unwrap()
            .current
            .current
            .reserved_response_bytes,
        65_620
    );
    // The uncertain reply must NOT permit a second unbounded debit or release.
    assert!(
        authority
            .reserve_encoded_response_and_readback(&subject, 80)
            .is_err()
    );
    assert_eq!(
        stored
            .lock()
            .unwrap()
            .current
            .current
            .reserved_response_bytes,
        65_620
    );
}
