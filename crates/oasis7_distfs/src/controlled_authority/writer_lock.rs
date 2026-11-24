//! Explicitly release a journal flock before closing its file description.
use std::fs::File;

/// A concurrent fork may temporarily inherit this open file description before
/// exec closes it. Unlock while this owner still has the descriptor, so dropping
/// the writer permits the next owner without waiting for an unrelated child.
pub(super) struct WriterLock(File);
impl WriterLock {
    pub(super) fn acquire(file: File) -> Result<Self, std::fs::TryLockError> {
        file.try_lock()?;
        Ok(Self(file))
    }
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.0);
    }
}

#[cfg(all(test, unix))]
pub(super) mod regression {
    use std::{
        io,
        process::Command,
        time::{Duration, Instant},
    };

    struct Child {
        pid: libc::pid_t,
        release: libc::c_int,
    }
    impl Child {
        fn inherit() -> Self {
            let mut ready = [-1; 2];
            let mut release = [-1; 2];
            // SAFETY: the isolated fork child only uses async-signal-safe libc
            // operations and _exit, without Rust allocation or destructors.
            unsafe {
                assert_eq!(libc::pipe(ready.as_mut_ptr()), 0);
                assert_eq!(libc::pipe(release.as_mut_ptr()), 0);
                let pid = libc::fork();
                assert!(pid >= 0);
                if pid == 0 {
                    libc::close(ready[0]);
                    libc::close(release[1]);
                    let byte = [1u8];
                    if libc::write(ready[1], byte.as_ptr().cast(), 1) != 1 {
                        libc::_exit(101);
                    }
                    libc::close(ready[1]);
                    let mut poll = libc::pollfd {
                        fd: release[0],
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let status = if libc::poll(&mut poll, 1, 10000) > 0 {
                        0
                    } else {
                        102
                    };
                    libc::_exit(status);
                }
                libc::close(ready[1]);
                libc::close(release[0]);
                let child = Self {
                    pid,
                    release: release[1],
                };
                let mut poll = libc::pollfd {
                    fd: ready[0],
                    events: libc::POLLIN,
                    revents: 0,
                };
                let mut byte = [0u8];
                let result = libc::poll(&mut poll, 1, 5000);
                let count = if result > 0 {
                    libc::read(ready[0], byte.as_mut_ptr().cast(), 1)
                } else {
                    -1
                };
                libc::close(ready[0]);
                assert_eq!(count, 1, "fork child readiness");
                assert_eq!(byte, [1]);
                child
            }
        }
        fn assert_alive(&self) {
            let mut status = 0;
            // SAFETY: this fixture owns the PID and writable status.
            assert_eq!(
                unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) },
                0
            );
        }
    }
    impl Drop for Child {
        fn drop(&mut self) {
            // SAFETY: close wakes the bounded child poll with EOF. Reap only
            // the owned child, including on assertion unwind.
            unsafe {
                libc::close(self.release);
                let mut status = 0;
                while libc::waitpid(self.pid, &mut status, 0) < 0 {
                    if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                        break;
                    }
                }
            }
        }
    }

    pub(crate) fn isolate(name: &str) -> bool {
        const MARKER: &str = "OASIS7_JOURNAL_LOCK_FORK_CASE";
        if std::env::var(MARKER).as_deref() == Ok(name) {
            return false;
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(MARKER, name)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "isolated journal regression failed");
                return true;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("isolated journal regression exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn assert_release<T, E: std::fmt::Debug>(
        mut acquire: impl FnMut() -> Result<T, E>,
        is_locked: impl Fn(E) -> bool,
    ) {
        let owner = acquire().unwrap();
        let child = Child::inherit();
        match acquire() {
            Err(error) => assert!(is_locked(error)),
            Ok(_) => panic!("competing writer admitted"),
        }
        drop(owner);
        // The fork child is deliberately alive: only explicit unlock can
        // release this shared open-file-description before the child closes it.
        child.assert_alive();
        let successor = acquire().expect("dropped owner must release inherited writer lock");
        child.assert_alive();
        drop(successor);
        drop(child);
    }
}
