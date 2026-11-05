use super::*;
use oasis7_distfs::incremental_page_store::PageCompression;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "oasis7-buffered-targets-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn quota() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 10000,
        read_bytes_remaining: 32 << 20,
        writes_remaining: 10000,
        write_bytes_remaining: 32 << 20,
    }
}
#[test]
fn buffered_replica_transfer_requires_finish_and_preserves_store_failure() {
    let fixture = Fixture::new();
    let cancelled = AtomicBool::new(false);
    let stores: [IncrementalPageStore; 2] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture.0.join(format!("finish-{i}")),
            MemoryBudget::new(16 << 20, 0).unwrap(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let mut quotas = std::array::from_fn(|_| quota());
    quotas[1].writes_remaining = 0;
    let mut first = || false;
    let mut second = || false;
    let targets = Targets::new(
        [&stores[0], &stores[1]],
        &mut quotas,
        &cancelled,
        [
            &mut first as &mut dyn FnMut() -> bool,
            &mut second as &mut dyn FnMut() -> bool,
        ],
    )
    .unwrap();
    let bytes = b"actual queued page, not a durable completion";
    let id = *blake3::hash(bytes).as_bytes();
    targets.send(id, bytes).unwrap();
    targets.check_healthy().unwrap();
    assert!(targets.check_complete().is_err());
    assert!(matches!(
        targets.finish(),
        Err(
            oasis7_distfs::incremental_page_store::PageStoreError::ResourceLimited(
                "physical write allowance"
            )
        )
    ));
    assert!(targets.check_complete().is_err());
    assert!(targets.check_healthy().is_err());
    let actual = stores[0]
        .read_page(
            &id,
            bytes.len(),
            MemoryClass::Ordinary,
            &mut quota(),
            &cancelled,
        )
        .unwrap();
    assert_eq!(actual.as_slice(), bytes);
}
