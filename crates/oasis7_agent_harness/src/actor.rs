//! Bounded asynchronous provider actors.
//!
//! Actor workers receive only shared request DTOs and return typed proposals.
//! They never receive a World, Runtime, lease, receipt, or mutable application
//! state. Polling the runner is non-blocking; a slow provider cannot stall the
//! host loop.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

use oasis7_agent_api::{
    CognitionError, ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, FeedbackEnvelopeV1, ProviderDecision,
};
use serde::Serialize;
use serde_json::Value;

use crate::authority::RuntimeAuthority;
use crate::cognition::AgentCognitionStore;
use crate::provider::{DecisionProvider, DecisionProviderError};

#[path = "actor_feedback_transaction.rs"]
mod feedback_transaction;
use self::feedback_transaction::{AbortOnDrop, FeedbackDeliveryGate};
#[path = "actor_runner_dispatch.rs"]
mod runner_dispatch;
#[path = "actor_runner_feedback.rs"]
mod runner_feedback;
#[path = "actor_runner_handoff.rs"]
mod runner_handoff;

#[cfg(test)]
#[path = "actor_completion_test_support.rs"]
mod completion_test_support;

const DEFAULT_MAILBOX_CAPACITY: usize = 16;

/// Stable process-local identity for one logical actor turn. This is not a
/// wire identifier and has no effect on Runtime request identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AsyncTurnId(u64);

impl AsyncTurnId {
    pub const fn get(self) -> u64 {
        self.0
    }

