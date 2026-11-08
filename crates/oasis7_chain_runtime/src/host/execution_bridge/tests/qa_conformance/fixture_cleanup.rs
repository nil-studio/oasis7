//! Join all fixture workers before releasing the serialized scenario permit.
use super::*;

impl Fixture {
    pub(super) fn finish_http_workers(&self) -> Result<(), &'static str> {
        let cleanup = cleanup_deadline::CleanupDeadline::start("fixture_http_workers");
        self.halt.store(true, Ordering::SeqCst);
        let (worker, poisoned) = match self.worker.lock() {
            Ok(mut worker) => (worker.take(), false),
            Err(poisoned) => (poisoned.into_inner().take(), true),
        };
        let result = if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| "actual fixture listener or connection worker failed")
        } else {
            Ok(())
        };
        if result.is_err() {
            // A listener panic may have dropped still-live connection handles.
            // Do not release the scenario permit on an unproven worker cleanup.
            cleanup.fail_with_diagnostics("listener_join_failed", &self.failure_diagnostics);
        }
        cleanup.complete();
        if poisoned {
            return Err("actual fixture listener ownership lock poisoned");
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let cleanup = cleanup_deadline::CleanupDeadline::start("fixture_drop");
        // Release an automatic commit before joining HTTP workers that may be
        // waiting for it. Explicit finish_http_workers keeps normal gate semantics.
        self.abort_automatic_commit.store(true, Ordering::SeqCst);
        self.automatic_commit_gate.store(true, Ordering::SeqCst);
        let _ = self.finish_http_workers();
        let _ = self
            .node
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stop();
        if !self.preserve_root {
            let _ = fs::remove_dir_all(&self.root);
        }
        cleanup.complete();
    }
}
