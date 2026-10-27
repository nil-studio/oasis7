use super::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-distfs-{prefix}-{unique}"))
}

#[test]
fn atomic_temp_paths_do_not_collide_between_concurrent_writers() {
    let barrier = std::sync::Barrier::new(8);
    let paths = std::thread::scope(|scope| {
        let writers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    (0..4096)
                        .map(|_| unique_atomic_temp_path(Path::new("same.blob")))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        writers
            .into_iter()
            .flat_map(|writer| writer.join().expect("writer completed"))
            .collect::<Vec<_>>()
    });
    let unique: BTreeSet<_> = paths.iter().collect();
    assert_eq!(
        unique.len(),
        paths.len(),
        "writers must own distinct temporary files"
    );
}

#[test]
fn concurrent_atomic_writers_publish_complete_files() {
    let dir = temp_dir("concurrent-atomic-writers");
    fs::create_dir_all(&dir).expect("create root");
    let path = dir.join("shared.blob");
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for writer in 0..8u8 {
            let path = &path;
            let barrier = &barrier;
            scope.spawn(move || {
                let bytes = vec![writer; 8192];
                barrier.wait();
                for _ in 0..32 {
                    write_bytes_atomic(path, &bytes).expect("publish complete file");
                }
            });
        }
    });
    let bytes = fs::read(&path).expect("read published file");
    assert_eq!(bytes.len(), 8192);
    assert!(bytes.iter().all(|byte| *byte == bytes[0]));
    assert_eq!(fs::read_dir(&dir).expect("list root").count(), 1);
    fs::remove_dir_all(dir).expect("remove test root");
}

