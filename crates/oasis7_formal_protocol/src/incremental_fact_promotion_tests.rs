//! Include as a child of incremental_fact_occurrence_tests to use signed fixtures.
use super::*;
#[test]
fn prepaid_fact_promotion_refines_without_new_admission() {
    let f = current();
    let checked = check_candidate_fact_v1(
        &CandidateFactRequestV1 {
            kind: ExecutionFactKindV1::Result,
            candidate: &f.candidate,
            core: &f.core,
            originals: &f.originals,
            ordinal: 1,
        },
        &f.s.memory,
        || false,
    )
    .unwrap();
    let baseline = f.s.memory.used();
    assert!(matches!(
        prepare_candidate_fact_promotion_v1(&checked, &f.s.memory, || true),
        Err(CoreError::Cancelled)
    ));
    let prepared = prepare_candidate_fact_promotion_v1(&checked, &f.s.memory, || false).unwrap();
    assert!(finish_prepared_fact_promotion_v1(prepared, &f.s.commit).is_err());
    assert_eq!(f.s.memory.used(), baseline);
    let head = formal::verify_formal_qualified(&f.proof, &f.s.authority, None, None).unwrap();
    let prepared = prepare_candidate_fact_promotion_v1(&checked, &f.s.memory, || false).unwrap();
    // Exhaust every remaining byte without assuming a particular fixture limit.
    let mut held = Vec::new();
    for power in (0..usize::BITS).rev() {
        let amount = 1usize << power;
        while let Ok(r) = f.s.memory.reserve(amount, MemoryClass::Ordinary) {
            held.push(r);
        }
    }
    assert!(prepare_candidate_fact_promotion_v1(&checked, &f.s.memory, || false).is_err());
    let accepted = finish_prepared_fact_promotion_v1(prepared, &head).unwrap();
    assert_eq!(accepted.head_record_id(), checked.candidate_record_id());
    drop(held);
    drop(accepted);
    assert_eq!(f.s.memory.used(), baseline);
    let memory = f.s.memory.clone();
    drop(checked);
    drop(f);
    assert_eq!(memory.used(), 0);
}
