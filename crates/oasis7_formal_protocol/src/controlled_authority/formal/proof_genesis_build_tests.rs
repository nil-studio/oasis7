//! Signed protocol DATA only; no physical pages or Accepted GEN fixture.
use super::*;
use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
fn memory() -> MemoryBudget {
    MemoryBudget::new(128 << 20, 0).unwrap()
}
#[test]
fn original_decision_and_fullproof_wire_match_paid_builders_and_release() {
    let (proof, candidate, context) = super::super::tests::fixture();
    let memory = memory();
    let work = FormalBoundedWorkV1::new(10_000_000, 64 << 20).unwrap();
    let mut no = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut no);
    let decision = build_genesis_decision_bounded(
        &candidate,
        &context,
        &proof.decision.primary,
        &proof.decision.replica,
        &mut ctx,
    )
    .unwrap();
    assert_eq!(
        serde_cbor::to_vec(decision.decision()).unwrap(),
        serde_cbor::to_vec(&proof.decision).unwrap()
    );
    let paid = build_genesis_qualified_bounded(
        &decision,
        &candidate,
        &context,
        &proof.primary_complete,
        &proof.replica_complete,
        &mut ctx,
    )
    .unwrap();
    assert_eq!(
        serde_cbor::to_vec(paid.proof()).unwrap(),
        serde_cbor::to_vec(&proof).unwrap()
    );
    let old = verify_formal_qualified(&proof, context.finalizing_authority(), None, None).unwrap();
    assert_eq!(
        paid.verified().verified_record_id(),
        old.verified_record_id()
    );
    assert!(paid.uses_budget(&memory));
    assert_eq!(
        memory.used(),
        paid.reserved_bytes() + decision.reserved_bytes()
    );
    drop(paid);
    drop(decision);
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_crypto_rejects_changed_final_or_complete_no_output_fee_survives() {
    let (proof, candidate, context) = super::super::tests::fixture();
    let memory = memory();
    let work = FormalBoundedWorkV1::new(10_000_000, 64 << 20).unwrap();
    let mut no = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut no);
    let mut changed = proof.decision.primary.clone();
    changed.signature_hex = "0".repeat(128);
    assert!(
        build_genesis_decision_bounded(
            &candidate,
            &context,
            &changed,
            &proof.decision.replica,
            &mut ctx
        )
        .is_err()
    );
    assert_eq!(memory.used(), 0);
    let decision = build_genesis_decision_bounded(
        &candidate,
        &context,
        &proof.decision.primary,
        &proof.decision.replica,
        &mut ctx,
    )
    .unwrap();
    let baseline = memory.used();
    let mut changed = proof.replica_complete.clone();
    changed.signature_hex = "0".repeat(128);
    assert!(
        build_genesis_qualified_bounded(
            &decision,
            &candidate,
            &context,
            &proof.primary_complete,
            &changed,
            &mut ctx
        )
        .is_err()
    );
    assert_eq!(memory.used(), baseline);
    drop(decision);
    assert_eq!(memory.used(), 0);
}
#[test]
fn cancellation_or_zero_memory_before_clone_never_returns_a_paid_proof() {
    let (proof, candidate, context) = super::super::tests::fixture();
    for mode in 0..2 {
        let memory = if mode == 0 {
            MemoryBudget::new(0, 0).unwrap()
        } else {
            memory()
        };
        let work = FormalBoundedWorkV1::new(10_000_000, 64 << 20).unwrap();
        let mut stop = || mode == 1;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        assert!(
            build_genesis_decision_bounded(
                &candidate,
                &context,
                &proof.decision.primary,
                &proof.decision.replica,
                &mut ctx
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
