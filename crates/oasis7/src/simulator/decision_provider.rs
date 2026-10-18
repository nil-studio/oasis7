use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::time::Instant;

use oasis7_wasm_abi::ModuleCommandCatalogEntry;
use serde_json::Value;

use super::cognition_response_identity::{
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
    cognition_response_digest,
};
use super::continuous_agent_harness::{
    CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION,
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, FeedbackEnvelopeV1,
};
use super::{
    Action, ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace, AgentQuery,
    AgentQueryResult, Observation, WorldEvent, WorldTime,
};

pub use oasis7_agent_api::{
    ActionCatalogEntry, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION,
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION, MemoryWriteIntent, ObservationEnvelope,
    ProviderDiagnostics, ProviderErrorEnvelope, ProviderExecutionMode, ProviderInteractionTarget,
    ProviderMissionContext, ProviderModuleCommand, ProviderNavigationNode, ProviderNearbyEntity,
    ProviderObservation, ProviderRecentEvent, ProviderSelfState, ProviderTokenUsage,
    ProviderTraceEnvelope, ProviderTranscriptEntry,
};
pub type ProviderDecision = oasis7_agent_api::ProviderDecision<Action, AgentQuery>;

#[path = "decision_provider_capability.rs"]
mod decision_provider_capability;
#[path = "decision_provider_cognition.rs"]
mod decision_provider_cognition;
#[path = "decision_provider_observation.rs"]
mod decision_provider_observation;
#[path = "decision_provider_support.rs"]
mod decision_provider_support;
#[path = "decision_provider_trace.rs"]
mod decision_provider_trace;
use self::decision_provider_observation::{
    provider_observation_from_runtime_observation,
    provider_observation_from_runtime_observation_with_goal,
};
pub use decision_provider_trace::DecisionResponseExplanation;

const DEFAULT_PROVIDER_TIMEOUT_BUDGET_MS: u64 = 3_000;
const MAX_RECENT_EVENT_SUMMARIES: usize = 8;
pub use decision_provider_capability::{DecisionRequestContractError, ProviderCapabilityContext};
pub use decision_provider_support::{
    GoldenDecisionFixture, MockDecisionProvider, MockDecisionProviderState,
    golden_decision_provider_fixtures,
};

pub use oasis7_agent_api::DecisionRequest;
pub type DecisionResponse = oasis7_agent_api::DecisionResponse<Action, AgentQuery>;

pub type FeedbackEnvelope = oasis7_agent_api::FeedbackEnvelope<WorldEvent>;

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionProviderError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub upstream_trace: Option<Value>,
}

impl DecisionProviderError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::new_with_upstream_trace(code, message, retryable, None)
    }

    pub fn new_with_upstream_trace(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        upstream_trace: Option<Value>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            upstream_trace,
        }
    }

    fn as_trace_message(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}

impl fmt::Display for DecisionProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Error for DecisionProviderError {}

pub trait DecisionProvider {
    fn provider_id(&self) -> &str;

