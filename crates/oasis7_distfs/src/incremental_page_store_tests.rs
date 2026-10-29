use super::*;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext, write_value};
use std::sync::atomic::AtomicU64;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        loop {
            let root = std::env::temp_dir().join(format!(
                "oasis7-iws-pages-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Self(root),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
    pub(super) fn store(&self) -> PathBuf {
        self.0.join("store")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
pub(super) fn physical_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100000,
        read_bytes_remaining: 1 << 30,
        writes_remaining: 100000,
        write_bytes_remaining: 1 << 30,
    }
}
pub(super) fn logical_io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    }
}
pub(super) fn charged(memory: &MemoryBudget, bytes: Vec<u8>) -> ReservedValue<Vec<u8>> {
    memory
        .reserve(bytes.capacity(), MemoryClass::Ordinary)
        .unwrap()
        .retain(bytes)
}
fn counts(root: &Path) -> (usize, usize) {
    (
        fs::read_dir(root.join("recipes")).unwrap().count(),
        fs::read_dir(root.join("frames")).unwrap().count(),
    )
}

#[test]
fn real_store_reopens_exact_pages_without_duplicate_payload_or_authority() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
    let cancelled = AtomicBool::new(false);
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let path = fixture.0.join(match compression {
            PageCompression::Raw => "raw",
            PageCompression::Zstd => "zstd",
        });
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let mut io = physical_io();
        for length in [1, 4096, 65536, VALUE_MAX_NODE_BYTES] {
            let bytes = charged(&memory, vec![17; length]);
            let id = *blake3::hash(&bytes).as_bytes();
            store.put_page(id, &bytes, &mut io, &cancelled).unwrap();
            let before = counts(&path);
            store.put_page(id, &bytes, &mut io, &cancelled).unwrap();
            assert_eq!(counts(&path), before);
            let reopened = IncrementalPageStore::open(
                &path,
                memory.clone(),
                PageCompression::Raw,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let read = reopened
                .read_page(&id, length, MemoryClass::Recovery, &mut io, &cancelled)
                .unwrap();
            assert_eq!(read.as_slice(), bytes.as_slice());
            assert!(read.uses_budget(&memory));
            assert!(matches!(
                reopened.read_page(&id, length - 1, MemoryClass::Recovery, &mut io, &cancelled),
                Err(PageStoreError::ResourceLimited(_))
            ));
        }
        assert_eq!(fs::read_dir(path.join("staging")).unwrap().count(), 0);
        assert!(!path.join("head").exists());
    }
    assert_eq!(memory.used(), 0);
}

#[test]
fn value_tree_uses_real_durable_page_adapter_and_late_range_avoids_prefix_scan() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut session = store.value_session(physical_io());
    // Input is external caller-owned memory, explicitly admitted to the same
    // pool. The representation contains multiple original logical chunks.
    let bytes = charged(
        &memory,
        (0..(3 * 65536 + 17)).map(|n| (n % 251) as u8).collect(),
    );
    let mut input = bytes.as_slice();
    let root = write_value(
        &mut input,
        bytes.len() as u64,
        &mut session,
        &memory,
        &mut logical_io(),
        || false,
    )
    .unwrap();
    let root_id = root.digest().unwrap();
    drop(session);
    drop(store);
    let reopened = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let session = reopened.value_session(physical_io());
    let before = session.remaining_io().unwrap();
    let mut io = logical_io();
    let mut context = ValueReadContext {
        source: &session,
        memory: &memory,
        class: MemoryClass::Recovery,
        io: &mut io,
    };
    assert_eq!(context.load_root(&root_id).unwrap(), root);
    let mut output = [0u8; 12];
    let mut target = output.as_mut_slice();
    context
        .read_range(&root, 3 * 65536 - 6, 12, &mut target, || false)
        .unwrap();
    assert_eq!(&output, &bytes[3 * 65536 - 6..3 * 65536 + 6]);
    let after = session.remaining_io().unwrap();
    // Four logical nodes, each with a format read, one recipe and at most 17 CDC frames.
    // The byte bound additionally rejects reading the two earlier data pages.
    assert!(before.reads_remaining - after.reads_remaining <= 4 * 19);
    assert!(before.read_bytes_remaining - after.read_bytes_remaining < 2 * 65536 + 8192);
    assert!(
        session
            .read_node(
                &root_id,
                1024,
                &MemoryBudget::new(8 << 20, 0).unwrap(),
                MemoryClass::Ordinary
            )
            .is_err()
    );
}

