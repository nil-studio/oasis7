use super::*;
use crate::incremental_projection::WORLD_STATE_FIELDS;
use std::collections::BTreeSet;

fn field(name: &'static str, shape: FieldShape) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        destination: FieldDestination::State(Namespace::Business),
        shape,
    }
}
#[test]
fn frozen_vector_borrows_name_and_preserves_zero_member_digest() {
    let key = WorldFieldKeyV1::new(
        field("material_ledgers", FieldShape::NestedMap),
        WorldFieldRecordKindV1::Group,
        Some([0x11; 32]),
    )
    .unwrap();
    // Independently formed with Python struct.pack(">BHBBH", 1,1,3,2,16).
    assert_eq!(
        hex::encode(key.as_bytes()),
        "6f61736973372e6977732e776f726c642d6669656c642d6b65792e763100010001030200106d6174657269616c5f6c6564676572731111111111111111111111111111111111111111111111111111111111111111"
    );
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).unwrap();
    assert_eq!(parsed.name, "material_ledgers");
    assert_eq!(
        parsed.name.as_ptr(),
        key.as_bytes()[DOMAIN.len() + 7..].as_ptr()
    );
    assert_eq!(
        (
            parsed.owner,
            parsed.namespace,
            parsed.shape,
            parsed.kind,
            parsed.semantic_key
        ),
        (
            FieldOwner::WorldState,
            Namespace::Business,
            FieldShape::NestedMap,
            WorldFieldRecordKindV1::Group,
            Some([0x11; 32])
        )
    );
    let metadata = WorldFieldKeyV1::new(
        field("ledger", FieldShape::NestedMap),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .unwrap();
    let entry = WorldFieldKeyV1::new(
        field("ledger", FieldShape::NestedMap),
        WorldFieldRecordKindV1::Entry,
        Some([0; 32]),
    )
    .unwrap();
    assert_ne!(metadata.as_bytes(), entry.as_bytes());
    assert_eq!(
        WorldFieldKeyV1::from_bytes(metadata.as_bytes())
            .unwrap()
            .semantic_key,
        None
    );
    assert_eq!(
        WorldFieldKeyV1::from_bytes(entry.as_bytes())
            .unwrap()
            .semantic_key,
        Some([0; 32])
    );
}
#[test]
fn all_ownership_dimensions_are_bound_and_catalog_names_fit_the_profile() {
    let mut keys = BTreeSet::new();
    for owner in [
        FieldOwner::WorldState,
        FieldOwner::Runtime,
        FieldOwner::Cognition,
    ] {
        for namespace in [
            Namespace::Business,
            Namespace::Allocator,
            Namespace::Scheduler,
            Namespace::Authorization,
            Namespace::Effects,
            Namespace::Artifacts,
        ] {
            for shape in [
                FieldShape::Value,
                FieldShape::Map,
                FieldShape::NestedMap,
                FieldShape::Set,
                FieldShape::MapSet,
                FieldShape::Ordered,
                FieldShape::ByteMap,
            ] {
                for name in ["ledger", "ledger.entry"] {
                    let descriptor = FieldDescriptor {
                        owner,
                        name,
                        destination: FieldDestination::State(namespace),
                        shape,
                    };
                    let key = WorldFieldKeyV1::new(
                        descriptor,
                        WorldFieldRecordKindV1::Entry,
                        Some([3; 32]),
                    )
                    .unwrap();
                    assert!(keys.insert(key.as_bytes().to_vec()));
                    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).unwrap();
                    assert_eq!(
                        (parsed.owner, parsed.namespace, parsed.shape, parsed.name),
                        (owner, namespace, shape, name)
                    );
                }
            }
        }
    }
    assert_eq!(keys.len(), 252);
    for descriptor in WORLD_STATE_FIELDS.iter().copied() {
        let key = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None);
        assert_eq!(
            key.is_ok(),
            matches!(descriptor.destination, FieldDestination::State(_)),
            "{}",
            descriptor.name
        );
    }
    let maximum = "x".repeat(WORLD_FIELD_NAME_MAX_BYTES);
    // Parse the exact bound without requiring a process-global static string.
    let mut raw = b"oasis7.iws.world-field-key.v1\0".to_vec();
    raw.extend_from_slice(&[1, 0, 1, 2, 1, 0, 128]);
    raw.extend_from_slice(maximum.as_bytes());
    raw.extend_from_slice(&[0; 32]);
    assert_eq!(raw.len(), WORLD_FIELD_KEY_MAX_BYTES);
    assert_eq!(WorldFieldKeyV1::from_bytes(&raw).unwrap().name, maximum);
}
#[test]
fn malformed_tags_framing_names_and_kind_combinations_fail_closed() {
    let original = WorldFieldKeyV1::new(
        field("ledger", FieldShape::NestedMap),
        WorldFieldRecordKindV1::Group,
        Some([9; 32]),
    )
    .unwrap();
    let bytes = original.as_bytes();
    for end in 0..bytes.len() {
        assert!(WorldFieldKeyV1::from_bytes(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(WorldFieldKeyV1::from_bytes(&trailing).is_err());
    for (offset, value) in [
        (0, 0),
        (DOMAIN.len(), 4),
        (DOMAIN.len() + 1, 1),
        (DOMAIN.len() + 2, 0),
        (DOMAIN.len() + 3, 8),
        (DOMAIN.len() + 4, 0),
        (DOMAIN.len() + 5, 255),
        (DOMAIN.len() + 6, 0),
        (DOMAIN.len() + 7, b' '),
    ] {
        let mut corrupt = bytes.to_vec();
        corrupt[offset] = value;
        assert!(
            WorldFieldKeyV1::from_bytes(&corrupt).is_err(),
            "offset {offset}"
        );
    }
    for name in [
        "",
        ".field",
        "field.",
        "field..name",
        "two words",
        "é",
        "_field",
    ] {
        assert!(
            WorldFieldKeyV1::new(
                field(name, FieldShape::Map),
                WorldFieldRecordKindV1::Entry,
                Some([0; 32])
            )
            .is_err()
        );
    }
    for shape in [
        FieldShape::History,
        FieldShape::Process,
        FieldShape::Children,
    ] {
        assert!(
            WorldFieldKeyV1::new(
                field("field", shape),
                WorldFieldRecordKindV1::Metadata,
                None
            )
            .is_err()
        );
    }
    for destination in [
        FieldDestination::History,
        FieldDestination::ProofLedger,
        FieldDestination::RetentionLedger,
        FieldDestination::Process,
        FieldDestination::Children,
    ] {
        let descriptor = FieldDescriptor {
            destination,
            ..field("field", FieldShape::Map)
        };
        assert!(WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None).is_err());
    }
    for shape in [
        FieldShape::Value,
        FieldShape::Map,
        FieldShape::Set,
        FieldShape::Ordered,
        FieldShape::ByteMap,
    ] {
        assert!(
            WorldFieldKeyV1::new(
                field("field", shape),
                WorldFieldRecordKindV1::Group,
                Some([1; 32])
            )
            .is_err()
        );
    }
    assert!(
        WorldFieldKeyV1::new(
            field("field", FieldShape::Map),
            WorldFieldRecordKindV1::Bytes,
            Some([1; 32])
        )
        .is_err()
    );
    assert!(
        WorldFieldKeyV1::new(
            field("field", FieldShape::Map),
            WorldFieldRecordKindV1::Metadata,
            Some([0; 32])
        )
        .is_err()
    );
    assert!(
        WorldFieldKeyV1::new(
            field("field", FieldShape::Map),
            WorldFieldRecordKindV1::Entry,
            None
        )
        .is_err()
    );
    let mut metadata = WorldFieldKeyV1::new(
        field("field", FieldShape::Map),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .unwrap()
    .as_bytes()
    .to_vec();
    *metadata.last_mut().unwrap() = 1;
    assert!(WorldFieldKeyV1::from_bytes(&metadata).is_err());
    // Producer validation is not a substitute for validation of untrusted wire.
    let mut invalid_group = bytes.to_vec();
    invalid_group[DOMAIN.len() + 3] = 2;
    assert!(WorldFieldKeyV1::from_bytes(&invalid_group).is_err());
}

#[test]
fn all_record_kinds_have_distinct_keys_and_the_name_limit_is_explicit() {
    let descriptor = field("mixed", FieldShape::ByteMap);
    for (shape, kind, digest) in [
        (FieldShape::ByteMap, WorldFieldRecordKindV1::Metadata, None),
        (
            FieldShape::MapSet,
            WorldFieldRecordKindV1::Group,
            Some([0; 32]),
        ),
        (
            FieldShape::ByteMap,
            WorldFieldRecordKindV1::Entry,
            Some([0; 32]),
        ),
        (
            FieldShape::ByteMap,
            WorldFieldRecordKindV1::Bytes,
            Some([0; 32]),
        ),
    ] {
        let key = WorldFieldKeyV1::new(
            FieldDescriptor {
                shape,
                ..descriptor
            },
            kind,
            digest,
        )
        .unwrap();
        let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).unwrap();
        assert_eq!((parsed.kind, parsed.semantic_key), (kind, digest));
    }
    let entry =
        WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Entry, Some([0; 32])).unwrap();
    let bytes =
        WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Bytes, Some([0; 32])).unwrap();
    assert_ne!(entry.as_bytes(), bytes.as_bytes());
    const TOO_LONG: &str = concat!(
        "abcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefgh",
        "abcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefgh",
        "a"
    );
    assert_eq!(TOO_LONG.len(), WORLD_FIELD_NAME_MAX_BYTES + 1);
    assert!(
        WorldFieldKeyV1::new(
            field(TOO_LONG, FieldShape::Value),
            WorldFieldRecordKindV1::Metadata,
            None
        )
        .is_err()
    );
    let mut invalid_utf8 = entry.as_bytes().to_vec();
    invalid_utf8[DOMAIN.len() + 7] = 255;
    assert!(WorldFieldKeyV1::from_bytes(&invalid_utf8).is_err());
}
