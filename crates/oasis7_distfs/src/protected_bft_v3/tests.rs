//! Parser and wire refusal checks independent of the native Node fixture.
use super::*;

#[test]
fn protected_header_state_declared_counts_are_rejected_before_entries() {
    for field in ["history", "applied_installs", "completed_journals"] {
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

#[test]
fn legacy_state_extension_absence_preserves_original_cbor_and_digest_input() {
    let value = serde_json::json!({"schema_version":3,"binding_digest":"a".repeat(64),"revision":7,"history":[],"journal":null,"completed_journals":[],"pending_driver_apply":null,"applied_installs":[],"state_digest":"b".repeat(64)});
    let state: BftSignerStateV3 = serde_json::from_value(value.clone()).unwrap();
    assert!(state.membership_install.is_none());
    let original = oasis7_cas::to_canonical_cbor(&value).unwrap();
    assert_eq!(oasis7_cas::to_canonical_cbor(&state).unwrap(), original);
    let mut original_input = value.clone();
    original_input
        .as_object_mut()
        .unwrap()
        .remove("state_digest");
    assert_eq!(
        super::state::test_state_digest(&state),
        super::crypto::digest_hex("oasis7.bft.protected-signer.v3.state", &original_input).unwrap()
    );
    let mut enabled = state.clone();
    enabled.membership_install = Some(super::membership::BftMembershipInstallStateV3::enabled());
    assert_ne!(
        super::state::test_state_digest(&state),
        super::state::test_state_digest(&enabled)
    );
    assert_ne!(oasis7_cas::to_canonical_cbor(&enabled).unwrap(), original);
}
#[test]
fn membership_receipts_declared_capacity_rejected_before_first_entry() {
    let mut bytes = vec![0xa1];
    bytes.extend(serde_cbor::to_vec(&"receipts").unwrap());
    bytes.extend([0x98, 0x41]);
    let error = serde_cbor::from_slice::<BftMembershipInstallStateV3>(&bytes)
        .unwrap_err()
        .to_string();
    assert!(error.contains("record count exceeds bound"), "{error}");
}
