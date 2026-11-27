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
        BftPrivateRetentionAdmissionReadGrantV1, BftSignerErrorV3, BftSigningKeyHandleV3,
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
    struct Authority {
        bound: AdmissionInputBoundContextV2,
        bytes: Vec<u8>,
        generation: Arc<AtomicU64>,
        permitted: Arc<AtomicBool>,
        reads: AtomicUsize,
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
            Ok(ProtectedOriginalAdmissionPartsV2 {
                parts,
                transport_hold: Box::new(()), // Test fixture, never production custody.
            })
        }
        fn reserve_encoded_response_and_readback(
            &self,
            _: &VerifiedAdmissionBatchSubjectV2,
            outer: u64,
        ) -> Result<(), String> {
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

    let original = vec![0x5a; 4 * CHUNK as usize + 7];
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
    request.read.requester_peer_id = client.peer_id().to_string();
    request.read.serving_peer_id = server.peer_id().to_string();
    let issuer = Issuer(ed25519_dalek::SigningKey::from_bytes(&[71; 32]));
    let mut body = request.authorization.grant.body.clone();
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
    let reads_after_success = authority.reads.load(Ordering::SeqCst);

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
}
