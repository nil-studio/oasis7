//! Non-blocking simulator actor lifecycle for in-world Agents.
//!
//! `AgentRunner` is the legacy synchronous observe/decide/act adapter.  This
//! module owns the production boundary for provider-backed turns: each
//! registered behavior lives behind a bounded actor mailbox and the world
//! thread only performs non-blocking sends/receives.  A provider may take an
//! arbitrary amount of time (or fail); that work cannot hold a world step.
//!
//! The actor deliberately returns a decision outcome instead of applying a
//! world action.  Runtime owns the later MVCC/authority validation and effect
//! commit.  This keeps the async boundary useful for both Builtin and
//! ProviderBacked behaviors without granting an actor authority over the
//! world.
use super::WorldKernel;
use super::archive::WorldEvent;

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::runtime::{CognitionLeaseV1, RuntimeReceiptLineageV1};

use self::agent_harness_adapter::{
    AdapterControl, BehaviorCallback, BehaviorTurnResult, OasisBehaviorProvider,
    production_key_from_context,
};
use super::Observation;
use super::agent::{ActionResult, AgentBehavior, AgentDecision, AgentDecisionTrace, AgentQuery};
use super::cognition_policy::{
    ContinuationHandle, ContinuationHarness, ContinuationInvalidationReason,
    MemoryWriteIntentPolicyV1, MemoryWritePolicyContextV1, MemoryWriteStore,
};
use super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1, MemoryWriteIntentV1, h_v1,
};
use super::decision_provider::{
    ActionCatalogEntry, MemoryWriteIntent, MockDecisionProvider, ProviderBackedAgentBehavior,
};

use oasis7_agent_harness::{
    AsyncAgentRunner as HarnessRunner, AsyncAgentRunnerError as HarnessRunnerError,
    AsyncAgentTurnOutcome as HarnessTurnOutcome, AsyncTurnId as HarnessTurnId,
    CompatibilityTurnRequest as HarnessCompatibilityTurnRequest, RuntimeAuthority,
};
use oasis7_simulator::WorldTime;

pub(crate) struct CommittedFeedbackMemoryInput<'a> {
    pub(crate) agent: &'a str,
    pub(crate) feedback: FeedbackEnvelopeV1,
    pub(crate) receipt: &'a RuntimeReceiptLineageV1,
}

#[path = "agent_harness_adapter.rs"]
mod agent_harness_adapter;

#[path = "async_agent_runner_feedback.rs"]
mod feedback;
pub use self::feedback::{RuntimeReceiptReadbackHandleV1, RuntimeReceiptReadbackVerifier};
use self::feedback::{validate_feedback, validate_runtime_receipt_lineage};
#[path = "async_agent_runner_receipt_memory.rs"]
mod receipt_memory;
pub(crate) use receipt_memory::project_receipt_memory;
#[path = "async_agent_runner_outcome.rs"]
mod outcome;
use self::outcome::{
    compatibility_outcome_from_harness, default_observation, outcome_from_completion,
    validate_cognition_lease_for_request,
};
#[path = "async_agent_runner_lease_view.rs"]
mod lease_view;
pub(crate) use self::lease_view::validated_cognition_lease_consumption_view;
#[path = "async_agent_runner_actor_controls.rs"]
mod actor_controls;
#[path = "async_agent_runner_budget.rs"]
mod budget;
#[path = "async_agent_runner_continuation.rs"]
mod continuation;
#[path = "async_agent_runner_retry.rs"]
mod retry;
#[path = "async_agent_runner_runtime_feedback.rs"]
mod runtime_feedback;
#[path = "async_agent_runner_terminal_budget.rs"]
mod terminal_budget;
#[path = "async_agent_runner_test_support.rs"]
mod test_support;
use self::test_support::{BlockingProviderBehavior, BuiltinWaitBehavior};

const DEFAULT_MAILBOX_CAPACITY: usize = 16;
/// Stable identifier for an actor turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AsyncTurnId(u64);

impl AsyncTurnId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Lifecycle of a decision turn at the async simulator boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncTurnLifecycle {
    Completed,
    Failed,
}

/// Feedback produced by the actor after deciding.  Runtime may append the
/// later action receipt without changing this initial provider outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncTurnFeedback {
    Wait,
    WaitTicks(u64),
    ActionProposed,
    QueryProposed,
    ModuleCommandProposed,
    ProviderError { code: String },
    ActorPanicked,
}

/// The async actor's world-facing proposal.  `NoEffect` is intentional:
/// actual state mutation is outside this simulator actor and remains owned by
/// runtime validation/commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncWorldEffect {
    NoEffect,
    ActionProposal,
    QueryProposal,
    ModuleCommandProposal,
}

