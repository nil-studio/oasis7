//! Actual public-material archive I/O only. These tests establish neither
//! protocol authority, protected custody nor a canonical World commit.
use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "oasis7-successor-public-io-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn store(&self) -> LocalBftSuccessorArchiveStoreV1 {
        LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(self.path().join("cas"))).unwrap()
    }
    fn blob(&self, reference: &BftSuccessorWindowArchiveRefV1) -> PathBuf {
        LocalCasStore::new(self.path().join("cas"))
            .blobs_dir()
            .join(format!("{}.blob", reference.digest))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn typed_raw_objects_keep_caller_limits_and_fresh_reopen_without_state_authority() {
    let root = Scratch::new();
    let bytes = vec![b'p'; 8192];
    let reference = root.store().persist_object(&bytes, bytes.len()).unwrap();
    let reopened = root.store();
    assert_eq!(
        reopened.load_object(&reference, bytes.len()).unwrap(),
        bytes
    );
    assert!(reopened.load_object(&reference, bytes.len() - 1).is_err());
    assert!(reopened.load_public_state(&reference).is_err());
    assert!(reopened.persist_object(b"payload", 0).is_err());
    assert!(
        reopened
            .persist_object(b"payload", 512 * 1024 * 1024 + 1)
            .is_err()
    );
    assert_eq!(
        reopened.load_object(&reference, bytes.len()).unwrap(),
        bytes
    );
    let blob = root.blob(&reference);
    let mut corrupted = fs::read(&blob).unwrap();
    let last = corrupted.last_mut().unwrap();
    *last ^= 0xff;
    fs::write(&blob, corrupted).unwrap();
    assert!(reopened.load_object(&reference, bytes.len()).is_err());
}

#[test]
fn independent_reopen_keeps_exact_original_bytes_and_duplicate_publication() {
    let root = Scratch::new();
    // Exercise both raw and compressed CAS disk formats through the adapter,
    // without depending on a particular compressed representation or length.
    for original in [b"original public material".to_vec(), vec![b'a'; 64 * 1024]] {
        let reference = {
            let store = root.store();
            let reference = store.persist_bytes(&original).unwrap();
            assert_eq!(reference.digest, blake3::hash(&original).to_hex().as_str());
            assert_eq!(reference.bytes, original.len() as u64);
            reference
        };
        let disk_before = fs::read(root.blob(&reference)).unwrap();
        let reopened = root.store();
        assert_eq!(reopened.load_bytes(&reference).unwrap(), original);
        assert_eq!(reopened.persist_bytes(&original).unwrap(), reference);
        assert_eq!(
            fs::read(root.blob(&reference)).unwrap(),
            disk_before,
            "same-reference publication must not replace the original immutable object"
        );
    }
}

#[test]
fn corrupt_existing_hash_path_is_not_accepted_or_silently_repaired_by_put() {
    let root = Scratch::new();
    let original = b"exact original public bytes";
    let store = root.store();
    let reference = store.persist_bytes(original).unwrap();
    let path = root.blob(&reference);
    let corrupt = vec![b'x'; original.len()];
    fs::write(&path, &corrupt).unwrap();
    assert!(store.load_bytes(&reference).is_err());
    assert!(
        root.store().persist_bytes(original).is_err(),
        "CAS put skips an existing path, so preparation needs an actual verified reread"
    );
    assert_eq!(
        fs::read(path).unwrap(),
        corrupt,
        "rejected persistence must not conceal corruption by overwriting it"
    );
}

#[test]
fn compressed_existing_object_is_reread_after_its_payload_changes() {
    let root = Scratch::new();
    let original = vec![b'z'; 64 * 1024];
    let store = root.store();
    let reference = store.persist_bytes(&original).unwrap();
    // Read through CAS first: this deliberately warms the existing range cache.
    // A later cache hit cannot stand in for fresh checkpoint-preparation I/O.
    assert_eq!(
        store
            .store
            .get_range(&reference.digest, 0, original.len())
            .unwrap(),
        (original.clone(), true)
    );
    let path = root.blob(&reference);
    let mut corrupt = fs::read(&path).unwrap();
    assert!(
        corrupt.starts_with(b"O7CBLOB1"),
        "fixture must genuinely enter compressed CAS storage"
    );
    assert!(
        corrupt.len() > 80,
        "the compressed object must have payload beyond its fixed header"
    );
    // Preserve exact file length and the CAS header, but make the encoded
    // payload deterministically cease to be a zstd frame.
    corrupt[80..].fill(0);
    fs::write(&path, &corrupt).unwrap();
    assert!(
        root.store().persist_bytes(&original).is_err(),
        "same hash/header plus a stale decompression cache must not authenticate changed disk bytes"
    );
    assert!(store.load_bytes(&reference).is_err());
    assert_eq!(fs::read(path).unwrap(), corrupt);
}

#[test]
fn truncation_and_forged_reference_lengths_fail_closed() {
    let root = Scratch::new();
    let store = root.store();
    let original = b"bounded original material";
    let reference = store.persist_bytes(original).unwrap();
    let mut forged = reference.clone();
    forged.bytes += 1;
    assert!(store.load_bytes(&forged).is_err());
    forged.bytes = reference.bytes - 1;
    assert!(store.load_bytes(&forged).is_err());
    forged.bytes = 0;
    assert!(store.load_bytes(&forged).is_err());
    forged.bytes = BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64 + 1;
    assert!(
        store.load_bytes(&forged).is_err(),
        "committed ref is bounded before reading an object"
    );
    fs::OpenOptions::new()
        .write(true)
        .open(root.blob(&reference))
        .unwrap()
        .set_len(reference.bytes - 1)
        .unwrap();
    assert!(root.store().load_bytes(&reference).is_err());
    assert!(root.store().persist_bytes(original).is_err());
}

#[cfg(unix)]
#[test]
fn enormous_sparse_blob_does_not_determine_the_read_allocation() {
    use std::os::unix::fs::MetadataExt;
    let root = Scratch::new();
    let store = root.store();
    let reference = store
        .persist_bytes(b"small authenticated reference")
        .unwrap();
    let path = root.blob(&reference);
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let sparse_len = 8_u64 * 1024 * 1024 * 1024;
    file.set_len(sparse_len).unwrap();
    drop(file);
    let metadata = fs::metadata(&path).unwrap();
    assert_eq!(metadata.len(), sparse_len);
    assert!(
        metadata.blocks().saturating_mul(512) < 1024 * 1024,
        "the hostile disk-length fixture must remain sparse rather than occupy the cache disk"
    );
    // The committed ref is tiny. get_range/decode must cap its read by that ref,
    // detect the extra bytes, and reject instead of allocating disk-length RAM.
    assert!(root.store().load_bytes(&reference).is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), sparse_len);
}

