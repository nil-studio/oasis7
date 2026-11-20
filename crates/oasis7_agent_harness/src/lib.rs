//! Portable provider execution and cognition algorithms.
//!
//! This crate deliberately depends on the shared `oasis7_agent_api` contracts,
//! not on the Oasis7 application, Runtime, World, or kernel implementation.
//! Runtime-owned leases, receipts, durable continuation state, and any World
//! effects cross this boundary only through host-supplied opaque values and
//! callbacks.

/// Shared portable model-selection default and budget contract; transport stays native.
pub use oasis7_agent_api::{BudgetContractV1, DEFAULT_LLM_MODEL};

mod actor;
mod authority;
mod cognition;
mod continuation;
pub mod feedback_recovery;
pub mod llm_defaults;
mod memory;
mod model_provider;
mod provider;
mod turn_engine;

pub use actor::{
    AsyncAgentRunner, AsyncAgentRunnerError, AsyncAgentTurnOutcome, AsyncTurnFeedback, AsyncTurnId,
    AsyncTurnLifecycle, AsyncWorldEffect, CompatibilityDecisionProvider, CompatibilityTurnOutcome,
    CompatibilityTurnRequest, TurnHandoff, TurnHandoffDispatch,
};
pub use authority::{RuntimeAuthority, ScopedFeedbackHistoryAuthority};
pub use cognition::{AgentCognitionStore, FeedbackHistoryLineage, FeedbackHistoryProjection};
pub use continuation::{
    ContinuationAuthorityContextV1, ContinuationBudgetProgressV1, ContinuationHandle,
    ContinuationHarness, ContinuationHostProjection, ContinuationInvalidationReason,
    ContinuationProjectionV1, ContinuationStatusProjectionV1, RuntimeContinuationStatusV1,
};
pub use feedback_recovery::{FinalityBindingV1, project_preverified_scoped_feedback_history};
pub use llm_defaults::{
    DEFAULT_CONFIG_FILE_NAME, DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
    DEFAULT_LLM_LONG_TERM_GOAL, DEFAULT_LLM_MAX_DECISION_STEPS, DEFAULT_LLM_MAX_MODULE_CALLS,
    DEFAULT_LLM_MAX_REPAIR_ROUNDS, DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS,
    DEFAULT_LLM_SHORT_TERM_GOAL, DEFAULT_LLM_SYSTEM_PROMPT, ENV_LLM_API_KEY, ENV_LLM_BASE_URL,
    ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION, ENV_LLM_LONG_TERM_GOAL, ENV_LLM_MAX_DECISION_STEPS,
    ENV_LLM_MAX_MODULE_CALLS, ENV_LLM_MAX_REPAIR_ROUNDS, ENV_LLM_MODEL,
    ENV_LLM_PROMPT_MAX_HISTORY_ITEMS, ENV_LLM_PROMPT_PROFILE, ENV_LLM_SHORT_TERM_GOAL,
    ENV_LLM_SYSTEM_PROMPT, ENV_LLM_TIMEOUT_MS,
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
pub use provider::{
    COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, COGNITION_RESPONSE_DIGEST_DOMAIN,
    CognitionResponseDigestDisposition, DecisionProvider, DecisionProviderError,
    classify_cognition_response_digest, cognition_legacy_response_digest,
    cognition_response_digest, validate_response_artifact_identity,
};
pub use turn_engine::{
    MemoryProjectionReport, RuntimeReadbackProof, TurnEngine, TurnEngineError, TurnEngineEvent,
    TurnEnginePhase, TurnRequest,
};

#[cfg(not(target_arch = "wasm32"))]
mod native_task;
#[cfg(not(target_arch = "wasm32"))]
pub use native_task::{
    NativeTaskContext, NativeTaskControl, NativeTaskHost, NativeTaskModelFailure,
    NativeTaskPreparation, NativeTaskRecovery, NativeTaskRunConfig, NativeTaskRunReport,
    NativeTaskRunner, NativeTaskStopReason, NativeTaskTool, NativeTaskToolResult,
    SdkNativeTaskHost,
};