    fn decide(
        &mut self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, DecisionProviderError>;

    /// Versioned additive adapter for the continuous-agent host projection.
    ///
    /// Existing providers implement the legacy `decide` method and therefore
    /// continue to work unchanged.  Providers that understand the continuous
    /// cognition lane can override this hook to consume the frozen context and
    /// return the outer response envelope.  The default implementation keeps
    /// legacy provider semantics while making the response lineage explicit at
    /// the host boundary.
    fn decide_with_continuous_context(
        &mut self,
        request: &DecisionRequest,
        context: &ContinuousAgentTurnContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
        Ok(wrap_continuous_response(self.decide(request)?, context))
    }

    /// Production-target additive lane. The full outer request is validated
    /// and carried through retries; the default adapter is a fenced legacy
    /// compatibility bridge for providers that only understand the reduced
    /// turn context.
    fn decide_with_continuous_request_context(
        &mut self,
        request: &DecisionRequest,
        turn_context: &ContinuousAgentTurnContextV1,
        request_context: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
        validate_request_context_pair(request, turn_context, request_context)?;
        let mut response = self.decide_with_continuous_context(request, turn_context)?;
        response.context_discriminator = CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string();
        response.context_version = CONTINUOUS_AGENT_CONTEXT_VERSION;
        response.agent_session_id = request_context.agent_session_id.clone();
        response.agent_turn_id = request_context.agent_turn_id.clone();
        response.decision_request_id = request_context.decision_request_id.clone();
        response.retry_seq = request_context.retry_seq;
        response.transport_attempt = request_context.transport_attempt;
        response.request_digest = request_context.request_digest.clone();
        Ok(response)
    }

    fn push_feedback(&mut self, _feedback: &FeedbackEnvelope) -> Result<(), DecisionProviderError> {
        Ok(())
    }

    /// Runtime feedback lane carrying complete subject/session sequencing.
    /// The legacy hook above is intentionally retained as a compatibility
    /// lane and cannot establish authoritative cognition lineage.
    fn push_continuous_feedback(
        &mut self,
        _feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        Ok(())
    }

    fn on_world_event(&mut self, _event: &WorldEvent) -> Result<(), DecisionProviderError> {
        Ok(())
    }
}

fn wrap_continuous_response(
    response: DecisionResponse,
    context: &ContinuousAgentTurnContextV1,
) -> ContinuousAgentResponseContextV1 {
    let response_digest = cognition_response_digest(&response);
    ContinuousAgentResponseContextV1 {
        base_decision_response: response,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: 0,
        transport_attempt: 0,
        request_digest: context.request_digest.clone(),
        response_digest,
    }
}

fn validate_continuous_response_lineage(
    response: &ContinuousAgentResponseContextV1,
    context: &ContinuousAgentTurnContextV1,
    agent_id: &str,
) -> Result<(), DecisionProviderError> {
    context
        .validate_for_agent(agent_id)
        .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
    if response.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
        || response.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION
        || response.agent_session_id != context.agent_session_id
        || response.agent_turn_id != context.agent_turn_id
        || response.decision_request_id != context.decision_request_id
        || response.retry_seq != 0
        || response.transport_attempt != 0
        || response.request_digest != context.request_digest
    {
        return Err(DecisionProviderError::new(
            "response_identity_mismatch",
            "provider response does not match the host-bound cognition request",
            false,
        ));
    }
    match classify_cognition_response_digest(
        &response.base_decision_response,
        &response.response_digest,
    ) {
        CognitionResponseDigestDisposition::Current => {}
        CognitionResponseDigestDisposition::Legacy => {
            return Err(DecisionProviderError::new(
                "legacy_response_digest_unsupported",
                "legacy full-response digest requires an explicit compatibility migration",
                false,
            ));
        }
        CognitionResponseDigestDisposition::Mismatch => {
            return Err(DecisionProviderError::new(
                "response_digest_mismatch",
                "provider response digest does not match its content",
                false,
            ));
        }
    }
    let artifact_identity = response.response_artifact_identity();
    response
        .validate_response_artifact_identity(&artifact_identity)
        .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
    Ok(())
}

fn validate_request_context_pair(
    request: &DecisionRequest,
    turn_context: &ContinuousAgentTurnContextV1,
    request_context: &ContinuousAgentRequestContextV1,
) -> Result<(), DecisionProviderError> {
    request_context
        .validate_production_lane()
        .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
    turn_context
        .validate_for_agent(&request_context.agent_subject)
        .map_err(|error| DecisionProviderError::new(error.code(), error.message(), false))?;
    if request_context.base_decision_request != *request
        || request_context.agent_session_id != turn_context.agent_session_id
        || request_context.agent_turn_id != turn_context.agent_turn_id
        || request_context.decision_request_id != turn_context.decision_request_id
        || request_context.request_digest != turn_context.request_digest
        || request.observation.agent_id != request_context.agent_subject
    {
        return Err(DecisionProviderError::new(
            "request_context_identity_mismatch",
            "outer request, reduced context, and provider request are not the same turn",
            false,
        ));
    }
    Ok(())
}

fn validate_continuous_request_response_lineage(
    response: &ContinuousAgentResponseContextV1,
    request_context: &ContinuousAgentRequestContextV1,
    agent_id: &str,
) -> Result<(), DecisionProviderError> {
    if response.context_discriminator != CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
        || response.context_version != CONTINUOUS_AGENT_CONTEXT_VERSION
        || response.agent_session_id != request_context.agent_session_id
        || response.agent_turn_id != request_context.agent_turn_id
        || response.decision_request_id != request_context.decision_request_id
        || response.retry_seq != request_context.retry_seq
        || response.transport_attempt != request_context.transport_attempt
        || response.request_digest != request_context.request_digest
    {
        return Err(DecisionProviderError::new(
            "response_identity_mismatch",
            "provider response does not preserve the complete outer request lineage",
            false,
        ));
    }
    match classify_cognition_response_digest(
        &response.base_decision_response,
        &response.response_digest,
    ) {
        CognitionResponseDigestDisposition::Current => {}
        CognitionResponseDigestDisposition::Legacy => {
            return Err(DecisionProviderError::new(
                "legacy_response_digest_unsupported",
                "legacy full-response digest requires an explicit compatibility migration",
                false,
            ));
        }
        CognitionResponseDigestDisposition::Mismatch => {
            return Err(DecisionProviderError::new(
                "response_digest_mismatch",
                "provider response digest does not match its content",
                false,
            ));
        }
    }
    if request_context.agent_subject != agent_id {
        return Err(DecisionProviderError::new(
            "agent_identity_mismatch",
            "provider response request subject does not match actor",
            false,
        ));
    }
    Ok(())
}

pub struct ProviderBackedAgentBehavior<P: DecisionProvider> {
    agent_id: String,
    provider: P,
    action_catalog: Vec<ActionCatalogEntry>,
    module_command_catalog: Vec<ModuleCommandCatalogEntry>,
    capability_context: Option<ProviderCapabilityContext>,
    provider_config_ref: Option<String>,
    agent_profile: Option<String>,
    execution_mode: ProviderExecutionMode,
    observation_schema_version: String,
    action_schema_version: String,
    environment_class: Option<String>,
    fallback_reason: Option<String>,
    fixture_id: Option<String>,
    replay_id: Option<String>,
    timeout_budget_ms: u64,
    base_memory_summary: Option<String>,
    memory_summary: Option<String>,
    recent_event_summary: VecDeque<String>,
    pending_trace: Option<AgentDecisionTrace>,
    continuous_turn_context: Option<ContinuousAgentTurnContextV1>,
    continuous_request_context: Option<ContinuousAgentRequestContextV1>,
    require_continuous_request_context: bool,
    pending_response_context: Option<ContinuousAgentResponseContextV1>,
    pending_memory_write_intents: Vec<MemoryWriteIntent>,
}

impl<P: DecisionProvider> ProviderBackedAgentBehavior<P> {
    pub fn new(
        agent_id: impl Into<String>,
        provider: P,
        action_catalog: Vec<ActionCatalogEntry>,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            provider,
            action_catalog,
            module_command_catalog: Vec::new(),
            capability_context: None,
            provider_config_ref: None,
            agent_profile: None,
            execution_mode: ProviderExecutionMode::HeadlessAgent,
            observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
            action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
            environment_class: None,
            fallback_reason: None,
            fixture_id: None,
            replay_id: None,
            timeout_budget_ms: DEFAULT_PROVIDER_TIMEOUT_BUDGET_MS,
            base_memory_summary: None,
            memory_summary: None,
            recent_event_summary: VecDeque::new(),
            pending_trace: None,
            continuous_turn_context: None,
            continuous_request_context: None,
            require_continuous_request_context: true,
            pending_response_context: None,
            pending_memory_write_intents: Vec::new(),
        }
    }

