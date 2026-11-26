use super::*;
use super::super::admission_input_request_v2::{
    AdmissionBatchOperationV2, tests::fixture_with_original,
};
use ed25519_dalek::Signer;

fn fixture(
    operation: AdmissionBatchOperationV2,
    original: &[u8],
) -> (AdmissionBatchRequestV2, ed25519_dalek::SigningKey) {
    let digest = blake3::hash(original).to_hex().to_string();
    let (request, _, signer) =
        fixture_with_original(operation, original.len() as u64, digest);
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
            .sign(&updated.read.signing_bytes(&updated.authorization.grant).unwrap())
            .to_bytes(),
    );
    updated.validate().unwrap();
    updated
}

fn response(request: &AdmissionBatchRequestV2, original: &[u8]) -> OriginalAdmissionBatchResponseV2 {
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
    let mut stage = BftAdmissionInputV2StreamStage::new(&manifest.read.reference, Vec::new())
        .unwrap();
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes1, &first, &mut stage).unwrap();
    assert_eq!(stage.next_index(), 1);
    OriginalAdmissionBatchResponseV2::decode_and_stage(&bytes2, &next, &mut stage).unwrap();
    assert_eq!(stage.next_index(), 2);
    assert_eq!(stage.finish().unwrap(), original);
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
            6 => broken.parts[2].bytes =
                OriginalAdmissionPartBytesV2::new(vec![4; CHUNK as usize - 1]).unwrap(),
            7 => {
                broken.parts.pop();
            }
            _ => unreachable!(),
        }
        let encoded = super::super::wire::encode(&broken).unwrap();
        let mut stage = BftAdmissionInputV2StreamStage::new(
            &manifest.read.reference,
            Vec::new(),
        )
        .unwrap();
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
    assert!(OriginalAdmissionBatchResponseV2::decode_for(
        &super::super::wire::encode(&bad).unwrap(),
        &first
    ).is_err());
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
    assert!(super::super::wire::decode_frame::<OriginalAdmissionPartBytesV2>(
        &super::super::wire::encode(&vec![1u8, 2, 3]).unwrap()
    ).is_err());
    assert!(OriginalAdmissionPartBytesV2::new(vec![]).is_err());
    assert!(OriginalAdmissionPartBytesV2::new(vec![1; CHUNK as usize + 1]).is_err());
    let mut oversize = vec![0x59, 0x40, 0x01];
    oversize.extend(vec![7; CHUNK as usize + 1]);
    assert!(super::super::wire::decode_frame::<OriginalAdmissionPartBytesV2>(
        &oversize
    ).is_err());
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
    assert!(OriginalAdmissionBatchResponseV2::decode_for(
        &vec![0xff; 64 * 1024 + 1],
        &first,
    ).is_err());
}

#[test]
fn real_outer_network_response_charge_can_exceed_inner_64k_limit() {
    use libp2p::request_response::Codec;
    use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
    let protocol = libp2p::StreamProtocol::new(super::super::admission_input_request_v2::PROTOCOL_V2);
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
    assert!(outer.len() > frame.len());
    assert!(outer.len() > 64 * 1024);
    let mut reader = futures::io::Cursor::new(outer.clone());
    let decoded = futures::executor::block_on(codec.read_response(&protocol, &mut reader))
        .unwrap();
    assert_eq!(decoded.payload, frame);
}
