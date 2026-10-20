//! Capture actual step outputs before installing the authenticated cold root.
use super::*;
impl NodeRuntimeExecutionDriver {
    pub(super) fn persist_local_checkpoint(
        &self,
        record: &super::super::ExecutionBridgeRecord,
    ) -> Result<Option<String>, String> {
        // Formal candidates execute once in an isolated attempt directory.
        // Their verified parent and business-history delta carry durable
        // history; no predecessor record lives in this local scratch directory.
        // The legacy checkpoint would read that absent predecessor at each
        // interval, and its reference is excluded from the formal closure.
        if self.formal_retention_enabled {
            return Ok(None);
        }
        maybe_persist_execution_checkpoint_for_record(
            self.records_dir.as_path(),
            record,
            self.checkpoint_interval_heights,
            self.checkpoint_keep_latest,
        )
    }

    pub(super) fn capture_formal_retention(&mut self, before: &RuntimeWorld) -> Result<(), String> {
        let before_events = &before.journal().events;
        let after_events = &self.execution_world.journal().events;
        if after_events.len() < before_events.len()
            || after_events[..before_events.len()] != *before_events
        {
            return Err("formal execution lost its unarchived journal prefix".into());
        }
        self.formal_event_root = Some(blake3_hex(&super::super::to_cbor(
            &after_events[before_events.len()..],
        )?));
        self.formal_unpruned_journal = Some(self.execution_world.journal().clone());
        self.formal_journal_cursors = self.execution_world.formal_journal_event_cursors()?;
        self.formal_new_results = self
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .iter()
            .filter(|(key, value)| {
                before
                    .capability_revocation_state()
                    .world_service_results
                    .get(*key)
                    != Some(*value)
            })
            .map(|(key, value)| {
                oasis7::world_service::correlation::validate_result(key, value)?;
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let plan = self.execution_world.prepare_formal_retention_candidate()?;
        self.execution_world.apply_formal_retention_plan(&plan)?;
        self.formal_retention_delta = plan.delta;
        Ok(())
    }
}
