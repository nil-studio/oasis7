use super::*;

#[test]
fn private_page_buffer_is_charged_bounded_and_requires_an_explicit_flush() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let baseline = memory.used();
    let mut io = physical_io();
    let cancelled = AtomicBool::new(false);
    let mut session = BorrowedPageSession::new(&store, &mut io, &cancelled).unwrap();
    let mut ids = Vec::new();
    for n in 0..4 {
        let mut bytes = vec![17; 65536];
        bytes[..8].copy_from_slice(&(n as u64).to_be_bytes());
        let id = *blake3::hash(&bytes).as_bytes();
        ids.push(id);
        let bytes = memory
            .reserve(bytes.capacity(), MemoryClass::Ordinary)
            .unwrap()
            .retain(bytes);
        session.put_node(id, bytes).unwrap();
        assert!(session.pending_bytes <= 256 * 1024);
    }
    // Four exact 64 KiB pages fill the buffer; the next one forces a batch.
    assert_eq!(
        fs::read_dir(store.root().join("recipes")).unwrap().count(),
        0
    );
    let bytes = charged(&memory, vec![19; 100]);
    let id = *blake3::hash(&bytes).as_bytes();
    session.put_node(id, bytes).unwrap();
    assert_eq!(
        fs::read_dir(store.root().join("recipes")).unwrap().count(),
        4
    );
    assert_eq!(session.pending_bytes, 100);
    assert!(memory.used() >= baseline + 64 * 1024 + 100);
    session.flush().unwrap();
    drop(session);
    assert_eq!(memory.used(), baseline);
    for id in ids {
        assert_eq!(
            store
                .read_page(
                    &id,
                    65536,
                    MemoryClass::Recovery,
                    &mut physical_io(),
                    &cancelled
                )
                .unwrap()
                .len(),
            65536
        );
    }
    assert_eq!(
        store
            .read_page(
                &id,
                100,
                MemoryClass::Recovery,
                &mut physical_io(),
                &cancelled
            )
            .unwrap()
            .as_slice(),
        &[19; 100]
    );
}

#[test]
fn final_flush_failure_and_import_mismatch_never_publish_an_object_locator() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let baseline = memory.used();
    for mismatch in [false, true] {
        let mut physical = physical_io();
        physical.writes_remaining = 0;
        let result = objects.write_object(
            &mut &b"buffered original"[..],
            17,
            mismatch.then_some([99; 32]),
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        );
        if mismatch {
            assert!(matches!(
                result,
                Err(PageStoreError::Corrupt("original object import hash"))
            ));
        } else {
            assert!(matches!(result, Err(PageStoreError::ResourceLimited(_))));
        }
        assert_eq!(memory.used(), baseline);
        assert_eq!(
            fs::read_dir(pages.root().join("objects")).unwrap().count(),
            0
        );
        assert_eq!(
            fs::read_dir(pages.root().join("recipes")).unwrap().count(),
            0
        );
    }
}

#[test]
fn source_unwind_and_cancellation_release_buffered_pages_without_accepting_an_object() {
    struct Input<'a> {
        calls: usize,
        cancelled: &'a AtomicBool,
        panic: bool,
    }
    impl Read for Input<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls > 1 {
                panic!("injected source unwind");
            }
            bytes.fill(23);
            if !self.panic {
                self.cancelled.store(true, Ordering::Release);
            }
            Ok(bytes.len())
        }
    }
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let baseline = memory.used();
    for panic in [false, true] {
        cancelled.store(false, Ordering::Release);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            objects.write_object(
                &mut Input {
                    calls: 0,
                    cancelled: &cancelled,
                    panic,
                },
                128 << 10,
                None,
                &mut ObjectIo {
                    physical: &mut physical_io(),
                    logical: &mut logical_io(),
                    class: MemoryClass::Ordinary,
                    cancelled: &cancelled,
                },
            )
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(matches!(result.unwrap(), Err(PageStoreError::Cancelled)));
        }
        assert_eq!(memory.used(), baseline);
        assert_eq!(
            fs::read_dir(pages.root().join("objects")).unwrap().count(),
            0
        );
    }
}

