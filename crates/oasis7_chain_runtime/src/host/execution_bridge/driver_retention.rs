//! Post-commit retention refresh, degradation and reconciliation scheduling.
use super::super::{
    ExecutionBridgeRecord,
    checkpoint::{
        complete_execution_bridge_retention_transaction,
        fail_execution_bridge_retention_transaction, refresh_live_task_read_evidence,
        run_execution_bridge_incremental_retention_maintenance_after_task_refresh,
        run_execution_bridge_retention_maintenance,
    },
};
use super::NodeRuntimeExecutionDriver;
use std::time::{Duration, Instant};
impl NodeRuntimeExecutionDriver {
    pub(super) fn maintain_committed_retention(
        &mut self,
        committed_height: u64,
        record: &ExecutionBridgeRecord,
    ) -> Duration {
        let retention_started_at = Instant::now();
        let reconcile_due = self.retention_reconcile_pending
            && self
                .retention_reconcile_next_height
                .is_none_or(|height| committed_height >= height);
        let retention_result = refresh_live_task_read_evidence(
            self.records_dir.as_path(),
            &self.execution_store,
            &self.execution_world,
            self.hot_window_heights,
        )
        .and_then(|validated_task_retention| {
            if reconcile_due {
                run_execution_bridge_retention_maintenance(
                    self.records_dir.as_path(),
                    &self.execution_store,
                    self.hot_window_heights,
                )
            } else {
                run_execution_bridge_incremental_retention_maintenance_after_task_refresh(
                    self.records_dir.as_path(),
                    &self.execution_store,
                    record,
                    validated_task_retention,
                    self.hot_window_heights,
                    self.checkpoint_interval_heights,
                    self.checkpoint_keep_latest,
                )
            }
        });
        let retention_result = retention_result.and_then(|freed_bytes| {
            complete_execution_bridge_retention_transaction(self.records_dir.as_path())?;
            Ok(freed_bytes)
        });
        if let Err(err) = retention_result {
            let was_pending = self.retention_reconcile_pending;
            self.retention_reconcile_pending = true;
            if reconcile_due || !was_pending {
                self.retention_reconcile_next_height =
                    Some(committed_height.saturating_add(self.checkpoint_interval_heights.max(1)));
            }
            if let Err(marker_err) =
                fail_execution_bridge_retention_transaction(self.records_dir.as_path())
            {
                oasis7::observability::emit_stderr_or_event(
                    tracing::Level::ERROR,
                    format!(
                        "execution driver failed to publish retention degradation at height {}: {}",
                        committed_height, marker_err
                    )
                    .as_str(),
                    "execution bridge retention degradation publication failed",
                );
            }
            oasis7::observability::emit_stderr_or_event(
                tracing::Level::WARN,
                format!(
                    "execution driver retention pin-set sync failed at height {}: {}",
                    committed_height, err
                )
                .as_str(),
                "execution bridge retention maintenance failed",
            );
        } else if reconcile_due || !self.retention_reconcile_pending {
            self.retention_reconcile_pending = false;
            self.retention_reconcile_next_height = None;
        }
        retention_started_at.elapsed()
    }
}