#[test]
fn archive_configuration_rejects_sha256_and_parent_traversal_without_publication() {
    let root = Scratch::new();
    assert!(
        LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new_with_hash_algorithm(
            root.path().join("sha256"),
            HashAlgorithm::Sha256
        ))
        .is_err()
    );
    assert!(!root.path().join("sha256").exists());
    assert!(
        LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(
            root.path().join("unused").join("..").join("escape")
        ))
        .is_err()
    );
    assert!(!root.path().join("unused").exists());
    assert!(!root.path().join("escape").exists());
    let store = root.store();
    let invalid = BftSuccessorWindowArchiveRefV1 {
        digest: "../outside".into(),
        bytes: 1,
    };
    assert!(store.load_bytes(&invalid).is_err());
}

#[cfg(unix)]
#[test]
fn a_replaced_blobs_directory_cannot_alias_the_live_store_after_a_warm_read() {
    use std::os::unix::fs::symlink;
    let live = Scratch::new();
    let archive = Scratch::new();
    let original = b"same content does not prove an independent archive";
    let live_store = live.store();
    let reference = live_store.persist_bytes(original).unwrap();
    let archived = archive.store();
    assert_eq!(archived.persist_bytes(original).unwrap(), reference);
    assert_eq!(archived.load_bytes(&reference).unwrap(), original);
    let live_blob = live.blob(&reference);
    let live_before = fs::read(&live_blob).unwrap();
    let blob_directory = archived.store.blobs_dir().to_path_buf();
    fs::rename(
        &blob_directory,
        archive.path().join("retained-original-blobs"),
    )
    .unwrap();
    symlink(live_store.store.blobs_dir(), &blob_directory).unwrap();
    assert!(
        archived.load_bytes(&reference).is_err(),
        "matching bytes through a parent-directory alias are not an independent archive"
    );
    assert!(archived.persist_bytes(original).is_err());
    assert_eq!(fs::read(live_blob).unwrap(), live_before);
}

#[cfg(unix)]
#[test]
fn blob_symlink_is_rejected_on_load_and_persist_without_modifying_target() {
    use std::os::unix::fs::symlink;
    let root = Scratch::new();
    let target_root = Scratch::new();
    let original = b"original public symlink target";
    let reference = root.store().persist_bytes(original).unwrap();
    let blob = root.blob(&reference);
    let target = target_root.path().join("target");
    fs::write(&target, original).unwrap();
    fs::remove_file(&blob).unwrap();
    symlink(&target, &blob).unwrap();
    assert!(
        root.store().load_bytes(&reference).is_err(),
        "matching target bytes must not make a symlink acceptable archive storage"
    );
    assert!(root.store().persist_bytes(original).is_err());
    assert_eq!(fs::read(&target).unwrap(), original);
    assert!(
        fs::symlink_metadata(&blob)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
