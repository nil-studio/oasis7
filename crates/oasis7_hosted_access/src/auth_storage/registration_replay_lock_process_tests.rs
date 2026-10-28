use super::*;
use std::process::{Child, Command, Stdio};

pub(super) fn dead_pid() -> u32 {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--list")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    assert!(child.wait().unwrap().success());
    pid
}

fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !path.exists() {
        assert!(Instant::now() < deadline, "waiting for {}", path.display());
        std::thread::sleep(LOCK_RETRY_INTERVAL);
    }
}

fn spawn(root: &Path, ledger: &Path) -> Child {
    spawn_mode(root, ledger, false)
}

fn spawn_mode(root: &Path, ledger: &Path, fork_drop: bool) -> Child {
    spawn_mode_kind(root, ledger, fork_drop, false)
}

fn spawn_mode_kind(root: &Path, ledger: &Path, fork_drop: bool, transaction: bool) -> Child {
    fs::create_dir_all(root).unwrap();
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "auth_storage::registration_replay_lock::process_tests::lock_child",
            "--nocapture",
        ])
        .env("OASIS7_TEST_STALE_BARRIER", root)
        .env("OASIS7_TEST_LOCK_LEDGER", ledger)
        .env(
            "OASIS7_TEST_LOCK_TRANSACTION",
            if transaction { "1" } else { "0" },
        )
        .env(
            "OASIS7_TEST_LOCK_FORK_DROP",
            if fork_drop { "1" } else { "0" },
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn lock_child() {
    let Some(root) = std::env::var_os("OASIS7_TEST_STALE_BARRIER").map(PathBuf::from) else {
        return;
    };
    let ledger = PathBuf::from(std::env::var_os("OASIS7_TEST_LOCK_LEDGER").unwrap());
    let acquired = if std::env::var("OASIS7_TEST_LOCK_TRANSACTION").as_deref() == Ok("1") {
        ExclusiveDirectoryTransactionLock::try_acquire(&ledger).map(|guard| guard._guard)
    } else {
        ExclusiveDirectoryProcessLock::try_acquire(&ledger).map(|guard| guard._guard)
    };
    match acquired {
        Ok(guard) => {
            #[cfg(unix)]
            if std::env::var("OASIS7_TEST_LOCK_FORK_DROP").unwrap() == "1" {
                // Child branch only closes the inherited guard then _exit: no
                // allocation, filesystem cleanup or test-runner teardown after fork.
                let pid = unsafe { libc::fork() };
                assert!(pid >= 0);
                if pid == 0 {
                    drop(guard);
                    unsafe { libc::_exit(0) };
                }
                let mut status = 0;
                assert_eq!(unsafe { libc::waitpid(pid, &mut status, 0) }, pid);
                assert_eq!(status, 0);
            }
            fs::write(root.join("acquired"), b"yes").unwrap();
            wait_file(&root.join("finish"));
            drop(guard);
        }
        Err(error) => {
            assert!(
                error.contains("held"),
                "unexpected acquisition failure: {error}"
            );
            fs::write(root.join("blocked"), error).unwrap();
        }
    }
    fs::write(root.join("done"), b"yes").unwrap();
}

#[cfg(unix)]
#[test]
fn transaction_child_drop_cannot_release_the_acquiring_parent() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-transaction-fork-{}", new_lock_token()));
    let ledger = root.join("world");
    let control = root.join("control");
    let mut parent = spawn_mode_kind(&control, &ledger, true, true);
    // The spawned process has already forked, dropped the inherited guard in
    // that child and reaped it before acknowledging its own live ownership.
    wait_file(&control.join("acquired"));
    let still_exclusive = ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err();
    let transaction_exclusive = ExclusiveDirectoryTransactionLock::try_acquire(&ledger).is_err();
    fs::write(control.join("finish"), b"go").unwrap();
    assert!(parent.wait().unwrap().success());
    let next = ExclusiveDirectoryTransactionLock::try_acquire(&ledger).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    drop(next);
    assert!(ledger.with_extension("lock.guard").is_file());
    fs::remove_dir_all(root).unwrap();
    assert!(still_exclusive && transaction_exclusive);
}

