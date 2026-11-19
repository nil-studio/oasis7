use super::*;
#[test]
fn independent_capsule_has_exact_fixed_fields_and_absent_runtime_binding() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    let mut value = CognitionProofLedgerManifestV1 {
        wire_version: CognitionProofLedgerWireVersionV1::V1,
        roots: [17; 32],
        previous_record: None,
        fields: [
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            None,
            None,
            None,
            None,
        ],
    };
    let bytes = value.canonical_bytes().unwrap();
    assert_eq!(
        CognitionProofLedgerManifestV1::from_canonical_bytes(&bytes).unwrap(),
        value
    );
    value.fields[4] = Some(root);
    let explicit = value.canonical_bytes().unwrap();
    assert_ne!(bytes, explicit);
    assert!(
        CognitionProofLedgerManifestV1::from_canonical_bytes(&explicit[..explicit.len() - 1])
            .is_err()
    );
    let mut malformed = bytes;
    let at = COGNITION_PROOF_LEDGER_MAGIC_V1.len() + 32;
    malformed[at] = 0;
    malformed[at + 1] = 1;
    assert!(CognitionProofLedgerManifestV1::from_canonical_bytes(&malformed).is_err());
    value.fields[0] = None;
    assert!(value.canonical_bytes().is_err());
}

#[test]
fn v1_wire_is_exact_and_does_not_attest_sixth_field_absence() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    let value = CognitionProofLedgerManifestV1 {
        wire_version: CognitionProofLedgerWireVersionV1::V1,
        roots: [29; 32],
        previous_record: Some([7; 32]),
        fields: [
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            Some(root),
            None,
            None,
            None,
            None,
        ],
    };
    // Original V1 framing: magic, roots, optional predecessor, exactly five
    // presence/root pairs. This is independent of the new carrier layout.
    let mut original = Vec::new();
    original.extend_from_slice(b"oasis7.cognition.proof-ledger.v1\0");
    original.extend_from_slice(&[29; 32]);
    original.push(1);
    original.extend_from_slice(&[7; 32]);
    for field in &value.fields[..5] {
        original.push(u8::from(field.is_some()));
        match field {
            Some(root) => original.extend_from_slice(&root.canonical_bytes().unwrap()),
            None => original.extend_from_slice(
                &[0; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
            ),
        }
    }
    let bytes = value.canonical_bytes().unwrap();
    assert_eq!(bytes.len(), COGNITION_PROOF_LEDGER_BYTES_V1);
    assert_eq!(bytes.as_ref(), original);
    let decoded = CognitionProofLedgerManifestV1::from_canonical_bytes(&original).unwrap();
    assert_eq!(decoded, value);
    assert!(
        decoded
            .require_field_source(CognitionProofLedgerFieldV1::RuntimeBinding)
            .is_ok()
    );
    assert!(
        decoded
            .require_field_source(CognitionProofLedgerFieldV1::DelegationReceipts)
            .is_err()
    );
    let mut extended = original;
    extended
        .extend_from_slice(&[0; 1 + oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES]);
    assert!(CognitionProofLedgerManifestV1::from_canonical_bytes(&extended).is_err());
}

#[path = "incremental_cognition_proof_ledger_receipts_tests.rs"]
mod receipts;

#[test]
fn v2_wire_remains_exact_and_service_fields_remain_unproven() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    let value = CognitionProofLedgerManifestV1 {
        wire_version: CognitionProofLedgerWireVersionV1::V2,
        roots: [31; 32],
        previous_record: Some([5; 32]),
        fields: [
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            None,
            Some(root),
            None,
            None,
        ],
    };
    let mut original = Vec::new();
    original.extend_from_slice(b"oasis7.cognition.proof-ledger.v2\0");
    original.extend_from_slice(&[31; 32]);
    original.push(1);
    original.extend_from_slice(&[5; 32]);
    for field in &value.fields[..6] {
        original.push(u8::from(field.is_some()));
        match field {
            Some(root) => original.extend_from_slice(&root.canonical_bytes().unwrap()),
            None => original.extend_from_slice(
                &[0; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
            ),
        }
    }
    let wire = value.canonical_bytes().unwrap();
    assert_eq!(wire.len(), COGNITION_PROOF_LEDGER_BYTES_V2);
    assert_eq!(wire.as_ref(), original);
    let decoded = CognitionProofLedgerManifestV1::from_canonical_bytes(&original).unwrap();
    assert_eq!(value, decoded);
    assert!(
        decoded
            .require_field_source(CognitionProofLedgerFieldV1::DelegationReceipts)
            .is_ok()
    );
    for field in [
        CognitionProofLedgerFieldV1::ServiceCausalProposals,
        CognitionProofLedgerFieldV1::ActionDigests,
    ] {
        assert!(decoded.require_field_source(field).is_err())
    }
    let mut smuggled = value;
    smuggled.fields[7] = smuggled.fields[0].clone();
    assert!(smuggled.canonical_bytes().is_err());
}

#[test]
fn v3_wire_attests_each_optional_service_field_without_upgrading_old_versions() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    for mask in 0u8..16 {
        let mut fields = std::array::from_fn(|i| (i < 4).then(|| root.clone()));
        for i in 0..4 {
            fields[4 + i] = (mask & (1 << i) != 0).then(|| root.clone());
        }
        let value = CognitionProofLedgerManifestV1 {
            wire_version: CognitionProofLedgerWireVersionV1::V3,
            roots: [43; 32],
            previous_record: Some([11; 32]),
            fields,
        };
        let mut original = Vec::new();
        original.extend_from_slice(b"oasis7.cognition.proof-ledger.v3\0");
        original.extend_from_slice(&[43; 32]);
        original.push(1);
        original.extend_from_slice(&[11; 32]);
        for field in &value.fields {
            original.push(u8::from(field.is_some()));
            match field {
                Some(root) => original.extend_from_slice(&root.canonical_bytes().unwrap()),
                None => original.extend_from_slice(
                    &[0; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
                ),
            }
        }
        let wire = value.canonical_bytes().unwrap();
        assert_eq!(wire.as_ref(), original);
        assert_eq!(
            CognitionProofLedgerManifestV1::from_canonical_bytes(&original).unwrap(),
            value
        );
        for field in [
            CognitionProofLedgerFieldV1::ServiceCausalProposals,
            CognitionProofLedgerFieldV1::ActionDigests,
        ] {
            assert!(value.require_field_source(field).is_ok());
        }
        let mut old = value;
        old.wire_version = CognitionProofLedgerWireVersionV1::V2;
        assert_eq!(old.canonical_bytes().is_ok(), mask & 12 == 0);
        let mut trailing = original;
        trailing.push(0);
        assert!(CognitionProofLedgerManifestV1::from_canonical_bytes(&trailing).is_err());
    }
}
