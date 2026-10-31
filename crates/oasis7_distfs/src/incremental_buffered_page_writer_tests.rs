use super::*;
use crate::incremental_page_store::tests::{Fixture, physical_io};

fn charged(memory: &MemoryBudget, data: &[u8]) -> ReservedValue<Vec<u8>> {
    memory
        .reserve(data.len(), MemoryClass::Ordinary)
        .unwrap()
        .retain(data.to_vec())
}

#[test]
fn buffered_transfer_finishes_exact_pages_and_drop_never_flushes() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), compression).unwrap();
        let baseline = memory.used();
        let cancelled = AtomicBool::new(false);
        let data = b"exact original page";
        let id = *blake3::hash(data).as_bytes();
        let mut io = physical_io();
        {
            let mut check = || false;
            let mut writer = store
                .buffered_page_writer(&mut io, &cancelled, &mut check)
                .unwrap();
            writer.put_node(id, charged(&memory, data)).unwrap();
            assert_eq!(writer.pending_reserved_bytes(), data.len());
        }
        assert_eq!(
            std::fs::read_dir(store.root().join("recipes"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(memory.used(), baseline);
        let mut check = || false;
        let mut writer = store
            .buffered_page_writer(&mut io, &cancelled, &mut check)
            .unwrap();
        writer.put_node(id, charged(&memory, data)).unwrap();
        writer.finish().unwrap();
        assert_eq!(memory.used(), baseline);
        let read = store
            .read_page(
                &id,
                data.len(),
                MemoryClass::Recovery,
                &mut physical_io(),
                &cancelled,
            )
            .unwrap();
        assert_eq!(read.as_slice(), data);
    }
}

#[test]
fn buffered_transfer_observes_cancel_after_real_final_publication() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), compression).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut io = physical_io();
        let before = io.writes_remaining;
        let data = b"new physical page before cancellation";
        let id = *blake3::hash(data).as_bytes();
        // This event is observed only after a recipe is physically published;
        // it does not predict the implementation's callback count.
        let mut check = || {
            std::fs::read_dir(store.root().join("recipes"))
                .unwrap()
                .next()
                .is_some()
        };
        let mut writer = store
            .buffered_page_writer(&mut io, &cancelled, &mut check)
            .unwrap();
        writer.put_node(id, charged(&memory, data)).unwrap();
        assert!(matches!(writer.finish(), Err(PageStoreError::Cancelled)));
        assert!(io.writes_remaining < before);
        let read = store
            .read_page(
                &id,
                data.len(),
                MemoryClass::Recovery,
                &mut physical_io(),
                &cancelled,
            )
            .unwrap();
        assert_eq!(
            read.as_slice(),
            data,
            "physical prefix is present but was never acknowledged complete"
        );
    }
}

#[test]
fn failed_or_unwound_buffered_transfer_cannot_finish_its_prior_prefix() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 2 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let baseline = memory.used();
    let cancelled = AtomicBool::new(false);
    let mut io = physical_io();
    let mut check = || false;
    let mut writer = store
        .buffered_page_writer(&mut io, &cancelled, &mut check)
        .unwrap();
    writer
        .put_node(*blake3::hash(b"good").as_bytes(), charged(&memory, b"good"))
        .unwrap();
    let bad = memory
        .reserve(0, MemoryClass::Ordinary)
        .unwrap()
        .retain(vec![8]);
    assert!(matches!(
        writer.put_node(*blake3::hash(&bad).as_bytes(), bad),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        writer.finish(),
        Err(PageStoreError::Corrupt("buffered page writer aborted"))
    ));
    assert_eq!(memory.used(), baseline);
    assert_eq!(
        std::fs::read_dir(store.root().join("recipes"))
            .unwrap()
            .count(),
        0
    );

    let panic_now = std::cell::Cell::new(false);
    let mut check = || {
        assert!(!panic_now.get(), "injected cancellation callback unwind");
        false
    };
    let mut writer = store
        .buffered_page_writer(&mut io, &cancelled, &mut check)
        .unwrap();
    writer
        .put_node(*blake3::hash(b"good").as_bytes(), charged(&memory, b"good"))
        .unwrap();
    panic_now.set(true);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            writer.put_node(*blake3::hash(b"next").as_bytes(), charged(&memory, b"next"))
        }))
        .is_err()
    );
    assert!(matches!(
        writer.finish(),
        Err(PageStoreError::Corrupt("buffered page writer aborted"))
    ));
    assert_eq!(memory.used(), baseline);
    assert_eq!(
        std::fs::read_dir(store.root().join("recipes"))
            .unwrap()
            .count(),
        0
    );
}
