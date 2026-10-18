//! Oasis-owned adapter from simulator behaviors to the extracted Harness.
//!
//! The Harness owns worker threads, bounded mailboxes, and lane-specific
//! admission. This adapter keeps simulator observations and behavior callbacks
//! in Oasis while ensuring providers never receive Runtime or World access.

use super::super::archive::WorldEvent;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use oasis7_agent_api::{
    CognitionError, CognitionLeaseConsumptionViewV1,
    ContinuousAgentRequestContextV1 as ApiRequestContext,
    ContinuousAgentResponseContextV1 as ApiResponseContext,
    DecisionResponse as ApiDecisionResponse, FeedbackEnvelopeV1 as ApiFeedbackEnvelope,
    ProviderDecision as ApiProviderDecision, ResponseArtifactIdentityV1, RuntimeReceiptLineageV1,
};
use oasis7_agent_harness::{
    CompatibilityDecisionProvider, CompatibilityTurnRequest, ContinuationHostProjection,
    DecisionProvider as HarnessProvider, DecisionProviderError as HarnessProviderError,
    FeedbackHistoryProjection, MemoryWritePolicyContextV1, RuntimeAuthority,
    cognition_response_digest,
};

use super::super::agent::{ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace};
use super::super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1, ContinuousAgentTurnContextV1,
};
use super::super::decision_provider::MemoryWriteIntent;
use oasis7_simulator::Action;
use oasis7_simulator::Observation;
use oasis7_simulator::WorldTime;

pub(super) type ProductionKey = (String, String, String, u64, u64);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct BehaviorTurnResult {
    pub decision: AgentDecision,
    pub decision_trace: Option<AgentDecisionTrace>,
    pub prepared_context: Option<ContinuousAgentTurnContextV1>,
    pub prepared_response_context: Option<ContinuousAgentResponseContextV1>,
    pub memory_write_intents: Vec<MemoryWriteIntent>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum BehaviorCallback {
    ActionResult(Box<ActionResult>),
    Event(Box<WorldEvent>),
    PromptOverrides {
        system_prompt: Option<String>,
        short_term_goal: Option<String>,
        long_term_goal: Option<String>,
    },
    PlayerMessage {
        world_time: WorldTime,
        message: String,
    },
}

#[derive(Clone)]
pub(super) struct AdapterControl(Arc<Mutex<AdapterState>>);

#[derive(Default)]
struct AdapterState {
    pending_production: Option<ProductionInput>,
    production_results: BTreeMap<ProductionKey, BehaviorTurnResult>,
}

struct ProductionInput {
    key: ProductionKey,
    observation: Observation,
    turn_context: ContinuousAgentTurnContextV1,
    request_context: ContinuousAgentRequestContextV1,
}

/// RuntimeAuthority adapter for a scope-authenticated World Service View.
/// This authority is intentionally history-only: callers cannot obtain lease,
/// receipt, memory, or continuation authority from the reduced projection.
pub(super) struct ScopedFeedbackHistoryAuthority {
    history: Vec<FeedbackHistoryProjection>,
}

impl ScopedFeedbackHistoryAuthority {
    pub fn new(history: Vec<FeedbackHistoryProjection>) -> Self {
        Self { history }
    }
}

impl RuntimeAuthority for ScopedFeedbackHistoryAuthority {
    type Lease = ();
    type Receipt = ();
    type Continuation = ();

    fn validate_lease(
        &self,
        _lease: &Self::Lease,
        _request: &ApiRequestContext,
    ) -> Result<CognitionLeaseConsumptionViewV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn verify_receipt_readback(
        &self,
        _feedback: &ApiFeedbackEnvelope,
        _receipt: &Self::Receipt,
        _response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn verify_memory_receipt(
        &self,
        _receipt: &Self::Receipt,
        _context: Option<&MemoryWritePolicyContextV1>,
    ) -> Result<RuntimeReceiptLineageV1, CognitionError> {
        Err(scoped_history_only())
    }

    fn validated_feedback_history(&self) -> Result<Vec<FeedbackHistoryProjection>, CognitionError> {
        Ok(self.history.clone())
    }

    fn continuation_projection(
        &self,
        _continuation: &Self::Continuation,
    ) -> Result<ContinuationHostProjection, CognitionError> {
        Err(scoped_history_only())
    }
}

fn scoped_history_only() -> CognitionError {
    CognitionError::new(
        "scoped_feedback_history_only",
        "authenticated feedback scope does not authorize Runtime mutations or projections",
    )
}

impl AdapterControl {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(AdapterState::default())))
    }

    pub fn install_production_input(
        &self,
        key: ProductionKey,
        observation: Observation,
        turn_context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
    ) -> Result<(), HarnessProviderError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| adapter_error("adapter_state_unavailable"))?;
        if state.pending_production.is_some() {
            return Err(adapter_error("adapter_input_already_pending"));
        }
        state.pending_production = Some(ProductionInput {
            key,
            observation,
            turn_context,
            request_context,
        });
        Ok(())
    }

    pub fn clear_production_input(&self, key: &ProductionKey) {
        if let Ok(mut state) = self.0.lock()
            && state
                .pending_production
                .as_ref()
                .is_some_and(|pending| &pending.key == key)
        {
            state.pending_production = None;
        }
    }

    pub fn take_production_result(&self, key: &ProductionKey) -> Option<BehaviorTurnResult> {
        self.0.lock().ok()?.production_results.remove(key)
    }
}

pub(super) struct OasisBehaviorProvider {
    behavior: Box<dyn AgentBehavior + Send>,
    control: AdapterControl,
}

impl OasisBehaviorProvider {
    pub fn new(behavior: Box<dyn AgentBehavior + Send>, control: AdapterControl) -> Self {
        Self { behavior, control }
    }

