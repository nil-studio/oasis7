use super::*;
use serde_json::Value;

impl RuntimeLlmSidecar {
    pub(super) fn rebind_memory_corrections_for_stale_replan(
        &mut self,
        cause: &ProviderStaleReplanCause,
        turn: &crate::simulator::ContinuousAgentTurnContextV1,
    ) -> Result<(), String> {
        let Some(parent) = self.provider_terminal_states.get(&turn.agent_id) else {
            return Ok(());
        };
        if parent.status == "rejected"
            && parent.reject_reason.as_deref() == Some("stale_base")
            && parent.agent_session_id == turn.agent_session_id
            && parent.agent_turn_id == cause.parent_agent_turn_id
            && parent.decision_request_id == cause.parent_decision_request_id
        {
            self.provider_memory_store
                .rebind_corrections_after_stale_parent(
                    &parent.decision_request_id,
                    &parent.request_digest,
                    turn,
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn correction_refs_for_decision(
        &self,
        agent_id: &str,
        decision_request_id: &str,
    ) -> Vec<String> {
        self.provider_memory_store
            .corrections()
            .iter()
            .filter(|correction| {
                correction.agent_id == agent_id
                    && correction.status == "accepted"
                    && correction
                        .active_decision_request_id
                        .as_ref()
                        .or(correction.earliest_decision_request_id.as_ref())
                        .map(String::as_str)
                        == Some(decision_request_id)
            })
            .map(|correction| correction.correction_id.clone())
            .collect()
    }
    pub(in crate::viewer::runtime_live) fn agent_referenced_memory_context(
        &self,
        agent_id: &str,
    ) -> Option<Value> {
        let mut context = self
            .provider_memory_store
            .referenced_memory_context(agent_id)?
            .clone();
        let session = context.get("agent_session_id").and_then(Value::as_str)?;
        let current =
            self.provider_memory_store
                .context_snapshot(agent_id, session, "session_private", 8);
        context["stale"] = serde_json::json!(
            context.get("revision").and_then(Value::as_u64) != Some(current.revision)
        );
        Some(context)
    }
    /// Called only after the control plane verifies the caller owns the Agent.
    /// Session identity is read from persisted private memory, never the client.
    pub(in crate::viewer::runtime_live) fn correct_agent_memory(
        &mut self,
        agent_id: &str,
        correction_id: String,
        target_memory_id: String,
        expected_revision: u64,
        replacement_summary: String,
    ) -> Result<crate::simulator::MemoryCorrectionV1, String> {
        if self.provider_lineage_store.is_none() {
            return Err("durable private memory checkpoint unavailable".to_string());
        }
        let target = self
            .provider_memory_store
            .entries()
            .iter()
            .find(|entry| {
                entry.get("agent_id").and_then(Value::as_str) == Some(agent_id)
                    && entry.get("intent_digest").and_then(Value::as_str)
                        == Some(target_memory_id.as_str())
                    && entry.get("scope").and_then(Value::as_str) == Some("session_private")
            })
            .ok_or_else(|| "private memory target unavailable for this Agent".to_string())?;
        let session = target
            .get("agent_session_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "private memory session unavailable".to_string())?
            .to_string();
        let previous_store = self.provider_memory_store.clone();
        let correction = self
            .provider_memory_store
            .correct_memory(crate::simulator::MemoryCorrectionV1 {
                correction_id,
                agent_id: agent_id.to_string(),
                agent_session_id: session,
                scope: "session_private".to_string(),
                target_memory_id,
                expected_revision,
                replacement_summary,
                status: String::new(),
                reason: String::new(),
                memory_revision: 0,
                earliest_decision_request_id: None,
                earliest_request_digest: None,
                active_decision_request_id: None,
                active_request_digest: None,
                committed_decision_request_id: None,
                committed_request_digest: None,
                runtime_receipt_id: None,
                action_id: None,
            })
            .map_err(|error| error.to_string())?;
        if let Err(error) = self.persist_provider_lineage() {
            self.provider_memory_store = previous_store;
            return Err(error);
        }
        Ok(correction)
    }

    pub(in crate::viewer::runtime_live) fn agent_memory_corrections(
        &self,
        agent_id: &str,
    ) -> Vec<crate::simulator::MemoryCorrectionV1> {
        self.provider_memory_store
            .corrections()
            .iter()
            .filter(|correction| correction.agent_id == agent_id)
            .cloned()
            .collect()
    }
}
