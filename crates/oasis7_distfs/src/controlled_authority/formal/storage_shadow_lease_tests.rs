//! Isolated fork regression: an inherited flock descriptor survives parent close.
use super::*;

struct Child {
    pid: libc::pid_t,
    release: libc::c_int,
}
impl Child {
    fn holding_inherited_file(_file: &File) -> Self {
        let mut ready = [-1; 2];
        let mut release = [-1; 2];
        // SAFETY: pipe outputs point to two writable descriptors each. After
        // fork the child performs only async-signal-safe libc calls and _exit;
        // it never unwinds, allocates, locks Rust state, or runs destructors.
        unsafe {
            assert_eq!(libc::pipe(ready.as_mut_ptr()), 0);
            assert_eq!(libc::pipe(release.as_mut_ptr()), 0);
            let pid = libc::fork();
            assert!(pid >= 0, "fork failed");
            if pid == 0 {
                libc::close(ready[0]);
                libc::close(release[1]);
                let byte = [1u8];
                if libc::write(ready[1], byte.as_ptr().cast(), 1) != 1 {
                    libc::_exit(101);
                }
                libc::close(ready[1]);
                let mut value = [0u8];
                loop {
                    let result = libc::read(release[0], value.as_mut_ptr().cast(), 1);
                    if result >= 0 {
                        break;
                    }
                }
                libc::close(release[0]);
                libc::_exit(0);
            }
            libc::close(ready[1]);
            libc::close(release[0]);
            let child = Self {
                pid,
                release: release[1],
            };
            let mut byte = [0u8];
            assert_eq!(libc::read(ready[0], byte.as_mut_ptr().cast(), 1), 1);
            libc::close(ready[0]);
            child
        }
    }
    fn assert_alive(&self) {
        let mut status = 0;
        // SAFETY: valid child PID and writable status, WNOHANG never waits.
        assert_eq!(
            unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) },
            0
        );
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        // SAFETY: close releases the sole parent pipe writer, waking the child
        // with EOF. waitpid reaps this exact child before the fixture is removed.
        unsafe {
            libc::close(self.release);
            let mut status = 0;
            while libc::waitpid(self.pid, &mut status, 0) < 0 {
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
}
fn open(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap()
}
#[test]
fn shadow_source_lease_is_released_while_fork_child_keeps_descriptor_alive() {
    const NAME: &str = "controlled_authority::formal::storage_shadow::lease_tests::shadow_source_lease_is_released_while_fork_child_keeps_descriptor_alive";
    const CHILD_ENV: &str = "OASIS7_SHADOW_LEASE_FORK_TEST_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated fork lease regression failed");
        return;
    }
    let root = std::env::temp_dir().join(format!("o7-shadow-fork-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let path = root.join("lease");
    File::create(&path).unwrap();
    // Control: closing a raw File leaves the inherited open-file-description
    // lock active until the child is released. CLOEXEC does not prevent fork.
    let lease = open(&path);
    lease.try_lock().unwrap();
    let child = Child::holding_inherited_file(&lease);
    drop(lease);
    let probe = open(&path);
    assert!(probe.try_lock().is_err());
    child.assert_alive();
    drop(child);
    probe.try_lock().unwrap();
    File::unlock(&probe).unwrap();
    drop(probe);
    // Control: explicit unlock acts on the shared description immediately.
    let lease = open(&path);
    lease.try_lock().unwrap();
    let child = Child::holding_inherited_file(&lease);
    File::unlock(&lease).unwrap();
    let probe = open(&path);
    probe.try_lock().unwrap();
    child.assert_alive();
    File::unlock(&probe).unwrap();
    drop(probe);
    drop(lease);
    drop(child);
    // Regression: the exact production guard releases its operation lease even
    // while a fork child retains the inherited descriptor.
    let lease = SourceLease::acquire(open(&path), invalid("test source busy")).unwrap();
    let child = Child::holding_inherited_file(&lease.0);
    drop(lease);
    let probe = open(&path);
    let result = probe.try_lock();
    child.assert_alive();
    drop(child);
    fs::remove_dir_all(root).unwrap();
    assert!(
        result.is_ok(),
        "short source lease remained busy after its owner scope exited: {result:?}"
    );
}

#[test]
fn both_shadow_entrypoints_reject_independent_holder_and_release_error_paths() {
    let root = std::env::temp_dir().join(format!("o7-shadow-lease-errors-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source");
    let objects = source.join("formal-objects-v1");
    fs::create_dir_all(&objects).unwrap();
    let lock = source.join(".formal-endpoint.lock");
    File::create(&lock).unwrap();
    let id = blake3::hash(b"original").to_hex().to_string();
    fs::write(objects.join(id), b"original").unwrap();
    let target = root.join("shadow");
    let options = FormalStorageShadowOptions {
        optimized: true,
        max_logical_bytes: 1,
    };
    let holder = open(&lock);
    holder.try_lock().unwrap();
    assert!(matches!(
        build_formal_storage_shadow(&source, &target),
        Err(FormalError::Conflict(_))
    ));
    let error = advance_formal_storage_shadow(&source, &target, &options).unwrap_err();
    assert!(format!("{error:?}").contains("resumable shadow source busy"));
    assert!(!target.exists(), "busy source must remain fail-fast");
    File::unlock(&holder).unwrap();
    // These errors occur after acquiring the source, exercising RAII on ?/return.
    let nested = source.join("forbidden-shadow");
    assert!(build_formal_storage_shadow(&source, &nested).is_err());
    let probe = open(&lock);
    probe.try_lock().unwrap();
    File::unlock(&probe).unwrap();
    assert!(advance_formal_storage_shadow(&source, &nested, &options).is_err());
    probe.try_lock().unwrap();
    File::unlock(&probe).unwrap();
    assert!(
        !advance_formal_storage_shadow(&source, &target, &options)
            .unwrap()
            .complete
    );
    let id = blake3::hash(b"source changed").to_hex().to_string();
    fs::write(objects.join(id), b"source changed").unwrap();
    let error = advance_formal_storage_shadow(&source, &target, &options).unwrap_err();
    assert!(
        !format!("{error:?}").contains("source busy"),
        "source drift must not be masked by a stale lease"
    );
    probe.try_lock().unwrap();
    File::unlock(&probe).unwrap();
    fs::remove_dir_all(root).unwrap();
}
