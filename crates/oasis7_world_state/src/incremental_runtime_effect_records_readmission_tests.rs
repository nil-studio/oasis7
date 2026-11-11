//! Resource admission of immutable family summaries; structural replay has its own receiver.
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;
#[test]
fn effect_family_readmission_keeps_frontier_exact_and_checks_resource_identity() {
    let memory = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
    let roots = RootSetV1::empty("effect-readmission".into(), [3; 32]).unwrap();
    let frontier = RuntimeEffectQueueFrontierV1 {
        head: Some(17),
        tail: Some(29),
        count: 2,
        next_ordinal: 30,
    };
    let source = mint(
        roots.digest().unwrap(),
        frontier,
        [2, 3],
        &memory,
        MemoryClass::Recovery,
    )
    .unwrap();
    let before = memory.used();
    let ordinary =
        readmit_runtime_effect_family_ordinary_v1(&source, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(ordinary.pending_frontier(), frontier);
    assert_eq!(ordinary.record_counts(), [2, 3]);
    assert_eq!(memory.used(), before + ordinary.reserved_bytes());
    assert!(
        readmit_runtime_effect_family_ordinary_v1(&ordinary, &roots, &memory, || false).is_err()
    );
    assert!(readmit_runtime_effect_family_ordinary_v1(&source, &roots, &memory, || true).is_err());
    let foreign = MemoryBudget::new(2 << 20, 1 << 18).unwrap();
    assert!(
        readmit_runtime_effect_family_ordinary_v1(&source, &roots, &foreign, || false).is_err()
    );
    let other = RootSetV1::empty("other".into(), [4; 32]).unwrap();
    assert!(readmit_runtime_effect_family_ordinary_v1(&source, &other, &memory, || false).is_err());
    let fill = memory
        .reserve(
            (2 << 20) - (1 << 18) - memory.used() - 512,
            MemoryClass::Ordinary,
        )
        .unwrap();
    assert!(matches!(
        readmit_runtime_effect_family_ordinary_v1(&source, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(fill);
    drop(ordinary);
    assert_eq!(memory.used(), before);
}
