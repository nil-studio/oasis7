use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::atomic_file::sync_parent_directory;

const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(5);
#[cfg(test)]
const INCOMPLETE_LOCK_GRACE: Duration = Duration::from_secs(1);
static LOCK_TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(test)]
static FAIL_NEXT_LOCK_PARENT_SYNC: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static FAIL_NEXT_OWNER_WRITE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static FAIL_NEXT_QUARANTINE_REMOVE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static PAUSE_NEXT_OWNER_PUBLICATION: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static OWNER_PUBLICATION_PAUSED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static RELEASE_OWNER_PUBLICATION: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Serialize, Deserialize)]
struct RegistrationReplayLockOwner {
    pid: u32,
    token: String,
}

pub(super) struct RegistrationReplayProcessLock {
    lock_dir: PathBuf,
    token: String,
    pid: u32,
    // Close only: explicit unlock would release a fork parent's shared flock.
    _kernel_lock: std::fs::File,
}

/// Same-host exclusive guard. The permanent `<path>.lock.guard` must never be removed.
/// Requires trusted local storage and quiescing old directory-only writers on upgrade.
pub struct ExclusiveDirectoryProcessLock {
    _guard: RegistrationReplayProcessLock,
}

impl ExclusiveDirectoryProcessLock {
    pub fn try_acquire(path: &Path) -> Result<Self, String> {
        RegistrationReplayProcessLock::acquire_with_wait(path, false)
            .map(|guard| Self { _guard: guard })
    }
}

impl RegistrationReplayProcessLock {
    pub(super) fn acquire(ledger_path: &Path) -> Result<Self, String> {
        Self::acquire_with_wait(ledger_path, true)
    }

    fn acquire_with_wait(ledger_path: &Path, wait: bool) -> Result<Self, String> {
        let target = super::registration_replay_kernel_lock::normalized_target(ledger_path)?;
        let lock_dir = replay_lock_dir(&target);
        let parent = lock_dir.parent().unwrap_or_else(|| Path::new("."));
        let deadline = Instant::now() + LOCK_WAIT_TIMEOUT;
        let kernel_lock =
            super::registration_replay_kernel_lock::acquire(&lock_dir, deadline, wait)?;
        loop {
            if lock_dir.exists() {
                handle_existing_lock(&lock_dir, deadline, wait)?;
                continue;
            }
            let token = new_lock_token();
            let staging_dir = lock_staging_dir(&lock_dir, token.as_str());
            fs::create_dir(&staging_dir).map_err(|err| {
                format!("create registration replay ledger lock staging failed: {err}")
            })?;
            if let Err(error) = write_lock_owner(&staging_dir, token.as_str()) {
                let _ = fs::remove_dir_all(&staging_dir);
                return Err(error);
            }
            match fs::rename(&staging_dir, &lock_dir) {
                Ok(()) => {
                    let guard = Self {
                        lock_dir: lock_dir.clone(),
                        token,
                        pid: std::process::id(),
                        _kernel_lock: kernel_lock,
                    };
                    if let Err(err) = sync_lock_parent_directory(parent) {
                        drop(guard);
                        return Err(format!(
                            "sync registration replay ledger lock parent failed: {err}"
                        ));
                    }
                    return Ok(guard);
                }
                Err(_) if lock_dir.exists() => {
                    let _ = fs::remove_dir_all(&staging_dir);
                    handle_existing_lock(&lock_dir, deadline, wait)?;
                }
                Err(err) => {
                    let _ = fs::remove_dir_all(&staging_dir);
                    return Err(format!(
                        "publish registration replay ledger process lock failed: {err}"
                    ));
                }
            }
        }
    }
}

fn handle_existing_lock(lock_dir: &Path, deadline: Instant, wait: bool) -> Result<(), String> {
    if !wait && !reclaim_stale_lock(lock_dir)? {
        return Err("registration replay ledger lock is held by a live process".to_string());
    }
    if wait {
        wait_for_existing_lock(lock_dir, deadline)?;
    }
    Ok(())
}

fn wait_for_existing_lock(lock_dir: &Path, deadline: Instant) -> Result<(), String> {
    if reclaim_stale_lock(lock_dir)? {
        return Ok(());
    }
    if Instant::now() >= deadline {
        return Err("registration replay ledger lock is held by a live process".to_string());
    }
    std::thread::sleep(LOCK_RETRY_INTERVAL);
    Ok(())
}

fn sync_lock_parent_directory(parent: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_LOCK_PARENT_SYNC.swap(false, Ordering::SeqCst) {
        return Err(std::io::Error::other(
            "injected registration replay lock parent sync failure",
        ));
    }
    sync_parent_directory(parent)
}

