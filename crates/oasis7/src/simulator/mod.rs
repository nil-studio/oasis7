//! World Simulator module - provides the simulation kernel, agent interface, and world model.
//!
//! This module is organized into submodules:
//! - `types`: Core type definitions (IDs, constants, resources)
//! - `world_model`: World entities (Agent, Location, Asset, WorldModel)
//! - `kernel`: WorldKernel implementation (time, events, actions)
//! - `persist`: Snapshot, Journal, and persistence utilities
//! - `agent`: Agent interface trait and decision types
//! - `memory`: Agent memory system (short-term, long-term)
//! - `runner`: AgentRunner, quota, rate limiting, metrics
//! - `power`: Power system (M4 social system)

mod agent;
pub(crate) mod agent_api_compat;
mod agent_intent_summary;
#[cfg(not(target_arch = "wasm32"))]
mod async_agent_pilot;
#[cfg(not(target_arch = "wasm32"))]
mod async_agent_runner;
mod cognition_policy;
mod cognition_response_identity;
mod continuous_agent_budget;
mod continuous_agent_harness;
mod decision_provider;

#[cfg(not(target_arch = "wasm32"))]
mod llm_agent;
mod llm_defaults;
mod memory;
#[cfg(not(target_arch = "wasm32"))]
mod model_provider;
pub(crate) mod persist;
#[cfg(not(target_arch = "wasm32"))]
mod provider_loopback_adapter;
#[cfg(not(target_arch = "wasm32"))]
mod provider_loopback_http;
mod runner;
pub(crate) mod runtime_perf;
#[cfg(not(target_arch = "wasm32"))]
mod turn_engine;

#[cfg(test)]
mod tests;

