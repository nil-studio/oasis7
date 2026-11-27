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

// Existing test-tier entrypoint: production operation module has no new cfg gate.
mod private_retention_operation_tests {
    use std::collections::BTreeMap;

    use super::super::private_retention_operation::*;
    use super::super::{
        BftFirstReserveSnapshotV1, BftFreshCoverageSnapshotV1,
        BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionCreationMarkerV1,
        BftPrivateRetentionSourceV1, BftSignerBackendSnapshotV3, BftSignerStateV3,
        BftSuccessorWindowArchiveRefV1,
    };

    fn hex_hash(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn source() -> BftPrivateRetentionSourceV1 {
        BftPrivateRetentionSourceV1 {
            binding_digest: hex_hash(1),
            handoff_digest: hex_hash(2),
            material_source_digest: hex_hash(3),
        }
    }

    fn current(revision: u64, reserved: u64) -> BftFirstReserveSnapshotV1 {
        let state: BftSignerStateV3 = serde_json::from_value(serde_json::json!({
            "schema_version": 3,
            "binding_digest": hex_hash(1),
            "revision": revision,
            "history": [],
            "journal": null,
            "completed_journals": [],
            "pending_driver_apply": null,
            "applied_installs": [],
            "state_digest": hex_hash(revision as u8 + 10)
        }))
        .unwrap();
        BftFirstReserveSnapshotV1 {
            current: BftFreshCoverageSnapshotV1 {
                signer: BftSignerBackendSnapshotV3 {
                    state,
                    lease_generation: 7,
                    lease_token_digest: hex_hash(4),
                    protected_time_floor_ms: 100,
                },
                marker: BftPrivateRetentionCreationMarkerV1 {
                    schema_version: 1,
                    creation_identity: hex_hash(5),
                    source: source(),
                    ledger_origin_digest: hex_hash(6),
                    activation_digest: hex_hash(7),
                    initial_state_digest: hex_hash(8),
                    initial_height: 1,
                },
                original: None,
                reserved_response_bytes: reserved,
            },
            reservation: None,
        }
    }

    fn operation() -> BftRetainedOriginalOperationV1 {
        let signed_request_bytes = vec![0x31, 0, 0xff, 0x12];
        let signed_receipt_bytes = vec![0x44, 0, 0xfe, 0x30];
        let original_request_digest =
            BftRetainedOriginalOperationV1::request_digest_from_original_bytes(&signed_request_bytes);
        let original_receipt_digest =
            BftRetainedOriginalOperationV1::receipt_digest_from_original_bytes(&signed_receipt_bytes);
        BftRetainedOriginalOperationV1 {
            schema_version: 1,
            operation_id: "original-reserve-1".into(),
            source: source(),
            ledger_origin_digest: hex_hash(6),
            original_request_digest,
            signed_request_bytes,
            before_revision: 1,
            before_state_digest: hex_hash(11),
            after_revision: 2,
            after_state_digest: hex_hash(12),
            original_lease_generation: 7,
            original_lease_token_digest: hex_hash(4),
            input_refs: vec![BftPrivateRetentionAdmissionInputRefV1 {
                source: source(),
                key: super::super::BftPrivateRetentionKeyV1 {
                    ledger_origin_digest: hex_hash(6),
                    height: 1,
                    header_id: hex_hash(9),
                },
                object: BftSuccessorWindowArchiveRefV1 {
                    digest: hex_hash(10),
                    bytes: 1024,
                },
                admission_digest: hex_hash(11),
            }],
            original_receipt_digest,
            signed_receipt_bytes,
            cumulative_reserved_response_bytes: 100,
        }
    }

    #[test]
    fn exact_single_protected_transaction_retains_original_bytes_and_idempotent_result() {
        let before = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let committed = before
            .prepare_single_atomic_operation(current(2, 100), original.clone())
            .unwrap();
        assert!(before.original_operations.is_empty());
        assert_eq!(committed.original_operations.len(), 1);
        assert_eq!(
            committed
                .lookup_original(&original.operation_id, &original.signed_request_bytes)
                .unwrap()
                .unwrap(),
            &original
        );
        assert_eq!(
            committed
                .prepare_single_atomic_operation(committed.current.clone(), original.clone())
                .unwrap(),
            committed
        );
        assert!(
            committed
                .lookup_original(&original.operation_id, b"other original")
                .is_err()
        );
        let mut substituted = original.clone();
        substituted.signed_receipt_bytes[0] ^= 1;
        assert!(committed
            .prepare_single_atomic_operation(committed.current.clone(), substituted)
            .is_err());
        let encoded = serde_cbor::to_vec(&original).unwrap();
        let cold: BftRetainedOriginalOperationV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(cold, original);
        assert_eq!(cold.signed_receipt_bytes, [0x44, 0, 0xfe, 0x30]);
    }

    #[test]
    fn original_whole_tuple_refuses_stale_lease_floor_spend_and_changed_identity() {
        let before = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let good = current(2, 100);
        let mut stale = good.clone();
        stale.current.signer.lease_generation = 8;
        assert!(before.prepare_single_atomic_operation(stale, original.clone()).is_err());
        let mut token = good.clone();
        token.current.signer.lease_token_digest = hex_hash(12);
        assert!(before.prepare_single_atomic_operation(token, original.clone()).is_err());
        let mut floor = good.clone();
        floor.current.signer.protected_time_floor_ms = 99;
        assert!(before.prepare_single_atomic_operation(floor, original.clone()).is_err());
        let mut refund = good.clone();
        refund.current.reserved_response_bytes = 0;
        assert!(before.prepare_single_atomic_operation(refund, original.clone()).is_err());
        let mut wrong = original.clone();
        wrong.original_request_digest = hex_hash(15);
        assert!(before.prepare_single_atomic_operation(good.clone(), wrong).is_err());
        let mut crossed = original.clone();
        crossed.original_lease_generation += 1;
        assert!(before.prepare_single_atomic_operation(good.clone(), crossed).is_err());
        let mut changed = good.clone();
        changed.current.marker.creation_identity = hex_hash(16);
        assert!(before.prepare_single_atomic_operation(changed, original).is_err());
    }

    #[test]
    fn cold_history_keeps_original_operation_across_new_lease_and_rejects_corruption() {
        let first = BftOriginalRetentionTxnSnapshotV1 {
            current: current(1, 0),
            original_operations: BTreeMap::new(),
        };
        let original = operation();
        let mut stored = first
            .prepare_single_atomic_operation(current(2, 100), original.clone())
            .unwrap();
        stored.current.current.signer.lease_generation = 8;
        stored.current.current.signer.lease_token_digest = hex_hash(20);
        stored.validate().unwrap();
        // A newer current H/window may reset its local signer revision. The old
        // signed operation belongs to the original source, not the new marker.
        stored.current.current.marker.source.handoff_digest = hex_hash(21);
        stored.current.current.marker.ledger_origin_digest = hex_hash(22);
        stored.current.current.signer.state.revision = 0;
        stored.validate().unwrap();
        assert_eq!(
            stored
                .lookup_original(&original.operation_id, &original.signed_request_bytes)
                .unwrap()
                .unwrap()
                .signed_receipt_bytes,
            original.signed_receipt_bytes
        );
        let mut rollback = stored.clone();
        rollback.current.current.reserved_response_bytes = 99;
        assert!(rollback.validate().is_err());
        let mut rewritten = stored.clone();
        rewritten
            .original_operations
            .get_mut(&original.operation_id)
            .unwrap()
            .signed_receipt_bytes[0] ^= 1;
        assert!(rewritten.validate().is_err());
        let mut bounded = stored.clone();
        let mut another = operation();
        another.operation_id = "original-reserve-2".into();
        bounded
            .original_operations
            .insert(another.operation_id.clone(), another);
        bounded.validate_with_byte_budget(16).unwrap();
        assert!(bounded.validate_with_byte_budget(15).is_err());
        let mut collision = stored.clone();
        collision.original_operations.remove(&original.operation_id);
        collision.original_operations.insert("different".into(), original);
        assert!(collision.validate().is_err());
    }
}
