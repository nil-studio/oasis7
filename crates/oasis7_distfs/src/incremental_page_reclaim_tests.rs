use super::super::tests::{Fixture, charged, physical_io};
use super::*;

fn work() -> PageReclaimWork {
    PageReclaimWork {
        max_entries: 1000,
        entries_remaining: 10_000,
        references_remaining: 10_000,
        object_logical: super::super::tests::logical_io(),
        max_object_bytes: 64 << 20,
    }
}
fn orphan(root: &Path, bytes: &[u8]) -> PathBuf {
    let path = root
        .join("frames")
        .join(blake3::hash(bytes).to_hex().as_str());
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn physical_reclaim_resumes_after_real_unlink_and_keeps_every_selected_page() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::create(&root, memory.clone(), compression).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let original = [
            vec![7; VALUE_MAX_NODE_BYTES / 2],
            vec![8; VALUE_MAX_NODE_BYTES / 2],
        ];
        for bytes in &original {
            let admitted = charged(&memory, bytes.clone());
            store
                .put_page(
                    *blake3::hash(bytes).as_bytes(),
                    &admitted,
                    &mut physical_io(),
                    &stop,
                )
                .unwrap();
        }
        let first = orphan(&root, b"first unpublished frame");
        let second = orphan(&root, b"second unpublished frame");
        // A real open store owns the same shared kernel lease used by readers,
        // writers and borrowed transfer sessions. No deletion while it lives.
        assert!(matches!(
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop),
            Err(PageReclaimFailure::NotStarted(
                PageStoreError::ResourceLimited(_)
            ))
        ));
        assert!(first.exists() && second.exists());
        drop(store);
        files::cancel_after_sync(root.join("frames"), stop.clone(), 1);
        let failure =
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap_err();
        match failure {
            PageReclaimFailure::Incomplete {
                report,
                cause: PageStoreError::Cancelled,
            } => {
                assert_eq!(report.unlinked_frames, 1);
                assert!(report.directory_synced);
            }
            other => panic!("wrong partial result: {other:?}"),
        }
        assert_ne!(first.exists(), second.exists());
        stop.store(false, Ordering::Release);
        let reopened =
            IncrementalPageStore::open(&root, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        for bytes in &original {
            assert_eq!(
                reopened
                    .read_page(
                        blake3::hash(bytes).as_bytes(),
                        VALUE_MAX_NODE_BYTES,
                        MemoryClass::Ordinary,
                        &mut physical_io(),
                        &stop
                    )
                    .unwrap()
                    .as_slice(),
                bytes.as_slice()
            );
        }
        let bytes = charged(&memory, b"new work".to_vec());
        let id = *blake3::hash(&bytes).as_bytes();
        assert!(matches!(
            reopened.put_page(id, &bytes, &mut physical_io(), &stop),
            Err(PageStoreError::ResourceLimited(_))
        ));
        assert!(matches!(
            reopened.put_page_batch(
                &[(id, &bytes)],
                MemoryClass::Ordinary,
                &mut physical_io(),
                &stop
            ),
            Err(PageStoreError::ResourceLimited(_))
        ));
        drop(reopened);
        let report =
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap();
        assert_eq!(report.unlinked_frames, 1);
        assert!(!first.exists() && !second.exists() && !root.join(RETIRE).exists());
        let mut no_writes = physical_io();
        no_writes.writes_remaining = 0;
        assert_eq!(
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut no_writes, &stop)
                .unwrap()
                .unlinked_frames,
            0
        );
        let resumed =
            IncrementalPageStore::open(&root, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        resumed
            .put_page(id, &bytes, &mut physical_io(), &stop)
            .unwrap();
    }
}

#[test]
fn physical_reclaim_resumes_only_recognized_retire_prefixes() {
    for prefix in [0, 8, INTENT.len(), INTENT.len() + 1] {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        drop(IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap());
        let frame = orphan(&root, b"unfinished retirement");
        let bytes = if prefix <= INTENT.len() {
            &INTENT[..prefix]
        } else {
            b"unknown future retirement"
        };
        fs::write(root.join(RETIRE), bytes).unwrap();
        let result = reclaim_orphan_page_frames(
            &root,
            &memory,
            &mut work(),
            &mut physical_io(),
            &AtomicBool::new(false),
        );
        if prefix <= INTENT.len() {
            assert_eq!(result.unwrap().unlinked_frames, 1);
            assert!(!frame.exists() && !root.join(RETIRE).exists());
        } else {
            assert!(matches!(
                result,
                Err(PageReclaimFailure::Incomplete {
                    report: PageReclaimReport {
                        unlinked_frames: 0,
                        ..
                    },
                    cause: PageStoreError::Corrupt(_)
                })
            ));
            assert!(frame.exists() && root.join(RETIRE).exists());
        }
    }
}