#[test]
fn open_never_creates_or_repairs_missing_unknown_or_unsafe_stores() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    assert!(
        IncrementalPageStore::open(
            &fixture.store(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    assert!(!fixture.store().exists());
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    assert!(
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .is_err()
    );
    drop(store);
    fs::write(fixture.store().join("format"), b"unknown-version").unwrap();
    assert!(
        IncrementalPageStore::open(
            &fixture.store(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    fs::write(fixture.store().join("format"), FORMAT).unwrap();
    fs::remove_file(fixture.store().join("lease")).unwrap();
    assert!(
        IncrementalPageStore::open(
            &fixture.store(),
            memory,
            PageCompression::Raw,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    assert!(!fixture.store().join("lease").exists());
}

#[test]
fn corruption_missing_pages_and_replaced_live_lease_fail_closed() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut io = physical_io();
    assert!(matches!(
        store.read_page(&[1; 32], 1024, MemoryClass::Ordinary, &mut io, &cancelled),
        Err(PageStoreError::Missing(_))
    ));
    let bytes = charged(&memory, vec![19; 65536]);
    let id = *blake3::hash(&bytes).as_bytes();
    store.put_page(id, &bytes, &mut io, &cancelled).unwrap();
    let frame = fs::read_dir(fixture.store().join("frames"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let original = fs::read(&frame).unwrap();
    fs::write(&frame, b"corrupt").unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut io,
            &cancelled
        ),
        Err(PageStoreError::Corrupt(_))
    ));
    assert!(store.put_page(id, &bytes, &mut io, &cancelled).is_err());
    fs::write(&frame, &original).unwrap();
    let recipe = fixture.store().join("recipes").join(hex::encode(id));
    let original_recipe = fs::read(&recipe).unwrap();
    fs::write(&recipe, vec![0; recipe::MAX_RECIPE_BYTES + 1]).unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut io,
            &cancelled
        ),
        Err(PageStoreError::Corrupt(_))
    ));
    fs::write(&recipe, original_recipe).unwrap();
    let guard = File::open(fixture.store().join("lease")).unwrap();
    assert!(guard.try_lock().is_err());
    fs::rename(
        fixture.store().join("lease"),
        fixture.store().join("lease-old"),
    )
    .unwrap();
    File::create(fixture.store().join("lease")).unwrap();
    assert!(
        store
            .read_page(
                &id,
                VALUE_MAX_NODE_BYTES,
                MemoryClass::Ordinary,
                &mut io,
                &cancelled
            )
            .is_err()
    );
}

#[test]
fn memory_io_cancellation_and_live_reader_charges_are_enforced() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let bytes = charged(&memory, vec![21; 65536]);
    let id = *blake3::hash(&bytes).as_bytes();
    let cancelled = AtomicBool::new(true);
    let mut io = physical_io();
    assert!(matches!(
        store.put_page(id, &bytes, &mut io, &cancelled),
        Err(PageStoreError::Cancelled)
    ));
    assert_eq!(counts(&fixture.store()), (0, 0));
    cancelled.store(false, Ordering::Release);
    let mut none = physical_io();
    none.writes_remaining = 0;
    assert!(matches!(
        store.put_page(id, &bytes, &mut none, &cancelled),
        Err(PageStoreError::ResourceLimited(_))
    ));
    assert_eq!(counts(&fixture.store()), (0, 0));
    store.put_page(id, &bytes, &mut io, &cancelled).unwrap();
    let baseline = memory.used();
    let read = store
        .read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut io,
            &cancelled,
        )
        .unwrap();
    let clone = read.clone();
    let retained = memory.used();
    assert!(retained >= baseline + bytes.len());
    drop(read);
    assert_eq!(memory.used(), retained);
    drop(clone);
    assert_eq!(memory.used(), baseline);
    let ordinary = memory
        .reserve((5 << 20) - baseline, MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut io,
            &cancelled
        ),
        Err(PageStoreError::Memory(_))
    ));
    assert!(matches!(
        IncrementalPageStore::open(
            &fixture.store(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary
        ),
        Err(PageStoreError::Memory(_))
    ));
    let recovery_store = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    assert!(
        recovery_store
            .read_page(
                &id,
                VALUE_MAX_NODE_BYTES,
                MemoryClass::Recovery,
                &mut io,
                &cancelled
            )
            .is_ok()
    );
    drop(recovery_store);
    assert!(
        store
            .read_page(
                &id,
                VALUE_MAX_NODE_BYTES,
                MemoryClass::Recovery,
                &mut io,
                &cancelled
            )
            .is_ok()
    );
    drop(ordinary);
    let recipe_bytes = fs::read(fixture.store().join("recipes").join(hex::encode(id))).unwrap();
    let (_, frames) = recipe::decode(&id, &recipe_bytes).unwrap();
    let mut excludes_format = physical_io();
    let unique_frames: std::collections::BTreeSet<_> = frames
        .iter()
        .map(|frame| (frame.stored, frame.original, frame.length, frame.compressed))
        .collect();
    excludes_format.reads_remaining = unique_frames.len() as u64 + 1;
    assert!(
        matches!(
            store.read_page(
                &id,
                VALUE_MAX_NODE_BYTES,
                MemoryClass::Ordinary,
                &mut excludes_format,
                &cancelled
            ),
            Err(PageStoreError::ResourceLimited(_))
        ),
        "recipe/frame allowance alone must not cover the additional format read"
    );
    none = physical_io();
    none.read_bytes_remaining = 0;
    assert!(matches!(
        store.read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut none,
            &cancelled
        ),
        Err(PageStoreError::ResourceLimited(_))
    ));
}