    pub fn with_provider_config_ref(mut self, provider_config_ref: impl Into<String>) -> Self {
        self.provider_config_ref = Some(provider_config_ref.into());
        self
    }

    /// Explicit compatibility-only constructor for reduced `/decision` and
    /// `/feedback` adapters. Production callers use the target outer-context
    /// lane by default and must provide a complete request context.
    pub fn new_legacy_compatibility(
        agent_id: impl Into<String>,
        provider: P,
        action_catalog: Vec<ActionCatalogEntry>,
    ) -> Self {
        Self::new(agent_id, provider, action_catalog).legacy_compatibility()
    }

    /// Mark this behavior as an explicit legacy compatibility fixture.
    pub fn legacy_compatibility(mut self) -> Self {
        self.require_continuous_request_context = false;
        self
    }

    /// Reassert the production outer-context lane after configuration.
    pub fn require_continuous_request_context(mut self) -> Self {
        self.require_continuous_request_context = true;
        self
    }

    /// Attach the current active-module command projection for provider
    /// discovery. This is descriptive only; it does not grant authority.
    pub fn with_module_command_catalog(
        mut self,
        module_command_catalog: Vec<ModuleCommandCatalogEntry>,
    ) -> Self {
        self.module_command_catalog = module_command_catalog;
        self
    }