/// A completed actor outcome.  The full decision and trace are retained so a
/// runtime adapter can bind them to an envelope and preserve diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AsyncAgentTurnOutcome {
    pub turn_id: AsyncTurnId,
    pub agent_id: String,
    pub lifecycle: AsyncTurnLifecycle,
    pub feedback: AsyncTurnFeedback,
    pub world_effect: AsyncWorldEffect,
    pub decision: Option<AgentDecision>,
    pub decision_trace: Option<AgentDecisionTrace>,
    pub prepared_context: Option<ContinuousAgentTurnContextV1>,
    /// The full trusted outer V1 request when this turn used the production
    /// provider lane. It is retained through completion for Runtime lineage.
    #[serde(default)]
    pub prepared_request_context: Option<ContinuousAgentRequestContextV1>,
    #[serde(default)]
    pub prepared_response_context: Option<ContinuousAgentResponseContextV1>,
    /// Runtime-issued reservation carried through the provider outcome. The
    /// Agent runner may validate and retain this identity, but only Runtime
    /// can settle, release, or refund the lease.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cognition_lease: Option<CognitionLeaseV1>,
    pub memory_write_intents: Vec<MemoryWriteIntent>,
}

/// Non-blocking progress report returned by a world step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncWorldProgress {
    pub logical_tick: WorldTime,
    pub completed_turns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncAgentRunnerError {
    InvalidCapacity,
    AgentAlreadyRegistered(String),
    AgentNotRegistered(String),
    AgentBusy(String),
    MailboxFull,
    AgentIdentityMismatch { expected: String, observed: String },
    ActorUnavailable(String),
    FeedbackUnavailable(String),
    Cognition(String),
}

impl AsyncAgentRunnerError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidCapacity => "invalid_capacity",
            Self::AgentAlreadyRegistered(_) => "agent_already_registered",
            Self::AgentNotRegistered(_) => "agent_not_registered",
            Self::AgentBusy(_) => "agent_busy",
            Self::MailboxFull => "mailbox_full",
            Self::AgentIdentityMismatch { .. } => "agent_identity_mismatch",
            Self::ActorUnavailable(_) => "actor_unavailable",
            Self::FeedbackUnavailable(_) => "feedback_unavailable",
            Self::Cognition(_) => "cognition_error",
        }
    }
}

impl fmt::Display for AsyncAgentRunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity => write!(f, "actor mailbox capacity must be greater than zero"),
            Self::AgentAlreadyRegistered(agent_id) => {
                write!(f, "agent is already registered: {agent_id}")
            }
            Self::AgentNotRegistered(agent_id) => write!(f, "agent is not registered: {agent_id}"),
            Self::AgentBusy(agent_id) => write!(f, "agent already has an active turn: {agent_id}"),
            Self::MailboxFull => write!(f, "async agent mailbox is full"),
            Self::AgentIdentityMismatch { expected, observed } => {
                write!(
                    f,
                    "observation agent identity mismatch: expected {expected}, got {observed}"
                )
            }
            Self::ActorUnavailable(agent_id) => write!(f, "actor is unavailable: {agent_id}"),
            Self::FeedbackUnavailable(agent_id) => {
                write!(f, "actor feedback mailbox is unavailable: {agent_id}")
            }
            Self::Cognition(message) => write!(f, "cognition context error: {message}"),
        }
    }
}

impl std::error::Error for AsyncAgentRunnerError {}

struct ActorCompletion {
    turn_id: AsyncTurnId,
    agent_id: String,
    decision: Option<AgentDecision>,
    decision_trace: Option<AgentDecisionTrace>,
    prepared_context: Option<ContinuousAgentTurnContextV1>,
    prepared_request_context: Option<ContinuousAgentRequestContextV1>,
    prepared_response_context: Option<ContinuousAgentResponseContextV1>,
    memory_write_intents: Vec<MemoryWriteIntent>,
    panicked: bool,
}

fn map_harness_error(error: HarnessRunnerError) -> AsyncAgentRunnerError {
    match error {
        HarnessRunnerError::InvalidCapacity => AsyncAgentRunnerError::InvalidCapacity,
        HarnessRunnerError::AgentAlreadyRegistered(agent) => {
            AsyncAgentRunnerError::AgentAlreadyRegistered(agent)
        }
        HarnessRunnerError::AgentNotRegistered(agent) => {
            AsyncAgentRunnerError::AgentNotRegistered(agent)
        }
        HarnessRunnerError::AgentBusy(agent) => AsyncAgentRunnerError::AgentBusy(agent),
        HarnessRunnerError::MailboxFull => AsyncAgentRunnerError::MailboxFull,
        HarnessRunnerError::ActorUnavailable(agent) => {
            AsyncAgentRunnerError::ActorUnavailable(agent)
        }
        HarnessRunnerError::Cognition(error) => AsyncAgentRunnerError::Cognition(error.to_string()),
    }
}

