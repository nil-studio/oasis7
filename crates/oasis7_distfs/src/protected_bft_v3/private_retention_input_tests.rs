use super::*;
use sha2::{Digest, Sha256};

fn vector() -> BftPrivateRetentionAdmissionInputV1 {
    BftPrivateRetentionAdmissionInputV1 {
        schema_version: 1,
        source: BftPrivateRetentionSourceV1 {
            binding_digest: "01".repeat(32),
            handoff_digest: "02".repeat(32),
            material_source_digest: "03".repeat(32),
        },
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: "04".repeat(32),
            height: 1,
            header_id: "05".repeat(32),
        },
        publication_reservation_bytes: 123,
        objects: vec![
            BftPrivateRetentionInputObjectV1 {
                kind: BftPrivateRetentionInputObjectKindV1::OriginalData,
                reference: BftSuccessorWindowArchiveRefV1 {
                    digest: "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
                        .into(),
                    bytes: 3,
                },
                bytes: b"abc".to_vec(),
            },
            BftPrivateRetentionInputObjectV1 {
                kind: BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
                reference: BftSuccessorWindowArchiveRefV1 {
                    digest: "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
                        .into(),
                    bytes: 11,
                },
                bytes: b"hello world".to_vec(),
            },
        ],
    }
}
#[test]
fn private_retention_input_typed_exact_codec_has_fixed_independent_byte_hash_oracle() {
    let input = vector();
    let (bytes, reference) = input.canonical_with_descriptor().unwrap();
    // Independent fixed vector generated from the declared field order and
    // shortest CBOR integers/byte strings, without serde or the CAS encoder.
    assert_eq!(bytes.len(), 749);
    assert_eq!(
        hex::encode(Sha256::digest(&bytes)),
        "f716c1e8e72573e4bd46ce63c8be5696d803a27c6d2df0f0adb81500b74f4a06"
    );
    assert_eq!(bytes[0], 0xa5);
    assert_ne!(
        bytes,
        oasis7_cas::to_canonical_cbor(&input).unwrap(),
        "the tagged/sorted CAS codec is a distinct preimage"
    );
    assert_eq!(
        reference.object.digest,
        blake3::hash(&bytes).to_hex().to_string()
    );
    assert_eq!(reference, input.descriptor().unwrap());
    reference
        .validate_input(&BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(&bytes).unwrap())
        .unwrap();
    let mut extra = bytes;
    extra.push(0);
    assert!(BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(&extra).is_err());
}
#[test]
fn private_retention_input_preserves_existing_dependency_object_exact_bound() {
    let mut input = vector();
    let mut dependency = BftPrivateRetentionInputObjectV1 {
        kind: BftPrivateRetentionInputObjectKindV1::Dependency,
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: String::new(),
            bytes: BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1 as u64,
        },
        bytes: vec![42; BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1],
    };
    dependency.reference.digest = blake3::hash(&dependency.bytes).to_hex().to_string();
    input.objects.push(dependency);
    input
        .objects
        .sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    let bytes = input.canonical_bytes().unwrap();
    assert_eq!(
        BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(&bytes).unwrap(),
        input
    );
    let dependency = input
        .objects
        .iter_mut()
        .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::Dependency)
        .unwrap();
    dependency.bytes.push(43);
    dependency.reference.bytes += 1;
    dependency.reference.digest = blake3::hash(&dependency.bytes).to_hex().to_string();
    input
        .objects
        .sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    assert!(input.canonical_bytes().is_err());
}

#[test]
fn private_retention_cold_archive_demands_exact_source_key_objects_and_first_charge() {
    let directory = tempfile::tempdir().unwrap();
    let input = vector();
    let expected = input.descriptor().unwrap();
    let store =
        LocalBftSuccessorArchiveStoreV1::new(oasis7_cas::LocalCasStore::new(directory.path()))
            .unwrap();

    assert!(store.load_original_admission_input(&expected).is_err());
    let mut repriced = input.clone();
    repriced.publication_reservation_bytes += 1;
    assert!(
        store
            .persist_original_admission_input(&expected, &repriced)
            .is_err()
    );
    assert!(store.load_original_admission_input(&expected).is_err());
    store
        .persist_original_admission_input(&expected, &input)
        .unwrap();
    drop(store);

    let cold =
        LocalBftSuccessorArchiveStoreV1::new(oasis7_cas::LocalCasStore::new(directory.path()))
            .unwrap();
    let original = cold.load_original_admission_input(&expected).unwrap();
    assert_eq!(original, input);
    for wrong in [
        BftPrivateRetentionAdmissionInputRefV1 {
            source: repriced.source.clone(),
            key: BftPrivateRetentionKeyV1 {
                height: 2,
                ..expected.key.clone()
            },
            ..expected.clone()
        },
        BftPrivateRetentionAdmissionInputRefV1 {
            source: BftPrivateRetentionSourceV1 {
                binding_digest: "ab".repeat(32),
                ..expected.source.clone()
            },
            ..expected.clone()
        },
        BftPrivateRetentionAdmissionInputRefV1 {
            admission_digest: repriced.admission_digest().unwrap(),
            ..expected.clone()
        },
    ] {
        assert!(cold.load_original_admission_input(&wrong).is_err());
    }
    let mut invalid = input.clone();
    invalid.schema_version = 2;
    assert!(
        BftPrivateRetentionAdmissionInputV1::from_canonical_bytes(
            &serde_cbor::to_vec(&invalid).unwrap()
        )
        .is_err()
    );
    invalid.schema_version = 1;
    invalid.objects.pop();
    assert!(
        cold.persist_original_admission_input(&expected, &invalid)
            .is_err()
    );
}
