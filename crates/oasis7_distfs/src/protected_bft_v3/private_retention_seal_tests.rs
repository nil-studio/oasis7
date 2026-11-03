use super::super::successor_window::BftSuccessorWindowArchiveRefV1;
use super::*;
fn cut() -> BftSuccessorWindowCutV1 {
    let hash = "ab".repeat(32);
    BftSuccessorWindowCutV1 {
        height: 1,
        header_id: hash.clone(),
        certificate_digest: hash.clone(),
        execution_commitment_digest: hash.clone(),
        snapshot_ref: BftSuccessorWindowArchiveRefV1 {
            digest: hash.clone(),
            bytes: 1,
        },
        journal_ref: BftSuccessorWindowArchiveRefV1 {
            digest: hash.clone(),
            bytes: 1,
        },
        semantic_state_root: hash.clone(),
        post_manifest_hash: hash.clone(),
        business_result_root: hash.clone(),
        driver_record_digest: hash.clone(),
        driver_install_digest: hash,
    }
}
fn body() -> BftPrivateRetentionSealBodyV1 {
    let (_, mut map, receipt) = super::super::private_retention_tests::fixture();
    map.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
    let mut pending = map.entries[0].clone();
    pending.key.height = 2;
    pending.fact = super::super::private_retention_tests::input_for(&map.source, &pending.key)
        .fact()
        .unwrap();
    map.entries.push(pending);
    BftPrivateRetentionSealBodyV1::from_original(
        BftPrivateRetentionLedgerV1 {
            selection_index: None, map, receipt },
        cut(),
    )
    .unwrap()
}
#[test]
fn seal_shape_derives_all_unsigned_original_t_plus_one() {
    let original = body();
    assert_eq!(original.carry_keys.len(), 1);
    let carry = original.carry_map().unwrap();
    assert_eq!(carry.coverage_start_height, 2);
    assert_eq!(carry.entries, original.original.map.entries[1..]);
    assert_eq!(
        carry.previous_seal_receipt_digest.as_ref(),
        Some(&original.digest().unwrap())
    );
    let mut omitted = original.clone();
    omitted.carry_keys.clear();
    assert!(omitted.validate_shape().is_err());
    assert_eq!(omitted.original, original.original);
}
#[test]
fn seal_shape_rejects_reserved_and_non_adjacent_future_originals() {
    let mut reserved = body();
    reserved.original.map.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::ReservedPending;
    assert!(reserved.validate_shape().is_err());
    let mut future = body();
    future.original.map.entries[1].key.height = 3;
    future.carry_keys[0].height = 3;
    assert!(future.validate_shape().is_err());
}
#[test]
fn seal_memory_readback_rejects_coherent_deepcopy_of_original_payloads() {
    use super::super::private_retention_backend::{
        BftPrivateRetentionCurrentV1, BftPrivateRetentionSnapshotV1,
    };
    use std::sync::Arc;
    let body = body();
    let old = body.original.clone();
    let originals = super::super::private_retention_tests::fixture_inputs(&old.map);
    let seal = Arc::new(BftPrivateRetentionSealV1 {
        body,
        signature_hex: "00".repeat(64),
    });
    let map = seal.body.carry_map().unwrap();
    let carried = originals
        .iter()
        .filter(|(_, input)| input.key.height == 2)
        .map(|(id, input)| (id.clone(), input.clone()))
        .collect();
    let signer = super::super::private_retention_tests::snapshot(10, "0a".repeat(32));
    let expected = BftPrivateRetentionSealCasV1 {
        before: BftPrivateRetentionCurrentV1 {
            signer: signer.clone(),
            retention: Some(old.clone()),
            inputs: originals.clone(),
        },
        previous_seals: Default::default(),
        previous_inputs: Default::default(),
        seal: seal.clone(),
        after: BftPrivateRetentionSnapshotV1 {
            signer,
            retention: BftPrivateRetentionLedgerV1 {
            selection_index: None,
                map,
                receipt: old.receipt,
            },
            inputs: carried,
        },
        retained_seals: [(seal.body.digest().unwrap(), seal)].into_iter().collect(),
        retained_inputs: originals,
    };
    expected
        .validate_in_process_readback(&expected.clone())
        .unwrap();
    let mut copied = expected.clone();
    let copied_inputs = expected
        .retained_inputs
        .iter()
        .map(|(id, input)| (id.clone(), Arc::new((**input).clone())))
        .collect::<super::super::private_retention_backend::BftPrivateRetentionInputInventoryV1>();
    copied.before.inputs = copied_inputs.clone();
    copied.after.inputs = copied_inputs
        .iter()
        .filter(|(_, input)| input.key.height == 2)
        .map(|(id, input)| (id.clone(), input.clone()))
        .collect();
    copied.retained_inputs = copied_inputs;
    assert!(copied == expected);
    copied.validate_exact_inventory().unwrap();
    assert!(expected.validate_in_process_readback(&copied).is_err());
}

#[test]
fn original_seal_uses_exact_protected_protocol_codec() {
    // This fixture proves codec routing only, never signature/World validity.
    let seal = BftPrivateRetentionSealV1 {
        body: body(),
        signature_hex: "00".repeat(64),
    };
    let canonical = seal.canonical_bytes().unwrap();
    let private = serde_cbor::to_vec(&seal).unwrap();
    assert_ne!(canonical, private);
    let decoded = BftPrivateRetentionSealV1::from_canonical_bytes(&canonical).unwrap();
    assert_eq!(decoded, seal);
    assert_eq!(decoded.canonical_bytes().unwrap(), canonical);
    assert!(BftPrivateRetentionSealV1::from_canonical_bytes(&private).is_err());
}
