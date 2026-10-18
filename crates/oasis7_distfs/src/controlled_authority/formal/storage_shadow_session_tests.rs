use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    source: PathBuf,
    target: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "o7-shadow-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source = root.join("source");
        fs::create_dir_all(source.join("formal-objects-v1")).unwrap();
        File::create(source.join(".formal-endpoint.lock")).unwrap();
        for byte in 0..8u8 {
            let bytes = vec![byte; 10000];
            fs::write(
                source
                    .join("formal-objects-v1")
                    .join(blake3::hash(&bytes).to_hex().as_str()),
                bytes,
            )
            .unwrap();
        }
        let target = root.join("shadow");
        Self {
            root,
            source,
            target,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn live_shadow_session_recovers_once_fences_other_writers_and_recovers_after_error_or_reopen() {
    let fixture = Fixture::new();
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let mut session = FormalStorageShadowSession::default();
    super::super::physical::publication_recovery_count_reset();
    for _ in 0..2 {
        assert!(
            !advance_formal_storage_shadow_with_session(
                &fixture.source,
                &fixture.target,
                &options,
                &mut session
            )
            .unwrap()
            .complete
        );
    }
    assert_eq!(super::super::physical::publication_recovery_count(), 1);
    let error =
        advance_formal_storage_shadow(&fixture.source, &fixture.target, &options).unwrap_err();
    assert!(
        format!("{error:?}").contains("shadow destination busy"),
        "live owner must fence even a stateless competing writer"
    );
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 1);
    let invalid_options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 0,
    };
    assert!(
        advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &invalid_options,
            &mut session
        )
        .is_err()
    );
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 2);
    drop(session);
    let mut reopened = FormalStorageShadowSession::default();
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut reopened
        )
        .unwrap()
        .complete
    );
    assert_eq!(super::super::physical::publication_recovery_count(), 3);
}

fn copy_private_tree(source: &std::path::Path, target: &std::path::Path) {
    #[cfg(unix)]
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let output = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.is_dir() {
            copy_private_tree(&path, &output);
        } else {
            assert!(metadata.is_file());
            fs::copy(path, output).unwrap();
        }
    }
}
fn tree_bytes(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        directory: &std::path::Path,
        output: &mut Vec<(PathBuf, Vec<u8>)>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if fs::symlink_metadata(&path).unwrap().is_dir() {
                output.push((relative, Vec::new()));
                walk(root, &path, output);
            } else {
                output.push((relative, fs::read(path).unwrap()));
            }
        }
    }
    let mut output = Vec::new();
    walk(root, root, &mut output);
    output.sort();
    output
}
#[test]
fn live_shadow_session_rejects_replaced_destination_without_writing_the_replacement() {
    let fixture = Fixture::new();
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let mut session = FormalStorageShadowSession::default();
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    let moved = fixture.root.join("original-shadow-moved");
    fs::rename(&fixture.target, &moved).unwrap();
    copy_private_tree(&moved, &fixture.target);
    let replacement_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.target.join(".shadow-progress-v1/.owner-lock-v1"))
        .unwrap();
    replacement_lock.try_lock().unwrap();
    let before = tree_bytes(&fixture.target);
    let error = advance_formal_storage_shadow_with_session(
        &fixture.source,
        &fixture.target,
        &options,
        &mut session,
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("identity changed"));
    assert_eq!(
        tree_bytes(&fixture.target),
        before,
        "old lease must not mutate a same-named new directory"
    );
    // The rejected session reset must release its original inode lease as well.
    let original_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(moved.join(".shadow-progress-v1/.owner-lock-v1"))
        .unwrap();
    original_lock.try_lock().unwrap();
    File::unlock(&original_lock).unwrap();
    File::unlock(&replacement_lock).unwrap();
}
#[test]
fn live_shadow_session_rejects_replaced_owner_lock_without_writing_the_destination() {
    let fixture = Fixture::new();
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let mut session = FormalStorageShadowSession::default();
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    let lock_path = fixture.target.join(".shadow-progress-v1/.owner-lock-v1");
    let moved_lock = fixture
        .target
        .join(".shadow-progress-v1/original-owner-lock");
    fs::rename(&lock_path, &moved_lock).unwrap();
    let replacement_lock = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&lock_path)
        .unwrap();
    replacement_lock.try_lock().unwrap();
    let before = tree_bytes(&fixture.target);
    let error = advance_formal_storage_shadow_with_session(
        &fixture.source,
        &fixture.target,
        &options,
        &mut session,
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("identity changed"));
    assert_eq!(
        tree_bytes(&fixture.target),
        before,
        "a replaced stable lock cannot transfer the previous lease"
    );
    let original_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(moved_lock)
        .unwrap();
    original_lock.try_lock().unwrap();
    File::unlock(&original_lock).unwrap();
    File::unlock(&replacement_lock).unwrap();
}

#[test]
fn live_shadow_session_rejects_missing_renamed_destination_without_recreating_it() {
    let fixture = Fixture::new();
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let mut session = FormalStorageShadowSession::default();
    assert!(
        !advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .unwrap()
        .complete
    );
    let moved = fixture.root.join("original-shadow-moved");
    fs::rename(&fixture.target, &moved).unwrap();
    let before = tree_bytes(&moved);
    assert!(
        advance_formal_storage_shadow_with_session(
            &fixture.source,
            &fixture.target,
            &options,
            &mut session
        )
        .is_err()
    );
    assert!(
        !fixture.target.exists(),
        "a vanished destination cannot implicitly start a new tree under the old session"
    );
    assert_eq!(tree_bytes(&moved), before);
    let original_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(moved.join(".shadow-progress-v1/.owner-lock-v1"))
        .unwrap();
    original_lock.try_lock().unwrap();
    File::unlock(&original_lock).unwrap();
}
