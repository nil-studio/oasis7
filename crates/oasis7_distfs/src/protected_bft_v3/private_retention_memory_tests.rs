//! Transaction tests of a software backend, not H admission or hardware custody.
use super::*;
use crate::protected_bft_v3::private_retention::*;
use crate::protected_bft_v3::private_retention_tests::{fixture, fixture_inputs, snapshot};
use ed25519_dalek::{Signer, SigningKey};

fn backend() -> (BftPrivateRetentionMemoryBackendV1, BftPrivateRetentionCasV1) {
    let (mut role, map, mut receipt) = fixture();
    let mut before = snapshot(10, "0a".repeat(32));
    before.state.state_digest = crate::protected_bft_v3::state::test_state_digest(&before.state);
    let marker = BftPrivateRetentionCreationMarkerV1 {
        schema_version: 1,
        creation_identity: "0d".repeat(32),
        source: role.source.clone(),
        ledger_origin_digest: map.entries[0].key.ledger_origin_digest.clone(),
        activation_digest: "0e".repeat(32),
        initial_state_digest: before.state.state_digest.clone(),
        initial_height: 1,
    };
    role.creation_marker_digest = marker.digest().unwrap();
    receipt.body.head.capability_digest = role.digest().unwrap();
    let mut after = before.clone();
    after.state = before
        .state
        .with_private_retention_head(receipt.body.head.clone())
        .unwrap();
    receipt.body.before_state_digest = before.state.state_digest.clone();
    receipt.body.after_state_digest = after.state.state_digest.clone();
    let frame = crate::protected_bft_v3::crypto::framed_cbor(
        RECEIPT_DOMAIN,
        &receipt.body,
        crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
    )
    .unwrap();
    receipt.signature_hex = hex::encode(SigningKey::from_bytes(&[71; 32]).sign(&frame).to_bytes());
    let request = BftPrivateRetentionCasV1 {
        before: before.clone(),
        expected_retention: None,
        after: BftPrivateRetentionSnapshotV1 {
            inputs: fixture_inputs(&map),
            signer: after,
            retention: BftPrivateRetentionLedgerV1 {
                selection_index: None,
                map,
                receipt,
            },
        },
    };
    let backend = BftPrivateRetentionMemoryBackendV1 {
        inner: Arc::new(Mutex::new(Memory {
            _preparation: None,
            material_epoch: 0,
            validated_inputs: ValidatedMemoryInputs::empty(&role, 0).unwrap(),
            creation: Some(marker),
            signer: before,
            original: None,
            original_observed: true,
            operations: Default::default(),
            history: Default::default(),
            seals: Default::default(),
            sealed_inputs: Default::default(),
            hash_probe: Default::default(),
        })),
        trusted: role,
        clock_origin: Instant::now(),
        clock_floor: 123,
    };
    (backend, request)
}

fn continuation(
    before: &BftPrivateRetentionSnapshotV1,
    map: BftPrivateRetentionMapV1,
) -> BftPrivateRetentionCasV1 {
    let key = SigningKey::from_bytes(&[71; 32]);
    let mut head = before.retention.receipt.body.head.clone();
    head.sequence += 1;
    head.entry_count = map.entries.len() as u32;
    head.map_digest = map.digest().unwrap();
    head.map_byte_hash = blake3::hash(&map.canonical_bytes().unwrap())
        .to_hex()
        .to_string();
    head.previous_receipt_digest = Some(
        crate::protected_bft_v3::crypto::digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &before.retention.receipt,
        )
        .unwrap(),
    );
    head.retained_material_bytes = Some(
        crate::protected_bft_v3::private_retention_backend::retention_material_bound(
            &before.signer,
            Some(&before.retention),
            &map,
            &head,
            &before.inputs,
        )
        .unwrap(),
    );
    let mut after = before.signer.clone();
    after.state = before
        .signer
        .state
        .with_private_retention_head(head.clone())
        .unwrap();
    let body = BftPrivateRetentionReceiptBodyV1 {
        head,
        operation_digest: "ab".repeat(32),
        original_lease_generation: before.signer.lease_generation,
        before_revision: before.signer.state.revision,
        before_state_digest: before.signer.state.state_digest.clone(),
        after_revision: after.state.revision,
        after_state_digest: after.state.state_digest.clone(),
    };
    let frame = crate::protected_bft_v3::crypto::framed_cbor(
        RECEIPT_DOMAIN,
        &body,
        crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
    )
    .unwrap();
    BftPrivateRetentionCasV1 {
        before: before.signer.clone(),
        expected_retention: Some(before.retention.clone()),
        after: BftPrivateRetentionSnapshotV1 {
            inputs: before.inputs.clone(),
            signer: after,
            retention: BftPrivateRetentionLedgerV1 {
                selection_index: None,
                map,
                receipt: BftPrivateRetentionReceiptV1 {
                    body,
                    signature_hex: hex::encode(key.sign(&frame).to_bytes()),
                },
            },
        },
    }
}

