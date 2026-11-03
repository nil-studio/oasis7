//! Protocol/index component tests; no material verification or World claim.
use super::*;
use crate::protected_bft_v3::private_retention::*;

fn operation(
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    revision: u64,
) -> BftPrivateRetentionSelectionOperationV1 {
    BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
        reference: reference.clone(),
        expected_revision: revision,
        expected_state_digest: "aa".repeat(32),
        expected_head_digest: "bb".repeat(32),
    }
}

fn maximum_index() -> (
    BftPrivateRetentionMapV1,
    BftPrivateRetentionSelectionIndexV1,
) {
    let (_, mut map, _) = crate::protected_bft_v3::private_retention_tests::fixture();
    let template = map.entries[0].clone();
    map.entries.clear();
    let mut index = None;
    for height in 1..=256 {
        let mut entry = template.clone();
        entry.key.height = height;
        entry.lifecycle = BftPrivateRetentionLifecycleV1::Ready;
        let input =
            crate::protected_bft_v3::private_retention_tests::input_for(&map.source, &entry.key);
        entry.fact = input.fact().unwrap();
        let reference =
            BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, &entry).unwrap();
        let identity =
            VerifiedBftPrivateRetentionSelectionIdentityV1::from_independent_verification(
                BftPrivateRetentionSelectionKindV1::OriginalLocal,
                "ab".repeat(32),
                1024,
            )
            .unwrap();
        index = Some(
            select(
                index.as_ref(),
                &map.source,
                reference.clone(),
                identity,
                operation(&reference, height),
            )
            .unwrap()
            .0,
        );
        map.entries.push(entry);
    }
    (map, index.unwrap())
}

#[test]
fn protected_selection_maximum_index_is_bounded_and_exactly_committed() {
    let (map, index) = maximum_index();
    map.validate().unwrap();
    index.validate(&map).unwrap();
    let bytes = crate::protected_bft_v3::crypto::encode_bounded(
        &index,
        crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
    )
    .unwrap();
    assert_eq!(
        index.commitment().unwrap().encoded_bytes,
        bytes.len() as u64
    );
    let decoded: BftPrivateRetentionSelectionIndexV1 = {
        eprintln!("maximum 256-choice canonical index bytes={}", bytes.len());
        crate::protected_bft_v3::crypto::decode_bounded(
            &bytes,
            crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )
        .unwrap()
    };
    assert_eq!(decoded, index);
    let mut oversized = index;
    oversized.selections.push(oversized.selections[0].clone());
    let raw = serde_cbor::to_vec(&oversized).unwrap();
    assert!(serde_cbor::from_slice::<BftPrivateRetentionSelectionIndexV1>(&raw).is_err());
}

#[test]
fn protected_selection_filtered_index_retains_monotone_sequence() {
    let (map, mut index) = maximum_index();
    index.selections.clear();
    assert_eq!(index.commitment().unwrap().sequence, 256);
    let mut entry = map.entries[0].clone();
    entry.key.height = 257;
    entry.fact =
        crate::protected_bft_v3::private_retention_tests::input_for(&map.source, &entry.key)
            .fact()
            .unwrap();
    let reference =
        BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, &entry).unwrap();
    let identity = VerifiedBftPrivateRetentionSelectionIdentityV1::from_independent_verification(
        BftPrivateRetentionSelectionKindV1::OriginalLocal,
        "ab".repeat(32),
        1024,
    )
    .unwrap();
    let next = select(
        Some(&index),
        &map.source,
        reference.clone(),
        identity,
        operation(&reference, 257),
    )
    .unwrap()
    .0;
    assert_eq!(next.last_sequence, 257);
    assert_eq!(next.selections[0].sequence, 257);
}

#[test]
fn protected_selection_first_choice_conflict_and_exact_retry() {
    let (map, index) = maximum_index();
    let old = &index.selections[0];
    let identity = || {
        VerifiedBftPrivateRetentionSelectionIdentityV1::from_independent_verification(
            old.kind,
            old.local_identity_digest.clone(),
            old.local_identity_bytes,
        )
        .unwrap()
    };
    let (retried, changed) = select(
        Some(&index),
        &map.source,
        old.reference.clone(),
        identity(),
        old.operation.clone(),
    )
    .unwrap();
    assert!(!changed);
    assert_eq!(retried, index);
    let mut replacement = old.reference.clone();
    replacement.key.header_id = "fe".repeat(32);
    assert!(
        select(
            Some(&index),
            &map.source,
            replacement,
            identity(),
            old.operation.clone()
        )
        .is_err()
    );
}
