use super::super::registration_replay_kernel_lock::{self, KernelLock};
use super::*;
use std::cell::RefCell;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};

const CHILD_TEST: &str =
    "auth_storage::registration_replay_lock::failure_process_tests::acquisition_failure_child";
const CASE_ENV: &str = "OASIS7_TEST_FAILED_LOCK_CASE";
const ROOT_ENV: &str = "OASIS7_TEST_FAILED_LOCK_ROOT";
const CHILD_TIMEOUT: Duration = Duration::from_secs(8);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);

thread_local! {
    static INHERITED_CHILD: RefCell<Option<InheritedChild>> = const { RefCell::new(None) };
}

fn pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [-1; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // Both descriptors were just created and are uniquely owned here.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

// The holding phase uses only async-signal-safe libc operations on pre-created
// descriptors, then _exit without unwinding Rust heap-owning fixtures or libtest.
fn hold_inherited_anchor(ready: RawFd, release: RawFd) -> ! {
    let byte = b'R';
    let mut pollfd = libc::pollfd {
        fd: release,
        events: libc::POLLIN,
        revents: 0,
    };
    let mut release_byte = 0_u8;
    let success = unsafe {
        libc::write(ready, (&byte as *const u8).cast(), 1) == 1
            && libc::poll(&mut pollfd, 1, 8_000) == 1
            && libc::read(release, (&mut release_byte as *mut u8).cast(), 1) == 0
    };
    unsafe { libc::_exit(if success { 0 } else { 71 }) }
}

struct InheritedChild {
    pid: Option<libc::pid_t>,
    release: Option<OwnedFd>,
}

impl InheritedChild {
    fn still_running(&mut self) -> io::Result<bool> {
        let Some(pid) = self.pid else {
            return Ok(false);
        };
        let mut status = 0;
        match unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) } {
            0 => Ok(true),
            waited if waited == pid => {
                self.pid = None;
                Err(io::Error::other(format!(
                    "inherited child exited before release: wait status {status}"
                )))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn reap_until(&mut self, deadline: Instant) -> io::Result<Option<i32>> {
        let Some(pid) = self.pid else {
            return Ok(Some(0));
        };
        loop {
            let mut status = 0;
            let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            if result == pid {
                self.pid = None;
                return Ok(Some(status));
            }
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::yield_now();
        }
    }

    fn finish(&mut self) -> io::Result<()> {
        // EOF releases the child without a potentially blocking pipe write.
        drop(self.release.take());
        if let Some(status) = self.reap_until(Instant::now() + REAP_TIMEOUT)? {
            return if status == 0 {
                Ok(())
            } else {
                Err(io::Error::other(format!("inherited child status {status}")))
            };
        }
        if let Some(pid) = self.pid {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let status = self.reap_until(Instant::now() + REAP_TIMEOUT)?;
        Err(io::Error::other(format!(
            "inherited child exceeded release deadline; killed/reaped: {status:?}"
        )))
    }
}

impl Drop for InheritedChild {
    fn drop(&mut self) {
        if self.pid.is_some()
            && let Err(error) = self.finish()
        {
            eprintln!("inherited child cleanup failed: {error}");
        }
    }
}

fn dead_pid_bounded() -> u32 {
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        unsafe { libc::_exit(0) };
    }
    assert!(pid > 0, "fork dead owner: {}", io::Error::last_os_error());
    let mut child = InheritedChild {
        pid: Some(pid),
        release: None,
    };
    child
        .finish()
        .expect("reap dead owner before fixture setup");
    pid as u32
}

fn fork_anchor_child(drop_in_child: Option<&mut Option<KernelLock>>) -> InheritedChild {
    let (ready_read, ready_write) = pipe().expect("create readiness pipe");
    let (release_read, release_write) = pipe().expect("create release pipe");
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        unsafe {
            libc::close(ready_read.as_raw_fd());
            libc::close(release_write.as_raw_fd());
        }
        if let Some(kernel_lock) = drop_in_child {
            // KernelLock contains only File and scalar fields. Its child Drop
            // performs the real PID check and closes the FD without heap frees.
            drop(kernel_lock.take());
        }
        hold_inherited_anchor(ready_write.as_raw_fd(), release_read.as_raw_fd());
    }
    assert!(
        pid >= 0,
        "fork inherited anchor: {}",
        io::Error::last_os_error()
    );
    let child = InheritedChild {
        pid: Some(pid),
        release: Some(release_write),
    };
    drop(ready_write);
    drop(release_read);
    let mut pollfd = libc::pollfd {
        fd: ready_read.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(
        unsafe { libc::poll(&mut pollfd, 1, 2_000) },
        1,
        "child ready deadline"
    );
    let mut byte = 0_u8;
    assert_eq!(
        unsafe { libc::read(ready_read.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) },
        1,
        "read child readiness"
    );
    assert_eq!(byte, b'R');
    child
}

fn fork_after_kernel_acquire() {
    let child = fork_anchor_child(None);
    INHERITED_CHILD.with(|slot| assert!(slot.replace(Some(child)).is_none()));
}

fn assert_held(ledger: &Path) {
    match ExclusiveDirectoryProcessLock::try_acquire(ledger) {
        Ok(_) => panic!("a second claimant must not receive write ownership"),
        Err(error) => assert_eq!(
            error,
            "registration replay ledger lock is held by a live process"
        ),
    }
}

fn pending_child_drop_preserves_parent(ledger: &Path) {
    let mut kernel_lock = Some(
        registration_replay_kernel_lock::acquire(
            &replay_lock_dir(ledger),
            Instant::now() + CHILD_TIMEOUT,
            false,
        )
        .expect("acquire pending kernel ownership"),
    );
    let mut child = fork_anchor_child(Some(&mut kernel_lock));
    assert!(child.still_running().expect("child after pending drop"));
    assert_held(ledger);
    drop(kernel_lock);
    let guard = ExclusiveDirectoryProcessLock::try_acquire(ledger)
        .expect("acquiring parent may release its pending ownership");
    assert!(child.still_running().expect("child after parent release"));
    assert_held(ledger);
    drop(guard);
    child.finish().expect("reap pending-guard child");
    eprintln!("verified pending child cannot unlock acquiring parent");
}

fn completed_guard_release(ledger: &Path, transaction: bool) {
    registration_replay_kernel_lock::AFTER_KERNEL_ACQUIRE
        .with(|hook| hook.set(Some(fork_after_kernel_acquire)));
    let acquired = if transaction {
        ExclusiveDirectoryTransactionLock::try_acquire(ledger).map(|guard| guard._guard)
    } else {
        ExclusiveDirectoryProcessLock::try_acquire(ledger).map(|guard| guard._guard)
    };
    let mut child = INHERITED_CHILD
        .with(|slot| slot.take())
        .expect("successful constructor must reach the inheritance hook");
    let guard = acquired.expect("acquire completed ownership");
    assert_held(ledger);
    drop(guard);
    assert!(!replay_lock_dir(ledger).exists());
    assert!(child.still_running().expect("child after completed drop"));
    if transaction {
        let next = ExclusiveDirectoryProcessLock::try_acquire(ledger)
            .expect("completed transaction releases despite inherited descriptor");
        assert_held(ledger);
        drop(next);
    } else {
        // A successful lifetime guard remains close-only, unlike an acquisition
        // that never returned write authority to its caller.
        assert_held(ledger);
    }
    assert!(child.still_running().expect("child after release check"));
    child.finish().expect("reap completed-guard child");
    let next = ExclusiveDirectoryProcessLock::try_acquire(ledger)
        .expect("all inherited descriptors have closed");
    drop(next);
    eprintln!("verified completed guard release: transaction={transaction}");
}

#[test]
fn acquisition_failure_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("isolated failure root"));
    let ledger = root.join("ledger");
    match case.as_str() {
        "pending-child-drop" => return pending_child_drop_preserves_parent(&ledger),
        "completed-lifetime" => return completed_guard_release(&ledger, false),
        "completed-transaction" => return completed_guard_release(&ledger, true),
        _ => {}
    }
    let expected_error = match case.as_str() {
        "anchor-sync" => {
            super::super::registration_replay_kernel_lock::FAIL_NEXT_ANCHOR_SYNC
                .store(true, Ordering::SeqCst);
            "injected kernel lock sync failure"
        }
        "owner-write" => {
            FAIL_NEXT_OWNER_WRITE.store(true, Ordering::SeqCst);
            "injected owner persistence failure"
        }
        "parent-sync" => {
            FAIL_NEXT_LOCK_PARENT_SYNC.store(true, Ordering::SeqCst);
            "sync registration replay ledger lock parent failed: injected registration replay lock parent sync failure"
        }
        "quarantine-remove" => {
            let lock = replay_lock_dir(&ledger);
            fs::create_dir(&lock).unwrap();
            fs::write(
                lock.join("owner.json"),
                serde_json::to_vec(&RegistrationReplayLockOwner {
                    pid: dead_pid_bounded(),
                    token: "dead".into(),
                })
                .unwrap(),
            )
            .unwrap();
            FAIL_NEXT_QUARANTINE_REMOVE.store(true, Ordering::SeqCst);
            "injected quarantine cleanup failure"
        }
        _ => panic!("unknown acquisition failure case {case}"),
    };
    super::super::registration_replay_kernel_lock::AFTER_KERNEL_ACQUIRE
        .with(|hook| hook.set(Some(fork_after_kernel_acquire)));
    let acquired = ExclusiveDirectoryProcessLock::try_acquire(&ledger);
    let mut child = INHERITED_CHILD
        .with(|slot| slot.take())
        .expect("constructor must reach the post-kernel-acquire hook");
    let error = match acquired {
        Ok(_) => panic!("{case} injection unexpectedly granted ownership"),
        Err(error) => error,
    };
    assert_eq!(
        error, expected_error,
        "must reach the intended failure branch"
    );
    assert!(!replay_lock_dir(&ledger).exists());
    eprintln!("verified injected failure: {case}: {error}");
    assert!(child.still_running().expect("inherited child before retry"));
    let retry = ExclusiveDirectoryProcessLock::try_acquire(&ledger);
    assert!(child.still_running().expect("inherited child after retry"));
    let guard = retry.unwrap_or_else(|error| {
        panic!(
            "failed {case} acquisition must release kernel ownership while inherited child is live: {error}"
        )
    });
    assert_held(&ledger);
    drop(guard);
    child.finish().expect("release and reap inherited child");
}

fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::yield_now();
    }
}

struct IsolatedChild(Child);

impl Drop for IsolatedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        // The helper has its own process group, including its raw-fork child.
        unsafe { libc::kill(-(self.0.id() as libc::pid_t), libc::SIGKILL) };
        if !matches!(
            wait_until(&mut self.0, Instant::now() + REAP_TIMEOUT),
            Ok(Some(_))
        ) {
            eprintln!("isolated failure helper could not be reaped before deadline");
        }
    }
}

fn isolated_case(case: &str, expected_evidence: &str) {
    let root = std::env::temp_dir().join(format!("oasis7-lock-inherited-{}", new_lock_token()));
    fs::create_dir(&root).unwrap();
    let log = root.join("helper.log");
    let output = fs::File::create(&log).unwrap();
    let mut child = IsolatedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
            .env(CASE_ENV, case)
            .env(ROOT_ENV, &root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output))
            .process_group(0)
            .spawn()
            .expect("spawn isolated constructor regression"),
    );
    let status = wait_until(&mut child.0, Instant::now() + CHILD_TIMEOUT);
    drop(child);
    let evidence = fs::read_to_string(&log).unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        matches!(status, Ok(Some(status)) if status.success()),
        "isolated {case} constructor regression failed: {status:?}\n{evidence}"
    );
    assert!(evidence.contains("running 1 test"), "{evidence}");
    assert!(evidence.contains(expected_evidence), "{evidence}");
}

fn isolated_failure(case: &str) {
    isolated_case(case, "verified injected failure:");
}

#[test]
fn failed_anchor_sync_releases_inherited_kernel_lock() {
    isolated_failure("anchor-sync");
}

#[test]
fn failed_owner_write_releases_inherited_kernel_lock() {
    isolated_failure("owner-write");
}

#[test]
fn failed_quarantine_cleanup_releases_inherited_kernel_lock() {
    isolated_failure("quarantine-remove");
}

#[test]
fn failed_parent_sync_releases_inherited_kernel_lock() {
    isolated_failure("parent-sync");
}

#[test]
fn pending_kernel_child_drop_cannot_unlock_acquiring_parent() {
    isolated_case("pending-child-drop", "verified pending child");
}

#[test]
fn completed_lifetime_guard_stays_locked_until_inherited_descriptor_closes() {
    isolated_case("completed-lifetime", "verified completed guard release:");
}

#[test]
fn completed_transaction_releases_before_inherited_descriptor_closes() {
    isolated_case("completed-transaction", "verified completed guard release:");
}
