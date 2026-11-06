use super::super::tests::{Fixture, charged, physical_io};
use super::*;

#[test]
fn state_read_memo_reuses_real_pages_and_keeps_live_reader_charged() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let bytes = charged(&memory, vec![17; 1024]);
    let id = *blake3::hash(&bytes).as_bytes();
    let name = hex::encode(id);
    let cancelled = AtomicBool::new(false);
    store
        .put_page(id, &bytes, &mut physical_io(), &cancelled)
        .unwrap();
    drop(bytes);
    let baseline = memory.used();
    let session = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap()
        .with_read_memo(2, 16 << 10)
        .unwrap();
    let first = session.read_node(&name, 1024).unwrap().unwrap();
    let after_read = session.remaining_io().unwrap();
    let held = first.0.reserved_bytes();
    let second = session.read_node(&name, 1024).unwrap().unwrap();
    assert_eq!(second.as_ref(), &[17; 1024]);
    let after_hit = session.remaining_io().unwrap();
    assert_eq!(after_hit.reads_remaining, after_read.reads_remaining);
    assert_eq!(
        after_hit.read_bytes_remaining,
        after_read.read_bytes_remaining
    );
    let stats = session.read_memo_stats().unwrap().unwrap();
    assert_eq!((stats.entries, stats.hits, stats.misses), (1, 1, 1));
    drop(second);
    drop(session);
    assert_eq!(memory.used(), baseline + held);
    drop(first);
    assert_eq!(memory.used(), baseline);
}

#[test]
fn state_read_memo_preserves_bounds_cancellation_and_finite_capacity() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let stop = AtomicBool::new(false);
    let mut names = vec![];
    for value in [17, 29] {
        let bytes = charged(&memory, vec![value; 1024]);
        let id = *blake3::hash(&bytes).as_bytes();
        store
            .put_page(id, &bytes, &mut physical_io(), &stop)
            .unwrap();
        names.push(hex::encode(id));
    }
    let session = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap()
        .with_read_memo(1, 16 << 10)
        .unwrap();
    drop(session.read_node(&names[0], 1024).unwrap());
    for _ in 0..2 {
        drop(session.read_node(&names[1], 1024).unwrap());
    }
    assert_eq!(session.read_memo_stats().unwrap().unwrap().entries, 1);
    assert!(
        session.read_node(&names[0], 1).is_err(),
        "cached bytes cannot bypass a tighter request"
    );
    assert!(session.take_read_error().is_some());
    session.cancelled.store(true, Ordering::Release);
    assert!(session.read_node(&names[0], 1024).is_err());
    assert!(matches!(
        session.take_read_error(),
        Some(PageStoreError::Cancelled)
    ));
    assert!(
        store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .unwrap()
            .with_read_memo(usize::MAX, 16 << 10)
            .is_err()
    );
}