#[test]
fn private_retention_memory_atomically_preserves_fact_and_advances_only_ready() {
    let (mut memory, request) = backend();
    let committed = memory.compare_and_swap_private_retention(&request).unwrap();
    request.validate_exact_readback(&committed).unwrap();
    let mut ready = committed.retention.map.clone();
    ready.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
    let request = continuation(&committed, ready);
    let committed = memory.compare_and_swap_private_retention(&request).unwrap();
    request.validate_exact_readback(&committed).unwrap();
    for mutation in 0..3 {
        let mut changed = committed.retention.map.clone();
        match mutation {
            0 => changed.entries.clear(),
            1 => changed.entries[0].fact = BftPrivateRetentionFactV1::ExplicitLegacyNone,
            _ => changed.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::ReservedPending,
        }
        let request = continuation(&committed, changed);
        assert!(memory.compare_and_swap_private_retention(&request).is_err());
        assert_eq!(
            memory
                .read_for_lease(
                    committed.signer.lease_generation,
                    &committed.signer.lease_token_digest
                )
                .unwrap(),
            committed.signer
        );
    }
}

#[test]
fn private_retention_memory_unknown_recovers_original_receipt_under_new_current_lease() {
    let (mut memory, request) = backend();
    // Model committed-then-transport-Err: original atomic result is discarded.
    memory.compare_and_swap_private_retention(&request).unwrap();
    let released = memory
        .release_lease(7, &request.before.lease_token_digest)
        .unwrap();
    let current = match memory
        .acquire_lease(
            &request.before.state.binding_digest,
            released.lease_generation,
            &"ee".repeat(32),
        )
        .unwrap()
    {
        BftSignerAcquireOutcomeV3::Acquired(current) => current,
        _ => panic!("new owner not acquired"),
    };
    assert!(
        memory
            .read_private_retention_operation(
                7,
                &request.before.lease_token_digest,
                &request.after.retention.receipt.body.operation_digest
            )
            .is_err()
    );
    let mut next = current.state.clone();
    next.revision += 1;
    assert!(
        memory
            .compare_and_swap(
                current.lease_generation,
                &current.lease_token_digest,
                current.state.revision,
                &current.state.state_digest,
                None,
                next
            )
            .is_err()
    );
    let original = memory
        .read_private_retention_operation(
            current.lease_generation,
            &current.lease_token_digest,
            &request.after.retention.receipt.body.operation_digest,
        )
        .unwrap();
    request.validate_exact_readback(&original).unwrap();
    assert_ne!(original.signer.lease_generation, current.lease_generation);
    assert_eq!(
        original.retention.receipt.signature_hex,
        request.after.retention.receipt.signature_hex
    );
    assert_eq!(
        memory
            .read_for_lease(current.lease_generation, &current.lease_token_digest)
            .unwrap(),
        current
    );
}

#[test]
fn private_retention_unsigned_head_is_digest_bound_and_legacy_none_bytes_stay_absent() {
    let (_, request) = backend();
    assert_ne!(
        request.before.state.state_digest,
        request.after.signer.state.state_digest
    );
    let before = oasis7_cas::to_canonical_cbor(&request.before.state).unwrap();
    let value: serde_cbor::Value = serde_cbor::from_slice(&before).unwrap();
    let serde_cbor::Value::Map(fields) = value else {
        panic!("map");
    };
    assert!(!fields.contains_key(&serde_cbor::Value::Text("private_retention".into())));
    assert!(request.before.state.private_retention.is_none());
    let mut changed = request.after.clone();
    changed
        .signer
        .state
        .private_retention
        .as_mut()
        .unwrap()
        .entry_count = 0;
    assert!(request.validate_exact_readback(&changed).is_err());
}

