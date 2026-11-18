//! These are cryptographic input-binding tests, not GEN/native/Host proofs.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_context::FormalBoundedWorkV1, initial_activation_input_bytes, test_fixtures,
};

#[test]
fn original_signed_input_matches_the_authority_and_preserves_its_wire() {
    let setup = test_fixtures::incremental::setup();
    let expected = initial_activation_input_bytes(&setup.certificate).unwrap();
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let wire = signed_input(
        &GenesisApprovalInputV1 {
            authority: &setup.authority,
            certificate: &setup.certificate,
        },
        1 << 20,
        &mut common,
    )
    .unwrap();
    assert!(wire.uses_budget(&memory));
    assert_eq!(wire.as_slice(), expected);
    assert_eq!(
        setup.authority.initial_input_digest(),
        Some(blake3::hash(&expected).to_hex().as_str())
    );
    assert!(work.remaining_steps() < 1_000_000);
    assert!(work.remaining_read_bytes() < 64 << 20);
}

#[test]
fn changed_signature_with_the_same_approved_body_is_rejected() {
    let setup = test_fixtures::incremental::setup();
    let mut changed = setup.certificate.clone();
    // Keep the body and canonical signature grammar, alter only its first byte.
    let first = if changed.signature_hex.starts_with('0') {
        "1"
    } else {
        "0"
    };
    changed.signature_hex.replace_range(..1, first);
    assert_eq!(changed.body, setup.certificate.body);
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 64 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let result = signed_input(
        &GenesisApprovalInputV1 {
            authority: &setup.authority,
            certificate: &changed,
        },
        1 << 20,
        &mut common,
    );
    assert!(matches!(
        result,
        Err(Error::Store(PageStoreError::Corrupt(
            "GEN original signed input identity"
        )))
    ));
    assert!(work.remaining_steps() < 1_000_000);
    assert!(work.remaining_read_bytes() < 64 << 20);
    assert_eq!(memory.used(), 0);
}

#[test]
fn first_work_refusal_is_preserved_and_spent_allowance_is_not_replaced() {
    let setup = test_fixtures::incremental::setup();
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(1, 64 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let result = signed_input(
        &GenesisApprovalInputV1 {
            authority: &setup.authority,
            certificate: &setup.certificate,
        },
        1 << 20,
        &mut common,
    );
    assert!(matches!(result,
        Err(Error::Body(BodyMutationError::Original(ref source)))
            if source.downcast_ref::<FormalError>().is_some()
    ));
    assert_eq!(work.remaining_steps(), 0);
    assert_eq!(memory.used(), 0);
}
