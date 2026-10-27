//! Parser and wire refusal checks independent of the native Node fixture.
use super::*;

#[test]
fn protected_header_state_declared_counts_are_rejected_before_entries() {
    for field in ["history", "applied_installs"] {
        let mut bytes = vec![0xa1];
        bytes.extend(serde_cbor::to_vec(&field).unwrap());
        bytes.extend([0x98, 0x41]); // Array(65), deliberately no first entry.
        let error = serde_cbor::from_slice::<BftSignerCandidateV3>(&bytes)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("record count exceeds bound"),
            "{field}: {error}"
        );
    }
}

#[test]
fn protected_header_tcp_frame_refuses_excess_length_before_body_allocation() {
    let length = u32::try_from(super::wire::BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3 + 1).unwrap();
    let bytes = length.to_be_bytes();
    let error = super::wire::read_frame::<BftSignerCandidateV3>(&mut bytes.as_slice())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("frame length is outside its bound"),
        "{error}"
    );
}

#[test]
fn protected_header_wire_rejects_old_kind_and_impossible_cbor_container() {
    let bytes = serde_cbor::to_vec(&serde_json::json!({
        "kind":"PrepareTransitionInstall", "body":{},
    }))
    .unwrap();
    assert!(serde_cbor::from_slice::<super::wire::BftSignerCommandV3>(&bytes).is_err());
    let error = super::crypto::decode_bounded::<BftSignerCandidateV3>(
        &[0x9a, 0xff, 0xff, 0xff, 0xff],
        1024,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("container exceeds encoded byte bound"),
        "{error}"
    );
}