    #[cfg(test)]
    pub(crate) const fn from_test_only(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTurnLifecycle {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTurnFeedback {
    Wait,
    WaitTicks(u64),
    ActionProposed,
    QueryProposed,
    ModuleCommandProposed,
    ProviderError,
    ActorPanicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncWorldEffect {
    NoEffect,
    Proposal,
}

/// Explicit non-production admission input for callers that still use the
/// reduced, pre-outer-request compatibility contract. This request has no
/// Runtime binding, lease, or response-readback identity.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibilityTurnRequest<I> {
    pub agent_subject: String,
    pub turn_context: Option<ContinuousAgentTurnContextV1>,
    pub input: I,
}

/// Result of a compatibility-lane decision. It intentionally exposes only
/// the reduced turn context; production request, lease, receipt, and readback
/// fields are absent from this type.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibilityTurnOutcome<O> {
    pub turn_id: AsyncTurnId,
    pub agent_id: String,
    pub lifecycle: AsyncTurnLifecycle,
    pub turn_context: Option<ContinuousAgentTurnContextV1>,
    pub result: Result<O, DecisionProviderError>,
}

/// Host adapter for old/reduced turn callers. The compatibility result and
/// callbacks are distinct from production request/feedback DTOs and do not
/// produce a Runtime binding or readback proof.
pub trait CompatibilityDecisionProvider<I, O, F>: Send {
    fn decide_compatibility(
        &mut self,
        request: CompatibilityTurnRequest<I>,
    ) -> Result<O, DecisionProviderError>;

    fn push_compatibility_feedback(&mut self, feedback: F) -> Result<(), DecisionProviderError>;
}

/// Internal shared actor vtable. A single mailbox and worker serialize both
/// lanes; adapters select the lane explicitly for every decision and feedback
/// command.
trait RunnerDecisionProvider<A, Q, I, O, F>: Send {
    fn decide_production(
        &mut self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError>;

    fn push_production_feedback(
        &mut self,
        feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError>;

    fn push_production_callback(&mut self, callback: F) -> Result<(), DecisionProviderError>;

    fn decide_compatibility(
        &mut self,
        request: CompatibilityTurnRequest<I>,
    ) -> Result<O, DecisionProviderError>;

    fn push_compatibility_feedback(&mut self, feedback: F) -> Result<(), DecisionProviderError>;
}

struct ProductionOnly<P: ?Sized>(Box<P>);

impl<A, Q, I, O, F, P: ?Sized> RunnerDecisionProvider<A, Q, I, O, F> for ProductionOnly<P>
where
    A: Serialize + Send + 'static,
    Q: Serialize + Send + 'static,
    I: Send + 'static,
    O: Clone + Send + 'static,
    F: Send + 'static,
    P: DecisionProvider<A, Q> + Send,
{
    fn decide_production(
        &mut self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError> {
        self.0.decide_checked(request)
    }

    fn push_production_feedback(
        &mut self,
        feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        self.0.push_feedback(feedback)
    }

    fn push_production_callback(&mut self, _callback: F) -> Result<(), DecisionProviderError> {
        Err(DecisionProviderError::new(
            "production_callback_unavailable",
            "provider was registered without a production host-callback adapter",
            false,
        ))
    }

    fn decide_compatibility(
        &mut self,
        _request: CompatibilityTurnRequest<I>,
    ) -> Result<O, DecisionProviderError> {
        Err(DecisionProviderError::new(
            "compatibility_lane_unavailable",
            "provider was registered for production requests only",
            false,
        ))
    }

    fn push_compatibility_feedback(&mut self, _feedback: F) -> Result<(), DecisionProviderError> {
        Err(DecisionProviderError::new(
            "compatibility_lane_unavailable",
            "provider was registered for production requests only",
            false,
        ))
    }
}

struct CompatibilityOnly<P>(P);

impl<A, Q, I, O, F, P> RunnerDecisionProvider<A, Q, I, O, F> for CompatibilityOnly<P>
where
    A: Serialize + Send + 'static,
    Q: Serialize + Send + 'static,
    I: Send + 'static,
    O: Clone + Send + 'static,
    F: Send + 'static,
    P: CompatibilityDecisionProvider<I, O, F>,
{
    fn decide_production(
        &mut self,
        _request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError> {
        Err(DecisionProviderError::new(
            "production_lane_unavailable",
            "provider was registered for compatibility requests only",
            false,
        ))
    }

    fn push_production_feedback(
        &mut self,
        _feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        Err(DecisionProviderError::new(
            "production_lane_unavailable",
            "provider was registered for compatibility requests only",
            false,
        ))
    }

    fn push_production_callback(&mut self, _callback: F) -> Result<(), DecisionProviderError> {
        Err(DecisionProviderError::new(
            "production_lane_unavailable",
            "provider was registered for compatibility requests only",
            false,
        ))
    }

    fn decide_compatibility(
        &mut self,
        request: CompatibilityTurnRequest<I>,
    ) -> Result<O, DecisionProviderError> {
        self.0.decide_compatibility(request)
    }

    fn push_compatibility_feedback(&mut self, feedback: F) -> Result<(), DecisionProviderError> {
        self.0.push_compatibility_feedback(feedback)
    }
}

struct DualLane<P>(P);

impl<A, Q, I, O, F, P> RunnerDecisionProvider<A, Q, I, O, F> for DualLane<P>
where
    A: Serialize + Send + 'static,
    Q: Serialize + Send + 'static,
    I: Send + 'static,
    O: Clone + Send + 'static,
    F: Send + 'static,
    P: DecisionProvider<A, Q> + CompatibilityDecisionProvider<I, O, F>,
{
    fn decide_production(
        &mut self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<ContinuousAgentResponseContextV1<A, Q>, DecisionProviderError> {
        self.0.decide_checked(request)
    }

    fn push_production_feedback(
        &mut self,
        feedback: &FeedbackEnvelopeV1,
    ) -> Result<(), DecisionProviderError> {
        self.0.push_feedback(feedback)
    }

    fn push_production_callback(&mut self, callback: F) -> Result<(), DecisionProviderError> {
        self.0.push_compatibility_feedback(callback)
    }

    fn decide_compatibility(
        &mut self,
        request: CompatibilityTurnRequest<I>,
    ) -> Result<O, DecisionProviderError> {
        self.0.decide_compatibility(request)
    }

    fn push_compatibility_feedback(&mut self, feedback: F) -> Result<(), DecisionProviderError> {
        self.0.push_compatibility_feedback(feedback)
    }
}

/// A provider result retained for the host's Runtime decision and readback
/// boundary. The response remains typed; no action/query discriminator or
/// JSON-erasure protocol is introduced here.
#[derive(Debug)]
pub struct AsyncAgentTurnOutcome<A = Value, Q = Value> {
    pub turn_id: AsyncTurnId,
    pub agent_id: String,
    pub lifecycle: AsyncTurnLifecycle,
    pub feedback: AsyncTurnFeedback,
    pub world_effect: AsyncWorldEffect,
    pub request_context: ContinuousAgentRequestContextV1,
    pub response_context: Option<Arc<ContinuousAgentResponseContextV1<A, Q>>>,
    pub provider_error: Option<DecisionProviderError>,
}

impl<A, Q> Clone for AsyncAgentTurnOutcome<A, Q> {
    fn clone(&self) -> Self {
        Self {
            turn_id: self.turn_id,
            agent_id: self.agent_id.clone(),
            lifecycle: self.lifecycle,
            feedback: self.feedback,
            world_effect: self.world_effect,
            request_context: self.request_context.clone(),
            response_context: self.response_context.clone(),
            provider_error: self.provider_error.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncAgentRunnerError {
    InvalidCapacity,
    AgentAlreadyRegistered(String),
    AgentNotRegistered(String),
    AgentBusy(String),
    MailboxFull,
    ActorUnavailable(String),
    Cognition(CognitionError),
}

impl AsyncAgentRunnerError {
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidCapacity => "invalid_capacity",
            Self::AgentAlreadyRegistered(_) => "agent_already_registered",
            Self::AgentNotRegistered(_) => "agent_not_registered",
            Self::AgentBusy(_) => "agent_busy",
            Self::MailboxFull => "mailbox_full",
            Self::ActorUnavailable(_) => "actor_unavailable",
            Self::Cognition(error) => error.code(),
        }
    }
}

impl fmt::Display for AsyncAgentRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity => formatter.write_str("actor mailbox capacity must be positive"),
            Self::AgentAlreadyRegistered(agent) => {
                write!(formatter, "actor already registered: {agent}")
            }
            Self::AgentNotRegistered(agent) => {
                write!(formatter, "actor is not registered: {agent}")
            }
            Self::AgentBusy(agent) => write!(
                formatter,
                "actor already has an active or awaiting turn: {agent}"
            ),
            Self::MailboxFull => formatter.write_str("actor mailbox is full"),
            Self::ActorUnavailable(agent) => write!(formatter, "actor is unavailable: {agent}"),
            Self::Cognition(error) => write!(formatter, "cognition request rejected: {error}"),
        }
    }
}

impl std::error::Error for AsyncAgentRunnerError {}

impl From<CognitionError> for AsyncAgentRunnerError {
    fn from(error: CognitionError) -> Self {
        Self::Cognition(error)
    }
}

enum ActorCommand<I, F> {
    DecideProduction {
        turn_id: AsyncTurnId,
        request: Box<ContinuousAgentRequestContextV1>,
    },
    DecideCompatibility {
        turn_id: AsyncTurnId,
        request: Box<CompatibilityTurnRequest<I>>,
    },
    ProductionFeedback(Box<FeedbackEnvelopeV1>),
    ProductionFeedbackTransaction(Arc<FeedbackDeliveryGate>),
    ProductionCallback(F),
    CompatibilityCallback(F),
    Shutdown,
}

enum ActorCompletionResult<A, Q, O> {
    Production(Result<Box<ContinuousAgentResponseContextV1<A, Q>>, DecisionProviderError>),
    Compatibility(Result<O, DecisionProviderError>),
}

struct ActorCompletion<A, Q, O> {
    turn_id: AsyncTurnId,
    result: ActorCompletionResult<A, Q, O>,
}

struct AgentActor<A, Q, I, O, F> {
    sender: SyncSender<ActorCommand<I, F>>,
    completions: Receiver<ActorCompletion<A, Q, O>>,
    active_turn: Arc<AtomicBool>,
    accepting_commands: Arc<AtomicBool>,
    supports_production: bool,
    supports_compatibility: bool,
    join: Option<JoinHandle<()>>,
    #[cfg(test)]
    publication: Arc<completion_test_support::Publication>,
}

impl<A, Q, I, O, F> AgentActor<A, Q, I, O, F>
where
    A: Serialize + Send + 'static,
    Q: Serialize + Send + 'static,
    I: Send + 'static,
    O: Clone + Send + 'static,
    F: Send + 'static,
{
    fn new(
        agent_id: String,
        provider: Box<dyn RunnerDecisionProvider<A, Q, I, O, F> + 'static>,
        capacity: usize,
        supports_production: bool,
        supports_compatibility: bool,
    ) -> Result<Self, AsyncAgentRunnerError> {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let (completion_sender, completions) = mpsc::channel();
        let active_turn = Arc::new(AtomicBool::new(false));
        let accepting_commands = Arc::new(AtomicBool::new(true));
        let worker_accepting = Arc::clone(&accepting_commands);
        #[cfg(test)]
        let publication = Arc::new(completion_test_support::Publication::default());
        #[cfg(test)]
        let worker_publication = Arc::clone(&publication);
        let worker = thread::Builder::new()
            .name(format!("agent-harness-{agent_id}"))
            .spawn(move || {
                let mut provider = provider;
                while let Ok(command) = receiver.recv() {
                    match command {
                        ActorCommand::DecideProduction { turn_id, request } => {
                            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                                provider.decide_production(&request)
                            }));
                            let result = match result {
                                Ok(Ok(response)) => {
                                    ActorCompletionResult::Production(Ok(Box::new(response)))
                                }
                                Ok(Err(error)) => ActorCompletionResult::Production(Err(error)),
                                Err(_) => ActorCompletionResult::Production(Err(
                                    DecisionProviderError::new(
                                        "actor_panicked",
                                        "provider panicked while handling a decision request",
                                        false,
                                    ),
                                )),
                            };
                            #[cfg(test)]
                            worker_publication.before_send(turn_id);
                            if completion_sender
                                .send(ActorCompletion { turn_id, result })
                                .is_err()
                            {
                                break;
                            }
                            #[cfg(test)]
                            worker_publication.after_send(turn_id);
                        }
                        ActorCommand::DecideCompatibility { turn_id, request } => {
                            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                                provider.decide_compatibility(*request)
                            }));
                            let result = match result {
                                Ok(result) => ActorCompletionResult::Compatibility(result),
                                Err(_) => ActorCompletionResult::Compatibility(Err(
                                    DecisionProviderError::new(
                                        "actor_panicked",
                                        "provider panicked while handling a compatibility turn",
                                        false,
                                    ),
                                )),
                            };
                            #[cfg(test)]
                            worker_publication.before_send(turn_id);
                            if completion_sender
                                .send(ActorCompletion { turn_id, result })
                                .is_err()
                            {
                                break;
                            }
                            #[cfg(test)]
                            worker_publication.after_send(turn_id);
                        }
                        ActorCommand::ProductionFeedback(feedback) => {
                            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                                let _ = provider.push_production_feedback(&feedback);
                            }));
                        }
                        ActorCommand::ProductionFeedbackTransaction(gate) => {
                            if let Some(feedback) = gate.wait() {
                                let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                                    let _ = provider.push_production_feedback(&feedback);
                                }));
                            }
                        }
                        ActorCommand::ProductionCallback(callback) => {
                            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                                let _ = provider.push_production_callback(callback);
                            }));
                        }
                        ActorCommand::CompatibilityCallback(callback) => {
                            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                                let _ = provider.push_compatibility_feedback(callback);
                            }));
                        }
                        ActorCommand::Shutdown => break,
                    }
                }
                worker_accepting.store(false, Ordering::Release);
            })
            .map_err(|_| AsyncAgentRunnerError::ActorUnavailable(agent_id.clone()))?;
        Ok(Self {
            sender,
            completions,
            active_turn,
            accepting_commands,
            supports_production,
            supports_compatibility,
            join: Some(worker),
            #[cfg(test)]
            publication,
        })
    }

    fn try_send(
        &self,
        command: ActorCommand<I, F>,
        agent_id: &str,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.sender.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => AsyncAgentRunnerError::MailboxFull,
            TrySendError::Disconnected(_) => {
                AsyncAgentRunnerError::ActorUnavailable(agent_id.to_string())
            }
        })
    }

    fn try_completion(
        &self,
        agent_id: &str,
    ) -> Result<Option<ActorCompletion<A, Q, O>>, AsyncAgentRunnerError> {
        match self.completions.try_recv() {
            Ok(completion) => Ok(Some(completion)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AsyncAgentRunnerError::ActorUnavailable(
                agent_id.to_string(),
            )),
        }
    }
}

