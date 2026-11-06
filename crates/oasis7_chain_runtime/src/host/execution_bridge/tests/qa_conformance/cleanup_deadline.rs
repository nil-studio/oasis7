//! Fail closed if fixture cleanup cannot join its owned threads in time.
use super::*;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};

const CLEANUP_BUDGET: Duration = Duration::from_secs(10);
const CLEANUP_EXIT_CODE: i32 = 124;

pub(super) struct CleanupDeadline {
    stage: &'static str,
    completed: Option<Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl CleanupDeadline {
    pub(super) fn start(stage: &'static str) -> Self {
        Self::with_budget(stage, CLEANUP_BUDGET)
    }

    fn with_budget(stage: &'static str, budget: Duration) -> Self {
        let deadline = Instant::now() + budget;
        let (completed, receiver) = mpsc::channel();
        let worker = thread::spawn(move || match receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(()) => {}
            Err(RecvTimeoutError::Timeout) => terminate(stage, "deadline"),
            Err(RecvTimeoutError::Disconnected) => terminate(stage, "incomplete"),
        });
        Self {
            stage,
            completed: Some(completed),
            worker: Some(worker),
        }
    }

    pub(super) fn fail(self, reason: &'static str) -> ! {
        terminate(self.stage, reason)
    }

    /// Only call after all owned workers have joined and resources have stopped.
    pub(super) fn complete(mut self) {
        let completed = self.completed.take().unwrap();
        if completed.send(()).is_err() {
            terminate(self.stage, "watchdog_failed");
        }
        if self.worker.take().unwrap().join().is_err() {
            terminate(self.stage, "watchdog_failed");
        }
    }
}

impl Drop for CleanupDeadline {
    fn drop(&mut self) {
        if self.completed.is_some() || self.worker.is_some() {
            // Early return/panic is not successful cleanup. Never detach a live
            // worker and then release the serialized scenario's resource permit.
            terminate(self.stage, "incomplete");
        }
    }
}

fn terminate(stage: &'static str, reason: &'static str) -> ! {
    // A blocked stderr must not turn the timeout diagnostic into another hang.
    // This last-resort thread owns no fixture state and always exits the process.
    if thread::Builder::new()
        .spawn(|| {
            thread::sleep(Duration::from_millis(100));
            std::process::exit(CLEANUP_EXIT_CODE);
        })
        .is_err()
    {
        std::process::exit(CLEANUP_EXIT_CODE);
    }
    // Bypass libtest capture; retain only a fixed stage and reason, no request data.
    let _ = writeln!(
        std::io::stderr(),
        "conformance_cleanup_failed stage={stage} reason={reason} exit_code={CLEANUP_EXIT_CODE}"
    );
    // Exit the test process rather than dropping JoinHandles or unwinding through
    // a poisoned fixture. The outer process supervisor owns its remaining tree.
    std::process::exit(CLEANUP_EXIT_CODE)
}

#[path = "cleanup_deadline_tests.rs"]
mod tests;
