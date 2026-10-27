use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oasis7-cas-retention-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn retention_process_worker() {
    let Ok(root) = std::env::var("OASIS7_CAS_RETENTION_TEST_ROOT") else {
        return;
    };
    let store = LocalCasStore::new(&root);
    let hash = std::env::var("OASIS7_CAS_RETENTION_TEST_HASH").unwrap();
    let operation = std::env::var("OASIS7_CAS_RETENTION_TEST_OPERATION").unwrap();
    if operation == "hold" {
        let _guard = store.acquire_retention_guard().unwrap();
        fs::write(Path::new(&root).join("started"), []).unwrap();
        loop {
            std::thread::park();
        }
    }
    fs::write(Path::new(&root).join("started"), []).unwrap();
    match operation.as_str() {
        "pin" => store.pin(&hash).unwrap(),
        "prune" => {
            store.prune_orphan_blobs().unwrap();
        }
        "quota" => {
            store.prune_unpinned(0).unwrap();
        }
        "delete" => {
            store.delete_file("retained").unwrap();
        }
        _ => panic!("unknown operation"),
    }
    fs::write(Path::new(&root).join("completed"), []).unwrap();
}

#[test]
fn killed_process_releases_retention_lock_without_replacing_carrier() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let hash = store.put_bytes(b"recoverable material").unwrap();
    let mut child = Process(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "retention_tests::retention_process_worker"])
            .env("OASIS7_CAS_RETENTION_TEST_ROOT", &scratch.0)
            .env("OASIS7_CAS_RETENTION_TEST_HASH", &hash)
            .env("OASIS7_CAS_RETENTION_TEST_OPERATION", "hold")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !scratch.0.join("started").exists() {
        assert!(Instant::now() < deadline);
        assert!(child.0.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(5));
    }
    let carrier = scratch.0.join(".retention.lock");
    let probe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&carrier)
        .unwrap();
    assert!(probe.try_lock().is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    // The original open handle now acquires the same carrier: no stale-PID
    // decision, lock unlink, timeout takeover or new lock inode is involved.
    probe.try_lock().unwrap();
    drop(probe);
    store.pin(&hash).unwrap();
    assert_eq!(store.prune_orphan_blobs().unwrap(), 0);
    assert_eq!(store.get_verified(&hash).unwrap(), b"recoverable material");
}

#[test]
fn gc_before_pin_returns_missing_and_unpin_explicitly_allows_collection() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let hash = store.put_bytes(b"not yet retained").unwrap();
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(matches!(
        store.pin(&hash),
        Err(WorldError::BlobNotFound { .. })
    ));
    assert!(!store.is_effectively_pinned(&hash).unwrap());
    let hash = store.put_bytes(b"retained until released").unwrap();
    store.pin(&hash).unwrap();
    assert_eq!(store.prune_unpinned(0).unwrap(), 0);
    assert!(store.unpin(&hash).unwrap());
    assert!(store.prune_unpinned(0).unwrap() > 0);
    assert!(!store.has(&hash).unwrap());
}

#[test]
fn retention_lock_serializes_other_processes_before_reference_snapshots() {
    for operation in ["pin", "prune", "quota", "delete"] {
        let scratch = Scratch::new();
        let store = LocalCasStore::new(&scratch.0);
        let hash = if operation == "delete" {
            store
                .write_file("retained", b"original material")
                .unwrap()
                .content_hash
        } else {
            // Orphan pruning must actually depend on the newly published pin,
            // rather than being independently protected by a file mapping.
            store.put_bytes(b"original material").unwrap()
        };
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(scratch.0.join(".retention.lock"))
            .unwrap();
        lock.lock().unwrap();
        let mut child = Process(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "retention_tests::retention_process_worker",
                    "--nocapture",
                ])
                .env("OASIS7_CAS_RETENTION_TEST_ROOT", &scratch.0)
                .env("OASIS7_CAS_RETENTION_TEST_HASH", &hash)
                .env("OASIS7_CAS_RETENTION_TEST_OPERATION", operation)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !scratch.0.join("started").exists() {
            assert!(Instant::now() < deadline, "worker did not start");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "worker exited before operation"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "{operation} bypassed the held retention lock"
        );
        assert!(!scratch.0.join("completed").exists());
        assert!(store.has(&hash).unwrap());
        // Complete a producer's publication while its transaction still owns
        // the lock. A waiting collector must take a fresh reference snapshot.
        if operation != "pin" {
            write_json_atomic(
                &PinFile {
                    pins: BTreeSet::from([hash.clone()]),
                },
                store.pins_path(),
            )
            .unwrap();
        }
        drop(lock);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker did not finish after release"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(scratch.0.join("completed").exists());
        assert_eq!(store.get_verified(&hash).unwrap(), b"original material");
        assert!(store.is_effectively_pinned(&hash).unwrap());
    }
}

