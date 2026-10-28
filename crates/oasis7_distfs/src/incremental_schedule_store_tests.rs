use super::super::tests::{Fixture, logical_io, physical_io};
use super::*;
use oasis7_formal_protocol::{
    incremental_schedule::{ScheduleIndexV1, ScheduleScanBudget, ScheduleScanContext},
    incremental_value::{ValueReadContext, write_value},
};

fn scan(source: &StatePageSession<'_>, root: &ScheduleIndexV1, now: u64) -> Vec<(u64, String)> {
    let mut result = Vec::new();
    let mut io = ScheduleScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 100_000_000,
        buckets_remaining: 100,
        instances_remaining: 100,
    };
    ScheduleScanContext {
        source,
        memory: &source.store.memory,
        class: source.class,
        io: &mut io,
    }
    .visit_due(
        root,
        now,
        |wake, id| {
            result.push((wake, id.to_owned()));
            Ok::<_, ()>(())
        },
        || false,
    )
    .unwrap();
    result
}

#[test]
fn schedule_pages_and_descriptor_reopen_and_continue_identically_in_raw_and_zstd() {
    for (compression, total_mib) in [(PageCompression::Raw, 16), (PageCompression::Zstd, 32)] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(total_mib << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), compression).unwrap();
        let mut root = ScheduleIndexV1::empty().unwrap();
        let mut old = root.clone();
        for (id, old_wake, new_wake) in [
            ("a", None, Some(5)),
            ("b", None, Some(5)),
            ("c", None, Some(u64::MAX)),
            ("a", Some(5), Some(13)),
        ] {
            // Each explicit update/verification is a separate fixture phase;
            // its finite physical allowance is not reset during the operation.
            let mut source = store
                .state_session(physical_io(), MemoryClass::Ordinary)
                .unwrap();
            let delta = root
                .update(id, old_wake, new_wake, &source, &memory, 256 * 1024)
                .unwrap();
            delta
                .verify(root.digest().unwrap(), &source, &memory, 256 * 1024)
                .unwrap();
            source.persist_schedule_pages(&delta).unwrap();
            root = delta.after.clone();
            if id == "a" && old_wake.is_none() {
                old = root.clone();
            }
        }
        let descriptor = root.canonical_bytes().unwrap();
        let descriptor_id = write_value(
            &mut descriptor.as_slice(),
            descriptor.len() as u64,
            &mut store.value_session(physical_io()),
            &memory,
            &mut logical_io(),
            || false,
        )
        .unwrap()
        .digest()
        .unwrap();
        let mut source = store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap();
        let uninterrupted = root
            .update("b", Some(5), None, &source, &memory, 256 * 1024)
            .unwrap();
        assert_eq!(scan(&source, &root, 5), vec![(5, "b".into())]);
        // A different pool cannot launder an otherwise valid producer result.
        let foreign = MemoryBudget::new(4 << 20, 0).unwrap();
        let charge = foreign.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let alien = charge.retain((*uninterrupted).clone());
        assert!(matches!(
            source.persist_schedule_pages(&alien),
            Err(PageStoreError::Corrupt("schedule producer pool"))
        ));
        drop(alien);
        assert_eq!(foreign.used(), 0);
        drop(source);
        drop(store);
        let store = IncrementalPageStore::open(
            &fixture.store(),
            memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let source = store
            .state_session(physical_io(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical_io());
        let mut io = logical_io();
        let mut read = ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut io,
        };
        let value = read.load_root(&descriptor_id).unwrap();
        let _output_charge = memory.reserve(128, MemoryClass::Recovery).unwrap();
        let mut output = Vec::with_capacity(128);
        read.read_range(&value, 0, value.0.logical_len, &mut output, || false)
            .unwrap();
        let recovered = ScheduleIndexV1::from_canonical_bytes(&output).unwrap();
        assert_eq!(recovered, root);
        assert_eq!(scan(&source, &recovered, 5), vec![(5, "b".into())]);
        assert_eq!(scan(&source, &old, u64::MAX), vec![(5, "a".into())]);
        drop(source);
        let mut source = store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap();
        let continued = recovered
            .update("b", Some(5), None, &source, &memory, 256 * 1024)
            .unwrap();
        assert_eq!(*continued, *uninterrupted);
        source.persist_schedule_pages(&continued).unwrap();
        assert_eq!(
            scan(&source, &continued.after, u64::MAX),
            vec![(13, "a".into()), (u64::MAX, "c".into())]
        );
        // Missing actual page recipes are a typed storage failure, never an
        // empty schedule or an accepted recovered world.
        fs::remove_file(
            fixture
                .store()
                .join("recipes")
                .join(&continued.after.node_root),
        )
        .unwrap();
        let mut io = ScheduleScanBudget {
            nodes_remaining: 1000,
            read_bytes_remaining: 1 << 20,
            buckets_remaining: 10,
            instances_remaining: 10,
        };
        let result = ScheduleScanContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        }
        .visit_due::<()>(&continued.after, u64::MAX, |_, _| Ok(()), || false);
        assert!(result.is_err());
        assert!(matches!(
            source.take_read_error(),
            Some(PageStoreError::Missing(_))
        ));
    }
}
