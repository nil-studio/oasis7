//! Codec/resource and atomic-input regressions. Registered with the next frozen batch.
use super::*;
use crate::protected_bft_v3::*;
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
fn private_retention_head2_nonzero_material_transfer_exact_boundary_and_overflow() {
    let (_, _, receipt) = crate::protected_bft_v3::private_retention_tests::fixture();
    let mut state = crate::protected_bft_v3::private_retention_tests::snapshot(10, "0a".repeat(32))
        .state
        .with_private_retention_head(receipt.body.head)
        .unwrap();
    let material = state
        .private_retention
        .as_ref()
        .unwrap()
        .retained_material_bytes
        .unwrap();
    assert!(material > 0);
    let mut transfer = BftSuccessorArchiveTransferLedgerV1 {
        schema_version: 1,
        reserved_response_bytes: BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1 - material,
    };
    validate_successor_archive_budget(&state, Some(&transfer)).unwrap();
    transfer.reserved_response_bytes += 1;
    assert!(validate_successor_archive_budget(&state, Some(&transfer)).is_err());
    state
        .private_retention
        .as_mut()
        .unwrap()
        .retained_material_bytes = Some(u64::MAX);
    transfer.reserved_response_bytes = 1;
    assert!(
        validate_successor_archive_budget(&state, Some(&transfer)).is_err(),
        "checked overflow is an error rather than wraparound"
    );
}
#[test]
fn private_retention_head1_none_bytes_remain_exact_and_old_codec_rejects_head2() {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyHead {
        schema_version: u32,
        source: BftPrivateRetentionSourceV1,
        capability_digest: String,
        sequence: u64,
        map_digest: String,
        map_byte_hash: String,
        entry_count: u32,
        previous_receipt_digest: Option<String>,
    }
    let (_, _, receipt) = crate::protected_bft_v3::private_retention_tests::fixture();
    let head2 = receipt.body.head;
    let mut old = head2.clone();
    old.schema_version = 1;
    old.retained_material_bytes = None;
    let bytes = serde_cbor::to_vec(&old).unwrap();
    let decoded: LegacyHead = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(serde_cbor::to_vec(&decoded).unwrap(), bytes);
    assert!(serde_cbor::from_slice::<LegacyHead>(&serde_cbor::to_vec(&head2).unwrap()).is_err());
}

#[test]
fn archive_encoded_transfer_owns_permit_after_atomic_decoded_release() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Hold(Arc<AtomicBool>);
    impl Drop for Hold {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let (_, map, _) = crate::protected_bft_v3::private_retention_tests::fixture();
    let input = crate::protected_bft_v3::private_retention_tests::input_for(
        &map.source,
        &map.entries[0].key,
    );
    let reference = input.descriptor().unwrap();
    let weak = Arc::downgrade(&input);
    let released = Arc::new(AtomicBool::new(false));
    let encoded =
        BftPrivateRetentionAdmissionInputReadV1::new(input, Box::new(Hold(released.clone())))
            .into_canonical_archive_read(&reference)
            .unwrap();
    assert!(
        weak.upgrade().is_none(),
        "decoded allocation must be atomically retired before durable readback"
    );
    assert!(!released.load(Ordering::SeqCst));
    reference.object.validate_bytes(encoded.bytes()).unwrap();
    drop(encoded);
    assert!(released.load(Ordering::SeqCst));
}
#[test]
fn archive_encoded_transfer_rejects_alias_and_releases_original_permit() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Hold(Arc<AtomicBool>);
    impl Drop for Hold {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let (_, map, _) = crate::protected_bft_v3::private_retention_tests::fixture();
    let input = crate::protected_bft_v3::private_retention_tests::input_for(
        &map.source,
        &map.entries[0].key,
    );
    let reference = input.descriptor().unwrap();
    let alias = input.clone();
    let released = Arc::new(AtomicBool::new(false));
    let result =
        BftPrivateRetentionAdmissionInputReadV1::new(input, Box::new(Hold(released.clone())))
            .into_canonical_archive_read(&reference);
    assert!(result.is_err());
    assert!(released.load(Ordering::SeqCst));
    assert_eq!(Arc::strong_count(&alias), 1);
}
