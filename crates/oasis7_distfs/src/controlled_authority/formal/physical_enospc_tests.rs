//! Real ENOSPC on a caller-mounted, empty small tmpfs. No global fault hook.
use super::*;
use std::{mem::MaybeUninit, os::fd::AsRawFd};

const MAX_FILESYSTEM_BYTES: u64 = 128 * 1024 * 1024;
struct OwnedCase(PathBuf);
impl Drop for OwnedCase {
    fn drop(&mut self) {
        // Only the exclusive child created by this invocation is ours to remove.
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn checked_tmpfs() -> (OwnedCase, u64) {
    let supplied = std::env::var_os("OASIS7_STORAGE_ENOSPC_ROOT")
        .expect("ENOSPC prerequisite: explicitly supplied dedicated tmpfs mount root");
    let root = PathBuf::from(supplied);
    assert!(
        root.is_absolute(),
        "ENOSPC prerequisite: absolute mount path"
    );
    assert_eq!(
        root.canonicalize().unwrap(),
        root,
        "ENOSPC prerequisite: canonical path, no symlink"
    );
    let metadata = fs::symlink_metadata(&root).unwrap();
    paths::metadata(&metadata, true).expect("ENOSPC prerequisite: private owned mount root");
    assert_ne!(
        metadata.dev(),
        fs::metadata(root.parent().expect("not root filesystem"))
            .unwrap()
            .dev(),
        "ENOSPC prerequisite: dedicated mount point, not an ordinary host directory"
    );
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(&root)
        .unwrap();
    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: directory is a live descriptor and stat points to writable statfs storage.
    assert_eq!(
        unsafe { libc::fstatfs(directory.as_raw_fd(), stat.as_mut_ptr()) },
        0
    );
    // SAFETY: successful fstatfs initialized every field.
    let stat = unsafe { stat.assume_init() };
    assert_eq!(
        stat.f_type,
        libc::TMPFS_MAGIC,
        "ENOSPC prerequisite: Linux tmpfs only"
    );
    let capacity = u64::try_from(stat.f_bsize)
        .unwrap()
        .checked_mul(stat.f_blocks)
        .unwrap();
    assert!(
        (1024 * 1024..=MAX_FILESYSTEM_BYTES).contains(&capacity),
        "ENOSPC prerequisite: dedicated tmpfs must be between 1 and 128 MiB"
    );
    assert!(
        fs::read_dir(&root).unwrap().next().is_none(),
        "ENOSPC prerequisite: mount must be empty and exclusive"
    );
    let child = root.join(format!("oasis7-enospc-owned-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&child).unwrap();
    eprintln!(
        "STORAGE_ENOSPC_PREREQUISITE=verified filesystem=tmpfs capacity_bytes={capacity} root={}",
        root.display()
    );
    (OwnedCase(child), capacity)
}
fn fill_to_enospc(path: &Path, capacity: u64) -> u64 {
    let mut filler = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .unwrap();
    let block = [0xa5; 65536];
    let mut written = 0u64;
    loop {
        assert!(
            written < capacity && written < MAX_FILESYSTEM_BYTES,
            "bounded filler failed to reach ENOSPC"
        );
        let remaining = usize::try_from((capacity - written).min(block.len() as u64)).unwrap();
        match filler.write(&block[..remaining]) {
            Ok(0) => panic!("filler returned WriteZero without ENOSPC"),
            Ok(bytes) => written += bytes as u64,
            Err(error) => {
                eprintln!(
                    "STORAGE_ENOSPC_ACTUAL stage=filler raw_os_error={:?} written_bytes={written} error={error}",
                    error.raw_os_error()
                );
                assert_eq!(
                    error.raw_os_error(),
                    Some(libc::ENOSPC),
                    "must observe actual filesystem ENOSPC"
                );
                break;
            }
        }
    }
    filler.sync_all().unwrap();
    written
}
fn exact(root: &Path, value: &(String, Vec<u8>)) {
    let bytes = read_object(&root.join(&value.0), &value.0, true)
        .unwrap()
        .unwrap();
    assert_eq!(blake3::hash(&bytes).to_hex().as_str(), value.0);
    assert_eq!(bytes, value.1);
}
#[test]
#[ignore = "manual Linux acceptance requires an empty dedicated <=128MiB tmpfs mount"]
fn physical_real_filesystem_enospc_preserves_published_bytes_and_retry() {
    let (case, capacity) = checked_tmpfs();
    let root = case.0.join("formal-objects-v1");
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    initialize(&root).unwrap();
    let old = super::tests::object(vec![17; 131072]);
    assert!(persist(&root, std::slice::from_ref(&old)).unwrap());
    exact(&root, &old);
    let selected = root.join(SELECTORS).join(&old.0);
    let selector_bytes = fs::read(&selected).unwrap();
    let selector_inode = fs::metadata(&selected).unwrap().ino();
    let mut batch = vec![
        super::tests::object(vec![32; 131072]),
        super::tests::object(vec![33; 131072]),
    ];
    batch.sort_by(|a, b| a.0.cmp(&b.0));
    let filler = case.0.join("bounded-filler");
    let written = fill_to_enospc(&filler, capacity);
    let error = persist(&root, &batch)
        .expect_err("new physical publication must fail on the full filesystem");
    eprintln!("STORAGE_ENOSPC_ACTUAL stage=physical_publication error={error:?}");
    match error {
        FormalError::Io(detail) => assert!(
            detail.contains(&format!("os error {}", libc::ENOSPC)),
            "publication must fail due to ENOSPC: {detail}"
        ),
        other => panic!("unexpected publication failure classification: {other:?}"),
    }
    exact(&root, &old);
    assert_eq!(fs::read(&selected).unwrap(), selector_bytes);
    assert_eq!(fs::metadata(&selected).unwrap().ino(), selector_inode);
    for (id, _) in &batch {
        assert!(
            !root.join(SELECTORS).join(id).exists(),
            "failed frame publication cannot publish a new selector"
        );
    }
    fs::remove_file(&filler).unwrap();
    sync(&case.0).unwrap();
    // Reopen and run the production recovery/validation path before retrying.
    initialize(&root).unwrap();
    exact(&root, &old);
    assert!(persist(&root, &batch).unwrap());
    for value in &batch {
        exact(&root, value);
    }
    exact(&root, &old);
    assert_eq!(fs::read(&selected).unwrap(), selector_bytes);
    eprintln!(
        "STORAGE_ENOSPC_RESULT=passed filler_bytes={written} old_selector_unchanged=true exact_retry=true scope=tmpfs_physical_publication_only"
    );
}
