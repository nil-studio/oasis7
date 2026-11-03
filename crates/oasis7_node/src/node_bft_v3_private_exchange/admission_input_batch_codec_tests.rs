//! Wire and subject-PoP components only. No endpoint, issuer freshness gate,
//! ActualWorld material, physical isolation or near-capacity transfer is proved.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionReadGrantBodyV1, BftPrivateRetentionAdmissionReadPurposeV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSignerErrorV3, BftSigningKeyHandleV3,
    BftSuccessorWindowArchiveRefV1,
};
use oasis7_proto::distributed_bft::v2::BftScopeV2;

struct Key(SigningKey);
impl BftSigningKeyHandleV3 for Key {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        Ok(self.0.sign(bytes).to_bytes())
    }
}
fn fixture(total: u64, operation: BatchOperationV2) -> (BatchRequestV2, SigningKey, String) {
    let subject = SigningKey::from_bytes(&[32; 32]);
    let issuer = Key(SigningKey::from_bytes(&[71; 32]));
    let read = BatchReadV2 {
        reference: InputRef {
            source: BftPrivateRetentionSourceV1 {
                binding_digest: "11".repeat(32),
                handoff_digest: "22".repeat(32),
                material_source_digest: "33".repeat(32),
            },
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: "44".repeat(32),
                height: u64::MAX,
                header_id: "55".repeat(32),
            },
            object: BftSuccessorWindowArchiveRefV1 {
                digest: "66".repeat(32),
                bytes: total,
            },
            admission_digest: "77".repeat(32),
        },
        requester_peer_id: libp2p::PeerId::random().to_string(),
        serving_peer_id: libp2p::PeerId::random().to_string(),
        nonce: "99".repeat(32),
        operation,
    };
    let subject_public_key = hex::encode(subject.verifying_key().to_bytes());
    let grant = BftPrivateRetentionAdmissionReadGrantV1::sign(
        BftPrivateRetentionAdmissionReadGrantBodyV1 {
            schema_version: 1,
            purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
            scope: BftScopeV2 {
                world_id: "wire-component".into(),
                chain_id: "wire-chain".into(),
                genesis_hash: "aa".repeat(32),
            },
            reference: read.reference.clone(),
            subject_public_key: subject_public_key.clone(),
            requester_peer_id: read.requester_peer_id.clone(),
            serving_peer_id: read.serving_peer_id.clone(),
            schedule_digest: "bb".repeat(32),
            view_generation: 0,
            request_nonce: read.nonce.clone(),
            grant_epoch: 1,
            not_before_unix_ms: 1,
            expires_unix_ms: 2,
        },
        &issuer,
    )
    .unwrap();
    let signature_hex = hex::encode(
        subject
            .sign(&read.signing_bytes(&grant).unwrap())
            .to_bytes(),
    );
    (
        BatchRequestV2 {
            domain: DOMAIN_V2.into(),
            read,
            authorization: BatchAuthorizationV2 {
                grant,
                subject_public_key,
                signature_hex,
            },
        },
        subject,
        issuer.public_key_hex(),
    )
}
fn response(request: &BatchRequestV2, value: u8) -> BatchResponseV2 {
    let ranges = request.read.ranges().unwrap();
    BatchResponseV2 {
        domain: DOMAIN_V2.into(),
        request_digest: request.digest().unwrap(),
        reference: request.read.reference.clone(),
        offset_bytes: ranges.first().map_or(0, |range| range.offset),
        parts: ranges
            .into_iter()
            .map(|range| BatchPartV2 {
                index: range.index,
                bytes: PartBytesV2::new(vec![value; range.length as usize]).unwrap(),
            })
            .collect(),
    }
}
#[test]
fn admission_batch_v2_subject_pop_binds_phase_offset_count_and_exact_original_grant() {
    let (request, _, issuer) = fixture(
        4 * u64::from(CHUNK) + 7,
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    request
        .authorization
        .grant
        .verify_signature(&issuer)
        .unwrap();
    let proof = request.verify_subject().unwrap();
    assert_eq!(proof.read(), &request.read);
    assert_eq!(proof.grant(), &request.authorization.grant);
    assert_eq!(
        proof.subject_public_key(),
        request.authorization.subject_public_key
    );
    for index in 0..3 {
        assert!(proof.authorizes_part(index));
    }
    assert!(!proof.authorizes_part(3));
    for operation in [
        BatchOperationV2::Manifest,
        BatchOperationV2::Batch {
            offset_bytes: u64::from(CHUNK),
            part_count: 3,
        },
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 2,
        },
    ] {
        let mut changed = request.clone();
        changed.read.operation = operation;
        changed.validate().unwrap(); // prove failure is PoP, not malformed range
        assert!(changed.verify_subject().is_err());
    }
    let mut changed = request.clone();
    changed.read.reference.key.height -= 1;
    assert!(changed.verify_subject().is_err());
    let mut changed = request.clone();
    changed.read.nonce = "dd".repeat(32);
    assert!(changed.verify_subject().is_err());
    let mut changed = request.clone();
    std::mem::swap(
        &mut changed.read.requester_peer_id,
        &mut changed.read.serving_peer_id,
    );
    assert!(changed.verify_subject().is_err());
    let mut changed = request.clone();
    changed.authorization.subject_public_key =
        hex::encode(SigningKey::from_bytes(&[33; 32]).verifying_key().to_bytes());
    assert!(changed.verify_subject().is_err());
    assert_eq!(
        BatchRequestV2::decode(&request.encode().unwrap()).unwrap(),
        request
    );
}
#[test]
fn admission_batch_v2_rejects_v1_signature_or_envelope_without_child_signature_adaptation() {
    let (mut request, subject, _) = fixture(
        3 * u64::from(CHUNK),
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let old_read = NodeBftAdmissionInputReadRequestV1 {
        reference: request.read.reference.clone(),
        requester_peer_id: request.read.requester_peer_id.clone(),
        serving_peer_id: request.read.serving_peer_id.clone(),
        nonce: request.read.nonce.clone(),
        operation: super::super::input_wire::package_operation(&request.read.reference).unwrap(),
    };
    let old_signature = hex::encode(
        subject
            .sign(
                &old_read
                    .signing_bytes(&request.authorization.grant)
                    .unwrap(),
            )
            .to_bytes(),
    );
    request.authorization.signature_hex = old_signature.clone();
    request.validate().unwrap();
    assert!(request.verify_subject().is_err());
    let old_request = super::super::input_wire::Request {
        domain: super::super::input_wire::DOMAIN.into(),
        read: old_read,
        authorization: NodeBftAdmissionInputAuthorizationV1 {
            grant: request.authorization.grant.clone(),
            subject_public_key: request.authorization.subject_public_key.clone(),
            signature_hex: old_signature,
        },
        chunk_index: Some(0),
    };
    assert!(BatchRequestV2::decode(&encode_frame(&old_request).unwrap()).is_err());
}
#[test]
fn admission_batch_v2_subject_proof_is_not_pinned_issuer_or_current_read_authority() {
    let (mut request, subject, issuer) = fixture(3 * u64::from(CHUNK), BatchOperationV2::Manifest);
    request.authorization.grant.issuer_public_key =
        hex::encode(SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes());
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
    let proof = request.verify_subject().unwrap();
    assert!(!proof.authorizes_part(0));
    assert!(
        proof.grant().verify_signature(&issuer).is_err(),
        "valid subject possession cannot mint trusted issuer permission"
    );
    let manifest = response(&request, 0);
    assert!(manifest.parts.is_empty());
    assert_eq!(
        BatchResponseV2::decode_for(&manifest.encode_for(&request).unwrap(), &request).unwrap(),
        manifest
    );
}
#[test]
fn admission_batch_v2_ranges_reject_invalid_counts_alignment_overflow_and_eof() {
    let (request, _, _) = fixture(3 * u64::from(CHUNK) + 7, BatchOperationV2::Manifest);
    for (offset_bytes, part_count) in [
        (0, 0),
        (0, 4),
        (1, 1),
        (u64::MAX, 3),
        (4 * u64::from(CHUNK), 1),
        (3 * u64::from(CHUNK), 2),
    ] {
        let mut changed = request.clone();
        changed.read.operation = BatchOperationV2::Batch {
            offset_bytes,
            part_count,
        };
        assert!(changed.validate().is_err());
    }
    let mut tail = request.read.clone();
    tail.operation = BatchOperationV2::Batch {
        offset_bytes: 3 * u64::from(CHUNK),
        part_count: 1,
    };
    assert_eq!(
        tail.ranges().unwrap(),
        vec![PartRangeV2 {
            index: 3,
            offset: 3 * u64::from(CHUNK),
            length: 7
        }]
    );
}
#[test]
fn admission_batch_v2_response_enforces_contiguous_exact_parts_and_binding() {
    let (request, _, _) = fixture(
        3 * u64::from(CHUNK),
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let valid = response(&request, 0xa5);
    let encoded = valid.encode_for(&request).unwrap();
    assert_eq!(
        BatchResponseV2::decode_for(&encoded, &request).unwrap(),
        valid
    );
    for mode in 0..7 {
        let mut bad = valid.clone();
        match mode {
            0 => bad.offset_bytes = u64::from(CHUNK),
            1 => bad.parts.swap(0, 1),
            2 => bad.parts[1].index = 0,
            3 => {
                bad.parts.pop();
            }
            4 => bad.parts[1].bytes = PartBytesV2::new(vec![1; CHUNK as usize - 1]).unwrap(),
            5 => bad.request_digest = "ee".repeat(32),
            6 => bad.reference.object.digest = "ed".repeat(32),
            _ => unreachable!(),
        }
        assert!(
            BatchResponseV2::decode_for(&encode_frame(&bad).unwrap(), &request).is_err(),
            "malformed response mode {mode}"
        );
    }
}
#[test]
fn admission_batch_v2_codec_has_bounded_binary_parts_not_default_integer_arrays() {
    let part = PartBytesV2::new(vec![1, 2, 3]).unwrap();
    let encoded = encode(&part).unwrap();
    assert_eq!(
        encoded,
        vec![0x43, 1, 2, 3],
        "transport bytes are a CBOR byte-string"
    );
    assert_eq!(decode_frame::<PartBytesV2>(&encoded).unwrap(), part);
    assert!(decode_frame::<PartBytesV2>(&encode(&vec![1u8, 2, 3]).unwrap()).is_err());
    assert!(PartBytesV2::new(vec![]).is_err());
    assert!(PartBytesV2::new(vec![0; CHUNK as usize + 1]).is_err());
    let mut too_big = vec![0x59, 0x40, 0x01];
    too_big.extend(vec![0; CHUNK as usize + 1]);
    assert!(
        decode_frame::<PartBytesV2>(&too_big).is_err(),
        "16KiB+1 must fail the typed byte visitor while below scanner/frame bounds"
    );
    assert!(decode_frame::<PartBytesV2>(&[0x5b, 255, 255, 255, 255, 255, 255, 255, 255]).is_err());
    let (request, _, _) = fixture(
        4 * u64::from(CHUNK),
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let mut four = response(&request, 7);
    four.parts
        .iter_mut()
        .for_each(|part| part.bytes = PartBytesV2::new(vec![7]).unwrap());
    four.parts.push(four.parts[0].clone());
    let small = encode_frame(&four).unwrap();
    assert!(small.len() < MAX_FRAME);
    let error = serde_cbor::from_slice::<BatchResponseV2>(&small)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("part count"),
        "must reject fourth short part at bounded visitor: {error}"
    );
    four.parts = vec![
        BatchPartV2 {
            index: 0,
            bytes: PartBytesV2::new(vec![7; CHUNK as usize]).unwrap()
        };
        4
    ];
    assert!(
        encode_frame(&four).is_err(),
        "four full parts cannot fit the original frame"
    );
    assert!(BatchResponseV2::decode_for(&vec![0; MAX_FRAME + 1], &request).is_err());
}
#[test]
fn admission_batch_v2_three_binary_items_fit_existing_scanner_but_one_aggregate_does_not() {
    let (request, _, _) = fixture(
        3 * u64::from(CHUNK),
        BatchOperationV2::Batch {
            offset_bytes: 0,
            part_count: 3,
        },
    );
    let encoded = response(&request, 0xff).encode_for(&request).unwrap();
    assert!(encoded.len() < MAX_FRAME);
    super::super::super::wire::scan(&encoded, false).unwrap();
    let aggregate = serde_cbor::value::Value::Bytes(vec![0; 3 * CHUNK as usize]);
    assert!(
        super::super::super::wire::scan(&encode(&aggregate).unwrap(), false).is_err(),
        "32KiB scanner bound applies per byte-string item, not aggregate items"
    );
}
#[test]
fn admission_batch_v2_actual_outer_codec_fee_handles_above_64k_wire_without_changing_inner_limit() {
    use libp2p::request_response::Codec;
    use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
    let protocol = libp2p::StreamProtocol::new(PROTOCOL_V2);
    for (total, first) in [
        (66 * 1024 * 1024u64, 4221u32),
        (66 * 1024 * 1024 - 1024, 4221),
        (3 * u64::from(CHUNK), 0),
    ] {
        let (request, _, _) = fixture(
            total,
            BatchOperationV2::Batch {
                offset_bytes: u64::from(first) * u64::from(CHUNK),
                part_count: 3,
            },
        );
        for value in [0u8, 23, 24, 255] {
            let frame = response(&request, value).encode_for(&request).unwrap();
            assert!(frame.len() <= MAX_FRAME);
            let mut codec = libp2p::request_response::cbor::codec::Codec::<
                NetworkRequest,
                NetworkResponse,
            >::default();
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
            assert_eq!(
                oasis7_distfs::protected_bft_v3::bft_original_input_network_response_bytes_v1(
                    &frame
                )
                .unwrap(),
                outer.len() as u64
            );
            if value >= 24 {
                assert!(
                    outer.len() > MAX_FRAME,
                    "actual outer >64KiB is legal while inner remains bounded"
                );
            }
            let mut reader = futures::io::Cursor::new(outer.clone());
            let decoded =
                futures::executor::block_on(codec.read_response(&protocol, &mut reader)).unwrap();
            assert_eq!(decoded.payload, frame);
            if total == 66 * 1024 * 1024 && first == 4221 {
                assert_eq!(frame.len(), 49992);
                assert_eq!(outer.len(), if value < 24 { 50833 } else { 99985 });
            }
        }
    }
}
