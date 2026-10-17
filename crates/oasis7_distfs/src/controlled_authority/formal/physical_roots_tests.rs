use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-retention-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&path).unwrap();
        initialize(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn object(root: &Path, n: u64) -> String {
    let bytes = n.to_le_bytes();
    let id = blake3::hash(&bytes).to_hex().to_string();
    create(&root.join(&id), &bytes).unwrap();
    id
}
#[test]
fn retention_segments_amortize_metadata_and_detect_old_missing_alias() {
    let temp = Temp::new();
    let ids: Vec<_> = (0..1025).map(|n| object(&temp.0, n)).collect();
    retain(&temp.0, &ids).unwrap();
    verify(&temp.0).unwrap();
    assert_eq!(
        fs::read_dir(temp.0.join(ROOTS)).unwrap().count(),
        3,
        "1025 registrations use two segments plus one head"
    );
    assert_eq!(
        fs::metadata(segment(&temp.0, 0)).unwrap().len(),
        SEGMENT_BYTES as u64
    );
    let mut cursor = PageCursor::start();
    let mut observed = Vec::new();
    loop {
        let (page_ids, next, complete) = page(&temp.0, &cursor, 1000).unwrap();
        observed.extend(page_ids);
        cursor = next;
        if complete {
            break;
        }
    }
    assert_eq!(observed, ids);
    assert_eq!(count(&temp.0).unwrap(), 1025);
    let extra = object(&temp.0, 1025);
    retain(&temp.0, std::slice::from_ref(&extra)).unwrap();
    let (overlay, next, complete) = page(&temp.0, &cursor, 1).unwrap();
    assert_eq!(overlay, vec![extra]);
    assert!(complete);
    assert_eq!(next.count, 1026);
    let mut forged = cursor.clone();
    forged.checksum = "0".repeat(64);
    assert!(page(&temp.0, &forged, 1).is_err());
    fs::remove_file(temp.0.join(&ids[0])).unwrap();
    assert!(
        verify(&temp.0).is_err(),
        "old aliases remain protected after segment rollover"
    );
}
#[test]
fn retention_reconciles_only_unpublished_suffix_and_blocks_prefix_loss() {
    let temp = Temp::new();
    let a = object(&temp.0, 1);
    let b = object(&temp.0, 2);
    retain(&temp.0, &[a]).unwrap();
    let path = segment(&temp.0, 0);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[9u8; 64])
        .unwrap();
    // The head still commits only the first record, so this is interrupted
    // unselected append data, not a new root or a reason to truncate prefix.
    verify(&temp.0).unwrap();
    retain(&temp.0, &[b]).unwrap();
    verify(&temp.0).unwrap();
    assert_eq!(head(&temp.0).unwrap().count, 2);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(63)
        .unwrap();
    assert!(verify(&temp.0).is_err());
    let c = object(&temp.0, 3);
    assert!(retain(&temp.0, &[c]).is_err());
}
#[test]
fn retention_unknown_metadata_and_checksum_corruption_fail_closed() {
    let temp = Temp::new();
    let a = object(&temp.0, 9);
    retain(&temp.0, &[a]).unwrap();
    create(&temp.0.join(ROOTS).join("unknown"), b"?").unwrap();
    assert!(verify(&temp.0).is_err());
    fs::remove_file(temp.0.join(ROOTS).join("unknown")).unwrap();
    let path = segment(&temp.0, 0);
    let mut bytes = read(&path, SEGMENT_BYTES).unwrap();
    bytes[0] ^= 1;
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    assert!(verify(&temp.0).is_err());
}

#[test]
fn retention_initialization_recovers_only_unselected_temp_head() {
    let temp = Temp::new();
    fs::remove_file(temp.0.join(ROOTS).join(HEAD)).unwrap();
    create(
        &temp.0.join(ROOTS).join(format!(".head-{}", "1".repeat(64))),
        b"partial",
    )
    .unwrap();
    initialize(&temp.0).unwrap();
    verify(&temp.0).unwrap();
    let id = object(&temp.0, 5);
    retain(&temp.0, &[id]).unwrap();
    fs::remove_file(temp.0.join(ROOTS).join(HEAD)).unwrap();
    assert!(
        initialize(&temp.0).is_err(),
        "a lost head cannot erase registered history"
    );
}