#[test]
fn cas_roundtrip_and_pin() {
    let dir = temp_dir("cas");
    let store = LocalCasStore::new(&dir);

    let bytes = b"hello distfs".to_vec();
    let hash = store.put_bytes(&bytes).expect("put");
    assert!(store.has(&hash).expect("has"));
    assert_eq!(store.get(&hash).expect("get"), bytes);

    store.pin(&hash).expect("pin");
    assert!(store.is_pinned(&hash).expect("is pinned"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn scoped_pin_shards_are_atomic_effective_pins_and_can_be_cleared() {
    let dir = temp_dir("scoped-pins");
    let store = LocalCasStore::new(&dir);
    let first = store.put_bytes(b"scoped-first").expect("put first");
    let second = store.put_bytes(b"scoped-second").expect("put second");
    let orphan = store.put_bytes(b"scoped-orphan").expect("put orphan");

    store
        .replace_pin_scope_shard(
            "execution_bridge_v1",
            "record-1",
            &BTreeSet::from([first.clone()]),
        )
        .expect("publish first shard");
    store
        .replace_pin_scope_shard(
            "execution_bridge_v1",
            "record-1",
            &BTreeSet::from([second.clone()]),
        )
        .expect("replace shard");
    assert_eq!(
        store
            .list_pin_scope_shards("execution_bridge_v1")
            .expect("list shards"),
        vec!["record-1"]
    );

    assert!(
        !store
            .is_effectively_pinned(first.as_str())
            .expect("first pin")
    );
    assert!(
        store
            .is_effectively_pinned(second.as_str())
            .expect("second pin")
    );
    assert!(store.list_pins().expect("legacy pins").is_empty());
    assert!(!store.is_pinned(second.as_str()).expect("legacy pin state"));
    assert!(!store.unpin(second.as_str()).expect("legacy unpin"));
    assert!(
        store
            .is_effectively_pinned(second.as_str())
            .expect("scoped pin remains independently owned")
    );
    store.prune_orphan_blobs().expect("prune with scoped pin");
    assert!(store.has(second.as_str()).expect("second exists"));
    assert!(!store.has(first.as_str()).expect("first removed"));
    assert!(!store.has(orphan.as_str()).expect("orphan removed"));

    store
        .clear_pin_scope("execution_bridge_v1")
        .expect("clear scope");
    assert!(
        store
            .list_pin_scope_shards("execution_bridge_v1")
            .expect("list cleared scope")
            .is_empty()
    );
    assert!(
        !store
            .is_effectively_pinned(second.as_str())
            .expect("cleared pin")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_sha256_roundtrip_and_verify() {
    let dir = temp_dir("cas-sha256");
    let store = LocalCasStore::new_with_hash_algorithm(&dir, HashAlgorithm::Sha256);

    let bytes = b"hello sha256 distfs".to_vec();
    let hash = store.put_bytes(&bytes).expect("put");
    assert!(store.has(&hash).expect("has"));
    assert_eq!(store.get_verified(&hash).expect("verified get"), bytes);

    let blob_path = store.blobs_dir().join(format!("{hash}.blob"));
    fs::write(&blob_path, b"tampered").expect("tamper blob");
    assert!(matches!(
        store.get_verified(&hash),
        Err(WorldError::BlobHashMismatch { .. })
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_transparently_compresses_large_blob_on_disk() {
    let dir = temp_dir("cas-compressed-blob");
    let store = LocalCasStore::new(&dir);

    let bytes = vec![b'a'; 16 * 1024];
    let hash = store.put_bytes(&bytes).expect("put");
    let blob_path = store.blobs_dir().join(format!("{hash}.blob"));
    let disk_bytes = fs::read(blob_path.as_path()).expect("read blob from disk");

    assert!(disk_bytes.starts_with(b"O7CBLOB1"));
    assert!(disk_bytes.len() < bytes.len());
    assert_eq!(store.get(&hash).expect("get"), bytes);
    assert_eq!(store.get_verified(&hash).expect("verified get"), bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_raw_range_honors_a_nonzero_offset_once() {
    let dir = temp_dir("cas-raw-range-offset");
    let store = LocalCasStore::new(&dir);
    let bytes = b"0123456789abcdef".to_vec();
    let hash = store.put_bytes(&bytes).expect("put");
    let blob_path = store.blobs_dir().join(format!("{hash}.blob"));
    // Keep this fixture raw: range reads must not skip after the file seek.
    fs::write(blob_path, &bytes).expect("write raw blob");

    assert_eq!(
        store.get_range(&hash, 4, 5).expect("range"),
        (b"45678".to_vec(), false)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_compressed_late_ranges_are_stable_for_concurrent_checkpoint_reads() {
    let dir = temp_dir("cas-compressed-range-cache");
    let store = LocalCasStore::new(&dir);
    let bytes = vec![b'z'; 16 * 1024 * 1024];
    let hash = store.put_bytes(&bytes).expect("put compressed blob");
    let offset = 12 * 1024 * 1024;
    assert_eq!(
        store.get_range(&hash, offset, 1024).expect("late range").0,
        bytes[offset as usize..offset as usize + 1024]
    );

    let mut readers = Vec::new();
    for _ in 0..4 {
        let store = store.clone();
        let hash = hash.clone();
        readers.push(std::thread::spawn(move || {
            store
                .get_range(&hash, offset, 1024)
                .expect("concurrent late range")
        }));
    }
    for reader in readers {
        assert_eq!(
            reader.join().expect("reader"),
            (
                bytes[offset as usize..offset as usize + 1024].to_vec(),
                false
            )
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_compressed_cache_rotates_distinct_blobs_without_global_decode_locking() {
    let dir = temp_dir("cas-compressed-range-rotation");
    let store = LocalCasStore::new(&dir);
    let blobs = [b'a', b'b', b'c'].map(|byte| vec![byte; 16 * 1024 * 1024]);
    let hashes: Vec<_> = blobs
        .iter()
        .map(|bytes| store.put_bytes(bytes).expect("put compressed blob"))
        .collect();
    let offset = 12 * 1024 * 1024;
    for (hash, bytes) in hashes.iter().zip(&blobs) {
        assert_eq!(
            store.get_range(hash, offset, 1024).expect("late range").0,
            bytes[offset as usize..offset as usize + 1024]
        );
    }
    let cache_paths = compressed_range_cache::paths();
    assert_eq!(
        cache_paths.len(),
        2,
        "decoded cache stays under its entry cap"
    );
    assert!(
        cache_paths
            .iter()
            .any(|path| path.ends_with(format!("{}.blob", hashes[2]))),
        "the later checkpoint blob must replace the oldest cache entry"
    );

    let first = store.clone();
    let first_hash = hashes[1].clone();
    let second = store.clone();
    let second_hash = hashes[2].clone();
    let first_reader = std::thread::spawn(move || first.get_range(&first_hash, offset, 1024));
    let second_reader = std::thread::spawn(move || second.get_range(&second_hash, offset, 1024));
    assert_eq!(
        first_reader
            .join()
            .expect("first reader")
            .expect("first range")
            .0,
        vec![b'b'; 1024]
    );
    assert_eq!(
        second_reader
            .join()
            .expect("second reader")
            .expect("second range")
            .0,
        vec![b'c'; 1024]
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cas_does_not_misdecode_raw_blob_that_looks_like_compressed_payload() {
    let dir = temp_dir("cas-magic-collision");
    let store = LocalCasStore::new(&dir);

    let mut bytes = b"O7CBLOB1".to_vec();
    bytes.extend_from_slice(&(4_u64).to_le_bytes());
    bytes.extend_from_slice(
        zstd::stream::encode_all(b"evil".as_slice(), 3)
            .expect("encode fake compressed suffix")
            .as_slice(),
    );
    let hash = store.put_bytes(&bytes).expect("put");

    assert_eq!(store.get(&hash).expect("get"), bytes);
    assert_eq!(store.get_verified(&hash).expect("verified get"), bytes);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_write_read_roundtrip() {
    let dir = temp_dir("file-roundtrip");
    let store = LocalCasStore::new(&dir);

    let metadata = store
        .write_file("docs/readme.txt", b"hello distfs file")
        .expect("write");
    assert_eq!(metadata.path, "docs/readme.txt");
    assert_eq!(metadata.size_bytes, 17);
    assert!(!metadata.content_hash.is_empty());
    assert!(metadata.updated_at_ms > 0);

    let loaded = store.read_file("docs/readme.txt").expect("read");
    assert_eq!(loaded, b"hello distfs file");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_overwrite_updates_hash_and_metadata() {
    let dir = temp_dir("file-overwrite");
    let store = LocalCasStore::new(&dir);

    let first = store.write_file("a/note.txt", b"v1").expect("write first");
    let first_blob_path = store
        .blob_path(first.content_hash.as_str())
        .expect("first blob path");
    assert!(
        first_blob_path.exists(),
        "first blob should exist before overwrite"
    );
    let second = store
        .write_file("a/note.txt", b"v2-data")
        .expect("write second");

    assert_eq!(first.path, second.path);
    assert_ne!(first.content_hash, second.content_hash);
    assert!(second.updated_at_ms >= first.updated_at_ms);
    assert_eq!(second.size_bytes, 7);

    let loaded = store.read_file("a/note.txt").expect("read");
    assert_eq!(loaded, b"v2-data");
    let stat = store
        .stat_file("a/note.txt")
        .expect("stat")
        .expect("exists");
    assert_eq!(stat.content_hash, second.content_hash);
    assert_eq!(stat.size_bytes, 7);
    assert!(
        !first_blob_path.exists(),
        "unreferenced overwrite blob should be pruned immediately"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_delete_removes_mapping() {
    let dir = temp_dir("file-delete");
    let store = LocalCasStore::new(&dir);

    let written = store
        .write_file("workspace/a.log", b"to-delete")
        .expect("write");
    let blob_path = store
        .blob_path(written.content_hash.as_str())
        .expect("blob path");
    assert!(blob_path.exists(), "blob should exist before delete");
    let removed = store.delete_file("workspace/a.log").expect("delete");
    assert!(removed);
    assert!(store.stat_file("workspace/a.log").expect("stat").is_none());
    assert!(!store.delete_file("workspace/a.log").expect("delete again"));
    assert!(
        !blob_path.exists(),
        "delete should prune the now-unreferenced blob"
    );

    let read_result = store.read_file("workspace/a.log");
    assert!(matches!(
        read_result,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_delete_preserves_blob_referenced_by_another_file() {
    let dir = temp_dir("file-delete-shared-blob");
    let store = LocalCasStore::new(&dir);

    let first = store
        .write_file("workspace/a.log", b"shared")
        .expect("write a");
    let second = store
        .write_file("workspace/b.log", b"shared")
        .expect("write b");
    assert_eq!(first.content_hash, second.content_hash);

    let blob_path = store
        .blob_path(first.content_hash.as_str())
        .expect("blob path");
    assert!(blob_path.exists(), "shared blob should exist before delete");

    let removed = store.delete_file("workspace/a.log").expect("delete a");
    assert!(removed);
    assert!(
        store
            .stat_file("workspace/a.log")
            .expect("stat a")
            .is_none()
    );
    assert!(blob_path.exists(), "delete should keep the shared blob");
    assert_eq!(
        store.read_file("workspace/b.log").expect("read b"),
        b"shared"
    );

    let removed = store.delete_file("workspace/b.log").expect("delete b");
    assert!(removed);
    assert!(
        !blob_path.exists(),
        "last reference deletion should prune the blob"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_delete_repairs_legacy_index_without_ref_counts() {
    let dir = temp_dir("file-delete-legacy-index");
    let store = LocalCasStore::new(&dir);

    let first = store
        .write_file("workspace/a.log", b"shared")
        .expect("write a");
    let second = store
        .write_file("workspace/b.log", b"shared")
        .expect("write b");
    assert_eq!(first.content_hash, second.content_hash);

    let mut legacy_index: serde_json::Value =
        serde_json::from_slice(&fs::read(store.files_index_path()).expect("read index"))
            .expect("parse index");
    legacy_index
        .as_object_mut()
        .expect("index object")
        .remove("content_hash_ref_counts");
    fs::write(
        store.files_index_path(),
        serde_json::to_vec(&legacy_index).expect("serialize legacy index"),
    )
    .expect("write legacy index");

    let blob_path = store
        .blob_path(first.content_hash.as_str())
        .expect("blob path");
    assert!(store.delete_file("workspace/a.log").expect("delete a"));
    assert!(
        blob_path.exists(),
        "legacy index migration should keep the shared blob"
    );
    assert_eq!(
        store.read_file("workspace/b.log").expect("read b"),
        b"shared"
    );

    let repaired_index: serde_json::Value =
        serde_json::from_slice(&fs::read(store.files_index_path()).expect("read repaired index"))
            .expect("parse repaired index");
    assert!(
        repaired_index
            .get("content_hash_ref_counts")
            .and_then(|value| value.get(first.content_hash.as_str()))
            .and_then(|value| value.as_u64())
            .is_some(),
        "mutating a legacy index should persist repaired ref counts"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_compacts_files_index_json() {
    let dir = temp_dir("file-index-compact");
    let store = LocalCasStore::new(&dir);

    store
        .write_file("docs/readme.txt", b"hello distfs file")
        .expect("write");

    let bytes = fs::read(store.files_index_path()).expect("read files index");
    assert!(
        !bytes.contains(&b'\n'),
        "files index should be written without pretty-print newlines"
    );
    assert!(
        !bytes.windows(2).any(|window| window == b"  "),
        "files index should not contain indentation padding"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_list_and_stat_return_indexed_entries() {
    let dir = temp_dir("file-list");
    let store = LocalCasStore::new(&dir);

    store.write_file("b/file.txt", b"bbb").expect("write b");
    store.write_file("./a/file.txt", b"aaa").expect("write a");

    let files = store.list_files().expect("list");
    let paths: Vec<String> = files.iter().map(|item| item.path.clone()).collect();
    assert_eq!(
        paths,
        vec!["a/file.txt".to_string(), "b/file.txt".to_string()]
    );

    let stat_a = store
        .stat_file("a/file.txt")
        .expect("stat a")
        .expect("exists");
    assert_eq!(stat_a.size_bytes, 3);
    let bytes = store.read_file("a/file.txt").expect("read a");
    assert_eq!(bytes, b"aaa");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_rejects_invalid_paths() {
    let dir = temp_dir("file-invalid-path");
    let store = LocalCasStore::new(&dir);

    for invalid in ["", "/", "/root.txt", "..", "../a.txt", "a/../../b"] {
        assert!(matches!(
            store.write_file(invalid, b"bad"),
            Err(WorldError::DistributedValidationFailed { .. })
        ));
        assert!(matches!(
            store.read_file(invalid),
            Err(WorldError::DistributedValidationFailed { .. })
        ));
        assert!(matches!(
            store.delete_file(invalid),
            Err(WorldError::DistributedValidationFailed { .. })
        ));
        assert!(matches!(
            store.stat_file(invalid),
            Err(WorldError::DistributedValidationFailed { .. })
        ));
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_write_if_match_enforces_hash_precondition() {
    let dir = temp_dir("file-cas-write");
    let store = LocalCasStore::new(&dir);

    let first = store.write_file("state/a.txt", b"v1").expect("write first");
    let second = store
        .write_file_if_match("state/a.txt", Some(first.content_hash.as_str()), b"v2")
        .expect("write with match");
    assert_ne!(first.content_hash, second.content_hash);
    assert_eq!(store.read_file("state/a.txt").expect("read"), b"v2");

    let stale_write =
        store.write_file_if_match("state/a.txt", Some(first.content_hash.as_str()), b"v3");
    assert!(matches!(
        stale_write,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let missing_precondition = store.write_file_if_match(
        "state/missing.txt",
        Some(first.content_hash.as_str()),
        b"new",
    );
    assert!(matches!(
        missing_precondition,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_delete_if_match_enforces_hash_precondition() {
    let dir = temp_dir("file-cas-delete");
    let store = LocalCasStore::new(&dir);

    let written = store.write_file("state/a.txt", b"v1").expect("write");
    let mismatch = store.delete_file_if_match("state/a.txt", Some("stale-hash"));
    assert!(matches!(
        mismatch,
        Err(WorldError::DistributedValidationFailed { .. })
    ));
    assert!(store.stat_file("state/a.txt").expect("stat").is_some());

    let removed = store
        .delete_file_if_match("state/a.txt", Some(written.content_hash.as_str()))
        .expect("delete with match");
    assert!(removed);
    assert!(store.stat_file("state/a.txt").expect("stat").is_none());
    assert!(
        !store
            .delete_file_if_match("state/a.txt", None)
            .expect("delete missing without precondition")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_store_if_match_rejects_invalid_expected_hash() {
    let dir = temp_dir("file-cas-invalid-hash");
    let store = LocalCasStore::new(&dir);

    let write_result = store.write_file_if_match("a.txt", Some("../bad"), b"hello");
    assert!(matches!(
        write_result,
        Err(WorldError::BlobHashInvalid { .. })
    ));

    let delete_result = store.delete_file_if_match("a.txt", Some("../bad"));
    assert!(matches!(
        delete_result,
        Err(WorldError::BlobHashInvalid { .. })
    ));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn file_index_audit_reports_missing_dangling_and_orphan_blobs() {
    let dir = temp_dir("file-audit");
    let store = LocalCasStore::new(&dir);

    let live = store.write_file("live.txt", b"live").expect("write live");
    let orphan_hash = store.put_bytes(b"orphan").expect("write orphan blob");

    let missing = store
        .write_file("missing.txt", b"missing")
        .expect("write missing");
    let missing_path = store
        .blob_path(missing.content_hash.as_str())
        .expect("missing path");
    fs::remove_file(missing_path).expect("remove missing blob");

    let dangling_pin_hash = store.put_bytes(b"dangling-pin").expect("put dangling");
    store.pin(dangling_pin_hash.as_str()).expect("pin");
    let dangling_path = store
        .blob_path(dangling_pin_hash.as_str())
        .expect("dangling path");
    fs::remove_file(dangling_path).expect("remove dangling blob");

    let report = store.audit_file_index().expect("audit");
    assert_eq!(report.total_indexed_files, 2);
    assert_eq!(report.total_pins, 1);
    assert!(
        report
            .missing_file_blob_hashes
            .contains(&missing.content_hash)
    );
    assert!(!report.missing_file_blob_hashes.contains(&live.content_hash));
    assert!(report.dangling_pin_hashes.contains(&dangling_pin_hash));
    assert!(report.orphan_blob_hashes.contains(&orphan_hash));
    assert!(!report.is_clean());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn prune_orphan_blobs_removes_only_unreferenced_data() {
    let dir = temp_dir("prune-orphan");
    let store = LocalCasStore::new(&dir);

    let live = store
        .write_file("live.txt", b"live-data")
        .expect("write live");
    let orphan_hash = store.put_bytes(b"old-data").expect("write orphan");

    let pinned_hash = store.put_bytes(b"pinned-data").expect("put pinned");
    store.pin(pinned_hash.as_str()).expect("pin");

    let freed = store.prune_orphan_blobs().expect("prune orphan");
    assert!(freed > 0);
    assert!(store.has(live.content_hash.as_str()).expect("live exists"));
    assert!(store.has(pinned_hash.as_str()).expect("pinned exists"));
    assert!(!store.has(orphan_hash.as_str()).expect("orphan removed"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_zstd_0133_frame_verified_and_range_reads() {
    let raw: Vec<u8> = (0..65536)
        .map(|i| ((i % 251) ^ ((i / 4096) % 17)) as u8)
        .collect();
    let hash = "0f227458a96cb2b18610d9624d5750390613bf9e84107e46ff48902b5b31be88";
    assert_eq!(blake3_hex(&raw), hash);
    let dir = temp_dir("legacy-zstd");
    let store = LocalCasStore::new(&dir);
    store.ensure_dirs().unwrap();
    let mut encoded = COMPRESSED_BLOB_MAGIC.to_vec();
    encoded.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    encoded.extend_from_slice(hash.as_bytes());
    encoded.extend_from_slice(include_bytes!(
        "../tests/fixtures/zstd-0.13.3-level3-stream.zst"
    ));
    fs::write(store.blob_path(hash).unwrap(), encoded).unwrap();
    // Reopen the store so this cannot pass through a current-version write cache.
    let store = LocalCasStore::new(&dir);
    assert_eq!(store.get_verified(hash).unwrap(), raw);
    for (offset, limit) in [(0, 1), (4090, 64), (65520, 64), (65536, 1)] {
        let (bytes, _) = store.get_range(hash, offset as u64, limit).unwrap();
        assert_eq!(bytes, raw[offset..raw.len().min(offset + limit)]);
    }
    fs::remove_dir_all(dir).unwrap();
}
