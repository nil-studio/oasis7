use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-prefix-pin-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn published_pin_verification_preserves_gc_material_and_never_repairs_a_missing_shard() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let keep = store.put_bytes(b"durable prefix World").unwrap();
    let orphan = store.put_bytes(b"unpublished candidate").unwrap();
    let expected = BTreeSet::from([keep.clone()]);
    store
        .replace_pin_scope_shard("formal-prefix-test", "height-4", &expected)
        .unwrap();
    store
        .verify_pin_scope_shard("formal-prefix-test", "height-4", &expected)
        .unwrap();
    assert!(
        store
            .verify_pin_scope_shard("formal-prefix-test", "height-4", &BTreeSet::from([orphan]))
            .is_err()
    );
    assert_eq!(
        store.list_pin_scope_shards("formal-prefix-test").unwrap(),
        vec!["height-4"]
    );
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(store.has(&keep).unwrap());
    store
        .remove_pin_scope_shard("formal-prefix-test", "height-4")
        .unwrap();
    assert!(
        store
            .verify_pin_scope_shard("formal-prefix-test", "height-4", &expected)
            .is_err()
    );
    assert!(
        store
            .list_pin_scope_shards("formal-prefix-test")
            .unwrap()
            .is_empty(),
        "verification must not republish the missing retention receipt"
    );
    assert!(store.prune_orphan_blobs().unwrap() > 0);
    assert!(!store.has(&keep).unwrap());
}

#[test]
fn pin_verification_rejects_oversized_and_malformed_retention_receipts() {
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let keep = store.put_bytes(b"pin material").unwrap();
    let expected = BTreeSet::from([keep]);
    store
        .replace_pin_scope_shard("formal-prefix-test", "height-5", &expected)
        .unwrap();
    let path = store
        .pin_scope_shard_path("formal-prefix-test", "height-5")
        .unwrap();
    for bytes in [b"{}".to_vec(), b"{broken".to_vec(), vec![b' '; 4096]] {
        fs::write(&path, bytes).unwrap();
        assert!(
            store
                .verify_pin_scope_shard("formal-prefix-test", "height-5", &expected)
                .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn pin_verification_rejects_symlink_and_shared_file_carriers() {
    use std::os::unix::fs::symlink;
    let scratch = Scratch::new();
    let store = LocalCasStore::new(&scratch.0);
    let keep = store.put_bytes(b"pin material").unwrap();
    let expected = BTreeSet::from([keep]);
    store
        .replace_pin_scope_shard("formal-prefix-test", "height-6", &expected)
        .unwrap();
    let path = store
        .pin_scope_shard_path("formal-prefix-test", "height-6")
        .unwrap();
    let shared = scratch.0.join("shared-pin.json");
    fs::rename(&path, &shared).unwrap();
    symlink(&shared, &path).unwrap();
    assert!(
        store
            .verify_pin_scope_shard("formal-prefix-test", "height-6", &expected)
            .is_err()
    );
    fs::remove_file(&path).unwrap();
    fs::hard_link(&shared, &path).unwrap();
    assert!(
        store
            .verify_pin_scope_shard("formal-prefix-test", "height-6", &expected)
            .is_err()
    );
}