    /// Attach the runtime-produced subject-bound catalog and invocation
    /// context for the next provider request.  The provider receives a copy;
    /// the behavior retains the immutable context and validates the typed
    /// response before surfacing it as a module decision.
    pub fn with_capability_context(mut self, context: ProviderCapabilityContext) -> Self {
        self.set_capability_context(Some(context));
        self
    }

    pub fn set_capability_context(&mut self, context: Option<ProviderCapabilityContext>) {
        self.module_command_catalog = context
            .as_ref()
            .map(|context| {
                context
                    .catalog
                    .entries
                    .iter()
                    .map(|entry| ModuleCommandCatalogEntry {
                        module_id: entry.module_id.clone(),
                        module_version: entry.module_version.clone(),
                        namespace: entry.namespace.clone(),
                        name: entry.command.clone(),
                        schema_version: entry.schema_version,
                        schema_hash: entry.schema_hash.clone(),
                        max_payload_bytes: entry.max_payload_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.capability_context = context;
    }

    pub fn capability_context(&self) -> Option<&ProviderCapabilityContext> {
        self.capability_context.as_ref()
    }

    pub fn with_agent_profile(mut self, agent_profile: impl Into<String>) -> Self {
        self.agent_profile = Some(agent_profile.into());
        self
    }

    pub fn with_execution_mode(mut self, execution_mode: ProviderExecutionMode) -> Self {
        self.execution_mode = execution_mode;
        self
    }

    pub fn with_observation_schema_version(
        mut self,
        observation_schema_version: impl Into<String>,
    ) -> Self {
        self.observation_schema_version = observation_schema_version.into();
        self
    }

    pub fn with_action_schema_version(mut self, action_schema_version: impl Into<String>) -> Self {
        self.action_schema_version = action_schema_version.into();
        self
    }

    pub fn with_environment_class(mut self, environment_class: impl Into<String>) -> Self {
        self.environment_class = Some(environment_class.into());
        self
    }

    pub fn with_fallback_reason(mut self, fallback_reason: impl Into<String>) -> Self {
        self.fallback_reason = Some(fallback_reason.into());
        self
    }

    pub fn with_fixture_id(mut self, fixture_id: impl Into<String>) -> Self {
        self.fixture_id = Some(fixture_id.into());
        self
    }

    pub fn with_replay_id(mut self, replay_id: impl Into<String>) -> Self {
        self.replay_id = Some(replay_id.into());
        self
    }

    pub fn with_timeout_budget_ms(mut self, timeout_budget_ms: u64) -> Self {
        self.timeout_budget_ms = timeout_budget_ms.max(1);
        self
    }

    pub fn with_memory_summary(mut self, memory_summary: impl Into<String>) -> Self {
        self.base_memory_summary = Some(memory_summary.into());
        self
    }

    pub fn replace_memory_summary(&mut self, memory_summary: Option<String>) {
        self.base_memory_summary = memory_summary;
    }

    fn composed_memory_summary(&self) -> Option<String> {
        match (
            self.base_memory_summary.as_deref(),
            self.memory_summary.as_deref(),
        ) {
            (Some(base), Some(latest)) if latest != base => {
                Some(format!("{base}\nrecent_action_feedback={latest}"))
            }
            (Some(base), _) => Some(base.to_string()),
            (None, Some(latest)) => Some(latest.to_string()),
            (None, None) => None,
        }
    }

    fn push_recent_event_summary(&mut self, summary: String) {
        if self.recent_event_summary.len() >= MAX_RECENT_EVENT_SUMMARIES {
            self.recent_event_summary.pop_front();
        }
        self.recent_event_summary.push_back(summary);
    }

    pub fn push_player_message_feedback(
        &mut self,
        world_time: WorldTime,
        message: &str,
    ) -> Result<(), DecisionProviderError> {
        let message = message.trim();
        if message.is_empty() {
            return Ok(());
        }
        let summary = format!("player_message: {message}");
        self.push_recent_event_summary(summary.clone());
        self.memory_summary = Some(summary.clone());
        self.provider.push_feedback(&FeedbackEnvelope {
            action_id: world_time,
            success: true,
            reject_reason: None,
            emitted_events: Vec::new(),
            world_delta_summary: Some(summary),
        })
    }

    fn provider_error_to_trace(error: &DecisionProviderError) -> AgentDecisionTrace {
        decision_provider_trace::provider_error_to_trace(error)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable provider trace seam keeps request and response identity inputs explicit."
    )]
    fn response_to_trace(
        &self,
        observation: &Observation,
        request: &DecisionRequest,
        response: &DecisionResponse,
        outer_response: Option<&ContinuousAgentResponseContextV1>,
        decision: &AgentDecision,
        parse_error: Option<String>,
        provider_error: Option<String>,
    ) -> AgentDecisionTrace {
        decision_provider_trace::response_to_trace(
            &self.agent_id,
            observation,
            request,
            response,
            outer_response,
            decision,
            parse_error,
            provider_error,
        )
    }

    fn feedback_from_action_result(result: &ActionResult) -> FeedbackEnvelope {
        FeedbackEnvelope {
            action_id: result.action_id,
            success: result.success,
            reject_reason: result.reject_reason().map(|reason| format!("{reason:?}")),
            emitted_events: vec![result.event.clone()],
            world_delta_summary: Some(format!(
                "action={:?}; success={}; event={:?}",
                result.action, result.success, result.event.kind
            )),
        }
    }
}

impl<P: DecisionProvider> AgentBehavior for ProviderBackedAgentBehavior<P> {
    fn agent_id(&self) -> &str {
        self.agent_id.as_str()
    }

    fn decide(&mut self, observation: &Observation) -> AgentDecision {
        self.pending_memory_write_intents.clear();
        self.pending_response_context = None;
        if self.require_continuous_request_context && self.continuous_request_context.is_none() {
            let mut trace = Self::provider_error_to_trace(&DecisionProviderError::new(
                "continuous_context_required",
                "production ProviderBacked turns require the complete outer V1 context",
                false,
            ));
            trace.agent_id = self.agent_id.clone();
            trace.time = observation.time;
            self.pending_trace = Some(trace);
            return AgentDecision::Wait;
        }
        let request = self
            .continuous_request_context
            .as_ref()
            .map(|context| context.base_decision_request.clone())
            .unwrap_or_else(|| self.build_request(observation));
        let started_at = Instant::now();
        let mut outer_response = None;
        let response = if let (Some(turn_context), Some(request_context)) = (
            self.continuous_turn_context.as_ref(),
            self.continuous_request_context.as_ref(),
        ) {
            let response = self.provider.decide_with_continuous_request_context(
                &request,
                turn_context,
                request_context,
            );
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let mut trace = Self::provider_error_to_trace(&error);
                    trace.agent_id = self.agent_id.clone();
                    trace.time = observation.time;
                    self.pending_trace = Some(trace);
                    return AgentDecision::Wait;
                }
            };
            if let Err(error) = validate_continuous_request_response_lineage(
                &response,
                request_context,
                &self.agent_id,
            ) {
                let mut trace = Self::provider_error_to_trace(&error);
                trace.agent_id = self.agent_id.clone();
                trace.time = observation.time;
                self.pending_trace = Some(trace);
                return AgentDecision::Wait;
            }
            let inner_response = response.base_decision_response.clone();
            self.pending_response_context = Some(response.clone());
            outer_response = Some(response);
            inner_response
        } else if let Some(context) = self.continuous_turn_context.as_ref() {
            let response = match context.validate_for_agent(&self.agent_id) {
                Ok(()) => self
                    .provider
                    .decide_with_continuous_context(&request, context),
                Err(error) => Err(DecisionProviderError::new(
                    error.code(),
                    error.message(),
                    false,
                )),
            };
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let mut trace = Self::provider_error_to_trace(&error);
                    trace.agent_id = self.agent_id.clone();
                    trace.time = observation.time;
                    self.pending_trace = Some(trace);
                    return AgentDecision::Wait;
                }
            };
            if let Err(error) =
                validate_continuous_response_lineage(&response, context, &self.agent_id)
            {
                let mut trace = Self::provider_error_to_trace(&error);
                trace.agent_id = self.agent_id.clone();
                trace.time = observation.time;
                self.pending_trace = Some(trace);
                return AgentDecision::Wait;
            }
            let inner_response = response.base_decision_response.clone();
            self.pending_response_context = Some(response.clone());
            outer_response = Some(response);
            inner_response
        } else {
            match self.provider.decide(&request) {
                Ok(response) => response,
                Err(error) => {
                    let mut trace = Self::provider_error_to_trace(&error);
                    trace.agent_id = self.agent_id.clone();
                    trace.time = observation.time;
                    self.pending_trace = Some(trace);
                    return AgentDecision::Wait;
                }
            }
        };
        self.pending_memory_write_intents = response.memory_write_intents.clone();
        let latency_ms = started_at.elapsed().as_millis() as u64;
        let mut provider_error = response
            .provider_error
            .as_ref()
            .map(|error| format!("{}: {}", error.code, error.message));
        let (decision, parse_error) = if let Some(module_command) = response.module_command.as_ref()
        {
            provider_error = provider_error.or_else(|| {
                Some("module_command_unroutable: no trusted module executor".to_string())
            });
            (
                AgentDecision::Wait,
                Some(format!(
                    "module_command_unroutable: {}:{}:{}:{}",
                    module_command.module_id,
                    module_command.module_version,
                    module_command.namespace,
                    module_command.name
                )),
            )
        } else {
            match &response.decision {
                ProviderDecision::Wait => (AgentDecision::Wait, None),
                ProviderDecision::WaitTicks { ticks } => (AgentDecision::WaitTicks(*ticks), None),
                ProviderDecision::Act { action_ref, action } => {
                    if self
                        .action_catalog
                        .iter()
                        .any(|entry| entry.action_ref == *action_ref)
                    {
                        (AgentDecision::Act(action.clone()), None)
                    } else {
                        provider_error = provider_error
                            .or_else(|| Some(format!("provider_invalid_action_ref: {action_ref}")));
                        (
                            AgentDecision::Wait,
                            Some(format!(
                                "unknown action_ref returned by provider: {action_ref}"
                            )),
                        )
                    }
                }
                ProviderDecision::Query { query_ref, query } => {
                    if self
                        .action_catalog
                        .iter()
                        .any(|entry| entry.action_ref == *query_ref)
                    {
                        (AgentDecision::Query(query.clone()), None)
                    } else {
                        provider_error = provider_error
                            .or_else(|| Some(format!("provider_invalid_query_ref: {query_ref}")));
                        (
                            AgentDecision::Wait,
                            Some(format!(
                                "unknown query_ref returned by provider: {query_ref}"
                            )),
                        )
                    }
                }
                ProviderDecision::ModuleCommand { module_command } => {
                    provider_error = provider_error.or_else(|| {
                        Some("module_command_unroutable: no trusted module executor".to_string())
                    });
                    (
                        AgentDecision::Wait,
                        Some(format!(
                            "module_command_unroutable: {}:{}:{}:{}",
                            module_command.module_id,
                            module_command.module_version,
                            module_command.namespace,
                            module_command.name
                        )),
                    )
                }
                ProviderDecision::ModuleCommandResponse { response } => {
                    match self.capability_context.as_ref() {
                        Some(context) => match context.validate_response(response) {
                            Ok(()) => (
                                AgentDecision::ModuleCommand {
                                    response: response.clone(),
                                },
                                None,
                            ),
                            Err(error) => {
                                provider_error = provider_error.or_else(|| Some(error.to_string()));
                                (
                                    AgentDecision::Wait,
                                    Some(format!("{}: {}", error.code, error.message)),
                                )
                            }
                        },
                        None => {
                            provider_error = provider_error
                                .or_else(|| Some("module_command_context_unavailable".to_string()));
                            (
                                AgentDecision::Wait,
                                Some(
                                    "module_command_context_unavailable: trusted runtime context is not attached"
                                        .to_string(),
                                ),
                            )
                        }
                    }
                }
            }
        };
        let mut response = response;
        if response.diagnostics.latency_ms.is_none() {
            response.diagnostics.latency_ms = Some(latency_ms);
        }
        if response.trace_payload.latency_ms.is_none() {
            response.trace_payload.latency_ms = Some(latency_ms);
        }
        let trace = self.response_to_trace(
            observation,
            &request,
            &response,
            outer_response.as_ref(),
            &decision,
            parse_error,
            provider_error,
        );
        self.pending_trace = Some(trace);
        decision
    }

    fn on_action_result(&mut self, result: &ActionResult) {
        let feedback = Self::feedback_from_action_result(result);
        let summary = feedback
            .world_delta_summary
            .clone()
            .unwrap_or_else(|| "action_feedback=unknown".to_string());
        self.push_recent_event_summary(summary.clone());
        self.memory_summary = Some(summary);
        let _ = self.provider.push_feedback(&feedback);
    }

    fn on_query_result(&mut self, result: &AgentQueryResult) {
        let summary = serde_json::to_string(result)
            .unwrap_or_else(|_| "query_feedback=serialization_failed".to_string());
        self.push_recent_event_summary(summary.clone());
        self.memory_summary = Some(summary);
    }

    fn on_event(&mut self, event: &WorldEvent) {
        self.push_recent_event_summary(format!("event: {:?}", event.kind));
        let _ = self.provider.on_world_event(event);
    }

    fn take_decision_trace(&mut self) -> Option<AgentDecisionTrace> {
        self.pending_trace.take()
    }

    fn set_continuous_turn_context(&mut self, context: Option<&ContinuousAgentTurnContextV1>) {
        self.continuous_turn_context = context.cloned();
    }

    fn set_continuous_request_context(
        &mut self,
        context: Option<&ContinuousAgentRequestContextV1>,
    ) {
        self.continuous_request_context = context.cloned();
    }

    fn take_continuous_response_context(&mut self) -> Option<ContinuousAgentResponseContextV1> {
        self.pending_response_context.take()
    }

    fn take_memory_write_intents(&mut self) -> Vec<MemoryWriteIntent> {
        std::mem::take(&mut self.pending_memory_write_intents)
    }
}
