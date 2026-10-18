//! Agent interface: AgentBehavior trait, AgentDecision, ActionResult.

use super::archive::WorldEvent;
use serde::{Deserialize, Serialize};

use oasis7_wasm_abi::AgentCommandResponse;

use super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, ContinuousAgentTurnContextV1,
};
use super::decision_provider::MemoryWriteIntent;
use oasis7_simulator::{Action, ActionId, WorldTime};
use oasis7_simulator::{Observation, RejectReason, WorldEventKind};

// ============================================================================
// Agent Interface (observe → decide → act)
// ============================================================================

/// Core trait for Agent behavior in the world.
///
/// An Agent follows the observe → decide → act loop:
/// 1. `observe`: Receive the current observation of the world
/// 2. `decide`: Based on observation, decide what action to take
/// 3. `act`: The kernel executes the decided action and produces events
///
/// This trait is designed to be implemented by various agent types:
/// - Simple rule-based agents
/// - LLM-powered agents
/// - Scripted agents for testing
pub trait AgentBehavior {
    /// Returns the agent's unique identifier.
    fn agent_id(&self) -> &str;

    /// Called when the agent receives a new observation.
    /// Returns an optional action to take, or None if the agent chooses to wait.
    fn decide(&mut self, observation: &Observation) -> AgentDecision;

    /// Called after an action is executed to notify the agent of the result.
    /// This allows the agent to update internal state based on action outcomes.
    fn on_action_result(&mut self, _result: &ActionResult) {
        // Default: no-op
    }

    /// Called after a read-only world query is evaluated.
    fn on_query_result(&mut self, _result: &AgentQueryResult) {
        // Default: no-op
    }

    /// Called when an event affecting this agent occurs.
    /// This allows the agent to react to external events.
    fn on_event(&mut self, _event: &WorldEvent) {
        // Default: no-op
    }

    /// Takes and clears the latest decision trace if available.
    ///
    /// Useful for live debugging/visualization pipelines that need decision internals
    /// (e.g. LLM prompt/completion I/O).
    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        None
    }

    /// Install the host-owned cognition projection before a turn executes.
    /// Legacy behaviors ignore it; continuous Builtin/ProviderBacked
    /// behaviors consume the same typed context.
    fn set_continuous_turn_context(&mut self, _context: Option<&ContinuousAgentTurnContextV1>) {}

    /// Install the complete trusted outer V1 request for a production
    /// provider lane. Legacy behaviors ignore this additive hook and remain
    /// explicitly fenced to the reduced compatibility lane.
    fn set_continuous_request_context(
        &mut self,
        _context: Option<&ContinuousAgentRequestContextV1>,
    ) {
    }

    /// Install a host-owned prompt projection before the next turn.  The
    /// async Harness uses this hook for both Builtin and ProviderBacked
    /// adapters; legacy behaviors may ignore it.
    fn set_prompt_overrides(
        &mut self,
        _system_prompt: Option<String>,
        _short_term_goal: Option<String>,
        _long_term_goal: Option<String>,
    ) {
    }

    /// Deliver a player message through the actor boundary.  This keeps the
    /// Builtin adapter from requiring a world-thread mutable behavior handle
    /// when it uses the same async Harness as ProviderBacked.
    fn on_player_message(&mut self, _world_time: WorldTime, _message: &str) {}

    /// Take the provider's response envelope, including its content digest,
    /// for Runtime replay/artifact binding.
    fn take_continuous_response_context(&mut self) -> Option<ContinuousAgentResponseContextV1> {
        None
    }

    /// Return provider memory candidates produced by the latest decision.
    /// Candidates remain non-authoritative until Runtime feedback commits the
    /// matching receipt.
    fn take_memory_write_intents(&mut self) -> Vec<MemoryWriteIntent> {
        Vec::new()
    }
}

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

pub use oasis7_simulator::{AgentQuery, AgentQueryResult, MicroDepotQuoteRequest};

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

pub use oasis7_simulator::{LlmEffectIntentTrace, LlmEffectReceiptTrace};

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

/// Result of an action execution, providing feedback to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    /// The action that was executed.
    pub action: Action,
    /// The action ID assigned by the kernel.
    pub action_id: ActionId,
    /// Whether the action succeeded.
    pub success: bool,
    /// The resulting event (success or rejection).
    pub event: WorldEvent,
}

impl ActionResult {
    /// Returns true if the action was rejected.
    pub fn is_rejected(&self) -> bool {
        matches!(self.event.kind, WorldEventKind::ActionRejected { .. })
    }

    /// Returns the rejection reason if the action was rejected.
    pub fn reject_reason(&self) -> Option<&RejectReason> {
        match &self.event.kind {
            WorldEventKind::ActionRejected { reason } => Some(reason),
            _ => None,
        }
    }
}