#[test]
fn two_stale_reclaimers_cannot_displace_a_live_guard() {
    let root = std::env::temp_dir().join(format!("oasis7-stale-race-{}", new_lock_token()));
    let ledger = root.join("ledger");
    let lock = replay_lock_dir(&ledger);
    fs::create_dir_all(&lock).unwrap();
    fs::write(
        lock.join("owner.json"),
        serde_json::to_vec(&RegistrationReplayLockOwner {
            pid: dead_pid(),
            token: "dead".into(),
        })
        .unwrap(),
    )
    .unwrap();
    let a = root.join("a");
    let b = root.join("b");
    let mut first = spawn(&a, &ledger);
    wait_file(&a.join("observed"));
    let mut second = spawn(&b, &ledger);
    let deadline = Instant::now() + Duration::from_secs(8);
    while !b.join("observed").exists() && !b.join("blocked").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(LOCK_RETRY_INTERVAL);
    }
    fs::write(a.join("release"), b"go").unwrap();
    wait_file(&a.join("acquired"));
    fs::write(b.join("release"), b"go").unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !b.join("acquired").exists() && !b.join("blocked").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(LOCK_RETRY_INTERVAL);
    }
    let overlap = b.join("acquired").exists();
    fs::write(a.join("finish"), b"go").unwrap();
    fs::write(b.join("finish"), b"go").unwrap();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    fs::remove_dir_all(root).unwrap();
    assert!(
        !overlap,
        "second stale reclaimer displaced the new live owner"
    );
}

