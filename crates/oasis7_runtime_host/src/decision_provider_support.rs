use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use oasis7_simulator::{GeoPos, ObservedAgent, ObservedLocation, ResourceKind, ResourceStock};
use super::{
    Action, ActionCatalogEntry, ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION,
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION, DEFAULT_PROVIDER_TIMEOUT_BUDGET_MS,
    DecisionProvider, DecisionProviderError, DecisionRequest, DecisionResponse, FeedbackEnvelope,
    Observation, ObservationEnvelope, ProviderDecision, ProviderExecutionMode, WorldEvent,
    provider_observation_from_runtime_observation,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldenDecisionFixture {
    pub fixture_id: String,
    pub request: DecisionRequest,
    pub expected_decision: ProviderDecision,
}

pub fn golden_decision_provider_fixtures() -> Vec<GoldenDecisionFixture> {
    let observation = Observation {
        time: 7,
        agent_id: "agent-1".to_string(),
        pos: GeoPos::new(0, 0, 0),
        self_resources: {
            let mut stock = ResourceStock::default();
            let _ = stock.add(ResourceKind::Electricity, 24);
            let _ = stock.add(ResourceKind::Data, 8);
            stock
        },
        visibility_range_cm: 1_000,
        visible_agents: vec![ObservedAgent {
            agent_id: "agent-2".to_string(),
            location_id: "loc-2".to_string(),
            pos: GeoPos::new(100, 0, 0),
            distance_cm: 100,
        }],
        visible_locations: vec![
            ObservedLocation {
                location_id: "loc-1".to_string(),
                name: "base".to_string(),
                pos: GeoPos::new(0, 0, 0),
                profile: Default::default(),
                distance_cm: 0,
            },
            ObservedLocation {
                location_id: "loc-2".to_string(),
                name: "neighbor".to_string(),
                pos: GeoPos::new(100, 0, 0),
                profile: Default::default(),
                distance_cm: 100,
            },
        ],
        module_lifecycle: Default::default(),
        module_market: Default::default(),
        power_market: Default::default(),
        social_state: Default::default(),
    };
    let action_catalog = vec![
        ActionCatalogEntry::new("wait", "Skip current tick without mutating world state"),
        ActionCatalogEntry::new(
            "move_agent",
            "Move the acting agent to a visible location via runtime validation",
        ),
    ];
    vec![GoldenDecisionFixture {
        fixture_id: "golden.move.visible_location.v1".to_string(),
        request: DecisionRequest {
            observation: ObservationEnvelope {
                agent_id: "agent-1".to_string(),
                world_time: observation.time,
                mode: ProviderExecutionMode::HeadlessAgent,
                observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
                action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
                environment_class: Some("golden_fixture".to_string()),
                fallback_reason: None,
                observation: provider_observation_from_runtime_observation(
                    ProviderExecutionMode::HeadlessAgent,
                    &observation,
                    Some("goal=巡游移动; recent_failure=none; location_hint=loc-2 visible"),
                    &[
                        "event: AgentRegistered(agent-1 @ loc-1)".to_string(),
                        "event: AgentRegistered(agent-2 @ loc-2)".to_string(),
                    ],
                    &action_catalog,
                ),
                recent_event_summary: vec![
                    "event: AgentRegistered(agent-1 @ loc-1)".to_string(),
                    "event: AgentRegistered(agent-2 @ loc-2)".to_string(),
                ],
                memory_summary: Some(
                    "goal=巡游移动; recent_failure=none; location_hint=loc-2 visible".to_string(),
                ),
                action_catalog,
                module_command_catalog: Vec::new(),
                timeout_budget_ms: DEFAULT_PROVIDER_TIMEOUT_BUDGET_MS,
            },
            provider_config_ref: Some("golden/mock-provider".to_string()),
            agent_profile: None,
            fixture_id: Some("golden.move.visible_location.v1".to_string()),
            replay_id: None,
            capability_catalog: None,
            capability_invocation_context: None,
            timeout_budget_ms: DEFAULT_PROVIDER_TIMEOUT_BUDGET_MS,
        },
        expected_decision: ProviderDecision::Act {
            action_ref: "move_agent".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        },
    }]
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MockDecisionProviderState {
    pub recorded_requests: Vec<DecisionRequest>,
    pub recorded_turn_contexts: Vec<ContinuousAgentTurnContextV1>,
    #[serde(default)]
    pub recorded_request_contexts: Vec<ContinuousAgentRequestContextV1>,
    pub recorded_feedback: Vec<FeedbackEnvelope>,
    pub recorded_events: Vec<WorldEvent>,
}

#[derive(Debug)]
pub struct MockDecisionProvider {
    provider_id: String,
    scripted_responses: VecDeque<Result<DecisionResponse, DecisionProviderError>>,
    shared_state: Arc<Mutex<MockDecisionProviderState>>,
}

impl MockDecisionProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            scripted_responses: VecDeque::new(),
            shared_state: Arc::new(Mutex::new(MockDecisionProviderState::default())),
        }
    }

    pub fn with_scripted_responses(
        provider_id: impl Into<String>,
        scripted_responses: Vec<Result<DecisionResponse, DecisionProviderError>>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            scripted_responses: scripted_responses.into(),
            shared_state: Arc::new(Mutex::new(MockDecisionProviderState::default())),
        }
    }

    pub fn shared_state(&self) -> Arc<Mutex<MockDecisionProviderState>> {
        Arc::clone(&self.shared_state)
    }

    pub fn enqueue_response(&mut self, response: DecisionResponse) {
        self.scripted_responses.push_back(Ok(response));
    }

    pub fn enqueue_error(&mut self, error: DecisionProviderError) {
        self.scripted_responses.push_back(Err(error));
    }
}

impl DecisionProvider for MockDecisionProvider {
    fn provider_id(&self) -> &str {
        self.provider_id.as_str()
    }

    fn decide(
        &mut self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, DecisionProviderError> {
        self.shared_state
            .lock()
            .expect("mock state lock")
            .recorded_requests
            .push(request.clone());
        match self.scripted_responses.pop_front() {
            Some(result) => result,
            None => Ok(DecisionResponse::wait(self.provider_id.clone())),
        }
    }

    fn decide_with_continuous_context(
        &mut self,
        request: &DecisionRequest,
        context: &ContinuousAgentTurnContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
        self.shared_state
            .lock()
            .expect("mock state lock")
            .recorded_turn_contexts
            .push(context.clone());
        Ok(super::wrap_continuous_response(
            self.decide(request)?,
            context,
        ))
    }

    fn decide_with_continuous_request_context(
        &mut self,
        request: &DecisionRequest,
        turn_context: &ContinuousAgentTurnContextV1,
        request_context: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
        self.shared_state
            .lock()
            .expect("mock state lock")
            .recorded_request_contexts
            .push(request_context.clone());
        let mut response = self.decide_with_continuous_context(request, turn_context)?;
        response.retry_seq = request_context.retry_seq;
        response.transport_attempt = request_context.transport_attempt;
        response.request_digest = request_context.request_digest.clone();
        Ok(response)
    }

    fn push_feedback(&mut self, feedback: &FeedbackEnvelope) -> Result<(), DecisionProviderError> {
        self.shared_state
            .lock()
            .expect("mock state lock")
            .recorded_feedback
            .push(feedback.clone());
        Ok(())
    }

    fn on_world_event(&mut self, event: &WorldEvent) -> Result<(), DecisionProviderError> {
        self.shared_state
            .lock()
            .expect("mock state lock")
            .recorded_events
            .push(event.clone());
        Ok(())
    }
}