// Re-export all public types
pub use crate::capability_invocation_context::CapabilityInvocationContext;
pub use agent::{
    ActionResult, AgentBehavior, AgentDecision, AgentDecisionExplanationV1, AgentDecisionTrace,
    AgentQuery, AgentQueryResult, LlmChatMessageTrace, LlmChatRole, LlmDecisionDiagnostics,
    LlmEffectIntentTrace, LlmEffectReceiptTrace, LlmPromptSectionTrace, LlmStepTrace,
    MicroDepotQuoteRequest,
};
pub use agent_intent_summary::{
    AGENT_INTENT_SUMMARY_MAX_CHARS, AGENT_INTENT_SUMMARY_SCHEMA_VERSION,
    AUTHORITATIVE_AGENT_INTENT_SOURCES, AgentIntentKindV1, AgentIntentSourceV1,
    AgentIntentStatusV1, AgentIntentSummaryError, AgentIntentSummaryTemplateV1,
    AgentIntentSummaryV1, canonical_agent_intent_summary,
};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use async_agent_runner::CommittedFeedbackMemoryInput;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use async_agent_runner::project_receipt_memory;
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)] // Retained for the B3 Harness adapter boundary.
pub(crate) use async_agent_runner::validated_cognition_lease_consumption_view;
#[cfg(not(target_arch = "wasm32"))]
pub use async_agent_runner::{
    AsyncAgentRunner, AsyncAgentRunnerError, AsyncAgentTurnOutcome, AsyncTurnFeedback, AsyncTurnId,
    AsyncTurnLifecycle, AsyncWorldEffect, AsyncWorldProgress, RuntimeReceiptReadbackHandleV1,
    RuntimeReceiptReadbackVerifier,
};
pub use cognition_policy::{
    ContinuationAuthorityContextV1, ContinuationBudgetProgressV1, ContinuationBudgetV1,
    ContinuationCurrentContextV1, ContinuationHandle, ContinuationHarness,
    ContinuationInvalidationReason, ContinuationProjectionV1, ContinuationProposalV1,
    GoalSnapshotInputV1, GoalSnapshotProjector, GoalSnapshotV1, MemoryContextEntryV1,
    MemoryContextSnapshotV1, MemoryCorrectionV1, MemoryWriteIntentPolicyV1,
    MemoryWritePolicyContextV1, MemoryWritePolicyOutcome, MemoryWriteStore,
    NormalizedMemoryWriteIntentV1, RuntimeContinuationStatusV1, WakeConditionSubjectV1,
    WakeConditionV1,
};
pub use cognition_response_identity::{
    COGNITION_LEGACY_RESPONSE_DIGEST_DOMAIN, COGNITION_RESPONSE_DIGEST_DOMAIN,
    CognitionResponseDigestDisposition, classify_cognition_response_digest,
    cognition_legacy_response_digest, cognition_response_digest,
};
pub use continuous_agent_budget::BudgetContractV1;
pub use continuous_agent_harness::{
    AgentCognitionStore, COGNITION_CAPABILITY_CATALOG_DOMAIN,
    COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, COGNITION_PROVIDER_INVOCATION_DOMAIN,
    COGNITION_REQUEST_DIGEST_DOMAIN, COGNITION_RESPONSE_ARTIFACT_IDENTITY_DOMAIN,
    CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION, CognitionError,
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ContinuousAgentTurnContextV1, Digest32, FeedbackEnvelopeV1, FinalityBindingV1,
    MemoryWriteIntentV1, ResponseArtifactIdentityV1, RuntimeBindingV1, h_v1,
};
pub use decision_provider::{
    ActionCatalogEntry, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION,
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION, DecisionProvider, DecisionProviderError,
    DecisionRequest, DecisionRequestContractError, DecisionResponse, FeedbackEnvelope,
    GoldenDecisionFixture, MemoryWriteIntent, MockDecisionProvider, MockDecisionProviderState,
    ObservationEnvelope, ProviderBackedAgentBehavior, ProviderCapabilityContext, ProviderDecision,
    ProviderDiagnostics, ProviderErrorEnvelope, ProviderExecutionMode, ProviderInteractionTarget,
    ProviderMissionContext, ProviderModuleCommand, ProviderNavigationNode, ProviderNearbyEntity,
    ProviderObservation, ProviderRecentEvent, ProviderSelfState, ProviderTokenUsage,
    ProviderTraceEnvelope, ProviderTranscriptEntry, golden_decision_provider_fixtures,
};
pub use oasis7_simulator::ChunkRuntimeConfig;
pub use oasis7_simulator::generate_fragments;
pub use oasis7_simulator::{
    AgentSpawnConfig, AsteroidFragmentInitConfig, LocationSeedConfig, OriginLocationConfig,
    PowerPlantSeedConfig, WorldInitConfig, WorldInitError, WorldInitReport, build_world_model,
};
pub use oasis7_simulator::{
    CHUNK_SIZE_X_CM, CHUNK_SIZE_Y_CM, CHUNK_SIZE_Z_CM, ChunkBounds, ChunkCoord, chunk_bounds,
    chunk_coord_of, chunk_coords, chunk_grid_dims, chunk_seed,
};
pub use oasis7_simulator::{
    CM3_PER_M3, CompoundComposition, CuboidSizeCm, FragmentBlock, FragmentBlockField,
    FragmentCompoundKind, FragmentPhysicalProfile, GridPosCm, MIN_BLOCK_EDGE_CM, infer_element_ppm,
    mass_grams_from_volume_density, synthesize_fragment_budget, synthesize_fragment_profile,
};
pub(crate) fn default_schedule_recipe_readiness(
    recipe_id: &str,
    batches: i64,
) -> Option<oasis7_simulator::RecipeResourceCost> {
    oasis7_simulator::WorldKernel::new().recipe_resource_cost(recipe_id, batches)
}

