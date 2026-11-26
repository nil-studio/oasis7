//! Exact-owner optional original pending preparation and frozen read route.
use super::*;

impl FormalLiveRuntime {
    pub(crate) fn freeze_original_pending(
        &mut self,
        timeout: Duration,
    ) -> Result<super::super::formal_live_original_pending_producer::OriginalPendingPinV1, String> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err("original pending owner freeze timeout in (0,60s]".into());
        }
        let producer = self
            .original_pending
            .as_ref()
            .ok_or("formal owner has no original pending producer")?
            .clone();
        let directory = self
            .original_pending_directory
            .as_ref()
            .ok_or("formal owner has no original pending directory")?
            .clone();
        let deadline = std::time::Instant::now() + timeout;
        self.state.ready.store(false, std::sync::atomic::Ordering::Release);
        self.state.stopping.store(true, std::sync::atomic::Ordering::Release);
        let Some(admission) = quiesce::acquire_for_stop(&self.admission_gate, deadline)? else {
            return Err("formal Node stop is still pending; original pending remains prepared only".into());
        };
        let prepared = producer
            .lock()
            .map_err(|_| "original pending producer lock poisoned")?
            .begin_freeze(&directory)?;
        drop(admission);

        self.stop(deadline.saturating_duration_since(std::time::Instant::now()))?;
        let frozen = producer
            .lock()
            .map_err(|_| "original pending producer lock poisoned")?
            .finish_node_freeze(&directory)?;
        if frozen != prepared {
            return Err("original pending owner Frozen pin changed after real Node stop".into());
        }
        Ok(frozen)
    }
}