#[test]
fn incomplete_frame_only_publication_is_not_readable_and_retry_reuses_original_frame() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let bytes = charged(&memory, vec![31; 100]);
    let id = *blake3::hash(&bytes).as_bytes();
    let cancelled = AtomicBool::new(false);
    let mut limited = physical_io();
    limited.writes_remaining = 1;
    assert!(matches!(
        store.put_page(id, &bytes, &mut limited, &cancelled),
        Err(PageStoreError::ResourceLimited(_))
    ));
    assert_eq!(counts(&fixture.store()), (0, 1));
    assert!(matches!(
        store.read_page(
            &id,
            100,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &cancelled
        ),
        Err(PageStoreError::Missing(_))
    ));
    drop(store);
    let reopened = IncrementalPageStore::open(
        &fixture.store(),
        memory,
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let mut retry = physical_io();
    reopened
        .put_page(id, &bytes, &mut retry, &cancelled)
        .unwrap();
    assert_eq!(counts(&fixture.store()), (1, 1));
    assert_eq!(physical_io().writes_remaining - retry.writes_remaining, 1);
}

#[test]
fn concurrent_raw_and_zstd_publication_keep_one_original_logical_identity() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(48 << 20, 4 << 20).unwrap();
    let raw = IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
        .unwrap();
    let zstd = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Zstd,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let bytes = charged(&memory, vec![41; VALUE_MAX_NODE_BYTES]);
    let id = *blake3::hash(&bytes).as_bytes();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            raw.put_page(id, &bytes, &mut physical_io(), &AtomicBool::new(false))
        });
        let b = scope.spawn(|| {
            barrier.wait();
            zstd.put_page(id, &bytes, &mut physical_io(), &AtomicBool::new(false))
        });
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
    });
    assert_eq!(counts(&fixture.store()).0, 1);
    assert_eq!(
        raw.read_page(
            &id,
            VALUE_MAX_NODE_BYTES,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &AtomicBool::new(false)
        )
        .unwrap()
        .as_slice(),
        bytes.as_slice()
    );
}