#[cfg(not(target_arch = "wasm32"))]
pub use llm_agent::{
    LlmAgentBehavior, LlmAgentBuildError, LlmAgentConfig, LlmClientError,
    OpenAiChatCompletionClient,
};
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) use llm_agent::{
    LlmCompletionClient, LlmCompletionRequest, LlmCompletionResult, LlmCompletionTurn,
    LlmPromptProfile,
};
pub use llm_defaults::{
    DEFAULT_CONFIG_FILE_NAME, DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
    DEFAULT_LLM_LONG_TERM_GOAL, DEFAULT_LLM_MAX_DECISION_STEPS, DEFAULT_LLM_MAX_MODULE_CALLS,
    DEFAULT_LLM_MAX_REPAIR_ROUNDS, DEFAULT_LLM_MODEL, DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS,
    DEFAULT_LLM_SHORT_TERM_GOAL, DEFAULT_LLM_SYSTEM_PROMPT, ENV_LLM_API_KEY, ENV_LLM_BASE_URL,
    ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION, ENV_LLM_LONG_TERM_GOAL, ENV_LLM_MAX_DECISION_STEPS,
    ENV_LLM_MAX_MODULE_CALLS, ENV_LLM_MAX_REPAIR_ROUNDS, ENV_LLM_MODEL,
    ENV_LLM_PROMPT_MAX_HISTORY_ITEMS, ENV_LLM_PROMPT_PROFILE, ENV_LLM_SHORT_TERM_GOAL,
    ENV_LLM_SYSTEM_PROMPT, ENV_LLM_TIMEOUT_MS,
};
pub use memory::{
    AgentMemory, LongTermMemory, LongTermMemoryEntry, MemoryEntry, MemoryEntryKind, ShortTermMemory,
};
#[cfg(not(target_arch = "wasm32"))]
pub use model_provider::{
    LlmCompletionModelProvider, ModelProvider, ModelProviderError, ModelProviderRequest,
    ModelProviderResponse, ModelProviderTool, ModelProviderTrace, ModelProviderTraceEntry,
    ModelProviderTurn, ModelProviderUsage,
};
pub use oasis7_simulator::{
    Action, ActionEnvelope, ActionId, ActionSubmitter, AgentId, AssetId,
    CHUNK_GENERATION_SCHEMA_VERSION, CM_PER_KM, ChunkResourceBudget,
    DEFAULT_ELEMENT_RECOVERABILITY_PPM, DEFAULT_MOVE_COST_PER_KM_ELECTRICITY,
    DEFAULT_VISIBILITY_RANGE_CM, ElementBudgetError, ElementComposition, FacilityId,
    FragmentElementKind, FragmentRefillPreview, FragmentResourceBudget, JOURNAL_VERSION,
    LocationId, LocationProfile, MaterialKind, ModuleInstallTarget, PPM_BASE, PowerOrderSide,
    RefineQuote, ResourceKind, ResourceOwner, ResourceStock, SNAPSHOT_VERSION, ScheduleQuote,
    StockError, WorldEventId, WorldTime,
};
pub use oasis7_simulator::{
    Agent, AgentActivityProjectionV1, AgentExecutionDebugContext, AgentKinematics,
    AgentPromptProfile, Asset, AssetKind, AsteroidFragmentConfig, BoundaryReservation, ChunkState,
    EconomyConfig, Factory, FragmentResourceError, InstalledModuleState, Location,
    MaterialDistributionStrategy, MaterialRadiationFactors, MaterialWeights,
    ModuleArtifactBidState, ModuleArtifactListingState, ModuleArtifactState, PhysicsConfig,
    PhysicsParameterSpec, PowerOrderBookState, PowerOrderState, RegionalInfrastructure,
    SpaceConfig, ThermalStatus, WorldConfig, WorldModel, physics_parameter_specs,
};
pub use oasis7_simulator::{
    CmMappingRule, NativeResolutionDeclaration, NativeResolutionKind, NativeResolutionValue,
    RUNTIME_NATIVE_RESOLUTIONS, RoundingRule, fragment_block_native_resolution,
    native_resolution_by_subsystem, runtime_native_resolutions,
};
pub use oasis7_simulator::{ModuleVisualAnchor, ModuleVisualEntity};
pub use oasis7_simulator::{
    Observation, ObservedAgent, ObservedLocation, ObservedModuleArtifactRecord,
    ObservedModuleLifecycleState, ObservedModuleMarketState, ObservedPowerMarketState,
    ObservedSocialState,
};
pub use oasis7_simulator::{ScenarioSpecError, WorldScenario, WorldScenarioSpec};
pub use oasis7_simulator::{
    SocialAdjudicationDecision, SocialChallengeState, SocialEdgeLifecycleState, SocialEdgeState,
    SocialFactImpactQuote, SocialFactLifecycleState, SocialFactState, SocialStake,
};
pub use persist::{
    PersistError, PlayerAgentClaimOwnedSnapshot, PlayerAgentClaimQuoteSnapshot,
    PlayerAgentClaimSnapshot, PlayerGameplayAction, PlayerGameplayCausalityKind,
    PlayerGameplayExecutionState, PlayerGameplayFactoryProductionFailureDisposition,
    PlayerGameplayGoalKind, PlayerGameplayRecentFeedback, PlayerGameplaySnapshot,
    PlayerGameplayStageId, PlayerGameplayStageStatus, WorldJournal, WorldSnapshot,
};
#[cfg(not(target_arch = "wasm32"))]
pub use provider_loopback_adapter::ProviderLoopbackAdapter;
#[cfg(not(target_arch = "wasm32"))]
pub use provider_loopback_adapter::{RuntimeTrustedModuleExecutor, TrustedModuleCommandExecutor};
#[cfg(not(target_arch = "wasm32"))]
pub use provider_loopback_http::{
    LOOPBACK_HTTP_PROVIDER_TRANSPORT, PROVIDER_PHASE1_ACTION_SET_ALIAS, ProviderAgentChatRequest,
    ProviderAgentChatResponse, ProviderCompatibilityReport, ProviderCompatibilityStatus,
    ProviderFeedbackAck, ProviderHealth, ProviderInfo, ProviderLoopbackHttpClient,
    ProviderLoopbackHttpError, REMOTE_HTTPS_PROVIDER_TRANSPORT, evaluate_provider_compatibility,
    provider_agent_chat_log_key, provider_phase1_required_actions,
    provider_phase1_required_capabilities, validate_provider_http_base_url,
    validate_provider_loopback_http_base_url,
};
pub use runner::{
    AgentQuota, AgentRunner, AgentStats, AgentTickResult, RateLimitPolicy, RateLimitState,
    RegisteredAgent, RunnerLogEntry, RunnerLogKind, RunnerMetrics, SkippedReason,
};
#[cfg(not(target_arch = "wasm32"))]
pub use turn_engine::{
    RuntimeReadbackProof, TurnEngine, TurnEngineError, TurnEngineEvent, TurnEnginePhase,
    TurnRequest,
};

