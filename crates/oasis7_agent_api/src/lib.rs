//! Stable, serializable contracts shared by the Runtime-facing adapter and an
//! agent Harness. These types carry proposals and read-only projections only;
//! they do not implement a World, execute provider calls, or grant state-write
//! authority.
//!
//! Runtime remains authoritative for request admission, capability checks,
//! resource reservation/settlement, world effects, durable receipts, and
//! continuation lifecycle. A receipt-lineage value is correlation data, not
//! proof of a durable world readback.

mod cognition;
pub mod game;
pub mod identity;
mod provider;
mod runtime;
#[cfg(feature = "schema")]
pub mod schema;

pub use cognition::{
    BudgetContractV1, CognitionError, ContinuationBudgetV1, ContinuationProposalV1,
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1, GoalSnapshotV1,
    MemoryContextEntryV1, MemoryContextSnapshotV1, MemoryWriteIntentV1, ResponseArtifactIdentityV1,
    RuntimeBindingV1, WakeConditionSubjectV1, WakeConditionV1, feedback_digest, h_v1,
};
pub use provider::{
    ActionCatalogEntry, CapabilityInvocationContext, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION,
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION, DecisionRequest, DecisionRequestContractError,
    DecisionResponse, FeedbackEnvelope, MemoryWriteIntent, ObservationEnvelope, ProviderDecision,
    ProviderDiagnostics, ProviderErrorEnvelope, ProviderExecutionMode, ProviderInteractionTarget,
    ProviderMissionContext, ProviderModuleCommand, ProviderNavigationNode, ProviderNearbyEntity,
    ProviderObservation, ProviderRecentEvent, ProviderSelfState, ProviderTokenUsage,
    ProviderTraceEnvelope, ProviderTranscriptEntry,
};
pub use runtime::{
    CognitionLeaseConsumptionViewV1, CognitionLeaseStatusV1, RuntimeReceiptLineageV1,
};

pub mod world_action;

pub mod world_cognition;

pub mod world_context;

pub mod world_quote;

pub mod intent_summary;
pub mod world_entity;
pub mod world_geometry;
pub mod world_resource;
pub mod world_scenario;

pub mod module_source;

pub mod node_contribution;

pub mod world_industry;

pub mod module_visual;

mod provider_chat;
pub use provider_chat::{
    ProviderAgentChatRequest, ProviderAgentChatResponse, provider_agent_chat_log_key,
};

mod response_identity;
pub use cognition::{CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION};
pub use response_identity::{
    COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, COGNITION_RESPONSE_DIGEST_DOMAIN,
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
    cognition_legacy_response_digest, cognition_response_digest,
};

pub use cognition::{
    COGNITION_CAPABILITY_CATALOG_DOMAIN, COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
};

pub mod legacy_economy;
pub mod legacy_scheduler;

/// Shared portable model-selection default; transport stays in the provider.
pub const DEFAULT_LLM_MODEL: &str = "gpt-5.4-mini";

mod model_provider;
pub use model_provider::{
    ModelProvider, ModelProviderError, ModelProviderHistory, ModelProviderRequest,
    ModelProviderResponse, ModelProviderTool, ModelProviderToolCall, ModelProviderToolResult,
    ModelProviderTrace, ModelProviderTraceEntry, ModelProviderTurn, ModelProviderUsage,
};