#[test]
#[cfg(unix)]
fn linked_or_writable_controlled_paths_are_rejected_before_read() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let bytes = charged(&memory, vec![43; 100]);
    let id = *blake3::hash(&bytes).as_bytes();
    let cancelled = AtomicBool::new(false);
    store
        .put_page(id, &bytes, &mut physical_io(), &cancelled)
        .unwrap();
    let frames = fixture.store().join("frames");
    let saved = fixture.0.join("saved-frames");
    fs::rename(&frames, &saved).unwrap();
    symlink(&saved, &frames).unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            100,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &cancelled
        ),
        Err(PageStoreError::Corrupt(_))
    ));
    fs::remove_file(&frames).unwrap();
    fs::rename(&saved, &frames).unwrap();
    fs::set_permissions(&frames, fs::Permissions::from_mode(0o722)).unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            100,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &cancelled
        ),
        Err(PageStoreError::Corrupt(_))
    ));
    fs::set_permissions(&frames, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[ignore = "exact worker for process-kill publication test"]
fn publication_crash_worker() {
    let path = PathBuf::from(std::env::var_os("OASIS7_IWS_PAGE_CRASH_ROOT").expect("worker root"));
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let bytes = charged(&memory, vec![47; 100]);
    let id = *blake3::hash(&bytes).as_bytes();
    store
        .put_page(id, &bytes, &mut physical_io(), &AtomicBool::new(false))
        .unwrap();
    panic!("worker did not reach publication barrier");
}

#[test]
fn killed_publisher_reopens_and_retries_the_same_page_at_each_filesystem_boundary() {
    use std::{process::Command, time::Duration};
    for (kind, step) in [
        ("frames", 1),
        ("frames", 2),
        ("frames", 3),
        ("recipes", 1),
        ("recipes", 2),
        ("recipes", 3),
    ] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        drop(
            IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
                .unwrap(),
        );
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "incremental_page_store::tests::publication_crash_worker",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OASIS7_IWS_PAGE_CRASH_ROOT", fixture.store())
            .env("OASIS7_IWS_PAGE_CRASH_POINT", format!("{kind}-{step}"));
        assert!(
            !crate::test_process::kill_at_marker(
                &mut command,
                &fixture.0.join("paused"),
                Duration::from_secs(15)
            )
            .unwrap()
            .success()
        );
        let store = IncrementalPageStore::open(
            &fixture.store(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let bytes = charged(&memory, vec![47; 100]);
        let id = *blake3::hash(&bytes).as_bytes();
        let read = store.read_page(
            &id,
            100,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &AtomicBool::new(false),
        );
        if kind == "recipes" && step >= 2 {
            assert_eq!(read.unwrap().as_slice(), bytes.as_slice());
        } else {
            assert!(matches!(read, Err(PageStoreError::Missing(_))));
        }
        files::take_sync_counts();
        store
            .put_page(id, &bytes, &mut physical_io(), &AtomicBool::new(false))
            .unwrap();
        let synced = files::take_sync_counts();
        assert!(
            synced.0 > 0 && synced.1 > 0,
            "retry must synchronize the selected frames and recipe, even when the recipe was already visible"
        );
        assert_eq!(
            store
                .read_page(
                    &id,
                    100,
                    MemoryClass::Ordinary,
                    &mut physical_io(),
                    &AtomicBool::new(false)
                )
                .unwrap()
                .as_slice(),
            bytes.as_slice()
        );
        assert_eq!(counts(&fixture.store()), (1, 1));
        // Kill residue is deliberately retained for bounded maintenance;
        // neither Open nor retry interprets it as an accepted commit.
        assert!(
            fs::read_dir(fixture.store().join("staging"))
                .unwrap()
                .count()
                >= 1
        );
        assert!(!fixture.store().join("head").exists());
    }
}

#[test]
fn complete_value_closure_reopens_above_sixty_four_mib_under_small_pool() {
    use std::io::Read;
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let length = (64 << 20) + 17;
    let mut sink = store.value_session(physical_io());
    let root = write_value(
        &mut std::io::repeat(0x4c).take(length),
        length,
        &mut sink,
        &memory,
        &mut logical_io(),
        || false,
    )
    .unwrap();
    let id = root.digest().unwrap();
    drop(sink);
    drop(store);
    let store = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let source = store.value_session(physical_io());
    let mut logical = logical_io();
    logical.write_bytes_remaining = 0;
    let before = memory.used();
    let checked = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    }
    .verify_closure(&id, length, || false)
    .unwrap();
    assert_eq!(checked.root(), root);
    assert_eq!(checked.node_occurrences(), 1036);
    assert_eq!(logical.nodes_remaining, 100000 - 1036);
    assert_eq!(logical.write_bytes_remaining, 0);
    assert_eq!(memory.used(), before);
    let mut calls = 0;
    assert!(matches!(
        ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical
        }
        .verify_closure(&id, length, || {
            calls += 1;
            calls > 10
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(logical.nodes_remaining, 100000 - 1036 - 10);
    assert_eq!(memory.used(), before);
}