// Re-export power system types
pub use oasis7_simulator::{
    AgentPowerState, AgentPowerStatus, ConsumeReason, PlantStatus, PowerConfig, PowerEvent,
    PowerPlant, PowerSaleQuote, PowerSurvivalQuote,
};
pub use runtime_perf::{
    RuntimePerfBottleneck, RuntimePerfHealth, RuntimePerfSeriesSnapshot, RuntimePerfSnapshot,
    unsupported_runtime_perf_snapshot,
};

// Re-export event types from kernel
pub use oasis7_simulator::{
    ChunkGenerationCause, KernelRuleCost, KernelRuleDecision, KernelRuleDecisionMergeError,
    KernelRuleModuleContext, KernelRuleModuleInput, KernelRuleModuleOutput, KernelRuleVerdict,
    MICRO_DEPOT_DEBIT_AMOUNT_CRITICAL, MICRO_DEPOT_DEBIT_AMOUNT_HIGH, MICRO_DEPOT_DEBIT_AMOUNT_LOW,
    MICRO_DEPOT_DEBIT_AMOUNT_MEDIUM, MICRO_DEPOT_DEBIT_AMOUNT_NONE, MICRO_DEPOT_INSTALL_DATA_COST,
    MICRO_DEPOT_PROPOSAL_EMIT_KIND, MICRO_DEPOT_UPKEEP_DATA_COST, MicroDepotActionContext,
    MicroDepotActionKind, MicroDepotConsumedResourceClass, MicroDepotDecision,
    MicroDepotDeltaClass, MicroDepotEffectPreview, MicroDepotEvalInput, MicroDepotFacilityContext,
    MicroDepotInstallQuote, MicroDepotPlayerContext, MicroDepotPlayerFacilitySnapshot,
    MicroDepotPressureClass, MicroDepotProposal, MicroDepotProposalResourceDebit,
    MicroDepotQuotePreview, MicroDepotRegionContext, MicroDepotResourceDebit, MicroDepotStatus,
    PowerOrderFill, PromptUpdateOperation, RejectReason, WorldEventKind,
    compute_micro_depot_proposal_hash, evaluate_micro_depot_quote_with_module,
    merge_kernel_rule_decisions,
};

pub mod archive;
pub use archive::WorldSnapshotExtensions;
pub use oasis7_simulator::{KernelEvent, KernelJournal, KernelSnapshot};

mod kernel_host;
pub use archive::WorldEvent;
pub use kernel_host::{WorldKernel, initialize_kernel};
pub use oasis7_simulator::WorldKernel as NativeKernel;
