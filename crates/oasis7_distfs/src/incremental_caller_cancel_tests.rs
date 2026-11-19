use super::*;
use crate::incremental_page_store::tests::{Fixture, physical_io};
use oasis7_formal_protocol::{
    incremental_schedule::{ScheduleIndexV1, SchedulePageWriter},
    incremental_state::{
        MerkleNodeReader, STATE_MAX_PAGE_BYTES, StateDeltaV1, StateNamespaceV1, StatePageWriter,
        StateRootV1, StateWriteV1,
    },
};
fn same_io(a: &PageIoBudget, b: &PageIoBudget) {
    assert_eq!(
        (
            a.reads_remaining,
            a.read_bytes_remaining,
            a.writes_remaining,
            a.write_bytes_remaining
        ),
        (
            b.reads_remaining,
            b.read_bytes_remaining,
            b.writes_remaining,
            b.write_bytes_remaining
        )
    );
}
fn page(memory: &MemoryBudget, bytes: &[u8]) -> ReservedValue<Vec<u8>> {
    memory
        .reserve(bytes.len() + 128, MemoryClass::Ordinary)
        .unwrap()
        .retain(bytes.to_vec())
}
#[test]
fn caller_value_flag_is_live_for_physical_get_put_and_default_flag_stays_local() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut session = store.value_session_with_cancelled(physical_io(), &cancelled);
    let bytes = b"actual physical page";
    let id = *blake3::hash(bytes).as_bytes();
    session.put_node(id, page(&memory, bytes)).unwrap();
    let before = session.remaining_io().unwrap();
    cancelled.store(true, Ordering::Release);
    assert!(!session.cancelled.load(Ordering::Acquire));
    assert!(matches!(
        session.read_node(&id, 4096, &memory, MemoryClass::Ordinary),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(matches!(
        session.put_node(id, page(&memory, bytes)),
        Err(ValueTreeError::Cancelled)
    ));
    same_io(&before, &session.remaining_io().unwrap());
    let mut local = store.value_session(physical_io());
    assert_eq!(
        local
            .read_node(&id, 4096, &memory, MemoryClass::Ordinary)
            .unwrap()
            .as_slice(),
        bytes
    );
    local.cancelled.store(true, Ordering::Release);
    assert!(matches!(
        local.put_node(id, page(&memory, bytes)),
        Err(ValueTreeError::Cancelled)
    ));
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        session
            .read_node(&id, 4096, &memory, MemoryClass::Ordinary)
            .unwrap()
            .as_slice(),
        bytes
    );
}
#[test]
fn caller_flag_reaches_actual_state_and_schedule_producer_sessions() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let _input = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let state = memory
        .reserve(4 << 20, MemoryClass::Ordinary)
        .unwrap()
        .retain(
            StateDeltaV1::build(
                StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
                vec![StateWriteV1 {
                    key: b"actual-key".to_vec(),
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
        .update("module", None, Some(9), &source, &memory, 256 << 10)
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut writer = store.value_session_with_cancelled(physical_io(), &cancelled);
    let before = writer.remaining_io().unwrap();
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        writer.put_state_pages(&state, &memory),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(matches!(
        writer.put_schedule_pages(&schedule, &memory),
        Err(ValueTreeError::Cancelled)
    ));
    same_io(&before, &writer.remaining_io().unwrap());
    cancelled.store(false, Ordering::Release);
    writer.put_state_pages(&state, &memory).unwrap();
    writer.put_schedule_pages(&schedule, &memory).unwrap();
    let reader = store
        .state_session_with_cancelled(physical_io(), MemoryClass::Recovery, &cancelled)
        .unwrap();
    let before = reader.remaining_io().unwrap();
    cancelled.store(true, Ordering::Release);
    assert!(
        reader
            .read_node(&state.after.node_root, STATE_MAX_PAGE_BYTES)
            .is_err()
    );
    assert!(matches!(
        reader.take_read_error(),
        Some(PageStoreError::Cancelled)
    ));
    same_io(&before, &reader.remaining_io().unwrap());
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        state.after.get(b"actual-key", &reader).unwrap(),
        Some([7; 32])
    );
    assert!(schedule.after.contains(9, "module", &reader).unwrap());
}