#[test]
fn physical_reclaim_unlinks_orphan_pack_alias_without_retiring_shared_inode() {
    use std::os::unix::fs::MetadataExt;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        let stop = AtomicBool::new(false);
        let store = IncrementalPageStore::create(&root, memory.clone(), compression).unwrap();
        let first = [3; 2048];
        let second = [4; 2048];
        let ids = [
            *blake3::hash(&first).as_bytes(),
            *blake3::hash(&second).as_bytes(),
        ];
        store
            .put_page_batch(
                &[(ids[0], &first), (ids[1], &second)],
                MemoryClass::Ordinary,
                &mut physical_io(),
                &stop,
            )
            .unwrap();
        let frame = |id| {
            let bytes = files::read(
                &root.join("recipes").join(hex::encode(id)),
                recipe::MAX_RECIPE_BYTES,
                &mut physical_io(),
                &stop,
            )
            .unwrap();
            let (_, frames) = recipe::decode(&id, &bytes).unwrap();
            assert_eq!(frames.len(), 1);
            root.join("frames").join(hex::encode(frames[0].stored))
        };
        let selected = frame(ids[0]);
        let orphan = frame(ids[1]);
        assert_eq!(
            fs::metadata(&selected).unwrap().ino(),
            fs::metadata(&orphan).unwrap().ino()
        );
        drop(store);
        // A fixture with one published recipe and a provisional unused member
        // in the same physical pack. There is no accepted history in this store.
        fs::remove_file(root.join("recipes").join(hex::encode(ids[1]))).unwrap();
        let report =
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap();
        assert_eq!(report.unlinked_frames, 1);
        assert!(!orphan.exists() && selected.exists());
        let store =
            IncrementalPageStore::open(&root, memory, compression, MemoryClass::Ordinary).unwrap();
        assert_eq!(
            store
                .read_page(
                    &ids[0],
                    2048,
                    MemoryClass::Ordinary,
                    &mut physical_io(),
                    &stop
                )
                .unwrap()
                .as_slice(),
            first
        );
    }
}

#[test]
fn physical_reclaim_preflight_failures_never_unlink_an_orphan() {
    for case in 0..6 {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let store =
            IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap();
        let stop = AtomicBool::new(false);
        let bytes = charged(&memory, vec![1; 128]);
        let id = *blake3::hash(&bytes).as_bytes();
        store
            .put_page(id, &bytes, &mut physical_io(), &stop)
            .unwrap();
        drop(store);
        let orphan = orphan(&root, b"preserve on preflight failure");
        let mut budget = work();
        let mut io = physical_io();
        match case {
            0 => fs::write(root.join("unknown-owner"), b"unknown").unwrap(),
            1 => fs::write(root.join("staging").join("unresolved"), b"pending").unwrap(),
            2 => fs::write(
                root.join("recipes").join(hex::encode(id)),
                b"unknown recipe",
            )
            .unwrap(),
            3 => {
                fs::remove_file(
                    root.join("frames")
                        .join(blake3::hash(&bytes).to_hex().as_str()),
                )
                .unwrap();
            }
            4 => budget.entries_remaining = 0,
            5 => io.writes_remaining = 0,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                reclaim_orphan_page_frames(&root, &memory, &mut budget, &mut io, &stop),
                Err(PageReclaimFailure::NotStarted(_))
            ),
            "case {case}"
        );
        assert!(orphan.exists(), "case {case}");
        assert!(!root.join(RETIRE).exists());
    }
}

#[test]
fn final_marker_removal_has_distinct_sync_and_cancellation_failures() {
    for cancel in [false, true] {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        drop(IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap());
        let frame = orphan(&root, b"finalization orphan");
        let stop = Arc::new(AtomicBool::new(false));
        if cancel {
            files::cancel_after_sync(root.clone(), stop.clone(), 2);
        } else {
            files::fail_after_sync(root.clone(), 2);
        }
        let failure =
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap_err();
        match failure {
            PageReclaimFailure::Finalization { report, cause } => {
                assert_eq!(report.unlinked_frames, 1);
                assert_eq!(report.directory_synced, cancel);
                if cancel {
                    assert!(matches!(cause, PageStoreError::Cancelled));
                } else {
                    assert!(matches!(cause, PageStoreError::Io(_)));
                }
            }
            other => panic!("wrong final stage: {other:?}"),
        }
        assert!(!frame.exists() && !root.join(RETIRE).exists());
        stop.store(false, Ordering::Release);
        assert_eq!(
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap()
                .unlinked_frames,
            0
        );
    }
}

