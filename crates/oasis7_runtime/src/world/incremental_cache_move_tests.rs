use super::*;
#[test]
fn actual_move_and_drop_release_the_same_pool() {
    let memory = MemoryBudget::new(65536, 0).unwrap();
    let mut slot = RuntimeCacheSlotV1::default();
    let prepared = slot
        .prepare_empty_bounded(17u64, &memory, &mut || false)
        .unwrap();
    assert!(memory.used() > 0);
    assert!(prepared.finish().is_none());
    assert_eq!(slot.current(), Some(&17));
    assert_eq!(slot.generation(), 1);
    assert!(matches!(
        slot.prepare_empty_bounded(18, &memory, &mut || false),
        Err(ExecutionError::ParentConflict)
    ));
    drop(slot);
    assert_eq!(memory.used(), 0);
}
#[test]
fn cancellation_admission_and_dropped_prepare_never_change_slot() {
    let memory = MemoryBudget::new(65536, 0).unwrap();
    let mut slot = RuntimeCacheSlotV1::default();
    assert!(
        slot.prepare_empty_bounded(1u64, &memory, &mut || true)
            .is_err()
    );
    let mut calls = 0;
    assert!(
        slot.prepare_empty_bounded(2, &memory, &mut || {
            calls += 1;
            calls == 2
        })
        .is_err()
    );
    drop(
        slot.prepare_empty_bounded(3, &memory, &mut || false)
            .unwrap(),
    );
    assert!(slot.is_empty());
    assert_eq!(slot.generation(), 0);
    assert_eq!(memory.used(), 0);
    let zero = MemoryBudget::new(0, 0).unwrap();
    assert!(slot.prepare_empty_bounded(4, &zero, &mut || false).is_err());
    slot.generation = u64::MAX;
    assert!(matches!(
        slot.prepare_empty_bounded(5, &memory, &mut || false),
        Err(ExecutionError::ResourceLimited("cache generation"))
    ));
    assert!(slot.is_empty());
    assert_eq!(memory.used(), 0);
}
