//! Agent interface: AgentBehavior trait, AgentDecision, ActionResult.

use super::archive::WorldEvent;
use serde::{Deserialize, Serialize};

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

    /// Take original transport content references captured by a native
    /// ProviderLoopback owner. They remain distinct from the typed response
    /// context above and do not grant Runtime acceptance.
    #[cfg(not(target_arch = "wasm32"))]
    fn take_provider_original_material_refs(
        &mut self,
    ) -> Option<super::ProviderOriginalMaterialRefsV1> {
        None
    }

    /// Keep native provider response decode memory charged through its
    /// in-process turn outcome. Legacy and non-provider behaviors return none.
    #[cfg(not(target_arch = "wasm32"))]
    fn take_provider_response_context_lease(
        &mut self,
    ) -> Option<super::provider_json_admission::ProviderResponseContextLease> {
        None
    }

    /// Return provider memory candidates produced by the latest decision.
    /// Candidates remain non-authoritative until Runtime feedback commits the
    /// matching receipt.
    fn take_memory_write_intents(&mut self) -> Vec<MemoryWriteIntent> {
        Vec::new()
    }
}

// Decision grammar and diagnostic projections are pure L2 values.
// Host behavior and action result remain bound to live provider/archive owners.
pub use oasis7_simulator::{
    AgentDecision, AgentDecisionExplanationV1, AgentDecisionTrace, AgentQuery, AgentQueryResult,
    LlmChatMessageTrace, LlmChatRole, LlmDecisionDiagnostics, LlmEffectIntentTrace,
    LlmEffectReceiptTrace, LlmPromptSectionTrace, LlmStepTrace, MicroDepotQuoteRequest,
};

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
