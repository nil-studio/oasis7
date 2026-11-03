use super::*;

fn fixture() -> RecordRootRefV1 {
    RecordRootRefV1 {
        schema: [0x22; 32],
        owner_key: [0x33; 32],
        root: StateRootV1 {
            namespace: StateNamespaceV1::Business,
            node_root: "11".repeat(32),
            leaf_count: 2,
        },
    }
}

#[test]
fn record_root_literal_wire_and_typed_child_binding() {
    let value = fixture();
    let mut expected = b"oasis7.iws.record-root.v1\0".to_vec();
    // 64 binding bytes plus the existing 71-byte canonical StateRoot.
    expected.extend_from_slice(&135u32.to_be_bytes());
    expected.extend_from_slice(&[0x22; 32]);
    expected.extend_from_slice(&[0x33; 32]);
    expected.extend_from_slice(b"oasis7.iws.state-root.v1\0");
    expected.extend_from_slice(&42u32.to_be_bytes());
    expected.extend_from_slice(&1u16.to_be_bytes());
    expected.extend_from_slice(&2u64.to_be_bytes());
    expected.extend_from_slice(&[0x11; 32]);
    assert_eq!(value.canonical_bytes().unwrap().as_slice(), expected);
    let decoded = RecordRootRefV1::from_canonical_bytes(&expected).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(
        decoded
            .validate_binding(&[0x22; 32], &[0x33; 32], StateNamespaceV1::Business)
            .unwrap(),
        &value.root
    );
    assert!(
        decoded
            .validate_binding(&[0x23; 32], &[0x33; 32], StateNamespaceV1::Business)
            .is_err()
    );
    assert!(
        decoded
            .validate_binding(&[0x22; 32], &[0x34; 32], StateNamespaceV1::Business)
            .is_err()
    );
    assert!(
        decoded
            .validate_binding(&[0x22; 32], &[0x33; 32], StateNamespaceV1::Scheduler)
            .is_err()
    );
}

#[test]
fn record_root_rejects_truncation_tail_domains_lengths_and_invalid_child() {
    let bytes = fixture().canonical_bytes().unwrap();
    for length in 0..bytes.len() {
        assert!(RecordRootRefV1::from_canonical_bytes(&bytes[..length]).is_err());
    }
    let mut tail = bytes.to_vec();
    tail.push(0);
    assert!(RecordRootRefV1::from_canonical_bytes(&tail).is_err());
    for offset in [0, MAGIC.len(), MAGIC.len() + 68, MAGIC.len() + 68 + 24] {
        let mut malformed = bytes;
        malformed[offset] ^= 0x80;
        assert!(
            RecordRootRefV1::from_canonical_bytes(&malformed).is_err(),
            "offset {offset}"
        );
    }
    let mut invalid = fixture();
    invalid.root.leaf_count = 0;
    assert!(invalid.canonical_bytes().is_err());
    assert!(
        invalid
            .validate_binding(
                &invalid.schema,
                &invalid.owner_key,
                StateNamespaceV1::Business
            )
            .is_err()
    );
}

#[test]
fn empty_child_is_an_explicit_typed_state_root_not_missing_record() {
    let mut value = fixture();
    value.root = StateRootV1::empty(StateNamespaceV1::Scheduler).unwrap();
    assert_eq!(
        RecordRootRefV1::new(value.schema, value.owner_key, value.root.clone()).unwrap(),
        value
    );
    let parsed = RecordRootRefV1::from_canonical_bytes(&value.canonical_bytes().unwrap()).unwrap();
    assert_eq!(
        parsed
            .validate_binding(&value.schema, &value.owner_key, StateNamespaceV1::Scheduler)
            .unwrap(),
        &value.root
    );
    let mut invalid = value.root;
    invalid.leaf_count = 1;
    assert!(RecordRootRefV1::new(value.schema, value.owner_key, invalid).is_err());
}
