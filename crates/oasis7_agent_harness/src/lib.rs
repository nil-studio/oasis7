//! Portable provider execution and cognition algorithms.
//!
//! This crate deliberately depends on the shared `oasis7_agent_api` contracts,
//! not on the Oasis7 application, Runtime, World, or kernel implementation.
//! Runtime-owned leases, receipts, durable continuation state, and any World
//! effects cross this boundary only through host-supplied opaque values and
//! callbacks.

/// Shared portable model-selection default; transport stays native.
pub use oasis7_agent_api::DEFAULT_LLM_MODEL;

mod actor;
mod authority;
mod cognition;
mod continuation;
mod memory;
mod model_provider;
mod provider;
mod turn_engine;

pub use actor::{
    AsyncAgentRunner, AsyncAgentRunnerError, AsyncAgentTurnOutcome, AsyncTurnFeedback, AsyncTurnId,
    AsyncTurnLifecycle, AsyncWorldEffect, CompatibilityDecisionProvider, CompatibilityTurnOutcome,
    CompatibilityTurnRequest, TurnHandoff, TurnHandoffDispatch,
};
pub use authority::RuntimeAuthority;
pub use cognition::{AgentCognitionStore, FeedbackHistoryLineage, FeedbackHistoryProjection};
pub use continuation::{
    ContinuationAuthorityContextV1, ContinuationBudgetProgressV1, ContinuationHandle,
    ContinuationHarness, ContinuationHostProjection, ContinuationInvalidationReason,
    ContinuationProjectionV1, ContinuationStatusProjectionV1,
};
pub use memory::{
    GoalSnapshotInputV1, GoalSnapshotProjector, MemoryCorrectionV1, MemoryWriteIntentPolicyV1,
    MemoryWritePolicyContextV1, MemoryWritePolicyOutcome, MemoryWriteStore,
    NormalizedMemoryWriteIntentV1,
};
pub use model_provider::{
    ModelProvider, ModelProviderError, ModelProviderHistory, ModelProviderRequest,
    ModelProviderResponse, ModelProviderTool, ModelProviderToolCall, ModelProviderToolResult,
    ModelProviderTrace, ModelProviderTraceEntry, ModelProviderTurn, ModelProviderUsage,
};
pub use provider::{DecisionProvider, DecisionProviderError, cognition_response_digest};
pub use turn_engine::{
    MemoryProjectionReport, RuntimeReadbackProof, TurnEngine, TurnEngineError, TurnEngineEvent,
    TurnEnginePhase, TurnRequest,
};

#[cfg(not(target_arch = "wasm32"))]
mod native_task;
#[cfg(not(target_arch = "wasm32"))]
pub use native_task::{
    NativeTaskContext, NativeTaskControl, NativeTaskHost, NativeTaskModelFailure,
    NativeTaskRecovery, NativeTaskRunConfig, NativeTaskRunReport, NativeTaskRunner,
    NativeTaskStopReason, NativeTaskTool, NativeTaskToolResult, SdkNativeTaskHost,
};