#[test]
fn concurrent_pin_updates_survive_collectors_and_reopen() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let hashes: Vec<_> = (0..32)
        .map(|n| store.put_bytes(format!("retained-{n}").as_bytes()).unwrap())
        .collect();
    let barrier = std::sync::Barrier::new(hashes.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = hashes
            .iter()
            .map(|hash| {
                let barrier = &barrier;
                let root = &scratch.0;
                scope.spawn(move || {
                    let independent = LocalCasStore::new(root);
                    barrier.wait();
                    independent.pin(hash).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
    });
    let reopened = LocalCasStore::new(&scratch.0);
    assert_eq!(
        reopened
            .list_pins()
            .unwrap()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        hashes.iter().cloned().collect()
    );
    assert_eq!(reopened.prune_orphan_blobs().unwrap(), 0);
    assert_eq!(reopened.prune_unpinned(0).unwrap(), 0);
    for (n, hash) in hashes.iter().enumerate() {
        assert_eq!(
            reopened.get_verified(hash).unwrap(),
            format!("retained-{n}").as_bytes()
        );
    }
}

#[test]
fn quota_pruning_preserves_the_existing_pin_only_policy() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let hash = store
        .write_file("unpinned", b"evictable mapping")
        .unwrap()
        .content_hash;
    assert!(store.prune_unpinned(0).unwrap() > 0);
    assert!(store.stat_file("unpinned").unwrap().is_some());
    assert!(!store.has(&hash).unwrap());
    assert_eq!(
        store.audit_file_index().unwrap().missing_file_blob_hashes,
        vec![hash]
    );
}

#[cfg(unix)]
#[test]
fn scope_retirement_preserves_other_owners_and_unrelated_material() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let shared = store.put_bytes(b"shared original").unwrap();
    let unrelated = store.put_bytes(b"unrelated retained material").unwrap();
    store
        .replace_pin_scope_shard(
            "attempt",
            "record",
            &BTreeSet::from([shared.clone(), unrelated.clone()]),
        )
        .unwrap();
    store
        .replace_pin_scope_shard("reader", "active", &BTreeSet::from([shared.clone()]))
        .unwrap();
    store.pin(&shared).unwrap();
    let guard = store.acquire_retention_guard().unwrap();
    guard
        .retire_pin_scope_references("attempt", &BTreeSet::from([shared.clone()]))
        .unwrap();
    drop(guard);
    let owner: PinFile =
        read_json_from_path(&store.pin_scope_shard_path("attempt", "record").unwrap()).unwrap();
    assert_eq!(owner.pins, BTreeSet::from([unrelated.clone()]));
    assert!(store.is_pinned(&shared).unwrap());
    assert_eq!(store.prune_unpinned(0).unwrap(), 0);
    assert_eq!(store.get_verified(&shared).unwrap(), b"shared original");
    store.unpin(&shared).unwrap();
    store.remove_pin_scope_shard("reader", "active").unwrap();
    assert!(store.prune_unpinned(0).unwrap() > 0);
    assert!(!store.has(&shared).unwrap());
    assert_eq!(
        store.get_verified(&unrelated).unwrap(),
        b"unrelated retained material"
    );
}

#[test]
fn competing_conditional_writes_cannot_both_replace_the_same_parent() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let parent = store.write_file("current", b"parent").unwrap();
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let threads: Vec<_> = [b"first".as_slice(), b"second".as_slice()]
            .into_iter()
            .map(|bytes| {
                let barrier = &barrier;
                let root = &scratch.0;
                let hash = &parent.content_hash;
                scope.spawn(move || {
                    let store = LocalCasStore::new(root);
                    barrier.wait();
                    store.write_file_if_match("current", Some(hash), bytes)
                })
            })
            .collect();
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(store.stat_file("current").unwrap().unwrap(), winner);
    assert!(store.audit_file_index().unwrap().is_clean());
}

#[test]
fn invalid_retention_carrier_is_preserved_and_blocks_mutation() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let hash = store.put_bytes(b"retain original").unwrap();
    let carrier = scratch.0.join(".retention.lock");
    fs::write(&carrier, b"unrecognized material").unwrap();
    assert!(store.pin(&hash).is_err());
    assert!(store.prune_orphan_blobs().is_err());
    assert!(store.prune_unpinned(0).is_err());
    assert_eq!(fs::read(&carrier).unwrap(), b"unrecognized material");
    assert_eq!(store.get_verified(&hash).unwrap(), b"retain original");
}

#[cfg(unix)]
#[test]
fn symlink_retention_carrier_does_not_lock_or_modify_its_target() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let target = scratch.0.join("outside-lock");
    fs::write(&target, []).unwrap();
    std::os::unix::fs::symlink(&target, scratch.0.join(".retention.lock")).unwrap();
    assert!(store.prune_orphan_blobs().is_err());
    assert!(target.exists());
    let external = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&target)
        .unwrap();
    external.try_lock().unwrap();
}