fn production_outcome_from_harness(
    turn_id: AsyncTurnId,
    completion: HarnessTurnOutcome<oasis7_simulator::Action, AgentQuery>,
    result: Option<BehaviorTurnResult>,
    metadata: TurnMetadata,
    cognition_lease: Option<CognitionLeaseV1>,
) -> AsyncAgentTurnOutcome {
    let panicked = completion
        .provider_error
        .as_ref()
        .is_some_and(|error| error.code == "actor_panicked");
    let provider_error = completion.provider_error.clone();
    let result = result.unwrap_or(BehaviorTurnResult {
        decision: AgentDecision::Wait,
        decision_trace: None,
        prepared_context: metadata.context.clone(),
        prepared_response_context: None,
        memory_write_intents: Vec::new(),
    });
    let mut outcome = outcome_from_completion(ActorCompletion {
        turn_id,
        agent_id: completion.agent_id,
        decision: Some(result.decision),
        decision_trace: result.decision_trace,
        prepared_context: result.prepared_context.or(metadata.context),
        prepared_request_context: metadata.request_context,
        prepared_response_context: result.prepared_response_context,
        memory_write_intents: result.memory_write_intents,
        panicked,
    });
    if let Some(error) = provider_error {
        outcome.lifecycle = AsyncTurnLifecycle::Failed;
        outcome.feedback = if error.code == "actor_panicked" {
            AsyncTurnFeedback::ActorPanicked
        } else {
            AsyncTurnFeedback::ProviderError { code: error.code }
        };
        outcome.world_effect = AsyncWorldEffect::NoEffect;
        outcome.decision = None;
    }
    outcome.cognition_lease = cognition_lease;
    outcome
}

struct AgentActor {
    control: AdapterControl,
    cancel_on_drop: Option<std::sync::Arc<AtomicBool>>,
}

impl Drop for AgentActor {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.cancel_on_drop {
            cancelled.store(true, std::sync::atomic::Ordering::Release);
        }
    }
}

struct CompatibilityTurnRecord {
    harness_turn_id: HarnessTurnId,
}

#[derive(Clone)]
struct TurnMetadata {
    context: Option<ContinuousAgentTurnContextV1>,
    request_context: Option<ContinuousAgentRequestContextV1>,
}

const MAX_RUNTIME_FEEDBACK_REPLAY_ENTRIES: usize = 8;

#[derive(Clone)]
struct RuntimeFeedbackReplayMetadata {
    receipt_lineage: Option<RuntimeReceiptLineageV1>,
}

type SharedHarnessRunner = HarnessRunner<
    oasis7_simulator::Action,
    AgentQuery,
    Observation,
    BehaviorTurnResult,
    BehaviorCallback,
>;

/// Bounded, non-blocking actor runner for simulator Agent behaviors.
pub struct AsyncAgentRunner {
    actors: BTreeMap<String, AgentActor>,
    harness: SharedHarnessRunner,
    harness_turns: BTreeMap<AsyncTurnId, HarnessTurnId>,
    compatibility_turns: BTreeMap<AsyncTurnId, CompatibilityTurnRecord>,
    turn_metadata: BTreeMap<AsyncTurnId, TurnMetadata>,
    mailbox_capacity: usize,
    active_turns: usize,
    next_turn_id: u64,
    logical_tick: WorldTime,
    completed: VecDeque<AsyncAgentTurnOutcome>,
    /// Outcomes remain here until Runtime terminal feedback; draining the
    /// world-facing completion queue must not release cognition single-flight.
    awaiting_runtime: BTreeMap<String, AsyncTurnId>,
    awaiting_outcomes: BTreeMap<AsyncTurnId, AsyncAgentTurnOutcome>,
    /// Leases are staged by turn until the actor returns. They are moved into
    /// the outcome before it is exposed to the world-facing queue.
    cognition_leases: BTreeMap<AsyncTurnId, CognitionLeaseV1>,
    /// Host receipt metadata for the same bounded envelope replay window the
    /// Harness retains. This is correlation state only, never proof creation.
    runtime_feedback_replays: BTreeMap<(String, String, String), RuntimeFeedbackReplayMetadata>,
    runtime_feedback_replay_order: BTreeMap<(String, String), VecDeque<String>>,
    continuation_harness: ContinuationHarness,
    continuations: BTreeMap<String, ContinuationHandle>,
}

