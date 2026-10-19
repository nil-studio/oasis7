//! Atomic serialization of the durable private lineage checkpoint.
use super::*;

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn persist_provider_lineage(&self) -> Result<(), String> {
        let Some(path) = self.provider_lineage_store.as_deref() else {
            return Ok(());
        };
        let checkpoint = PersistedProviderLineageV1 {
            schema_version: PROVIDER_LINEAGE_SCHEMA_VERSION,
            provider_session_ids: self.provider_session_ids.clone(),
            provider_agent_ids: self.provider_agent_ids.clone(),
            provider_context_seq: self.provider_context_seq.clone(),
            provider_contexts: self.provider_contexts.clone(),
            provider_retry_contexts: self.provider_retry_contexts.clone(),
            provider_active_turns: self.provider_active_turns.clone(),
            provider_cognition_leases: self.provider_cognition_leases.clone(),
            provider_capability_identities: self.provider_capability_identities.clone(),
            provider_continuation_proposals: self.provider_continuation_proposals.clone(),
            provider_continuation_recovery_pending: self
                .provider_continuation_recovery_pending
                .clone(),
            provider_recovery_pending: self.provider_recovery_pending.clone(),
            provider_wake_recovery_pending: self.provider_wake_recovery_pending.clone(),
            provider_service_pending: self.provider_service_pending.clone(),
            provider_scheduler_pending: self.provider_scheduler_pending.clone(),
            hosted_admission: self.hosted_admission.clone(),
            hosted_wait: self.hosted_wait.clone(),
            hosted_resume: self.hosted_resume.clone(),
            provider_wait_until: self.provider_wait_until.clone(),
            provider_feedback_seq: self.provider_feedback_seq.clone(),
            provider_feedback_seq_by_session: self.provider_feedback_seq_by_session.clone(),
            provider_memory_store: self.provider_memory_store.clone(),
            provider_completed_decisions: self.provider_completed_decisions.clone(),
            provider_held_decisions: self.provider_held_decisions.clone(),
            provider_stale_replans: self.provider_stale_replans.clone(),
            provider_transport_exhausted: self.provider_transport_exhausted.clone(),
            provider_terminal_states: self.provider_terminal_states.clone(),
            provider_late_response_diagnostics: self.provider_late_response_diagnostics.clone(),
            pending_actions: self.pending_actions.clone(),
            pending_provider_world_events: self.pending_provider_world_events.clone(),
            provider_world_event_quarantine: self.provider_world_event_quarantine.clone(),
            provider_lineage_recovery_pending: self.provider_lineage_recovery_pending.clone(),
            runtime_binding: self.provider_lineage_binding.clone(),
            pending_runtime_wakes: self.pending_runtime_wakes.clone(),
        };
        let encoded = serde_json::to_vec_pretty(&checkpoint)
            .map_err(|error| format!("provider lineage checkpoint encode failed: {error}"))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "provider lineage checkpoint directory creation failed ({}): {error}",
                parent.display()
            )
        })?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let temp_path = path.with_extension(format!("tmp-{}-{nonce}", std::process::id()));
        fs::write(&temp_path, encoded).map_err(|error| {
            format!(
                "provider lineage checkpoint temporary write failed ({}): {error}",
                temp_path.display()
            )
        })?;
        if let Err(error) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "provider lineage checkpoint commit failed ({}): {error}",
                path.display()
            ));
        }
        Ok(())
    }
}
