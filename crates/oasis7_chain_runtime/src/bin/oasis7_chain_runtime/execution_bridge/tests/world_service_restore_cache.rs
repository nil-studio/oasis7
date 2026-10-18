use super::*;
use std::cell::Cell;

fn input() -> (Snapshot, Journal) {
    let world = oasis7::runtime_host::world::new();
    (world.snapshot(), world.journal().clone())
}

#[test]
fn successful_restore_reuse_skips_repeated_restore_and_isolates_consumer_mutation() {
    let cache = Mutex::new(RestoreCache::default());
    let (snapshot, journal) = input();
    let key = Key::from_bytes(
        &serde_cbor::to_vec(&snapshot).unwrap(),
        &serde_cbor::to_vec(&journal).unwrap(),
    );
    let calls = Cell::new(0);
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let baseline = restore_with(&cache, key.clone(), run).unwrap();
    let mut returned = restore_with(&cache, key.clone(), run).unwrap();
    returned.submit_action(oasis7_runtime::Action::RegisterAgent {
        agent_id: "caller-private-mutation".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
    });
    returned.step().unwrap();
    let next = restore_with(&cache, key, run).unwrap();
    assert_eq!(
        calls.get(),
        1,
        "warm hit must avoid the actual restore closure"
    );
    assert!(next.snapshot() == baseline.snapshot());
    assert!(next.journal() == baseline.journal());
    assert!(!next.state().agents.contains_key("caller-private-mutation"));
    assert_eq!(next.module_cache_len(), 0);
}

#[test]
fn complete_journal_key_failures_and_oversized_entries_do_not_reuse_restore() {
    let cache = Mutex::new(RestoreCache::default());
    let calls = Cell::new(0);
    let (snapshot, journal) = input();
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let first = Key::from_bytes(b"same-snapshot", b"full-journal-a");
    let different_journal = Key::from_bytes(b"same-snapshot", b"full-journal-b");
    restore_with(&cache, first, run).unwrap();
    restore_with(&cache, different_journal, run).unwrap();
    assert_eq!(
        calls.get(),
        2,
        "complete journal material participates in identity"
    );
    let failed = Key::from_bytes(b"invalid", b"journal");
    for _ in 0..2 {
        assert!(
            restore_with(&cache, failed.clone(), || {
                calls.set(calls.get() + 1);
                Err(oasis7_runtime::WorldError::JournalMismatch)
            })
            .is_err()
        );
    }
    assert_eq!(calls.get(), 4, "failures cannot create a warm entry");
    let (mut invalid_snapshot, invalid_journal) = input();
    invalid_snapshot.journal_len = 1;
    let invalid_bytes = serde_cbor::to_vec(&invalid_snapshot).unwrap();
    let journal_bytes = serde_cbor::to_vec(&invalid_journal).unwrap();
    for _ in 0..2 {
        assert!(
            restore(
                &cache,
                &invalid_bytes,
                &journal_bytes,
                invalid_snapshot.clone(),
                invalid_journal.clone()
            )
            .is_err()
        );
    }
    assert!(
        !cache
            .lock()
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.key == Key::from_bytes(&invalid_bytes, &journal_bytes))
    );

    let oversized = Key {
        snapshot_len: MAX_ENTRY_INPUT_BYTES + 1,
        ..Key::from_bytes(b"large", b"journal")
    };
    for _ in 0..2 {
        restore_with(&cache, oversized.clone(), run).unwrap();
    }
    assert_eq!(
        calls.get(),
        6,
        "oversized inputs retain original validation behavior"
    );
    let retained = cache.lock().unwrap();
    assert_eq!(retained.entries.len(), 2);
    assert!(retained.retained_input_bytes <= MAX_RETAINED_INPUT_BYTES);
}

#[test]
fn fifo_capacity_and_poison_fallback_preserve_actual_restore() {
    let cache = Mutex::new(RestoreCache::default());
    let calls = Cell::new(0);
    let (snapshot, journal) = input();
    let run = || {
        calls.set(calls.get() + 1);
        oasis7::runtime_host::world::from_snapshot(snapshot.clone(), journal.clone())
    };
    let first = Key::from_bytes(b"first", b"journal");
    restore_with(&cache, first.clone(), run).unwrap();
    for value in [b"second".as_slice(), b"third".as_slice()] {
        restore_with(&cache, Key::from_bytes(value, b"journal"), run).unwrap();
    }
    restore_with(&cache, first, run).unwrap();
    assert_eq!(
        calls.get(),
        4,
        "evicted entry must execute the restore again"
    );
    assert_eq!(cache.lock().unwrap().entries.len(), MAX_ENTRIES);
    let _ = std::panic::catch_unwind(|| {
        let _guard = cache.lock().unwrap();
        panic!("test poison");
    });
    let key = Key::from_bytes(b"after-poison", b"journal");
    restore_with(&cache, key.clone(), run).unwrap();
    restore_with(&cache, key, run).unwrap();
    assert_eq!(
        calls.get(),
        6,
        "poisoned cache must fall back to original restore"
    );
}

#[test]
fn same_complete_journal_with_different_snapshot_bytes_restores_distinct_worlds() {
    let cache = Mutex::new(RestoreCache::default());
    let (first, journal) = input();
    let other_world = oasis7::runtime_host::world::new_with_state(oasis7_runtime::WorldState {
        time: 7,
        ..oasis7_runtime::WorldState::default()
    });
    let second = other_world.snapshot();
    assert!(other_world.journal() == &journal);
    let first_bytes = serde_cbor::to_vec(&first).unwrap();
    let second_bytes = serde_cbor::to_vec(&second).unwrap();
    let journal_bytes = serde_cbor::to_vec(&journal).unwrap();
    assert_eq!(
        restore(
            &cache,
            &first_bytes,
            &journal_bytes,
            first.clone(),
            journal.clone()
        )
        .unwrap()
        .state()
        .time,
        0
    );
    assert_eq!(
        restore(
            &cache,
            &second_bytes,
            &journal_bytes,
            second,
            journal.clone()
        )
        .unwrap()
        .state()
        .time,
        7
    );
    assert_eq!(
        restore(&cache, &first_bytes, &journal_bytes, first, journal)
            .unwrap()
            .state()
            .time,
        0
    );
    assert_eq!(cache.lock().unwrap().entries.len(), 2);
}