impl Drop for RegistrationReplayProcessLock {
    fn drop(&mut self) {
        if self.pid != std::process::id() {
            return;
        }
        let owner_path = self.lock_dir.join("owner.json");
        let owns_lock = fs::read(owner_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<RegistrationReplayLockOwner>(&bytes).ok())
            .is_some_and(|owner| owner.token == self.token);
        if owns_lock {
            let parent = self.lock_dir.parent().unwrap_or_else(|| Path::new("."));
            if fs::remove_dir_all(&self.lock_dir).is_ok() {
                let _ = sync_parent_directory(parent);
            }
        }
    }
}

fn replay_lock_dir(ledger_path: &Path) -> PathBuf {
    let mut name: OsString = ledger_path.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

fn lock_staging_dir(lock_dir: &Path, token: &str) -> PathBuf {
    let mut name: OsString = lock_dir.as_os_str().to_owned();
    name.push(format!(".pending-{token}"));
    PathBuf::from(name)
}

fn write_lock_owner(lock_dir: &Path, token: &str) -> Result<(), String> {
    let owner = RegistrationReplayLockOwner {
        pid: std::process::id(),
        token: token.to_string(),
    };
    let bytes = serde_json::to_vec(&owner)
        .map_err(|err| format!("encode registration replay ledger lock owner failed: {err}"))?;
    let path = lock_dir.join("owner.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| format!("create registration replay ledger lock owner failed: {err}"))?;
    #[cfg(test)]
    if PAUSE_NEXT_OWNER_PUBLICATION.swap(false, Ordering::SeqCst) {
        OWNER_PUBLICATION_PAUSED.store(true, Ordering::SeqCst);
        while !RELEASE_OWNER_PUBLICATION.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[cfg(test)]
    if FAIL_NEXT_OWNER_WRITE.swap(false, Ordering::SeqCst) {
        return Err("injected owner persistence failure".into());
    }
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|err| format!("persist registration replay ledger lock owner failed: {err}"))
}

fn reclaim_stale_lock(lock_dir: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(lock_dir)
        .map_err(|err| format!("inspect registration replay lock directory failed: {err}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("registration replay lock directory is not a real directory".into());
    }
    let owner_path = lock_dir.join("owner.json");
    if let Ok(metadata) = fs::symlink_metadata(&owner_path)
        && (!metadata.is_file() || metadata.file_type().is_symlink())
    {
        return Err("registration replay lock owner is not a regular file".into());
    }
    let owner = match fs::read(&owner_path) {
        Ok(bytes) => serde_json::from_slice::<RegistrationReplayLockOwner>(&bytes).ok(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(format!(
                "read registration replay ledger lock owner failed: {err}"
            ));
        }
    };
    let stale = owner.is_some_and(|owner| !owner.token.is_empty() && !process_is_alive(owner.pid));
    if !stale {
        return Ok(false);
    }
    #[cfg(test)]
    stale_reclaimer_barrier();
    let quarantine = lock_dir.with_extension(format!(
        "stale-{}-{}",
        std::process::id(),
        LOCK_TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    match fs::rename(lock_dir, &quarantine) {
        Ok(()) => {
            #[cfg(test)]
            if FAIL_NEXT_QUARANTINE_REMOVE.swap(false, Ordering::SeqCst) {
                return Err("injected quarantine cleanup failure".into());
            }
            fs::remove_dir_all(&quarantine).map_err(|err| {
                format!("remove stale registration replay ledger lock failed: {err}")
            })?;
            Ok(true)
        }
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::AlreadyExists
            ) =>
        {
            Ok(true)
        }
        Err(err) => Err(format!(
            "quarantine stale registration replay ledger lock failed: {err}"
        )),
    }
}

fn new_lock_token() -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = LOCK_TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{time}-{counter}", std::process::id())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return true; // An invalid/unrepresentable identity is not proof of death.
    };
    if pid <= 0 {
        return true;
    }
    if unsafe { kill(pid, 0) == 0 } {
        return true;
    }
    unix_process_lookup_error_is_alive(std::io::Error::last_os_error().raw_os_error())
}

#[cfg(unix)]
fn unix_process_lookup_error_is_alive(raw_os_error: Option<i32>) -> bool {
    raw_os_error != Some(3)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return true;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn OpenProcess(
            desired_access: u32,
            inherit_handle: i32,
            process_id: u32,
        ) -> *mut std::ffi::c_void;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        fn GetLastError() -> u32;
        fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return unsafe { GetLastError() } != 87;
    }
    // An exited process can still be opened while another process retains a
    // handle. Only a signaled process object proves termination; timeout or
    // WAIT_FAILED remains unknown/live and therefore cannot authorize reclaim.
    let status = unsafe { WaitForSingleObject(handle, 0) };
    unsafe { CloseHandle(handle) };
    status != 0 // WAIT_OBJECT_0
}

#[cfg(not(any(unix, windows)))]
fn process_is_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(unix)]
    #[test]
    fn permission_denied_process_lookup_fails_safe() {
        let _test_guard = TEST_LOCK.lock().expect("lock replay-lock tests");
        assert!(unix_process_lookup_error_is_alive(Some(1)));
        assert!(!unix_process_lookup_error_is_alive(Some(3)));
    }

    #[test]
    fn slow_owner_publication_never_returns_two_concurrent_guards() {
        let _test_guard = TEST_LOCK.lock().expect("lock replay-lock tests");
        let root = std::env::temp_dir().join(format!(
            "oasis7-registration-partial-lock-{}-{}",
            std::process::id(),
            new_lock_token()
        ));
        let ledger = root.join("ledger.json");
        fs::create_dir_all(&root).expect("create slow publication root");
        PAUSE_NEXT_OWNER_PUBLICATION.store(true, Ordering::SeqCst);
        OWNER_PUBLICATION_PAUSED.store(false, Ordering::SeqCst);
        RELEASE_OWNER_PUBLICATION.store(false, Ordering::SeqCst);

        let (first_tx, first_rx) = std::sync::mpsc::channel();
        let first_ledger = ledger.clone();
        let first = std::thread::spawn(move || {
            first_tx
                .send(RegistrationReplayProcessLock::acquire(&first_ledger))
                .expect("send first acquisition result");
        });
        let pause_deadline = Instant::now() + Duration::from_secs(2);
        while !OWNER_PUBLICATION_PAUSED.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < pause_deadline,
                "first claimant must pause with an incomplete owner publication"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        let (second_tx, second_rx) = std::sync::mpsc::channel();
        let second_ledger = ledger.clone();
        let second = std::thread::spawn(move || {
            second_tx
                .send(RegistrationReplayProcessLock::acquire(&second_ledger))
                .expect("send second acquisition result");
        });
        assert!(matches!(
            second_rx.recv_timeout(INCOMPLETE_LOCK_GRACE),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        RELEASE_OWNER_PUBLICATION.store(true, Ordering::SeqCst);
        let first_guard = first_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert!(matches!(
            second_rx.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(first_guard);
        let second_guard = second_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        drop(second_guard);
        first.join().unwrap();
        second.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parent_sync_failure_removes_owner_lock_and_allows_retry() {
        let _test_guard = TEST_LOCK.lock().expect("lock replay-lock tests");
        let root = std::env::temp_dir().join(format!(
            "oasis7-registration-sync-failure-lock-{}-{}",
            std::process::id(),
            new_lock_token()
        ));
        let ledger = root.join("ledger.json");
        let lock_dir = replay_lock_dir(&ledger);
        FAIL_NEXT_LOCK_PARENT_SYNC.store(true, Ordering::SeqCst);

        let error = match RegistrationReplayProcessLock::acquire(&ledger) {
            Ok(_) => panic!("injected parent sync failure must reject acquisition"),
            Err(error) => error,
        };
        assert!(error.contains("sync registration replay ledger lock parent"));
        assert!(
            !lock_dir.exists(),
            "failed publication must remove the owned lock so a retry can acquire immediately"
        );

        let retry = RegistrationReplayProcessLock::acquire(&ledger)
            .expect("retry must acquire after failed publication cleanup");
        drop(retry);
        assert!(!lock_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dead_process_lock_is_reclaimed() {
        let _test_guard = TEST_LOCK.lock().expect("lock replay-lock tests");
        let root = std::env::temp_dir().join(format!(
            "oasis7-registration-stale-lock-{}-{}",
            std::process::id(),
            new_lock_token()
        ));
        let ledger = root.join("ledger.json");
        let lock_dir = replay_lock_dir(&ledger);
        fs::create_dir_all(&lock_dir).expect("create stale lock");
        let stale = RegistrationReplayLockOwner {
            pid: process_tests::dead_pid(),
            token: "dead-owner".to_string(),
        };
        fs::write(
            lock_dir.join("owner.json"),
            serde_json::to_vec(&stale).expect("encode stale owner"),
        )
        .expect("write stale owner");

        let guard = RegistrationReplayProcessLock::acquire(&ledger)
            .expect("dead process lock must be reclaimed");
        drop(guard);
        assert!(!lock_dir.exists());
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
fn stale_reclaimer_barrier() {
    let Some(root) = std::env::var_os("OASIS7_TEST_STALE_BARRIER").map(PathBuf::from) else {
        return;
    };
    fs::write(root.join("observed"), b"ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("release").exists() {
        assert!(Instant::now() < deadline, "stale barrier timeout");
        std::thread::sleep(LOCK_RETRY_INTERVAL);
    }
}

#[cfg(test)]
#[path = "registration_replay_lock_process_tests.rs"]
mod process_tests;