struct UniqueInput {
    position: u64,
    remaining: u64,
}
impl Read for UniqueInput {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.remaining.min(bytes.len() as u64) as usize;
        for byte in &mut bytes[..count] {
            let block = self.position / 65536;
            let within = (self.position % 65536) as usize;
            *byte = if within < 8 {
                block.to_be_bytes()[within]
            } else {
                17
            };
            self.position += 1;
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}
#[test]
fn unique_original_above_sixty_four_mib_batches_under_small_pool_and_reopens_exactly() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    physical.reads_remaining = 1_000_000;
    let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
    let length = (64 << 20) + 17;
    // Independent block-wise flat hash oracle. The input adapter above can be
    // called at arbitrary boundaries and owns no original-sized buffer.
    let _oracle = memory.reserve(65536, MemoryClass::Ordinary).unwrap();
    let mut block = vec![17; 65536];
    let mut expected = blake3::Hasher::new();
    let mut remaining = length;
    for n in 0u64.. {
        block[..8].copy_from_slice(&n.to_be_bytes());
        let count = remaining.min(65536);
        expected.update(&block[..count]);
        remaining -= count;
        if remaining == 0 {
            break;
        }
    }
    let expected = *expected.finalize().as_bytes();
    let baseline = memory.used();
    let id = objects
        .write_object(
            &mut UniqueInput {
                position: 0,
                remaining: length as u64,
            },
            length as u64,
            Some(expected),
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    assert_eq!(id, expected);
    assert_eq!(memory.used(), baseline);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _stats = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
        let mut inodes = std::collections::BTreeSet::new();
        let mut names = 0;
        for entry in fs::read_dir(pages.root().join("recipes")).unwrap() {
            let entry = entry.unwrap();
            let m = entry.metadata().unwrap();
            inodes.insert((m.dev(), m.ino()));
            names += 1;
        }
        assert!(names > 1024);
        assert!(
            inodes.len() * 2 < names,
            "names={names} inodes={}",
            inodes.len()
        );
    }
    drop(objects);
    drop(pages);
    let pages = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let mut physical = physical_io();
    physical.reads_remaining = 1_000_000;
    let objects =
        IncrementalObjectStore::open(&pages, MemoryClass::Recovery, &mut physical, &cancelled)
            .unwrap();
    struct Hash(blake3::Hasher);
    impl Write for Hash {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut actual = Hash(blake3::Hasher::new());
    assert_eq!(
        objects
            .read_object_into(
                &id,
                length as u64,
                &mut actual,
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical_io(),
                    class: MemoryClass::Recovery,
                    cancelled: &cancelled,
                }
            )
            .unwrap(),
        length as u64
    );
    assert_eq!(actual.0.finalize().as_bytes(), &expected);
}

#[test]
fn batched_original_identity_is_unchanged_when_reimported_with_zstd() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let raw = IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let objects = IncrementalObjectStore::create(&raw, &mut physical, &cancelled).unwrap();
    let bytes = charged(&memory, vec![41; 1 << 20]);
    let id = *blake3::hash(&bytes).as_bytes();
    objects
        .write_object(
            &mut bytes.as_slice(),
            bytes.len() as u64,
            Some(id),
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    let before = fs::read_dir(raw.root().join("recipes")).unwrap().count();
    let zstd = IncrementalPageStore::open(
        &fixture.store(),
        memory,
        PageCompression::Zstd,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let objects =
        IncrementalObjectStore::open(&zstd, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    assert_eq!(
        objects
            .write_object(
                &mut bytes.as_slice(),
                bytes.len() as u64,
                Some(id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical_io(),
                    class: MemoryClass::Ordinary,
                    cancelled: &cancelled,
                }
            )
            .unwrap(),
        id
    );
    assert_eq!(
        fs::read_dir(raw.root().join("recipes")).unwrap().count(),
        before
    );
}

#[test]
#[ignore = "exact original-batch subprocess worker"]
fn original_batch_crash_worker() {
    let root = PathBuf::from(std::env::var_os("OASIS7_IWS_PAGE_CRASH_ROOT").unwrap());
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let pages =
        IncrementalPageStore::open(&root, memory, PageCompression::Raw, MemoryClass::Ordinary)
            .unwrap();
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let objects =
        IncrementalObjectStore::open(&pages, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    objects
        .write_object(
            &mut UniqueInput {
                position: 0,
                remaining: 1 << 20,
            },
            1 << 20,
            None,
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    panic!("batch crash boundary missed");
}

#[test]
fn killed_intermediate_object_batches_have_no_locator_and_retry_the_same_original() {
    use std::{process::Command, time::Duration};
    for point in ["pack-frames-2", "pack-recipes-2", "pack-recipes-3"] {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
        let pages =
            IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
                .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical_io();
        drop(IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap());
        drop(pages);
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "incremental_page_store::objects::tests::batch::original_batch_crash_worker",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("OASIS7_IWS_PAGE_CRASH_ROOT", fixture.store())
            .env("OASIS7_IWS_PAGE_CRASH_POINT", point);
        assert!(
            !crate::test_process::kill_at_marker(
                &mut command,
                &fixture.0.join("paused"),
                Duration::from_secs(15)
            )
            .unwrap()
            .success()
        );
        let pages = IncrementalPageStore::open(
            &fixture.store(),
            memory,
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap();
        let objects =
            IncrementalObjectStore::open(&pages, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        assert_eq!(
            fs::read_dir(pages.root().join("objects")).unwrap().count(),
            0
        );
        let mut expected = blake3::Hasher::new();
        let mut buffer = [17; 65536];
        for block in 0u64..16 {
            buffer[..8].copy_from_slice(&block.to_be_bytes());
            expected.update(&buffer);
        }
        let expected = *expected.finalize().as_bytes();
        assert_eq!(
            objects
                .write_object(
                    &mut UniqueInput {
                        position: 0,
                        remaining: 1 << 20
                    },
                    1 << 20,
                    Some(expected),
                    &mut ObjectIo {
                        physical: &mut physical,
                        logical: &mut logical_io(),
                        class: MemoryClass::Ordinary,
                        cancelled: &cancelled,
                    }
                )
                .unwrap(),
            expected
        );
        assert_eq!(
            fs::read_dir(pages.root().join("objects")).unwrap().count(),
            1
        );
        assert!(fs::read_dir(pages.root().join("staging")).unwrap().count() > 0);
    }
}
