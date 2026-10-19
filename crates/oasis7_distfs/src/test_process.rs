//! Run lock-sensitive regressions outside the parallel libtest process.
use std::{
    io,
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

const WORKER: &str = "OASIS7_DISTFS_ISOLATED_TEST";

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            if let Err(error) = self.0.kill() {
                eprintln!("isolated test child {} kill failed: {error}", self.0.id());
            }
            if let Err(error) = self.0.wait() {
                eprintln!("isolated test child {} reap failed: {error}", self.0.id());
            }
        }
    }
}

fn wait_owned(command: &mut Command, budget: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + budget;
    let mut child = OwnedChild(command.spawn()?);
    loop {
        if let Some(status) = child.0.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child.0.kill()?;
            child.0.wait()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "isolated test deadline expired; child killed and reaped",
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// True in the parent after a successful exact worker, false in that worker.
/// The worker owns its fixture, so unrelated parent spawn/fork calls cannot
/// inherit its locks. Failures and the outer deadline remain test failures.
pub(crate) fn run_isolated(name: &str, budget: Duration) -> bool {
    let name = name.strip_prefix("oasis7_distfs::").unwrap_or(name);
    assert_eq!(
        thread::current().name(),
        Some(name),
        "isolated worker must select the actual current test"
    );
    if std::env::var(WORKER).as_deref() == Ok(name) {
        return false;
    }
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(WORKER, name);
    let started = Instant::now();
    let result = wait_owned(&mut command, budget);
    assert!(
        matches!(&result, Ok(status) if status.success()),
        "isolated test {name} failed after {:?}: {result:?}",
        started.elapsed()
    );
    true
}

#[cfg(unix)]
#[test]
fn timeout_kills_and_reaps_owned_child() {
    let mut command = Command::new("/bin/sleep");
    command.arg("60");
    let started = Instant::now();
    let error = wait_owned(&mut command, Duration::from_millis(20)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
}
