use super::*;
use crate::incremental_page_store::tests::{Fixture, physical_io};

fn pages(count: usize) -> Vec<([u8; 32], Vec<u8>)> {
    (0..count)
        .map(|n| {
            let mut bytes = vec![17; 1200];
            bytes[..8].copy_from_slice(&(n as u64).to_be_bytes());
            (*blake3::hash(&bytes).as_bytes(), bytes)
        })
        .collect()
}
fn persist(store: &IncrementalPageStore, pages: &[([u8; 32], Vec<u8>)]) {
    let refs: Vec<_> = pages
        .iter()
        .map(|(id, bytes)| (*id, bytes.as_slice()))
        .collect();
    store
        .put_page_batch(
            &refs,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &AtomicBool::new(false),
        )
        .unwrap();
}
#[test]
fn direct_pack_wire_and_untrusted_locator_are_exact() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let entries = [
        Entry {
            id: [1; 32],
            bytes: b"first".to_vec(),
        },
        Entry {
            id: [2; 32],
            bytes: b"second".to_vec(),
        },
    ];
    publish(
        store.root(),
        "recipes",
        &entries,
        &mut physical_io(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut expected = b"oasis7.iws.page-pack.v1\0".to_vec();
    let index = expected.len() + 23 + 11;
    expected.extend(1u32.to_be_bytes());
    expected.push(2);
    expected.extend(2u16.to_be_bytes());
    expected.extend((index as u64).to_be_bytes());
    expected.extend(((index + 88) as u64).to_be_bytes());
    expected.extend(b"firstsecond");
    let offset = b"oasis7.iws.page-pack.v1\0".len() + 23;
    for (id, at, len) in [([1; 32], offset, 5u32), ([2; 32], offset + 5, 6u32)] {
        expected.extend(id);
        expected.extend((at as u64).to_be_bytes());
        expected.extend(len.to_be_bytes());
    }
    let path = store.root().join("recipes").join(hex::encode([1; 32]));
    assert_eq!(fs::read(&path).unwrap(), expected);
    assert_eq!(
        files::read(&path, 16, &mut physical_io(), &AtomicBool::new(false)).unwrap(),
        b"first"
    );
}

#[test]
fn general_batch_preserves_frozen_cuts_across_multiple_frame_packs_and_limits_bytes() {
    fn gear(state: u64, byte: u8) -> u64 {
        let value = (byte as u64 + 1).wrapping_mul(0x9e3779b185ebca87);
        state
            .rotate_left(1)
            .wrapping_add(value ^ value.rotate_left(23))
    }
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
    let _input = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let mut pages = Vec::new();
    for n in 0u64..255 {
        let mut page = vec![17; 4096];
        page[..8].copy_from_slice(&n.to_be_bytes());
        let prefix = page[..4093]
            .iter()
            .fold(0, |state, byte| gear(state, *byte));
        let mut found = None;
        'search: for a in 0..=255u8 {
            for b in 0..=255u8 {
                let state = gear(gear(prefix, a), b);
                for c in 0..=255u8 {
                    if gear(state, c) & 16383 == 0 {
                        found = Some((a, b, c));
                        break 'search;
                    }
                }
            }
        }
        let (a, b, c) = found.expect("three-byte boundary patch");
        page[4093..].copy_from_slice(&[a, b, c]);
        page.push(n as u8);
        assert_eq!(
            frame_codec::chunks(&page)
                .iter()
                .map(|s| s.len())
                .collect::<Vec<_>>(),
            vec![4096, 1]
        );
        pages.push((*blake3::hash(&page).as_bytes(), page));
    }
    persist(&store, &pages);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let inodes: std::collections::BTreeSet<_> = fs::read_dir(store.root().join("frames"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().ino())
            .collect();
        assert_eq!(inodes.len(), 2); // 510 original frames, max 256 per pack
    }
    for n in [0, 127, 254] {
        assert_eq!(
            store
                .read_page(
                    &pages[n].0,
                    4097,
                    MemoryClass::Recovery,
                    &mut physical_io(),
                    &AtomicBool::new(false)
                )
                .unwrap()
                .as_slice(),
            pages[n].1
        );
    }
    let mut over: Vec<_> = pages.iter().map(|(id, b)| (*id, b.as_slice())).collect();
    over.push(over[0]);
    let mut io = physical_io();
    assert!(matches!(
        store.put_page_batch(
            &over,
            MemoryClass::Ordinary,
            &mut io,
            &AtomicBool::new(false)
        ),
        Err(PageStoreError::ResourceLimited("page batch bytes"))
    ));
    assert_eq!(io.writes_remaining, physical_io().writes_remaining);
}

