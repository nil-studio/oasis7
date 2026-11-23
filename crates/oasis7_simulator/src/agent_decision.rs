//! Deterministic Agent decision and trace values, independent of host execution.
//! This is a wire-preserving relocation: neither a module response nor a trace
//! constitutes authority to execute, accept, charge or persist an action.

use crate::{Action, AgentQuery, LlmEffectIntentTrace, LlmEffectReceiptTrace, WorldTime};
use oasis7_wasm_abi::AgentCommandResponse;
use serde::{Deserialize, Serialize};

/// The result of an agent's decision process.
#[expect(
    clippy::large_enum_variant,
    reason = "Stable agent decision wire enum preserves direct gameplay action payloads."
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentDecision {
    /// The agent decides to perform an action.
    Act(Action),
    /// The agent requests read-only information from the current world snapshot.
    Query(AgentQuery),
    /// A provider-produced capability response.  This is deliberately a
    /// separate decision lane: the simulator runner must never turn a typed
    /// module response into a core [`Action`].  A trusted host adapter owns
    /// execution against the runtime World.
    ModuleCommand { response: AgentCommandResponse },
    /// The agent decides to wait/skip this turn.
    Wait,
    /// The agent decides to wait for a specific number of ticks.
    WaitTicks(u64),
}

impl AgentDecision {
    /// Returns true if the agent decided to act.
    pub fn is_act(&self) -> bool {
        matches!(self, AgentDecision::Act(_))
    }

    /// Returns the action if the agent decided to act.
    pub fn action(&self) -> Option<&Action> {
        match self {
            AgentDecision::Act(action) => Some(action),
            _ => None,
        }
    }
}

/// Optional decision trace payload, intended for observability/diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDecisionTrace {
    pub agent_id: String,
    pub time: WorldTime,
    pub decision: AgentDecision,
    pub llm_input: Option<String>,
    pub llm_output: Option<String>,
    pub llm_error: Option<String>,
    pub parse_error: Option<String>,
    #[serde(default)]
    pub llm_diagnostics: Option<LlmDecisionDiagnostics>,
    #[serde(default)]
    pub llm_effect_intents: Vec<LlmEffectIntentTrace>,
    #[serde(default)]
    pub llm_effect_receipts: Vec<LlmEffectReceiptTrace>,
    #[serde(default)]
    pub llm_step_trace: Vec<LlmStepTrace>,
    #[serde(default)]
    pub llm_prompt_section_trace: Vec<LlmPromptSectionTrace>,
    #[serde(default)]
    pub llm_chat_messages: Vec<LlmChatMessageTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmStepTrace {
    pub step_index: usize,
    pub step_type: String,
    pub input_summary: String,
    pub output_summary: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmPromptSectionTrace {
    pub step_index: usize,
    pub kind: String,
    pub priority: String,
    pub included: bool,
    pub estimated_tokens: usize,
    pub emitted_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmChatRole {
    Player,
    Agent,
    Tool,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmChatMessageTrace {
    pub time: WorldTime,
    pub agent_id: String,
    pub role: LlmChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LlmDecisionDiagnostics {
    /// A bounded public explanation supplied by the Agent. This is never
    /// Runtime evidence that its prediction or proposed action took effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_explanation: Option<AgentDecisionExplanationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub retry_count: u32,
    /// Native Harness request budget and consumed invocation counts. These
    /// are absent for legacy unbudgeted traces and never imply opaque
    /// ProviderBacked usage or billing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_model_calls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_calls_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls_used: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDecisionExplanationV1 {
    pub status: String,
    pub provenance: String,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dissent: Option<String>,
    pub evidence_refs: Vec<String>,
    pub stakes: Option<String>,
    pub expected_consequence: Option<String>,
    pub alternatives: Vec<String>,
    pub next_step: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_wire_and_wait_ticks_are_unchanged() {
        let decision = AgentDecision::WaitTicks(7);
        assert!(!decision.is_act());
        assert!(decision.action().is_none());
        assert_eq!(
            serde_json::to_value(&decision).unwrap(),
            serde_json::json!({"WaitTicks": 7})
        );
        assert_eq!(
            serde_json::from_value::<AgentDecision>(serde_json::json!({"WaitTicks": 7})).unwrap(),
            decision
        );
        assert_eq!(
            serde_json::to_value(AgentDecision::Wait).unwrap(),
            serde_json::json!("Wait")
        );
    }

    #[test]
    fn trace_wire_preserves_legacy_optional_and_diagnostic_fields() {
        let original = serde_json::json!({
            "agent_id": "agent/界",
            "time": 17,
            "decision": "Wait",
            "llm_input": null,
            "llm_output": null,
            "llm_error": null,
            "parse_error": null,
            "llm_diagnostics": {
                "model": "test-model",
                "latency_ms": 0,
                "retry_count": 0
            },
            "llm_effect_intents": [],
            "llm_effect_receipts": [],
            "llm_step_trace": [],
            "llm_prompt_section_trace": [],
            "llm_chat_messages": []
        });
        let parsed: AgentDecisionTrace = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(parsed.decision, AgentDecision::Wait);
        assert_eq!(serde_json::to_value(parsed).unwrap(), original);
        assert_eq!(
            serde_json::to_value(LlmChatRole::Player).unwrap(),
            serde_json::json!("player")
        );
    }

    #[test]
    fn missing_defaulted_diagnostics_and_collections_remain_accepted() {
        let original = serde_json::json!({
            "agent_id": "a",
            "time": 0,
            "decision": "Wait",
            "llm_input": null,
            "llm_output": null,
            "llm_error": null,
            "parse_error": null
        });
        let parsed: AgentDecisionTrace = serde_json::from_value(original).unwrap();
        assert!(parsed.llm_diagnostics.is_none());
        assert!(parsed.llm_step_trace.is_empty());
        assert!(parsed.llm_effect_receipts.is_empty());
    }
}