impl<A, Q, I, O, F> Drop for AgentActor<A, Q, I, O, F> {
    fn drop(&mut self) {
        self.accepting_commands.store(false, Ordering::Release);
        let shutdown_queued = self.sender.try_send(ActorCommand::Shutdown).is_ok();
        // Never join a provider call from the caller's loop. A busy provider
        // may be detached; idle workers are joined after receiving shutdown.
        if self.active_turn.load(Ordering::Acquire) || !shutdown_queued {
            let _ = self.join.take();
        } else if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug)]
struct RunnerTurn<A, Q> {
    agent_id: String,
    request: ContinuousAgentRequestContextV1,
    awaiting_runtime: bool,
    last_outcome: Option<AsyncAgentTurnOutcome<A, Q>>,
}

struct CompatibilityTurnRecord {
    agent_id: String,
    turn_context: Option<ContinuousAgentTurnContextV1>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActorLane {
    Production,
    Compatibility,
}

/// Bounded per-provider actor runner. It tracks request single-flight and
/// transport retries but delegates durable admission, lease settlement,
/// world effects, and receipt readback to the host.
pub struct AsyncAgentRunner<A = Value, Q = Value, I = Value, O = Value, F = FeedbackEnvelopeV1> {
    runner_identity: Arc<()>,
    actors: BTreeMap<String, AgentActor<A, Q, I, O, F>>,
    mailbox_capacity: usize,
    active_turns: usize,
    next_turn_id: u64,
    turns: BTreeMap<AsyncTurnId, RunnerTurn<A, Q>>,
    awaiting_runtime: BTreeMap<String, AsyncTurnId>,
    compatibility_turns: BTreeMap<AsyncTurnId, CompatibilityTurnRecord>,
    awaiting_compatibility: BTreeMap<String, AsyncTurnId>,
    actor_lanes: BTreeMap<String, ActorLane>,
    completed: VecDeque<AsyncAgentTurnOutcome<A, Q>>,
    compatibility_completed: VecDeque<CompatibilityTurnOutcome<O>>,
    reported_completed: usize,
    reported_compatibility_completed: usize,
    cognition: AgentCognitionStore,
}

/// A one-shot capability for handing one exact completed actor turn to its
/// successor. Its fields are private: callers can only obtain it from the
/// runner that currently owns the awaiting turn.
pub struct TurnHandoff {
    runner_identity: Arc<()>,
    turn_id: AsyncTurnId,
    lane: ActorLane,
    agent_id: String,
    session_id: String,
    turn_identity: String,
    request_id: String,
    request_digest: oasis7_agent_api::Digest32,
}

/// The successor dispatch is typed by lane so a reduced compatibility handoff
/// cannot be mistaken for a production Runtime request (or vice versa).
// Keep the established public enum constructors source-compatible. Boxing
// these variants would be a breaking API change solely to optimize stack size.
#[allow(clippy::large_enum_variant)]
pub enum TurnHandoffDispatch<I> {
    Production(ContinuousAgentRequestContextV1),
    Compatibility(CompatibilityTurnRequest<I>),
}

#[derive(Clone)]
enum HandoffPredecessor {
    Production(Box<ContinuousAgentRequestContextV1>),
    Compatibility(Box<ContinuousAgentTurnContextV1>),
}

fn handoff_stale() -> AsyncAgentRunnerError {
    CognitionError::new(
        "handoff_identity_stale",
        "handoff capability no longer matches the active awaiting turn",
    )
    .into()
}

impl<A, Q, I, O, F> AsyncAgentRunner<A, Q, I, O, F>
where
    A: Serialize + Send + 'static,
    Q: Serialize + Send + 'static,
    I: Send + 'static,
    O: Clone + Send + 'static,
    F: Send + 'static,
{
    pub fn new(mailbox_capacity: usize) -> Result<Self, AsyncAgentRunnerError> {
        if mailbox_capacity == 0 {
            return Err(AsyncAgentRunnerError::InvalidCapacity);
        }
        Ok(Self {
            runner_identity: Arc::new(()),
            actors: BTreeMap::new(),
            mailbox_capacity,
            active_turns: 0,
            next_turn_id: 1,
            turns: BTreeMap::new(),
            awaiting_runtime: BTreeMap::new(),
            compatibility_turns: BTreeMap::new(),
            awaiting_compatibility: BTreeMap::new(),
            actor_lanes: BTreeMap::new(),
            completed: VecDeque::new(),
            compatibility_completed: VecDeque::new(),
            reported_completed: 0,
            reported_compatibility_completed: 0,
            cognition: AgentCognitionStore::default(),
        })
    }

    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_MAILBOX_CAPACITY).expect("default actor capacity is positive")
    }

    pub fn mailbox_capacity(&self) -> usize {
        self.mailbox_capacity
    }

    pub fn register<P>(
        &mut self,
        agent_id: impl Into<String>,
        provider: P,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        P: DecisionProvider<A, Q> + 'static,
    {
        self.register_boxed_adapter(
            agent_id,
            Box::new(ProductionOnly(Box::new(provider))),
            true,
            false,
        )
    }

    pub fn register_boxed(
        &mut self,
        agent_id: impl Into<String>,
        provider: Box<dyn DecisionProvider<A, Q> + 'static>,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.register_boxed_adapter(agent_id, Box::new(ProductionOnly(provider)), true, false)
    }

    pub fn register_compatibility<P>(
        &mut self,
        agent_id: impl Into<String>,
        provider: P,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        P: CompatibilityDecisionProvider<I, O, F> + 'static,
    {
        self.register_boxed_adapter(agent_id, Box::new(CompatibilityOnly(provider)), false, true)
    }

    pub fn register_dual<P>(
        &mut self,
        agent_id: impl Into<String>,
        provider: P,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        P: DecisionProvider<A, Q> + CompatibilityDecisionProvider<I, O, F> + 'static,
    {
        self.register_boxed_adapter(agent_id, Box::new(DualLane(provider)), true, true)
    }

    fn register_boxed_adapter(
        &mut self,
        agent_id: impl Into<String>,
        provider: Box<dyn RunnerDecisionProvider<A, Q, I, O, F> + 'static>,
        supports_production: bool,
        supports_compatibility: bool,
    ) -> Result<(), AsyncAgentRunnerError> {
        let agent_id = agent_id.into();
        if self.actors.contains_key(&agent_id) {
            return Err(AsyncAgentRunnerError::AgentAlreadyRegistered(agent_id));
        }
        let actor = AgentActor::new(
            agent_id.clone(),
            provider,
            self.mailbox_capacity,
            supports_production,
            supports_compatibility,
        )?;
        self.actors.insert(agent_id, actor);
        Ok(())
    }

    pub fn agent_count(&self) -> usize {
        self.actors.len()
    }

    pub fn active_turn_count(&self) -> usize {
        self.active_turns + self.awaiting_runtime.len() + self.awaiting_compatibility.len()
    }

    /// Restore replay and collision metadata from the host's validated durable
    /// feedback history before the runner dispatches its first provider turn.
    /// A complete restore is idempotent, but a newer authenticated snapshot
    /// must still be compared with the live replay verifier. A fenced partition
    /// can be refreshed while idle, but never while a provider turn or Runtime
    /// handoff is active.
    pub fn restore_feedback_history<H: RuntimeAuthority>(
        &mut self,
        authority: &H,
    ) -> Result<(), AsyncAgentRunnerError> {
        if self.active_turn_count() > 0 {
            return Err(AsyncAgentRunnerError::Cognition(CognitionError::new(
                "feedback_recovery_active_turn",
                "feedback history cannot be restored while a turn is active or awaiting Runtime",
            )));
        }
        self.cognition
            .restore_from_runtime(authority)
            .map_err(AsyncAgentRunnerError::Cognition)
    }

    pub fn feedback_recovery_loaded(&self) -> bool {
        self.cognition.feedback_recovery_loaded()
    }

    pub fn needs_feedback_history_refresh(&self) -> bool {
        !self.cognition.feedback_recovery_loaded()
            || !self.cognition.feedback_recovery_initialized()
    }

    pub fn feedback_recovery_blocked(&self, agent: &str, session: &str) -> bool {
        self.cognition.feedback_recovery_blocked(agent, session)
    }

    pub fn agent_feedback_recovery_blocked(&self, agent: &str) -> bool {
        self.cognition.agent_feedback_recovery_blocked(agent)
    }

    pub fn provider_is_still_in_flight(&self, agent_id: &str) -> bool {
        self.actors
            .get(agent_id)
            .is_some_and(|actor| actor.active_turn.load(Ordering::Acquire))
    }

    /// Bind a one-shot handoff capability to the exact request currently
    /// awaiting the host. The process-local turn ID is the request version;
    /// the capability is also tied to this runner instance.
    /// Release local single-flight correlation only after the host has
    /// accepted authoritative terminal feedback or a continuation handoff.
    pub fn release_turn(&mut self, turn_id: AsyncTurnId) {
        if let Some(turn) = self.turns.remove(&turn_id) {
            self.awaiting_runtime.remove(&turn.agent_id);
            if self.actor_lanes.get(&turn.agent_id) == Some(&ActorLane::Production) {
                self.actor_lanes.remove(&turn.agent_id);
            }
            self.cognition.clear_agent(&turn.agent_id);
        }
    }
}

fn classify_decision<A, Q>(
    decision: &ProviderDecision<A, Q>,
) -> (AsyncTurnFeedback, AsyncWorldEffect) {
    match decision {
        ProviderDecision::Wait => (AsyncTurnFeedback::Wait, AsyncWorldEffect::NoEffect),
        ProviderDecision::WaitTicks { ticks } => (
            AsyncTurnFeedback::WaitTicks(*ticks),
            AsyncWorldEffect::NoEffect,
        ),
        ProviderDecision::Act { .. } => (
            AsyncTurnFeedback::ActionProposed,
            AsyncWorldEffect::Proposal,
        ),
        ProviderDecision::Query { .. } => {
            (AsyncTurnFeedback::QueryProposed, AsyncWorldEffect::Proposal)
        }
        ProviderDecision::ModuleCommand { .. } | ProviderDecision::ModuleCommandResponse { .. } => {
            (
                AsyncTurnFeedback::ModuleCommandProposed,
                AsyncWorldEffect::Proposal,
            )
        }
    }
}

#[cfg(test)]
#[path = "actor_recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "actor_compatibility_tests.rs"]
mod compatibility_tests;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::authority::test_support::FixtureAuthority;
    use oasis7_agent_api::{
        BudgetContractV1, CapabilityInvocationContext, DecisionRequest, DecisionResponse, Digest32,
        ObservationEnvelope, ProviderObservation, RuntimeBindingV1, h_v1,
    };
    use oasis7_wasm_abi::{
        CapabilityAudience, CapabilityCatalogSnapshot, CapabilityPresenter, CapabilitySubject,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    const HASH_A: &str = "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    pub(crate) fn request(
        agent: &str,
        request_id: &str,
        turn_id: &str,
    ) -> ContinuousAgentRequestContextV1 {
        let subject = CapabilitySubject::Agent {
            agent_id: agent.into(),
            owner_binding: "owner-1".into(),
            generation: 1,
        };
        let presenter = CapabilityPresenter {
            presenter_id: "provider-1".into(),
            presenter_kind: "provider".into(),
            session_id: Some(format!("session-{agent}")),
            attestation_ref: None,
        };
        let audience = CapabilityAudience {
            world_id: "world-1".into(),
            branch_id: "main".into(),
            finality_epoch: 1,
            target_kind: "world".into(),
            target_id: None,
        };
        let catalog = CapabilityCatalogSnapshot {
            snapshot_id: format!("catalog-{agent}"),
            world_id: "world-1".into(),
            world_head: 1,
            branch_id: "main".into(),
            finality_epoch: 1,
            logical_tick: 1,
            module_registry_hash: HASH_A.into(),
            policy_hash: HASH_B.into(),
            revocation_epoch: 0,
            subject: subject.clone(),
            presenter: presenter.clone(),
            audience: audience.clone(),
            entries: Vec::new(),
            valid_until_tick: 10,
        };
        let invocation = CapabilityInvocationContext {
            grant_id: "grant-1".into(),
            subject,
            presenter,
            audience,
            catalog_snapshot_id: format!("catalog-{agent}"),
            module_id: String::new(),
            module_version: String::new(),
            response_nonce: "nonce-1".into(),
        };
        let base_decision_request = DecisionRequest {
            observation: ObservationEnvelope {
                agent_id: agent.into(),
                world_time: 1,
                mode: Default::default(),
                observation_schema_version: "oc_dual_obs_v1".into(),
                action_schema_version: "oc_dual_act_v1".into(),
                environment_class: None,
                fallback_reason: None,
                observation: ProviderObservation::default(),
                recent_event_summary: Vec::new(),
                memory_summary: None,
                action_catalog: Vec::new(),
                module_command_catalog: Vec::new(),
                timeout_budget_ms: 1000,
            },
            provider_config_ref: None,
            agent_profile: None,
            fixture_id: None,
            replay_id: None,
            capability_catalog: Some(catalog.clone()),
            capability_invocation_context: Some(invocation.clone()),
            timeout_budget_ms: 1000,
        };
        let mut request = ContinuousAgentRequestContextV1 {
            base_decision_request,
            context_discriminator: "oasis7.continuous-agent-context".into(),
            context_version: 1,
            protocol_version: "continuous-agent-v1".into(),
            agent_session_id: format!("session-{agent}"),
            agent_turn_id: turn_id.into(),
            decision_request_id: request_id.into(),
            retry_seq: 1,
            transport_attempt: 1,
            agent_subject: agent.into(),
            runtime_binding: RuntimeBindingV1 {
                world_id: "world-1".into(),
                branch_id: "main".into(),
                finality_epoch: 1,
                finality_block_hash: Some(HASH_A.into()),
                finality_status: "verified".into(),
                base_tick: 1,
                base_world_hash: HASH_B.into(),
                reorg_epoch: 0,
                runtime_manifest_hash: HASH_A.into(),
            },
            observation_digest: HASH_A.into(),
            capability_catalog_digest: h_v1("oasis7.cognition.capability-catalog.v1", &catalog),
            capability_invocation_context_digest: h_v1(
                "oasis7.cognition.capability-invocation-context.v1",
                &invocation,
            ),
            memory_snapshot_digest: HASH_A.into(),
            goal_snapshot_digest: HASH_B.into(),
            continuation_digest: HASH_A.into(),
            adapter_protocol_version: "test-adapter-v1".into(),
            budget_contract: BudgetContractV1 {
                max_latency_ms: 1000,
                max_repair_attempts: 0,
                max_model_calls: 1,
                max_tool_calls: 0,
            },
            request_digest: Digest32::default(),
        };
        request.request_digest = request.request_digest();
        request
            .validate_production_lane()
            .expect("test request is valid");
        request
    }

    fn restore_empty_history(runner: &mut AsyncAgentRunner<Value, Value>) {
        runner
            .restore_feedback_history(&FixtureAuthority::default())
            .expect("empty host history is a successful restore");
    }

    pub(crate) struct WaitProvider {
        pub(crate) calls: Arc<AtomicUsize>,
        pub(crate) delay: Duration,
    }

    impl DecisionProvider for WaitProvider {
        fn provider_id(&self) -> &str {
            "wait"
        }
        fn decide(
            &mut self,
            request: &ContinuousAgentRequestContextV1,
        ) -> Result<ContinuousAgentResponseContextV1, DecisionProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            thread::sleep(self.delay);
            let base = DecisionResponse::wait("wait");
            let mut response = ContinuousAgentResponseContextV1 {
                base_decision_response: base,
                context_discriminator: request.context_discriminator.clone(),
                context_version: request.context_version,
                agent_session_id: request.agent_session_id.clone(),
                agent_turn_id: request.agent_turn_id.clone(),
                decision_request_id: request.decision_request_id.clone(),
                retry_seq: request.retry_seq,
                transport_attempt: request.transport_attempt,
                request_digest: request.request_digest.clone(),
                response_digest: Default::default(),
            };
            response.response_digest =
                crate::provider::cognition_response_digest(&response.base_decision_response);
            Ok(response)
        }
    }

    #[test]
    fn slow_provider_does_not_block_other_actor_progress_and_retry_changes_only_transport() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut runner = AsyncAgentRunner::<Value, Value>::new(1).unwrap();
        restore_empty_history(&mut runner);
        runner
            .register(
                "agent-a",
                WaitProvider {
                    calls: Arc::clone(&calls),
                    delay: Duration::from_millis(150),
                },
            )
            .unwrap();
        runner
            .register(
                "agent-b",
                WaitProvider {
                    calls: Arc::clone(&calls),
                    delay: Duration::from_millis(150),
                },
            )
            .unwrap();
        let a = request("agent-a", "request-a", "turn-a");
        let b = request("agent-b", "request-b", "turn-b");
        let turn_a = runner.start_turn(a.clone()).unwrap();
        let turn_b = runner.start_turn(b.clone()).unwrap();
        let start = Instant::now();
        let _ = runner.poll_completed().unwrap();
        assert!(start.elapsed() < Duration::from_millis(100));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut outcomes = Vec::new();
        while outcomes.len() < 2 {
            outcomes.extend(runner.poll_completed().unwrap());
            assert!(Instant::now() < deadline, "provider outcomes should arrive");
            if outcomes.len() < 2 {
                thread::yield_now();
            }
        }
        let outcome_a = outcomes
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_a)
            .unwrap();
        let outcome_b = runner.awaiting_outcome(turn_b).unwrap();
        assert_eq!(outcome_a.feedback, AsyncTurnFeedback::Wait);
        assert_eq!(outcome_b.feedback, AsyncTurnFeedback::Wait);
        let retry = runner.retry_awaiting_turn(turn_a, &a).unwrap();
        assert_eq!(retry, turn_a);
        let retry_request = runner.awaiting_request(turn_a);
        assert!(
            retry_request.is_none(),
            "retry is in flight until provider completion"
        );
        let result = loop {
            if let Some(outcome) = runner
                .poll_completed()
                .unwrap()
                .into_iter()
                .find(|outcome| outcome.turn_id == turn_a)
            {
                break outcome;
            }
            thread::yield_now();
        };
        assert_eq!(
            result.request_context.transport_attempt,
            a.transport_attempt + 1
        );
        assert_eq!(result.request_context.request_digest, a.request_digest);
        assert_eq!(result.request_context.retry_seq, a.retry_seq);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn request_identity_mismatch_is_rejected_before_provider_dispatch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut runner = AsyncAgentRunner::<Value, Value>::new(1).unwrap();
        restore_empty_history(&mut runner);
        runner
            .register(
                "agent-a",
                WaitProvider {
                    calls: Arc::clone(&calls),
                    delay: Duration::ZERO,
                },
            )
            .unwrap();
        let mut invalid = request("agent-a", "request-a", "turn-a");
        invalid.agent_subject = "agent-b".into();
        assert!(runner.start_turn(invalid).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