#[test]
fn nonempty_object_capability_is_verified_and_retirement_fences_all_producers() {
    use super::super::objects::{IncrementalObjectStore, ObjectIo};
    use std::cell::Cell;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(32 << 20, 0).unwrap();
        let stop = AtomicBool::new(false);
        let pages = IncrementalPageStore::create(&root, memory.clone(), compression).unwrap();
        let objects = IncrementalObjectStore::create(&pages, &mut physical_io(), &stop).unwrap();
        let bytes = b"real retained original";
        let mut physical = physical_io();
        let mut logical = super::super::tests::logical_io();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Ordinary,
            cancelled: &stop,
        };
        let id = objects
            .write_object(&mut bytes.as_slice(), bytes.len() as u64, None, &mut io)
            .unwrap();
        fs::write(root.join(RETIRE), INTENT).unwrap();
        assert!(IncrementalObjectStore::create(&pages, &mut physical_io(), &stop).is_err());
        struct NeverRead<'a>(&'a Cell<bool>);
        impl std::io::Read for NeverRead<'_> {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                self.0.set(true);
                panic!("fenced reader invoked")
            }
        }
        let called = Cell::new(false);
        assert!(
            objects
                .write_object(&mut NeverRead(&called), 1, None, &mut io)
                .is_err()
        );
        assert!(
            objects
                .write_object_from_stream::<()>(1, [0; 32], &mut io, &mut || false, |_| {
                    called.set(true);
                    panic!("fenced producer invoked")
                })
                .is_err()
        );
        assert!(!called.get());
        drop(objects);
        drop(pages);
        let frame = orphan(&root, b"orphan alongside original");
        let frames = || {
            fs::read_dir(root.join("frames"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<BTreeSet<_>>()
        };
        let before = frames();
        for bound in 0..3 {
            let mut insufficient = work();
            match bound {
                0 => insufficient.object_logical.nodes_remaining = 0,
                1 => insufficient.max_object_bytes = 1,
                _ => insufficient.object_logical.read_bytes_remaining = 0,
            }
            assert!(
                reclaim_orphan_page_frames(
                    &root,
                    &memory,
                    &mut insufficient,
                    &mut physical_io(),
                    &stop
                )
                .is_err()
            );
            assert_eq!(frames(), before);
            assert!(frame.exists());
        }
        let report =
            reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop)
                .unwrap();
        assert_eq!(report.verified_objects, 1);
        assert_eq!(report.unlinked_frames, 1);
        assert!(!frame.exists());
        let pages =
            IncrementalPageStore::open(&root, memory, compression, MemoryClass::Ordinary).unwrap();
        let objects =
            IncrementalObjectStore::open(&pages, MemoryClass::Ordinary, &mut physical_io(), &stop)
                .unwrap();
        let read = objects
            .read_object_owned(&id, bytes.len(), &mut io)
            .unwrap();
        assert_eq!(read.as_slice(), bytes);
        objects
            .write_object(&mut b"next".as_slice(), 4, None, &mut io)
            .unwrap();
    }
}

#[test]
fn unknown_and_partial_object_capabilities_preserve_every_orphan() {
    for kind in 0..3 {
        let fixture = Fixture::new();
        let root = fixture.store();
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        drop(IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap());
        let first = orphan(&root, b"first kept");
        let second = orphan(&root, b"second kept");
        match kind {
            0 => fs::create_dir(root.join("objects")).unwrap(),
            1 => fs::write(root.join("objects-format"), b"partial").unwrap(),
            _ => fs::write(root.join("unknown-object-capability"), b"unknown").unwrap(),
        }
        assert!(
            reclaim_orphan_page_frames(
                &root,
                &memory,
                &mut work(),
                &mut physical_io(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(first.exists() && second.exists());
    }
}

#[test]
fn resumed_empty_inventory_reestablishes_frame_directory_barrier() {
    let fixture = Fixture::new();
    let root = fixture.store();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    drop(IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap());
    let frame = orphan(&root, b"last orphan before interrupted sync");
    let stop = AtomicBool::new(false);
    files::fail_after_sync(root.join("frames"), 1);
    assert!(matches!(
        reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop),
        Err(PageReclaimFailure::Incomplete {
            report: PageReclaimReport {
                unlinked_frames: 1,
                directory_synced: false,
                ..
            },
            ..
        })
    ));
    assert!(!frame.exists() && root.join(RETIRE).exists());
    files::fail_after_sync(root.join("frames"), 1);
    assert!(matches!(
        reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop),
        Err(PageReclaimFailure::Incomplete {
            report: PageReclaimReport {
                unlinked_frames: 0,
                directory_synced: false,
                ..
            },
            ..
        })
    ));
    assert!(root.join(RETIRE).exists());
    let report =
        reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop).unwrap();
    assert!(report.directory_synced && !root.join(RETIRE).exists());
}

#[test]
fn marker_open_failure_does_not_claim_writer_fencing() {
    let fixture = Fixture::new();
    let root = fixture.store();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    drop(IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap());
    let frame = orphan(&root, b"keep until retirement begins");
    let stop = AtomicBool::new(false);
    FAIL_RETIRE_OPEN.with(|flag| flag.set(true));
    assert!(
        matches!(reclaim_orphan_page_frames(&root, &memory, &mut work(), &mut physical_io(), &stop),
        Err(PageReclaimFailure::NotStarted(PageStoreError::Io(error))) if error.raw_os_error() == Some(libc::ENOSPC))
    );
    assert!(frame.exists() && !root.join(RETIRE).exists());
    let store = IncrementalPageStore::open(
        &root,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let bytes = charged(&memory, b"allowed after no-marker failure".to_vec());
    store
        .put_page(
            *blake3::hash(&bytes).as_bytes(),
            &bytes,
            &mut physical_io(),
            &stop,
        )
        .unwrap();
}