    fn decide_behavior(
        &mut self,
        observation: &Observation,
        turn_context: Option<&ContinuousAgentTurnContextV1>,
        request_context: Option<&ContinuousAgentRequestContextV1>,
    ) -> BehaviorTurnResult {
        self.behavior.set_continuous_turn_context(turn_context);
        self.behavior
            .set_continuous_request_context(request_context);
        let decision = self.behavior.decide(observation);
        BehaviorTurnResult {
            decision,
            decision_trace: self.behavior.take_decision_trace(),
            prepared_context: turn_context.cloned(),
            prepared_response_context: self.behavior.take_continuous_response_context(),
            memory_write_intents: self.behavior.take_memory_write_intents(),
        }
    }

    fn callback(&mut self, callback: BehaviorCallback) {
        match callback {
            BehaviorCallback::ActionResult(result) => self.behavior.on_action_result(&result),
            BehaviorCallback::Event(event) => self.behavior.on_event(&event),
            BehaviorCallback::PromptOverrides {
                system_prompt,
                short_term_goal,
                long_term_goal,
            } => self
                .behavior
                .set_prompt_overrides(system_prompt, short_term_goal, long_term_goal),
            BehaviorCallback::PlayerMessage {
                world_time,
                message,
            } => self.behavior.on_player_message(world_time, &message),
        }
    }
}

impl CompatibilityDecisionProvider<Observation, BehaviorTurnResult, BehaviorCallback>
    for OasisBehaviorProvider
{
    fn decide_compatibility(
        &mut self,
        request: CompatibilityTurnRequest<Observation>,
    ) -> Result<BehaviorTurnResult, HarnessProviderError> {
        if request.input.agent_id != request.agent_subject {
            return Err(adapter_error("agent_identity_mismatch"));
        }
        Ok(self.decide_behavior(&request.input, request.turn_context.as_ref(), None))
    }

    fn push_compatibility_feedback(
        &mut self,
        feedback: BehaviorCallback,
    ) -> Result<(), HarnessProviderError> {
        self.callback(feedback);
        Ok(())
    }
}

impl HarnessProvider<Action, super::super::agent::AgentQuery> for OasisBehaviorProvider {
    fn provider_id(&self) -> &str {
        self.behavior.agent_id()
    }

    fn decide(
        &mut self,
        request: &ApiRequestContext,
    ) -> Result<ApiResponseContext<Action, super::super::agent::AgentQuery>, HarnessProviderError>
    {
        let key = production_key(request);
        let input = {
            let mut state = self
                .control
                .0
                .lock()
                .map_err(|_| adapter_error("adapter_state_unavailable"))?;
            let input = state
                .pending_production
                .take()
                .ok_or_else(|| adapter_error("production_observation_missing"))?;
            if input.key != key
                || super::super::agent_api_compat::request_context_to_api(&input.request_context)
                    != *request
            {
                return Err(adapter_error("production_observation_identity_mismatch"));
            }
            input
        };
        let result = self.decide_behavior(
            &input.observation,
            Some(&input.turn_context),
            Some(&input.request_context),
        );
        if let Ok(mut state) = self.control.0.lock() {
            state.production_results.insert(key, result.clone());
        }
        let response = decision_response(&result);
        Ok(ApiResponseContext {
            base_decision_response: response.clone(),
            context_discriminator: request.context_discriminator.clone(),
            context_version: request.context_version,
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            retry_seq: request.retry_seq,
            transport_attempt: request.transport_attempt,
            request_digest: request.request_digest.clone(),
            response_digest: cognition_response_digest(&response),
        })
    }

    fn push_feedback(
        &mut self,
        _feedback: &ApiFeedbackEnvelope,
    ) -> Result<(), HarnessProviderError> {
        // Runtime feedback stays in Oasis' validated sidecar path. The
        // behavior-facing action/query/event callbacks use the typed host
        // callback channel below, as they did before extraction.
        Ok(())
    }
}

fn production_key(request: &ApiRequestContext) -> ProductionKey {
    (
        request.agent_session_id.clone(),
        request.agent_turn_id.clone(),
        request.decision_request_id.clone(),
        request.retry_seq,
        request.transport_attempt,
    )
}

pub(super) fn production_key_from_context(
    request: &ContinuousAgentRequestContextV1,
) -> ProductionKey {
    production_key(&super::super::agent_api_compat::request_context_to_api(
        request,
    ))
}

fn decision_response(
    result: &BehaviorTurnResult,
) -> ApiDecisionResponse<Action, super::super::agent::AgentQuery> {
    let decision = match &result.decision {
        AgentDecision::Wait => ApiProviderDecision::Wait,
        AgentDecision::WaitTicks(ticks) => ApiProviderDecision::WaitTicks { ticks: *ticks },
        AgentDecision::Act(action) => ApiProviderDecision::Act {
            action_ref: "oasis7-agent-behavior".into(),
            action: action.clone(),
        },
        AgentDecision::Query(query) => ApiProviderDecision::Query {
            query_ref: "oasis7-agent-behavior".into(),
            query: query.clone(),
        },
        AgentDecision::ModuleCommand { response } => ApiProviderDecision::ModuleCommandResponse {
            response: response.clone(),
        },
    };
    ApiDecisionResponse {
        decision,
        module_command: None,
        provider_error: None,
        diagnostics: Default::default(),
        trace_payload: Default::default(),
        memory_write_intents: result.memory_write_intents.clone(),
    }
}

fn adapter_error(code: &str) -> HarnessProviderError {
    HarnessProviderError::new(code, "Oasis behavior adapter rejected the request", false)
}
