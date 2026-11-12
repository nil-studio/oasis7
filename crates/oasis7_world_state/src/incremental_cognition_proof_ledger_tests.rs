use super::*;
#[test]
fn independent_capsule_has_exact_fixed_fields_and_absent_runtime_binding() {
    let root = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
    let mut value = CognitionProofLedgerManifestV1 {
        roots: [17; 32],
        previous_record: None,
        fields: [
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
            Some(root.clone()),
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
