//! Atomic software-memory transaction evidence, not material/World admission.
use super::*;
use crate::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1;
use crate::protected_bft_v3::private_retention_selection::*;

fn selection_transaction(before: &BftPrivateRetentionSnapshotV1) -> BftPrivateRetentionCasV1 {
    let mut map = before.retention.map.clone();
    map.entries[0].lifecycle = BftPrivateRetentionLifecycleV1::Ready;
    let reference =
        BftPrivateRetentionAdmissionInputRefV1::from_entry(&map.source, &map.entries[0]).unwrap();
    let identity = VerifiedBftPrivateRetentionSelectionIdentityV1::from_independent_verification(
        BftPrivateRetentionSelectionKindV1::OriginalLocal,
        "ef".repeat(32),
        321,
    )
    .unwrap();
    let operation = BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
        reference: reference.clone(),
        expected_revision: before.signer.state.revision,
        expected_state_digest: before.signer.state.state_digest.clone(),
        expected_head_digest: before.retention.receipt.body.head.digest().unwrap(),
    };
    let index = select(None, &map.source, reference, identity, operation)
        .unwrap()
        .0;
    let mut request = continuation(before, map);
    request.after.retention.selection_index = Some(index);
    rebind(&mut request);
    request
}

fn rebind(request: &mut BftPrivateRetentionCasV1) {
    let operation_digest = request
        .after
        .retention
        .selection_index
        .as_ref()
        .unwrap()
        .selections
        .last()
        .map(|s| s.operation_digest.clone())
        .unwrap_or_else(|| "cd".repeat(32));
    let body = &mut request.after.retention.receipt.body;
    body.operation_digest = operation_digest;
    body.head.schema_version = 3;
    body.head.selection_commitment = Some(
        request
            .after
            .retention
            .selection_index
            .as_ref()
            .unwrap()
            .commitment()
            .unwrap(),
    );
    body.head.retained_material_bytes = Some(
        crate::protected_bft_v3::private_retention_backend::retention_material_bound(
            &request.before,
            request.expected_retention.as_ref(),
            &request.after.retention.map,
            &body.head,
            &request.after.inputs,
        )
        .unwrap(),
    );
    request.after.signer.state = request
        .before
        .state
        .with_private_retention_head(body.head.clone())
        .unwrap();
    body.after_revision = request.after.signer.state.revision;
    body.after_state_digest = request.after.signer.state.state_digest.clone();
    let frame = crate::protected_bft_v3::crypto::framed_cbor(
        RECEIPT_DOMAIN,
        body,
        crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_SIGNING_BYTES_V3,
    )
    .unwrap();
    request.after.retention.receipt.signature_hex =
        hex::encode(SigningKey::from_bytes(&[71; 32]).sign(&frame).to_bytes());
}

#[test]
fn protected_selection_atomic_ready_keeps_original_input_and_first_receipt() {
    let (mut memory, request) = backend();
    let reserved = memory.compare_and_swap_private_retention(&request).unwrap();
    let select = selection_transaction(&reserved);
    let selected = memory.compare_and_swap_private_retention(&select).unwrap();
    select.validate_exact_readback(&selected).unwrap();
    let id = &selected.retention.receipt.body.operation_digest;
    let original = memory
        .read_private_retention_operation(
            selected.signer.lease_generation,
            &selected.signer.lease_token_digest,
            id,
        )
        .unwrap();
    assert_eq!(original, selected);
    for (key, input) in &reserved.inputs {
        assert!(std::sync::Arc::ptr_eq(
            input,
            selected.inputs.get(key).unwrap()
        ));
    }
    assert_eq!(
        selected.retention.map.entries[0].fact,
        reserved.retention.map.entries[0].fact
    );
    assert_eq!(
        selected
            .retention
            .selection_index
            .as_ref()
            .unwrap()
            .last_sequence,
        1
    );
}

#[test]
fn protected_selection_invalid_sequence_fails_without_partial_publication() {
    let (mut memory, request) = backend();
    let reserved = memory.compare_and_swap_private_retention(&request).unwrap();
    let mut invalid = selection_transaction(&reserved);
    let index = invalid.after.retention.selection_index.as_mut().unwrap();
    index.last_sequence = 2;
    index.selections[0].sequence = 2;
    rebind(&mut invalid);
    assert!(memory.compare_and_swap_private_retention(&invalid).is_err());
    let current = memory
        .read_private_retention_current(
            reserved.signer.lease_generation,
            &reserved.signer.lease_token_digest,
        )
        .unwrap();
    assert_eq!(current.signer, reserved.signer);
    assert_eq!(current.retention.as_ref(), Some(&reserved.retention));
    assert_eq!(current.inputs, reserved.inputs);
}

#[test]
fn protected_selection_signed_drop_choice_rejected() {
    let (mut memory, request) = backend();
    let reserved = memory.compare_and_swap_private_retention(&request).unwrap();
    let request = selection_transaction(&reserved);
    let selected = memory.compare_and_swap_private_retention(&request).unwrap();
    let mut invalid = continuation(&selected, selected.retention.map.clone());
    let mut index = selected.retention.selection_index.clone().unwrap();
    index.selections.clear();
    invalid.after.retention.selection_index = Some(index);
    rebind(&mut invalid);
    assert!(memory.compare_and_swap_private_retention(&invalid).is_err());
    let original = memory
        .read_private_retention_operation(
            selected.signer.lease_generation,
            &selected.signer.lease_token_digest,
            &selected.retention.receipt.body.operation_digest,
        )
        .unwrap();
    assert_eq!(original, selected);
}
