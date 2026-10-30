use super::*;
use crate::incremental_page_store::tests::{Fixture, physical_io};
use oasis7_formal_protocol::{
    incremental_schedule::{ScheduleDeltaV1, ScheduleIndexV1, SchedulePageWriter},
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
};

fn deltas(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (ReservedValue<StateDeltaV1>, ReservedValue<ScheduleDeltaV1>) {
    let source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let state = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            vec![StateWriteV1 {
                key: b"alice".to_vec(),
                before: None,
                after: Some([7; 32]),
            }],
            &source,
            4 << 20,
        )
        .unwrap(),
    );
    let schedule = ScheduleIndexV1::empty()
        .unwrap()
        .update("module\0雪", None, Some(9), &source, memory, 256 << 10)
        .unwrap();
    (state, schedule)
}

#[test]
fn producer_state_and_schedule_pages_reopen_with_original_roots() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), compression).unwrap();
        let (state, schedule) = deltas(&store, &memory);
        let expected_state = state.after.clone();
        let expected_schedule = schedule.after.clone();
        let mut writer = store.value_session(physical_io());
        writer.put_state_pages(&state, &memory).unwrap();
        writer.put_schedule_pages(&schedule, &memory).unwrap();
        drop(writer);
        drop(state);
        drop(schedule);
        drop(store);
        drop(memory);

        let read_memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let reopened = IncrementalPageStore::open(
            &fixture.store(),
            read_memory,
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let source = reopened
            .state_session(physical_io(), MemoryClass::Recovery)
            .unwrap();
        assert_eq!(
            expected_state.get(b"alice", &source).unwrap(),
            Some([7; 32])
        );
        assert!(
            expected_schedule
                .contains(9, "module\0雪", &source)
                .unwrap()
        );
        assert!(
            !expected_schedule
                .contains(8, "module\0雪", &source)
                .unwrap()
        );
    }
}

#[test]
fn producer_writers_reject_foreign_pool_and_corrupt_pages_before_io() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (state, schedule) = deltas(&store, &memory);
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut writer = store.value_session(physical_io());
    let before = writer.remaining_io().unwrap();
    for pool in [&memory, &foreign] {
        // With the store's pool the incoming carrier is foreign; with the
        // foreign pool the physical store is foreign. Neither may write.
        let charge = foreign.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let foreign_state = charge.retain((*state).clone());
        assert!(matches!(
            writer.put_state_pages(&foreign_state, pool),
            Err(ValueTreeError::Invalid("state producer pool"))
        ));
        let charge = foreign.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let foreign_schedule = charge.retain((*schedule).clone());
        assert!(matches!(
            writer.put_schedule_pages(&foreign_schedule, pool),
            Err(ValueTreeError::Invalid("schedule producer pool"))
        ));
    }
    let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut bad = (*state).clone();
    bad.new_nodes.last_mut().unwrap().bytes[0] ^= 1;
    let bad = charge.retain(bad);
    assert!(matches!(
        writer.put_state_pages(&bad, &memory),
        Err(ValueTreeError::Invalid("state producer page"))
    ));
    let charge = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut bad = (*schedule).clone();
    bad.new_nodes.last_mut().unwrap().bytes[0] ^= 1;
    let bad = charge.retain(bad);
    assert!(matches!(
        writer.put_schedule_pages(&bad, &memory),
        Err(ValueTreeError::Invalid("schedule producer page"))
    ));
    let after = writer.remaining_io().unwrap();
    assert_eq!(before.writes_remaining, after.writes_remaining);
    assert_eq!(before.write_bytes_remaining, after.write_bytes_remaining);
}

#[test]
fn producer_writers_propagate_cancellation_and_physical_write_limits() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (state, schedule) = deltas(&store, &memory);
    let mut writer = store.value_session(physical_io());
    writer.cancelled.store(true, Ordering::Release);
    assert!(matches!(
        writer.put_state_pages(&state, &memory),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(matches!(
        writer.put_schedule_pages(&schedule, &memory),
        Err(ValueTreeError::Cancelled)
    ));
    let mut limited = physical_io();
    limited.writes_remaining = 0;
    let mut writer = store.value_session(limited);
    assert!(matches!(
        writer.put_state_pages(&state, &memory),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(matches!(
        writer.put_schedule_pages(&schedule, &memory),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}
