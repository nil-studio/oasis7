//! Codec-only fixtures; actual State/history authority is independently tested.
use super::*;
#[test]
fn original_operation_uses_exact_protected_protocol_codec() {
    let (_, map, receipt) = super::super::private_retention_tests::fixture();
    let proof = BftPrivateRetentionOperationProofV1 {
        state: super::super::private_retention_tests::snapshot(10, "0a".repeat(32)).state,
        ledger: BftPrivateRetentionLedgerV1 {
            selection_index: None, map, receipt },
    };
    let canonical = proof.canonical_bytes().unwrap();
    let private = serde_cbor::to_vec(&proof).unwrap();
    assert_ne!(canonical, private);
    let decoded = BftPrivateRetentionOperationProofV1::from_canonical_bytes(&canonical).unwrap();
    assert_eq!(decoded, proof);
    assert_eq!(decoded.canonical_bytes().unwrap(), canonical);
    assert!(BftPrivateRetentionOperationProofV1::from_canonical_bytes(&private).is_err());
    let mut trailing = canonical;
    trailing.push(0);
    assert!(BftPrivateRetentionOperationProofV1::from_canonical_bytes(&trailing).is_err());
}
