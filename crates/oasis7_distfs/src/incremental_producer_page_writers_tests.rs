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

#[test]
fn external_state_batch_cancellation_has_no_success_and_retains_original_roots() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), compression).unwrap();
        let (state, _) = deltas(&store, &memory);
        let mut writer = store.value_session(physical_io());
        let before = writer.remaining_io().unwrap();
        assert!(matches!(
            writer.put_state_pages_cancelled(&state, &memory, &mut || true),
            Err(ValueTreeError::Cancelled)
        ));
        let after = writer.remaining_io().unwrap();
        assert_eq!(before.writes_remaining, after.writes_remaining);
        assert_eq!(before.write_bytes_remaining, after.write_bytes_remaining);

        let mut checkpoints = 0;
        writer
            .put_state_pages_cancelled(&state, &memory, &mut || {
                checkpoints += 1;
                false
            })
            .unwrap();
        let batch_writes =
            before.writes_remaining - writer.remaining_io().unwrap().writes_remaining;
        assert!(batch_writes > 0);
        // The physical counter also charges frame/recipe metadata links.
        // Compare the original single-page path on an independent empty store.
        let reference_fixture = Fixture::new();
        let reference_store =
            IncrementalPageStore::create(&reference_fixture.store(), memory.clone(), compression)
                .unwrap();
        let mut reference = reference_store.value_session(physical_io());
        let reference_before = reference.remaining_io().unwrap();
        for page in &state.new_nodes {
            reference
                .put_node(
                    *blake3::hash(&page.bytes).as_bytes(),
                    memory
                        .reserve(page.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(page.bytes.clone()),
                )
                .unwrap();
        }
        let single_writes =
            reference_before.writes_remaining - reference.remaining_io().unwrap().writes_remaining;
        eprintln!(
            "state batch pages={} physical_writes={batch_writes} single_page_writes={single_writes}",
            state.new_nodes.len()
        );
        // Hard-link publication is also charged as a physical write, so batch
        // counters can be slightly higher. Its saving is shared pack inodes
        // and file/directory barriers, not fewer namespace publication events.
        use std::os::unix::fs::MetadataExt;
        let _inode_charge = memory
            .reserve(state.new_nodes.len() * 4 * 64, MemoryClass::Ordinary)
            .unwrap();
        let unique_inodes = |root: &std::path::Path| {
            let mut ids = std::collections::BTreeSet::new();
            for directory in ["frames", "recipes"] {
                for entry in std::fs::read_dir(root.join(directory)).unwrap() {
                    let metadata = entry.unwrap().metadata().unwrap();
                    ids.insert((metadata.dev(), metadata.ino()));
                }
            }
            ids.len()
        };
        let batch_inodes = unique_inodes(&fixture.store());
        let single_inodes = unique_inodes(&reference_fixture.store());
        eprintln!("state batch unique_inodes={batch_inodes} single_page_inodes={single_inodes}");
        assert!(batch_inodes < single_inodes);
        let reference_state = reference_store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(
            state.after.get(b"alice", &reference_state).unwrap(),
            Some([7; 32])
        );
        assert!(checkpoints > state.new_nodes.len());

        // Use another empty destination so final cancellation follows real new
        // physical publication rather than an all-CAS-hit repeat.
        let cancelled_fixture = Fixture::new();
        let cancelled_store =
            IncrementalPageStore::create(&cancelled_fixture.store(), memory.clone(), compression)
                .unwrap();
        let mut cancelled_writer = cancelled_store.value_session(physical_io());
        let cancelled_before = cancelled_writer.remaining_io().unwrap();
        let mut visited = 0;
        assert!(matches!(
            cancelled_writer.put_state_pages_cancelled(&state, &memory, &mut || {
                visited += 1;
                visited == checkpoints
            }),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(
            visited, checkpoints,
            "final bounded batch cannot hide cancellation"
        );
        assert!(
            cancelled_writer.remaining_io().unwrap().writes_remaining
                < cancelled_before.writes_remaining
        );
        cancelled_writer
            .put_state_pages_cancelled(&state, &memory, &mut || false)
            .unwrap();
        let source = cancelled_store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(state.after.get(b"alice", &source).unwrap(), Some([7; 32]));
    }
}