impl AsyncAgentRunner {
    pub fn new(mailbox_capacity: usize) -> Result<Self, AsyncAgentRunnerError> {
        if mailbox_capacity == 0 {
            return Err(AsyncAgentRunnerError::InvalidCapacity);
        }
        Ok(Self {
            actors: BTreeMap::new(),
            harness: HarnessRunner::new(mailbox_capacity).map_err(map_harness_error)?,
            harness_turns: BTreeMap::new(),
            compatibility_turns: BTreeMap::new(),
            turn_metadata: BTreeMap::new(),
            mailbox_capacity,
            active_turns: 0,
            next_turn_id: 0,
            logical_tick: 0,
            completed: VecDeque::new(),
            awaiting_runtime: BTreeMap::new(),
            awaiting_outcomes: BTreeMap::new(),
            cognition_leases: BTreeMap::new(),
            runtime_feedback_replays: BTreeMap::new(),
            runtime_feedback_replay_order: BTreeMap::new(),
            continuation_harness: ContinuationHarness::default(),
            continuations: BTreeMap::new(),
        })
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_MAILBOX_CAPACITY).expect("default actor mailbox capacity is valid")
    }

    pub fn mailbox_capacity(&self) -> usize {
        self.mailbox_capacity
    }

    pub fn register<B>(&mut self, behavior: B) -> Result<(), AsyncAgentRunnerError>
    where
        B: AgentBehavior + Send + 'static,
    {
        self.register_boxed(Box::new(behavior))
    }

    pub fn register_boxed(
        &mut self,
        behavior: Box<dyn AgentBehavior + Send>,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.register_boxed_with_shutdown(behavior, None)
    }

    fn register_boxed_with_shutdown(
        &mut self,
        behavior: Box<dyn AgentBehavior + Send>,
        cancel_on_drop: Option<std::sync::Arc<AtomicBool>>,
    ) -> Result<(), AsyncAgentRunnerError> {
        let agent_id = behavior.agent_id().to_string();
        if self.actors.contains_key(&agent_id) {
            return Err(AsyncAgentRunnerError::AgentAlreadyRegistered(agent_id));
        }
        let control = AdapterControl::new();
        self.harness
            .register_dual(
                agent_id.clone(),
                OasisBehaviorProvider::new(behavior, control.clone()),
            )
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let actor = AgentActor {
            control,
            cancel_on_drop,
        };
        self.actors.insert(agent_id, actor);
        Ok(())
    }

    pub fn agent_count(&self) -> usize {
        self.actors.len()
    }

    pub fn logical_tick(&self) -> WorldTime {
        self.logical_tick
    }

    /// Align the actor scheduler's observation clock with the authoritative
    /// Runtime clock before a host admits a new provider turn. A provider
    /// lease is reserved at that Runtime tick and must never appear to the
    /// actor as reserved in the future.
    pub fn sync_logical_tick(&mut self, logical_tick: WorldTime) {
        self.logical_tick = self.logical_tick.max(logical_tick);
    }

    pub fn active_turn_count(&self) -> usize {
        self.active_turns
    }

    pub fn provider_is_still_in_flight(&self, agent_id: &str) -> bool {
        self.harness.provider_is_still_in_flight(agent_id)
    }

    /// Legacy compatibility lane. Production cognition must use
    /// `start_turn_with_request_context`; this method intentionally has no
    /// Runtime binding or retry lineage.
    pub fn start_turn(&mut self, agent_id: &str) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        let observation = default_observation(agent_id, self.logical_tick);
        self.start_turn_with_context_and_observation(agent_id, observation, None)
    }

    /// Legacy compatibility lane without the trusted outer V1 context.
    pub fn start_turn_with_observation(
        &mut self,
        agent_id: &str,
        observation: Observation,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        self.start_turn_with_context_and_observation(agent_id, observation, None)
    }

    /// Reduced V1 compatibility lane. Production target callers must provide
    /// the complete outer context with `start_turn_with_request_context`.
    pub fn start_turn_with_context(
        &mut self,
        agent_id: &str,
        context: ContinuousAgentTurnContextV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        let observation = default_observation(agent_id, self.logical_tick);
        self.start_turn_with_context_and_observation(agent_id, observation, Some(context))
    }

    /// Start an explicitly reduced, non-production turn while preserving the
    /// caller's simulator observation. This is the host adapter used by the
    /// legacy native `TurnEngine` facade; it does not bind the outer request,
    /// admit a Runtime lease, or imply that durable feedback history has been
    /// restored. Committed memory feedback still requires the strict Runtime
    /// readback path.
    pub(crate) fn start_compatibility_turn_with_context_and_observation(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: ContinuousAgentTurnContextV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        self.start_turn_with_context_and_observation(agent_id, observation, Some(context))
    }

    /// Start a production-target provider turn with the complete trusted
    /// outer request. The reduced context remains present for behavior APIs,
    /// while the outer request carries retry, transport, and Runtime binding.
    pub fn start_turn_with_request_context(
        &mut self,
        agent_id: &str,
        context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        request_context
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if request_context.agent_subject != agent_id
            || request_context.agent_session_id != context.agent_session_id
            || request_context.agent_turn_id != context.agent_turn_id
            || request_context.decision_request_id != context.decision_request_id
            || request_context.request_digest != context.request_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "outer and reduced cognition contexts do not correlate".to_string(),
            ));
        }
        let observation = default_observation(agent_id, self.logical_tick);
        self.start_turn_with_context_and_observation_and_request(
            agent_id,
            observation,
            Some(context),
            Some(request_context),
            None,
        )
    }

    /// Start a production provider turn only after Runtime has reserved a
    /// cognition lease for this exact request identity. The runner validates
    /// the returned lease before queueing the actor command; settlement,
    /// release, refund, and receipt projection remain Runtime operations.
    pub fn start_turn_with_request_context_and_lease(
        &mut self,
        agent_id: &str,
        context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
        cognition_lease: CognitionLeaseV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        let observation = default_observation(agent_id, self.logical_tick);
        self.start_turn_with_request_context_and_observation_and_lease(
            agent_id,
            observation,
            context,
            request_context,
            cognition_lease,
        )
    }

    /// Observation-preserving form of
    /// [`Self::start_turn_with_request_context_and_lease`]. The observation is
    /// captured by the same host admission that produced the request context.
    pub fn start_turn_with_request_context_and_observation_and_lease(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
        cognition_lease: CognitionLeaseV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        request_context
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if request_context.agent_subject != agent_id
            || request_context.agent_session_id != context.agent_session_id
            || request_context.agent_turn_id != context.agent_turn_id
            || request_context.decision_request_id != context.decision_request_id
            || request_context.request_digest != context.request_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "outer and reduced cognition contexts do not correlate".to_string(),
            ));
        }
        validate_cognition_lease_for_request(
            agent_id,
            &request_context,
            &cognition_lease,
            self.logical_tick,
        )?;
        self.start_turn_with_context_and_observation_and_request(
            agent_id,
            observation,
            Some(context),
            Some(request_context),
            Some(cognition_lease),
        )
    }

    pub fn start_turn_with_request_context_and_observation(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: ContinuousAgentTurnContextV1,
        request_context: ContinuousAgentRequestContextV1,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        context
            .validate_for_agent(agent_id)
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        request_context
            .validate_production_lane()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        if request_context.agent_subject != agent_id
            || request_context.agent_session_id != context.agent_session_id
            || request_context.agent_turn_id != context.agent_turn_id
            || request_context.decision_request_id != context.decision_request_id
            || request_context.request_digest != context.request_digest
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "outer and reduced cognition contexts do not correlate".to_string(),
            ));
        }
        self.start_turn_with_context_and_observation_and_request(
            agent_id,
            observation,
            Some(context),
            Some(request_context),
            None,
        )
    }

    fn start_turn_with_context_and_observation(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: Option<ContinuousAgentTurnContextV1>,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        self.start_turn_with_context_and_observation_and_request(
            agent_id,
            observation,
            context,
            None,
            None,
        )
    }

    fn start_turn_with_context_and_observation_and_request(
        &mut self,
        agent_id: &str,
        observation: Observation,
        context: Option<ContinuousAgentTurnContextV1>,
        request_context: Option<ContinuousAgentRequestContextV1>,
        cognition_lease: Option<CognitionLeaseV1>,
    ) -> Result<AsyncTurnId, AsyncAgentRunnerError> {
        if self
            .continuations
            .get(agent_id)
            .is_some_and(|continuation| {
                continuation.active
                    && !context.as_ref().is_some_and(|context| {
                        context.continuation.as_ref().is_some_and(|next| {
                            next.continuation_proposal_id
                                == continuation.proposal.continuation_proposal_id
                                && next.proposal_digest == continuation.proposal.proposal_digest
                                && next.agent_session_id == continuation.proposal.agent_session_id
                                && next.agent_turn_id == continuation.proposal.agent_turn_id
                                && next.decision_request_id
                                    == continuation.proposal.decision_request_id
                        })
                    })
            })
        {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        if self.awaiting_runtime.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let actor = self
            .actors
            .get(agent_id)
            .ok_or_else(|| AsyncAgentRunnerError::AgentNotRegistered(agent_id.to_string()))?;
        if observation.agent_id != agent_id {
            return Err(AsyncAgentRunnerError::AgentIdentityMismatch {
                expected: agent_id.to_string(),
                observed: observation.agent_id,
            });
        }
        if self.harness.provider_is_still_in_flight(agent_id) {
            return Err(AsyncAgentRunnerError::AgentBusy(agent_id.to_string()));
        }
        let turn_id = AsyncTurnId(self.next_turn_id);
        self.next_turn_id = self.next_turn_id.saturating_add(1);
        let harness_turn_id = if let Some(request_context) = request_context.as_ref() {
            let key = production_key_from_context(request_context);
            actor
                .control
                .install_production_input(
                    key.clone(),
                    observation,
                    context.clone().ok_or_else(|| {
                        AsyncAgentRunnerError::Cognition(
                            "production turn context is missing".to_string(),
                        )
                    })?,
                    request_context.clone(),
                )
                .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
            match self
                .harness
                .start_turn(super::agent_api_compat::request_context_to_api(
                    request_context,
                )) {
                Ok(harness_turn_id) => harness_turn_id,
                Err(error) => {
                    actor.control.clear_production_input(&key);
                    return Err(map_harness_error(error).with_agent(agent_id));
                }
            }
        } else {
            self.harness
                .start_compatibility_turn(HarnessCompatibilityTurnRequest {
                    agent_subject: agent_id.to_string(),
                    turn_context: context.clone(),
                    input: observation,
                })
                .map_err(|error| map_harness_error(error).with_agent(agent_id))?
        };
        self.harness_turns.insert(turn_id, harness_turn_id);
        self.turn_metadata.insert(
            turn_id,
            TurnMetadata {
                context: context.clone(),
                request_context: request_context.clone(),
            },
        );
        if request_context.is_none() {
            self.compatibility_turns
                .insert(turn_id, CompatibilityTurnRecord { harness_turn_id });
        }
        if let Some(lease) = cognition_lease {
            self.cognition_leases.insert(turn_id, lease);
        }
        self.active_turns = self.active_turns.saturating_add(1);
        Ok(turn_id)
    }

    /// Poll all actor completion mailboxes without blocking.
    pub fn poll_completed(&mut self) -> Result<Vec<AsyncAgentTurnOutcome>, AsyncAgentRunnerError> {
        let mut outcomes = Vec::new();
        // This compatibility poll advances the shared actor mailbox for both
        // lanes. Drain its newly completed compatibility outcomes first, then
        // drain the production queue below; a delta-only production poll would
        // miss production completions already pumped by this call.
        for completion in self
            .harness
            .poll_compatibility_completed()
            .map_err(map_harness_error)?
        {
            let Some(main_turn_id) = self
                .harness_turns
                .iter()
                .find_map(|(main, harness)| (*harness == completion.turn_id).then_some(*main))
            else {
                return Err(AsyncAgentRunnerError::Cognition(
                    "Harness completed an unmapped compatibility turn".to_string(),
                ));
            };
            self.active_turns = self.active_turns.saturating_sub(1);
            let metadata = self
                .turn_metadata
                .get(&main_turn_id)
                .cloned()
                .ok_or_else(|| {
                    AsyncAgentRunnerError::Cognition(
                        "Harness completed a compatibility turn without host metadata".to_string(),
                    )
                })?;
            let outcome = compatibility_outcome_from_harness(main_turn_id, completion, metadata);
            if outcome.prepared_context.is_some() {
                self.awaiting_runtime
                    .insert(outcome.agent_id.clone(), main_turn_id);
                self.awaiting_outcomes.insert(main_turn_id, outcome.clone());
            } else {
                self.compatibility_turns.remove(&main_turn_id);
                self.harness_turns.remove(&main_turn_id);
            }
            self.turn_metadata.remove(&main_turn_id);
            self.completed.push_back(outcome.clone());
            outcomes.push(outcome);
        }
        // The compatibility poll above already pumps the shared Harness
        // mailbox. Drain the accumulated production queue here rather than
        // polling it for only newly pumped outcomes; otherwise a production
        // completion observed during the compatibility pass is stranded.
        for completion in self.harness.take_completed() {
            let Some(main_turn_id) = self
                .harness_turns
                .iter()
                .find_map(|(main, harness)| (*harness == completion.turn_id).then_some(*main))
            else {
                return Err(AsyncAgentRunnerError::Cognition(
                    "Harness completed an unmapped production turn".to_string(),
                ));
            };
            self.active_turns = self.active_turns.saturating_sub(1);
            let agent_id = completion.agent_id.clone();
            let request_context = super::agent_api_compat::request_context_from_api(
                completion.request_context.clone(),
            );
            let behavior_result = self.actors.get(&agent_id).and_then(|actor| {
                actor
                    .control
                    .take_production_result(&production_key_from_context(&request_context))
            });
            let lease = self.cognition_leases.remove(&main_turn_id);
            // Runtime may request a transport-only retry while this outcome is
            // awaiting terminal feedback. Keep the host context with that
            // lease so the retry completion can be projected identically.
            let metadata = self
                .turn_metadata
                .get(&main_turn_id)
                .cloned()
                .ok_or_else(|| {
                    AsyncAgentRunnerError::Cognition(
                        "Harness completed a production turn without host metadata".to_string(),
                    )
                })?;
            let outcome = production_outcome_from_harness(
                main_turn_id,
                completion,
                behavior_result,
                metadata,
                lease,
            );
            if outcome.prepared_context.is_some() {
                self.awaiting_runtime.insert(agent_id.clone(), main_turn_id);
                self.awaiting_outcomes.insert(main_turn_id, outcome.clone());
            }
            self.completed.push_back(outcome.clone());
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    pub fn take_completed(&mut self) -> Vec<AsyncAgentTurnOutcome> {
        self.completed.drain(..).collect()
    }

    /// Advance the simulator's logical clock and poll actor results.  There
    /// is no provider call, receive wait, or behavior invocation on this path.
    pub fn step_world_without_waiting_for_provider(
        &mut self,
    ) -> Result<AsyncWorldProgress, AsyncAgentRunnerError> {
        self.logical_tick = self.logical_tick.saturating_add(1);
        let completed_turns = self.poll_completed()?.len();
        Ok(AsyncWorldProgress {
            logical_tick: self.logical_tick,
            completed_turns,
        })
    }

    /// Runtime-facing adapter: execute one kernel step while keeping provider
    /// work outside the world thread.  Kernel action application is still
    /// synchronous and authoritative; only actor polling is async here.
    pub fn step_kernel_without_waiting_for_provider(
        &mut self,
        kernel: &mut WorldKernel,
    ) -> Result<(Option<WorldEvent>, AsyncWorldProgress), AsyncAgentRunnerError> {
        let event = kernel.step();
        self.logical_tick = kernel.time();
        let completed_turns = self.poll_completed()?.len();
        Ok((
            event,
            AsyncWorldProgress {
                logical_tick: self.logical_tick,
                completed_turns,
            },
        ))
    }

    /// Wait for one explicitly requested turn.  World steps must use
    /// `step_world_without_waiting_for_provider`; this convenience is for
    /// tests/CLI adapters that intentionally await an actor outcome.
    pub fn run_one_turn(&mut self) -> Result<AsyncAgentTurnOutcome, AsyncAgentRunnerError> {
        let agent_id = self
            .actors
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| AsyncAgentRunnerError::AgentNotRegistered("<none>".to_string()))?;
        let turn_id = self.start_turn(&agent_id)?;
        loop {
            if let Some(outcome) = self
                .poll_completed()?
                .into_iter()
                .find(|outcome| outcome.turn_id == turn_id)
            {
                return Ok(outcome);
            }
            thread::yield_now();
        }
    }

    /// Send an action result to the actor without executing callback code on
    /// the world thread.  The callback is bounded by the actor mailbox.
    pub fn notify_action_result(
        &mut self,
        agent_id: &str,
        result: ActionResult,
    ) -> Result<(), AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        self.harness
            .notify_behavior_callback(agent_id, BehaviorCallback::ActionResult(Box::new(result)))
            .map_err(map_harness_error)
            .map_err(|error| error.with_feedback_agent(agent_id))
    }

    pub fn notify_world_event(
        &mut self,
        agent_id: &str,
        event: WorldEvent,
    ) -> Result<(), AsyncAgentRunnerError> {
        if !self.actors.contains_key(agent_id) {
            return Err(AsyncAgentRunnerError::AgentNotRegistered(
                agent_id.to_string(),
            ));
        }
        self.harness
            .notify_behavior_callback(agent_id, BehaviorCallback::Event(Box::new(event)))
            .map_err(map_harness_error)
            .map_err(|error| error.with_feedback_agent(agent_id))
    }

    fn remember_runtime_feedback_replay(
        &mut self,
        feedback: &FeedbackEnvelopeV1,
        receipt_lineage: Option<RuntimeReceiptLineageV1>,
    ) {
        let partition = (
            feedback.agent_subject.clone(),
            feedback.agent_session_id.clone(),
        );
        let replay_key = (
            partition.0.clone(),
            partition.1.clone(),
            feedback.feedback_id.clone(),
        );
        let order = self
            .runtime_feedback_replay_order
            .entry(partition.clone())
            .or_default();
        if !order
            .iter()
            .any(|feedback_id| feedback_id == &feedback.feedback_id)
        {
            order.push_back(feedback.feedback_id.clone());
        }
        self.runtime_feedback_replays.insert(
            replay_key,
            RuntimeFeedbackReplayMetadata { receipt_lineage },
        );
        while order.len() > MAX_RUNTIME_FEEDBACK_REPLAY_ENTRIES {
            let Some(expired_id) = order.pop_front() else {
                break;
            };
            self.runtime_feedback_replays.remove(&(
                partition.0.clone(),
                partition.1.clone(),
                expired_id,
            ));
        }
    }

    fn find_feedback_outcome(
        &self,
        agent_id: &str,
        feedback: &FeedbackEnvelopeV1,
    ) -> Option<AsyncAgentTurnOutcome> {
        self.awaiting_outcomes
            .values()
            .find(|outcome| {
                outcome.agent_id == agent_id
                    && outcome.prepared_context.as_ref().is_some_and(|context| {
                        context.agent_session_id == feedback.agent_session_id
                            && context.agent_turn_id == feedback.agent_turn_id
                            && context.decision_request_id == feedback.decision_request_id
                    })
            })
            .cloned()
            .or_else(|| {
                self.completed
                    .iter()
                    .rev()
                    .find(|outcome| {
                        outcome.agent_id == agent_id
                            && outcome.prepared_context.as_ref().is_some_and(|context| {
                                context.agent_session_id == feedback.agent_session_id
                                    && context.agent_turn_id == feedback.agent_turn_id
                                    && context.decision_request_id == feedback.decision_request_id
                            })
                    })
                    .cloned()
            })
    }

    fn release_runtime_turn(&mut self, agent_id: &str, turn_id: AsyncTurnId) {
        self.turn_metadata.remove(&turn_id);
        if let Some(record) = self.compatibility_turns.remove(&turn_id) {
            let _ = self
                .harness
                .release_compatibility_turn(record.harness_turn_id);
            self.harness_turns.remove(&turn_id);
        } else if let Some(harness_turn) = self.harness_turns.get(&turn_id).copied() {
            self.harness.release_turn(harness_turn);
        }
        self.awaiting_runtime.remove(agent_id);
        self.awaiting_outcomes.remove(&turn_id);
    }

    fn invalidate_continuation_for_turn(
        &mut self,
        agent_id: &str,
        session_id: &str,
        turn_id: &str,
        request_id: &str,
        reason: ContinuationInvalidationReason,
    ) {
        let matches = self.continuations.get(agent_id).is_some_and(|handle| {
            handle.proposal.agent_session_id == session_id
                && handle.proposal.agent_turn_id == turn_id
                && handle.proposal.decision_request_id == request_id
        });
        if !matches {
            return;
        }
        if let Some(handle) = self.continuations.remove(agent_id) {
            let _ = self.continuation_harness.invalidate(handle, reason);
        }
    }

    /// Test helper: a provider that cooperatively waits until its actor is
    /// dropped.  It exercises the real actor path without leaking a permanent
    /// worker thread.
    pub fn blocking_provider_fixture(agent_id: &str) -> Self {
        let mut runner = Self::with_default_capacity();
        let shutdown = Arc::new(AtomicBool::new(false));
        let behavior = BlockingProviderBehavior::new(agent_id.to_string(), Arc::clone(&shutdown));
        runner
            .register_boxed_with_shutdown(Box::new(behavior), Some(shutdown))
            .expect("fixture agent registration");
        runner
    }

    /// Test helper representing a built-in behavior.  It uses the same actor
    /// lifecycle and outcome projection as ProviderBacked.
    pub fn builtin_fixture(agent_id: &str) -> Self {
        let mut runner = Self::with_default_capacity();
        runner
            .register(BuiltinWaitBehavior::new(agent_id.to_string()))
            .expect("fixture agent registration");
        runner
    }

    /// Test helper using the real ProviderBackedAgentBehavior adapter and a
    /// deterministic mock provider response.
    pub fn provider_backed_fixture(agent_id: &str) -> Self {
        let mut runner = Self::with_default_capacity();
        let behavior = ProviderBackedAgentBehavior::new_legacy_compatibility(
            agent_id.to_string(),
            MockDecisionProvider::new("async-fixture-provider"),
            vec![ActionCatalogEntry::new("wait", "wait")],
        );
        runner
            .register(behavior)
            .expect("fixture agent registration");
        runner
    }
}

