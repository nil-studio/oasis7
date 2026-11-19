use super::*;
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-normal-values-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
        read_bytes_remaining: 16 << 20,
        writes_remaining: 10000,
        write_bytes_remaining: 16 << 20,
    }
}
#[test]
fn distinct_context_handles_share_actual_physical_session_and_cancel() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let stopped = AtomicBool::new(false);
    let pages = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut session = pages.value_session_with_cancelled(quota(), &stopped);
    let before = session.remaining_io().unwrap();
    {
        let owner = HostNormalValuesV1::new(&mut session, &memory).unwrap();
        let reader = owner.handle();
        let mut members = owner.handle();
        let mut events = owner.handle();
        for (writer, bytes) in [
            (&mut members, &b"actual member pages"[..]),
            (&mut events, &b"actual event pages"[..]),
        ] {
            let id = *blake3::hash(bytes).as_bytes();
            let paid = memory
                .reserve(bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(bytes.to_vec());
            writer.put_node(id, paid).unwrap();
            assert_eq!(
                reader
                    .read_node(&id, 4096, &memory, MemoryClass::Ordinary)
                    .unwrap()
                    .as_slice(),
                bytes
            );
        }
        stopped.store(true, Ordering::Release);
        assert!(matches!(
            reader.read_node(&[0; 32], 4096, &memory, MemoryClass::Ordinary),
            Err(ValueTreeError::Cancelled)
        ));
    }
    let after = session.remaining_io().unwrap();
    assert!(after.writes_remaining < before.writes_remaining);
    assert!(after.reads_remaining < before.reads_remaining);
}
#[test]
fn foreign_pool_and_reentry_are_rejected_without_spending_physical_io() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let foreign = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let pages = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut session = pages.value_session(quota());
    let before = session.remaining_io().unwrap();
    {
        let owner = HostNormalValuesV1::new(&mut session, &memory).unwrap();
        let mut writer = owner.handle();
        let bytes = foreign
            .reserve(1, MemoryClass::Ordinary)
            .unwrap()
            .retain(vec![1]);
        assert!(matches!(
            writer.put_node([0; 32], bytes),
            Err(ValueTreeError::Invalid("normal value producer pool"))
        ));
        assert!(matches!(
            writer.read_node(&[0; 32], 4096, &foreign, MemoryClass::Ordinary),
            Err(ValueTreeError::Invalid("normal value IO pool"))
        ));
        let _held = owner.session.borrow_mut();
        assert!(matches!(
            writer.read_node(&[0; 32], 4096, &memory, MemoryClass::Ordinary),
            Err(ValueTreeError::Invalid("normal value IO reentry"))
        ));
        let bytes = memory
            .reserve(1, MemoryClass::Ordinary)
            .unwrap()
            .retain(vec![1]);
        assert!(matches!(
            writer.put_node([0; 32], bytes),
            Err(ValueTreeError::Invalid("normal value IO reentry"))
        ));
    }
    let after = session.remaining_io().unwrap();
    assert_eq!(after.reads_remaining, before.reads_remaining);
    assert_eq!(after.writes_remaining, before.writes_remaining);
    assert_eq!(after.read_bytes_remaining, before.read_bytes_remaining);
    assert_eq!(after.write_bytes_remaining, before.write_bytes_remaining);
}

#[test]
fn fixed_pair_publishes_both_namespaces_and_fails_on_replica_exhaustion() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let stop = AtomicBool::new(false);
    let primary = IncrementalPageStore::create(
        &fixture.0.join("primary"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let replica = IncrementalPageStore::create(
        &fixture.0.join("replica"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut p = primary.value_session_with_cancelled(quota(), &stop);
    let mut r = replica.value_session_with_cancelled(quota(), &stop);
    let bytes = b"actual independently published normal node";
    let id = *blake3::hash(bytes).as_bytes();
    {
        let owner = HostNormalValuesV1::new_pair(&mut p, &mut r, &memory).unwrap();
        owner
            .handle()
            .put_node(
                id,
                memory
                    .reserve(bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(bytes.to_vec()),
            )
            .unwrap();
    }
    for session in [&p, &r] {
        assert_eq!(
            session
                .read_node(&id, 4096, &memory, MemoryClass::Ordinary)
                .unwrap()
                .as_slice(),
            bytes
        );
        assert!(session.remaining_io().unwrap().writes_remaining < quota().writes_remaining);
    }
    let mut exhausted = quota();
    exhausted.writes_remaining = 0;
    let mut r = replica.value_session_with_cancelled(exhausted, &stop);
    let bytes = b"replica must never be silently skipped";
    let owner = HostNormalValuesV1::new_pair(&mut p, &mut r, &memory).unwrap();
    assert!(
        owner
            .handle()
            .put_node(
                *blake3::hash(bytes).as_bytes(),
                memory
                    .reserve(bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(bytes.to_vec())
            )
            .is_err()
    );
}