#[test]
fn process_exit_releases_kernel_lock_and_recovers_original_target() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-exit-{}", new_lock_token()));
    let ledger = root.join("world");
    let a = root.join("a");
    let mut child = spawn_mode(&a, &ledger, true);
    wait_file(&a.join("acquired"));
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(replay_lock_dir(&ledger).exists());
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&ledger).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    drop(guard);
    assert!(root.join("world.lock.guard").is_file());
    assert!(!replay_lock_dir(&ledger).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_owners_refuse_acquisition_without_mutation() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-unknown-{}", new_lock_token()));
    let ledger = root.join("ledger");
    let lock = replay_lock_dir(&ledger);
    fs::create_dir_all(&lock).unwrap();
    for bytes in [
        b"".as_slice(),
        b"invalid",
        br#"{"pid":0,"token":"unknown"}"#,
        br#"{"pid":4294967295,"token":"unknown"}"#,
    ] {
        fs::write(lock.join("owner.json"), bytes).unwrap();
        assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
        assert_eq!(fs::read(lock.join("owner.json")).unwrap(), bytes);
    }
    fs::remove_file(lock.join("owner.json")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn anchor_and_owner_aliases_and_non_private_anchor_are_refused() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = std::env::temp_dir().join(format!("oasis7-lock-alias-{}", new_lock_token()));
    fs::create_dir_all(&root).unwrap();
    let ledger = root.join("ledger");
    let anchor = root.join("ledger.lock.guard");
    let target = root.join("target");
    fs::write(&target, b"unchanged").unwrap();
    symlink(&target, &anchor).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    fs::remove_file(&anchor).unwrap();
    fs::hard_link(&target, &anchor).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    fs::remove_file(&anchor).unwrap();
    fs::write(&anchor, b"").unwrap();
    fs::set_permissions(&anchor, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    fs::set_permissions(&anchor, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&root, replay_lock_dir(&ledger)).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    fs::remove_file(replay_lock_dir(&ledger)).unwrap();
    fs::create_dir(replay_lock_dir(&ledger)).unwrap();
    symlink(&target, replay_lock_dir(&ledger).join("owner.json")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn owner_publication_and_quarantine_failures_never_grant_write_ownership() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-fault-{}", new_lock_token()));
    let ledger = root.join("ledger");
    super::super::registration_replay_kernel_lock::FAIL_NEXT_ANCHOR_SYNC
        .store(true, Ordering::SeqCst);
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert!(!replay_lock_dir(&ledger).exists());
    assert!(!ledger.exists());
    FAIL_NEXT_OWNER_WRITE.store(true, Ordering::SeqCst);
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert!(!replay_lock_dir(&ledger).exists());
    assert_eq!(
        fs::read_dir(&root).unwrap().count(),
        1,
        "only permanent anchor remains"
    );
    fs::create_dir(replay_lock_dir(&ledger)).unwrap();
    fs::write(
        replay_lock_dir(&ledger).join("owner.json"),
        serde_json::to_vec(&RegistrationReplayLockOwner {
            pid: dead_pid(),
            token: "dead".into(),
        })
        .unwrap(),
    )
    .unwrap();
    FAIL_NEXT_QUARANTINE_REMOVE.store(true, Ordering::SeqCst);
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert!(!replay_lock_dir(&ledger).exists());
    let quarantine = fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.file_type().unwrap().is_dir())
        .unwrap()
        .path();
    assert!(
        quarantine.join("owner.json").is_file(),
        "failed cleanup retains old evidence"
    );
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&ledger).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ordinary_path_spellings_share_one_write_owner() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-path-{}", new_lock_token()));
    let world = root.join("world");
    fs::create_dir_all(root.join("other")).unwrap();
    fs::create_dir(&world).unwrap();
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&world).unwrap();
    for alias in [world.join(""), world.join("."), root.join("other/../world")] {
        assert!(
            ExclusiveDirectoryProcessLock::try_acquire(&alias).is_err(),
            "alias {} must not receive a second guard",
            alias.display()
        );
    }
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn parent_aliases_share_anchor_and_target_aliases_are_refused() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    use std::os::unix::fs::symlink;
    let root = std::env::temp_dir().join(format!("oasis7-lock-target-{}", new_lock_token()));
    fs::create_dir_all(root.join("data")).unwrap();
    let ledger = root.join("data/ledger");
    fs::write(&ledger, b"original").unwrap();
    symlink(root.join("data"), root.join("alias")).unwrap();
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&ledger).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&root.join("alias/ledger")).is_err());
    drop(guard);
    symlink(&ledger, root.join("ledger-symlink")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&root.join("ledger-symlink")).is_err());
    fs::hard_link(&ledger, root.join("ledger-hardlink")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&root.join("ledger-hardlink")).is_err());
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert_eq!(fs::read(&ledger).unwrap(), b"original");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_ambiguous_target_spellings_are_refused() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-winpath-{}", new_lock_token()));
    fs::create_dir_all(&root).unwrap();
    let world = root.join("world");
    fs::create_dir(&world).unwrap();
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&world).unwrap();
    for name in [
        "world.",
        "world ",
        "world:stream",
        "world./child",
        "NUL",
        "NUL.txt",
        "CON",
        "conin$",
        "PRN.log",
        "AUX",
        "COM1",
        "LPT9.txt",
        "COM¹",
    ] {
        assert!(ExclusiveDirectoryProcessLock::try_acquire(&root.join(name)).is_err());
    }
    drop(guard);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_hardlinked_targets_and_anchors_are_refused() {
    let _test_guard = super::tests::TEST_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("oasis7-lock-winlink-{}", new_lock_token()));
    fs::create_dir_all(&root).unwrap();
    let ledger = root.join("ledger");
    fs::write(&ledger, b"original").unwrap();
    fs::hard_link(&ledger, root.join("alias")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&root.join("alias")).is_err());
    fs::remove_file(root.join("alias")).unwrap();
    let guard = ExclusiveDirectoryProcessLock::try_acquire(&ledger).unwrap();
    drop(guard);
    fs::hard_link(root.join("ledger.lock.guard"), root.join("anchor-alias")).unwrap();
    assert!(ExclusiveDirectoryProcessLock::try_acquire(&ledger).is_err());
    assert_eq!(fs::read(&ledger).unwrap(), b"original");
    fs::remove_dir_all(root).unwrap();
}
