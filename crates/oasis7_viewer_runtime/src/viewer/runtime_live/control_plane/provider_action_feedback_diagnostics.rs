//! Canonical feedback reasons and provider recovery diagnostics.
use super::*;

pub(super) fn canonical_feedback_reason(status: &str, reason: Option<&str>) -> Option<String> {
    match status {
        "committed" => None,
        "pending" => Some("retry_scheduled".to_string()),
        "failed" => Some(
            matches!(
                reason,
                Some("failed_provider")
                    | Some("failed_persist")
                    | Some("cognition_failed")
                    | Some("provider_unavailable")
            )
            .then_some(reason.unwrap_or("provider_unavailable"))
            .unwrap_or("provider_unavailable")
            .to_string(),
        ),
        "rejected" => Some(
            matches!(
                reason,
                Some("stale_base")
                    | Some("expired")
                    | Some("stale_capability_snapshot")
                    | Some("authority_denied")
                    | Some("intent_conflict")
                    | Some("reorg_invalidated")
                    | Some("finality_anchor_mismatch")
                    | Some("precondition_failed")
                    | Some("action_rejected")
                    | Some("idempotency_conflict")
                    | Some("no_effect")
                    | Some("cancelled")
                    | Some("late_response_after_cancel")
                    | Some("legacy_no_cognition_proof")
                    | Some("cognition_context_mismatch")
            )
            .then_some(reason.unwrap_or("action_rejected"))
            .unwrap_or("action_rejected")
            .to_string(),
        ),
        _ => reason.map(ToOwned::to_owned),
    }
}

pub(super) fn wake_handoff_error_trace(
    agent_id: &str,
    time: u64,
    error: String,
) -> AgentDecisionTrace {
    AgentDecisionTrace {
        agent_id: agent_id.to_string(),
        time,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: None,
        llm_error: Some(error),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    }
}

pub(super) fn stale_replan_exhausted_trace(
    world: &RuntimeWorld,
    agent_id: &str,
) -> AgentDecisionTrace {
    AgentDecisionTrace {
        agent_id: agent_id.to_string(),
        time: world.state().time,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: None,
        llm_error: Some("stale_base replan budget exhausted".to_string()),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    }
}