impl Default for AsyncAgentRunner {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

impl AsyncAgentTurnOutcome {
    pub fn feedback_for_runtime_status(
        &self,
        status: &str,
        runtime_receipt_id: Option<&str>,
    ) -> Result<FeedbackEnvelopeV1, AsyncAgentRunnerError> {
        let context = self.prepared_context.as_ref().ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("runtime feedback requires a host context".to_string())
        })?;
        if !matches!(status, "pending" | "committed" | "rejected" | "failed") {
            return Err(AsyncAgentRunnerError::Cognition(
                "unknown Runtime feedback status".to_string(),
            ));
        }
        if status == "committed" && runtime_receipt_id.is_none() {
            return Err(AsyncAgentRunnerError::Cognition(
                "committed feedback requires a Runtime receipt".to_string(),
            ));
        }
        let runtime_receipt_id = runtime_receipt_id
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let feedback_id = h_v1(
            "oasis7.cognition.feedback.v1",
            &json!({
                "agent_id": context.agent_id,
                "agent_session_id": context.agent_session_id,
                "agent_turn_id": context.agent_turn_id,
                "decision_request_id": context.decision_request_id,
                "request_digest": context.request_digest,
                "status": status,
                "runtime_receipt_id": runtime_receipt_id,
            }),
        )
        .to_string();
        Ok(FeedbackEnvelopeV1 {
            feedback_id,
            feedback_seq: 1,
            agent_subject: context.agent_id.clone(),
            agent_session_id: context.agent_session_id.clone(),
            agent_turn_id: context.agent_turn_id.clone(),
            decision_request_id: context.decision_request_id.clone(),
            candidate_action_id: None,
            runtime_receipt_id,
            status: status.to_string(),
            request_digest: context.request_digest.clone(),
            reject_reason: (status != "committed").then(|| status.to_string()),
            // This helper is caller-side fixture plumbing.  A real Runtime
            // feedback projection must be supplied to
            // `consume_runtime_feedback_with_lineage`; never self-label this
            // envelope as authoritative.
            provenance: "harness_unverified".to_string(),
        })
    }
}

impl AsyncAgentRunnerError {
    fn with_agent(self, agent_id: &str) -> Self {
        match self {
            Self::ActorUnavailable(_) => Self::ActorUnavailable(agent_id.to_string()),
            other => other,
        }
    }

    fn with_feedback_agent(self, agent_id: &str) -> Self {
        match self {
            Self::ActorUnavailable(_) | Self::MailboxFull => {
                Self::FeedbackUnavailable(agent_id.to_string())
            }
            other => other,
        }
    }
}