#[test]
fn private_retention_atomic_input_survives_lost_result_and_new_owner_exact_readback() {
    let (mut memory, request) = backend();
    let mut missing = request.clone();
    missing.after.inputs.clear();
    assert!(memory.compare_and_swap_private_retention(&missing).is_err());
    assert_eq!(
        memory
            .load_provisioned(&memory.trusted.source.binding_digest.clone())
            .unwrap(),
        request.before
    );
    // The protected transaction completed but its caller lost the result. A new
    // owner reads exactly the original four-part snapshot, not a new receipt.
    memory.compare_and_swap_private_retention(&request).unwrap();
    let released = memory
        .release_lease(
            request.before.lease_generation,
            &request.before.lease_token_digest,
        )
        .unwrap();
    let binding = memory.trusted.source.binding_digest.clone();
    let token = "ef".repeat(32);
    let acquired = match memory
        .acquire_lease(&binding, released.lease_generation, &token)
        .unwrap()
    {
        BftSignerAcquireOutcomeV3::Acquired(snapshot) => snapshot,
        _ => panic!("new owner must acquire original namespace"),
    };
    let original = memory
        .read_private_retention_operation(
            acquired.lease_generation,
            &token,
            &request.after.retention.receipt.body.operation_digest,
        )
        .unwrap();
    request.validate_exact_readback(&original).unwrap();
    assert_ne!(
        original.signer.lease_generation, acquired.lease_generation,
        "historical operation must not select current authority"
    );
    let input = original.inputs.values().next().unwrap();
    let restored = memory
        .load_private_retention_input(&binding, &input.descriptor().unwrap())
        .unwrap();
    assert_eq!(
        restored.canonical_bytes().unwrap(),
        input.canonical_bytes().unwrap()
    );
    assert_eq!(
        memory.load_provisioned(&binding).unwrap(),
        acquired,
        "input read cannot reset current State/lease/floor/transfer"
    );
    let mut changed = original;
    let key = changed.inputs.keys().next().unwrap().clone();
    std::sync::Arc::make_mut(changed.inputs.get_mut(&key).unwrap()).objects[0]
        .bytes
        .push(0);
    assert!(request.validate_exact_readback(&changed).is_err());
}

#[path = "private_retention_memory_hash_tests.rs"]
mod hash_tests;

#[path = "private_retention_memory_witness_tests.rs"]
mod witness_tests;

#[path = "private_retention_selection_memory_tests.rs"]
mod selection;

#[test]
fn private_retention_sealed_reopen_load_is_atomic_and_never_acquires() {
    let (mut memory, request) = backend();
    memory.compare_and_swap_private_retention(&request).unwrap();
    let binding = memory.trusted.source.binding_digest.clone();
    let released = memory
        .release_lease(
            request.before.lease_generation,
            &request.before.lease_token_digest,
        )
        .unwrap();
    assert!(released.lease_token_digest.is_empty());
    let loaded = memory
        .load_private_retention_sealed_current(&binding)
        .unwrap();
    assert_eq!(loaded.current.signer, released);
    assert_eq!(
        loaded.current.retention,
        Some(request.after.retention.clone())
    );
    assert_eq!(loaded.current.inputs, request.after.inputs);
    assert_eq!(loaded.history.len(), 1);
    assert!(loaded.seals.is_empty());
    assert!(loaded.sealed_inputs.is_empty());
    assert_eq!(memory.load_provisioned(&binding).unwrap(), released);
    assert!(
        memory
            .load_private_retention_sealed_current(&"ff".repeat(32))
            .is_err()
    );
    assert_eq!(memory.load_provisioned(&binding).unwrap(), released);
    // A stored generation is not a usable credential after release. The new
    // load remains available while the existing lease-fenced read fails.
    assert!(
        memory
            .read_private_retention_sealed_current(
                released.lease_generation,
                &released.lease_token_digest
            )
            .is_err()
    );
}