#[test]
fn arbitrary_raw_frame_starting_with_pack_magic_preserves_its_original_bytes() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut bytes = MAGIC.to_vec();
    bytes.extend([0; 100]);
    let id = *blake3::hash(&bytes).as_bytes();
    let bytes = memory
        .reserve(bytes.capacity(), MemoryClass::Ordinary)
        .unwrap()
        .retain(bytes);
    store
        .put_page(id, &bytes, &mut physical_io(), &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        store
            .read_page(
                &id,
                1024,
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
fn repeated_frames_within_one_page_do_not_repeat_physical_pack_reads() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let bytes = vec![0x5a; 65536];
    assert!(frame_codec::chunks(&bytes).len() > 8);
    let id = *blake3::hash(&bytes).as_bytes();
    store
        .put_page_batch(
            &[(id, bytes.as_slice())],
            MemoryClass::Ordinary,
            &mut physical_io(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let before = physical_io();
    let mut io = before.clone();
    assert_eq!(
        store
            .read_page(
                &id,
                65536,
                MemoryClass::Ordinary,
                &mut io,
                &AtomicBool::new(false)
            )
            .unwrap()
            .as_slice(),
        bytes
    );
    assert!(
        before.reads_remaining - io.reads_remaining < 16,
        "one recipe and one unique frame"
    );
    // The cache key must include the complete verified descriptor, not only
    // its stored ID. A checksum-correct forged original hash still fails.
    let path = store.root().join("recipes").join(hex::encode(id));
    let encoded = files::read(
        &path,
        recipe::MAX_RECIPE_BYTES,
        &mut physical_io(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let (_, mut frames) = recipe::decode(&id, &encoded).unwrap();
    frames[1].original[0] ^= 1;
    fs::write(path, recipe::encode(&id, bytes.len(), &frames).unwrap()).unwrap();
    assert!(matches!(
        store.read_page(
            &id,
            65536,
            MemoryClass::Ordinary,
            &mut physical_io(),
            &AtomicBool::new(false)
        ),
        Err(PageStoreError::Corrupt(_))
    ));
}

#[test]
fn packed_raw_and_zstd_pages_reopen_with_shared_inodes_and_bounded_point_reads() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _input = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let pages = pages(MAX_ENTRIES);
    let cancelled = AtomicBool::new(false);
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let root = fixture
            .0
            .join(if matches!(compression, PageCompression::Raw) {
                "raw"
            } else {
                "zstd"
            });
        let store = IncrementalPageStore::create(&root, memory.clone(), compression).unwrap();
        persist(&store, &pages);
        drop(store);
        let store = IncrementalPageStore::open(
            &root,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap();
        for (id, bytes) in &pages {
            assert_eq!(
                store
                    .read_page(
                        id,
                        bytes.len(),
                        MemoryClass::Recovery,
                        &mut physical_io(),
                        &cancelled
                    )
                    .unwrap()
                    .as_slice(),
                bytes
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            for directory in ["frames", "recipes"] {
                let inodes: std::collections::BTreeSet<_> = fs::read_dir(root.join(directory))
                    .unwrap()
                    .map(|e| e.unwrap().metadata().unwrap().ino())
                    .collect();
                assert_eq!(inodes.len(), 1, "one immutable pack per directory");
            }
        }
        assert_eq!(fs::read_dir(root.join("staging")).unwrap().count(), 0);
        let before = physical_io();
        let mut quota = before.clone();
        store
            .read_page(
                &pages[79].0,
                1200,
                MemoryClass::Recovery,
                &mut quota,
                &cancelled,
            )
            .unwrap();
        let reads = before.reads_remaining - quota.reads_remaining;
        let bytes = before.read_bytes_remaining - quota.read_bytes_remaining;
        assert!(reads <= 25 && bytes < 8192, "reads={reads} bytes={bytes}");
        let mut exact = PageIoBudget {
            reads_remaining: reads,
            read_bytes_remaining: bytes,
            writes_remaining: 0,
            write_bytes_remaining: 0,
        };
        store
            .read_page(
                &pages[79].0,
                1200,
                MemoryClass::Recovery,
                &mut exact,
                &cancelled,
            )
            .unwrap();
        assert_eq!((exact.reads_remaining, exact.read_bytes_remaining), (0, 0));
        for resource in 0..2 {
            let mut short = PageIoBudget {
                reads_remaining: reads - u64::from(resource == 0),
                read_bytes_remaining: bytes - u64::from(resource == 1),
                writes_remaining: 0,
                write_bytes_remaining: 0,
            };
            assert!(matches!(
                store.read_page(
                    &pages[79].0,
                    1200,
                    MemoryClass::Recovery,
                    &mut short,
                    &cancelled
                ),
                Err(PageStoreError::ResourceLimited(_))
            ));
        }
        persist(&store, &pages[..2]); // exact idempotence also syncs selected closure
        assert_eq!(
            fs::read_dir(root.join("recipes")).unwrap().count(),
            MAX_ENTRIES
        );
    }
}

#[test]
fn malformed_pack_headers_rows_payload_and_missing_members_never_return_a_page() {
    for corruption in 0..9 {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        let _input = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let mut pages = pages(3);
        pages.sort_by_key(|p| p.0);
        let store =
            IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
        persist(&store, &pages);
        let path = store.root().join("recipes").join(hex::encode(pages[0].0));
        let mut bytes = fs::read(&path).unwrap();
        let index = u64::from_be_bytes(bytes[MAGIC.len() + 7..MAGIC.len() + 15].try_into().unwrap())
            as usize;
        match corruption {
            0 => bytes[MAGIC.len()] ^= 1,
            1 => bytes[MAGIC.len() + 4] = 1,
            2 => bytes[MAGIC.len() + 5..MAGIC.len() + 7].copy_from_slice(&257u16.to_be_bytes()),
            3 => bytes[MAGIC.len() + 7..MAGIC.len() + 15].copy_from_slice(&u64::MAX.to_be_bytes()),
            4 => bytes[MAGIC.len() + 15..MAGIC.len() + 23].copy_from_slice(&0u64.to_be_bytes()),
            5 => bytes[index + 32..index + 40].copy_from_slice(&0u64.to_be_bytes()),
            6 => bytes[index + 40..index + 44].copy_from_slice(&u32::MAX.to_be_bytes()),
            7 => bytes[index..index + 32].copy_from_slice(&[0; 32]),
            8 => bytes[HEADER] ^= 1,
            _ => unreachable!(),
        }
        fs::write(path, bytes).unwrap();
        assert!(
            matches!(
                store.read_page(
                    &pages[0].0,
                    1200,
                    MemoryClass::Ordinary,
                    &mut physical_io(),
                    &AtomicBool::new(false)
                ),
                Err(PageStoreError::Corrupt(_))
            ),
            "corruption {corruption}"
        );
    }
}

#[test]
fn bounded_admission_cancellation_and_partial_publication_can_retry_exact_pages() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
    let pages = pages(3);
    let refs: Vec<_> = pages.iter().map(|(id, b)| (*id, b.as_slice())).collect();
    let _ordinary = store
        .memory
        .reserve((4 << 20) - store.memory.used(), MemoryClass::Ordinary)
        .unwrap();
    let mut io = physical_io();
    assert!(matches!(
        store.put_page_batch(
            &refs,
            MemoryClass::Ordinary,
            &mut io,
            &AtomicBool::new(false)
        ),
        Err(PageStoreError::Memory(_))
    ));
    assert_eq!(io.writes_remaining, physical_io().writes_remaining);
    assert!(matches!(
        store.put_page_batch(
            &refs,
            MemoryClass::Ordinary,
            &mut io,
            &AtomicBool::new(true)
        ),
        Err(PageStoreError::Cancelled)
    ));
    assert!(matches!(
        store.put_page_batch(
            &refs,
            MemoryClass::Recovery,
            &mut io,
            &AtomicBool::new(true)
        ),
        Err(PageStoreError::Cancelled)
    ));
    let mut io = physical_io();
    io.writes_remaining = 2; // durable frame pack, then one link
    assert!(matches!(
        store.put_page_batch(
            &refs,
            MemoryClass::Recovery,
            &mut io,
            &AtomicBool::new(false)
        ),
        Err(PageStoreError::ResourceLimited(_))
    ));
    assert!(fs::read_dir(store.root().join("staging")).unwrap().count() > 0);
    store
        .put_page_batch(
            &refs,
            MemoryClass::Recovery,
            &mut physical_io(),
            &AtomicBool::new(false),
        )
        .unwrap();
    for (id, bytes) in pages {
        assert_eq!(
            store
                .read_page(
                    &id,
                    1200,
                    MemoryClass::Recovery,
                    &mut physical_io(),
                    &AtomicBool::new(false)
                )
                .unwrap()
                .as_slice(),
            bytes
        );
    }
}

#[test]
fn concurrent_raw_pack_and_zstd_single_publish_select_exact_original_pages() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let _input = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let raw = IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
        .unwrap();
    let zstd = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Zstd,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let pages = pages(3);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|s| {
        let a = s.spawn(|| {
            barrier.wait();
            persist(&raw, &pages);
        });
        let b = s.spawn(|| {
            barrier.wait();
            for (id, bytes) in &pages {
                let bytes = memory
                    .reserve(bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(bytes.clone());
                zstd.put_page(*id, &bytes, &mut physical_io(), &AtomicBool::new(false))
                    .unwrap();
            }
        });
        a.join().unwrap();
        b.join().unwrap();
    });
    for (id, bytes) in pages {
        assert_eq!(
            raw.read_page(
                &id,
                1200,
                MemoryClass::Recovery,
                &mut physical_io(),
                &AtomicBool::new(false)
            )
            .unwrap()
            .as_slice(),
            bytes
        );
    }
}

#[test]
#[ignore = "exact subprocess worker"]
fn pack_crash_worker() {
    let root = PathBuf::from(std::env::var_os("OASIS7_IWS_PAGE_CRASH_ROOT").unwrap());
    let memory = MemoryBudget::new(16 << 20, 0).unwrap();
    let store =
        IncrementalPageStore::open(&root, memory, PageCompression::Raw, MemoryClass::Ordinary)
            .unwrap();
    persist(&store, &pages(3));
    panic!("crash boundary missed");
}
#[test]
fn killed_pack_publisher_reopens_partial_links_and_retries_without_overwriting() {
    use std::{process::Command, time::Duration};
    for directory in ["pack-frames", "pack-recipes"] {
        for step in 1..=3 {
            let fixture = Fixture::new();
            let memory = MemoryBudget::new(16 << 20, 0).unwrap();
            drop(
                IncrementalPageStore::create(
                    &fixture.store(),
                    memory.clone(),
                    PageCompression::Raw,
                )
                .unwrap(),
            );
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "incremental_page_store::pack::tests::pack_crash_worker",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("OASIS7_IWS_PAGE_CRASH_ROOT", fixture.store())
                .env("OASIS7_IWS_PAGE_CRASH_POINT", format!("{directory}-{step}"));
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
                memory,
                PageCompression::Raw,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut pages = pages(3);
            pages.sort_by_key(|p| p.0);
            let read = store.read_page(
                &pages[0].0,
                1200,
                MemoryClass::Ordinary,
                &mut physical_io(),
                &AtomicBool::new(false),
            );
            if directory == "pack-recipes" && step >= 2 {
                assert_eq!(read.unwrap().as_slice(), pages[0].1);
            } else {
                assert!(matches!(read, Err(PageStoreError::Missing(_))));
            }
            persist(&store, &pages);
            for (id, bytes) in pages {
                assert_eq!(
                    store
                        .read_page(
                            &id,
                            1200,
                            MemoryClass::Ordinary,
                            &mut physical_io(),
                            &AtomicBool::new(false)
                        )
                        .unwrap()
                        .as_slice(),
                    bytes
                );
            }
            assert!(fs::read_dir(store.root().join("staging")).unwrap().count() > 0);
        }
    }
}
