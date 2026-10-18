use serde_json::Value;

use super::AgentDecisionTrace;

/// A native budget denial is a completed no-effect Wait, even though the
/// diagnostic trace uses `llm_error` to carry the stable exhaustion code.
/// Require both the local denied admission step and the code prefix so an
/// arbitrary provider payload cannot bypass failure handling.
pub(super) fn is_budget_exhausted_wait(trace: &AgentDecisionTrace) -> bool {
    matches!(trace.decision, crate::simulator::AgentDecision::Wait)
        && trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
        && trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "budget_admission" && step.status == "denied")
}

pub(super) fn is_trace_only_overflow(trace: &AgentDecisionTrace) -> bool {
    trace
        .llm_error
        .as_deref()
        .is_some_and(|error| error.trim() == "trace_payload_too_large")
}

pub(super) fn append_decision_upstream_trace(reason: String, trace: &AgentDecisionTrace) -> String {
    if reason.contains("upstream_trace=") {
        return reason;
    }
    let Some(output) = trace.llm_output.as_deref() else {
        return reason;
    };
    let Ok(payload) = serde_json::from_str::<Value>(output) else {
        return reason;
    };
    let upstream_trace = payload
        .get("upstream_trace")
        .or_else(|| payload.get("trace_payload")?.get("upstream_trace"));
    let Some(upstream_trace) = upstream_trace else {
        return reason;
    };
    let Ok(mut serialized) = serde_json::to_string(upstream_trace) else {
        return reason;
    };
    const MAX_UPSTREAM_TRACE_REASON_CHARS: usize = 1200;
    if serialized.len() > MAX_UPSTREAM_TRACE_REASON_CHARS {
        let mut end = MAX_UPSTREAM_TRACE_REASON_CHARS;
        while end > 0 && !serialized.is_char_boundary(end) {
            end -= 1;
        }
        serialized.truncate(end);
        serialized.push_str("...");
    }
    format!("{reason}; upstream_trace={serialized}")
}

pub(super) fn decision_trace_provider_error_retryable(trace: &AgentDecisionTrace) -> Option<bool> {
    let output = trace.llm_output.as_deref()?;
    let payload = serde_json::from_str::<Value>(output).ok()?;
    payload
        .get("provider_error")
        .or_else(|| payload.get("trace_payload")?.get("provider_error"))
        .and_then(|error| error.get("retryable"))
        .and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_trace(error: &str, step_type: &str, status: &str) -> AgentDecisionTrace {
        AgentDecisionTrace {
            agent_id: "agent-1".to_string(),
            time: 1,
            decision: crate::simulator::AgentDecision::Wait,
            llm_input: None,
            llm_output: None,
            llm_error: Some(error.to_string()),
            parse_error: None,
            llm_diagnostics: None,
            llm_effect_intents: Vec::new(),
            llm_effect_receipts: Vec::new(),
            llm_step_trace: vec![crate::simulator::LlmStepTrace {
                step_index: 0,
                step_type: step_type.to_string(),
                input_summary: String::new(),
                output_summary: String::new(),
                status: status.to_string(),
            }],
            llm_prompt_section_trace: Vec::new(),
            llm_chat_messages: Vec::new(),
        }
    }

    #[test]
    fn budget_wait_predicate_is_narrow_and_does_not_mask_provider_errors() {
        assert!(is_budget_exhausted_wait(&wait_trace(
            "budget_exhausted: max_model_calls used=1 max=1",
            "budget_admission",
            "denied",
        )));
        assert!(!is_budget_exhausted_wait(&wait_trace(
            "budget_exhausted: provider payload",
            "dialogue_turn",
            "degraded",
        )));
        assert!(!is_budget_exhausted_wait(&wait_trace(
            "provider_timeout: upstream unavailable",
            "budget_admission",
            "denied",
        )));
    }
}
