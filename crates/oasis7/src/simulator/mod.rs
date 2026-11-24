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

#[cfg(target_arch = "wasm32")]
mod agent;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::agent;
#[cfg(target_arch = "wasm32")]
pub(crate) mod agent_api_compat;
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)]
pub(crate) mod agent_api_compat {
    pub use oasis7_runtime_host::feedback::{feedback_to_api, request_context_to_api};
}
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::async_agent_runner;
#[cfg(target_arch = "wasm32")]
mod cognition_policy;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::cognition_policy;
#[cfg(target_arch = "wasm32")]
mod cognition_response_identity;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::cognition_response_identity;
#[cfg(target_arch = "wasm32")]
mod continuous_agent_harness;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::continuous_agent_harness;
#[cfg(target_arch = "wasm32")]
mod decision_provider;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::decision_provider;

#[cfg(not(target_arch = "wasm32"))]
mod llm_agent;
mod memory;
#[cfg(not(target_arch = "wasm32"))]
mod model_provider;
#[cfg(target_arch = "wasm32")]
pub(crate) mod persist;
#[cfg(not(target_arch = "wasm32"))]
#[path = "persist_host_facade.rs"]
pub(crate) mod persist;
#[cfg(not(target_arch = "wasm32"))]
pub use oasis7_runtime_host::agent_execution::provider_json_admission;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::provider_loopback_adapter;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::provider_loopback_http;
#[cfg(target_arch = "wasm32")]
mod provider_material_capture;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::provider_material_capture;
#[cfg(target_arch = "wasm32")]
mod runner;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::runner;
pub(crate) mod runtime_perf;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_runtime_host::agent_execution::turn_engine;

#[cfg(test)]
mod tests;

// The legacy public API must remain type-identical while the historical
// one-line compatibility modules are retired in favor of canonical owners.
#[cfg(test)]
mod canonical_contract_owner_tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_host_agent_execution_preserves_original_type_identity() {
        use oasis7_runtime_host::agent_execution as native;
        use std::any::TypeId;

        assert_eq!(
            TypeId::of::<ActionResult>(),
            TypeId::of::<native::ActionResult>()
        );
        assert_eq!(
            TypeId::of::<dyn AgentBehavior>(),
            TypeId::of::<dyn native::AgentBehavior>()
        );
        assert_eq!(
            TypeId::of::<AsyncAgentRunner>(),
            TypeId::of::<native::AsyncAgentRunner>()
        );
        assert_eq!(
            TypeId::of::<TurnEngine>(),
            TypeId::of::<native::TurnEngine>()
        );
        assert_eq!(
            TypeId::of::<ContinuousAgentRequestContextV1>(),
            TypeId::of::<native::ContinuousAgentRequestContextV1>()
        );
        assert_eq!(
            TypeId::of::<MemoryWriteStore>(),
            TypeId::of::<native::MemoryWriteStore>()
        );
        assert_eq!(
            TypeId::of::<ProviderLoopbackHttpClient>(),
            TypeId::of::<native::ProviderLoopbackHttpClient>()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn chain_native_composite_archive_has_one_type_identity_and_original_json_wire() {
        use oasis7_runtime_host::simulator_archive::{kernel_host, persist};

        // L4 archive-bearing Host is the original composite type, not pure L2.
        let native = kernel_host::WorldKernel::new();
        let legacy: WorldKernel = native;
        let snapshot: persist::WorldSnapshot = legacy.snapshot();
        let bytes = serde_json::to_vec(&snapshot).expect("native archive snapshot JSON");
        let decoded: WorldSnapshot =
            serde_json::from_slice(&bytes).expect("decode through legacy archive API");
        let (kernel, extensions) = decoded.into_kernel_parts();
        let rebuilt: persist::WorldSnapshot =
            WorldSnapshot::from_kernel_parts(kernel, extensions);
        assert_eq!(
            bytes,
            serde_json::to_vec(&rebuilt).expect("legacy archive snapshot JSON"),
        );

        let journal: persist::WorldJournal = legacy.journal_snapshot();
        let journal_bytes = serde_json::to_vec(&journal).expect("native archive journal JSON");
        let decoded: WorldJournal =
            serde_json::from_slice(&journal_bytes).expect("decode through legacy journal API");
        assert_eq!(
            journal_bytes,
            serde_json::to_vec(&decoded).expect("legacy archive journal JSON"),
        );
    }

    #[test]
    fn old_simulator_exports_are_canonical_owner_contracts() {
        assert_eq!(
            std::any::TypeId::of::<BudgetContractV1>(),
            std::any::TypeId::of::<oasis7_agent_harness::BudgetContractV1>()
        );
        assert_eq!(
            std::any::TypeId::of::<AgentIntentSummaryV1>(),
            std::any::TypeId::of::<oasis7_agent_api::intent_summary::AgentIntentSummaryV1>()
        );
        assert_eq!(
            DEFAULT_LLM_MODEL,
            oasis7_agent_harness::llm_defaults::DEFAULT_LLM_MODEL
        );
        assert_eq!(
            DEFAULT_LLM_SYSTEM_PROMPT,
            oasis7_agent_harness::llm_defaults::DEFAULT_LLM_SYSTEM_PROMPT
        );
    }
}

// Re-export all public types
pub use oasis7_agent_api::CapabilityInvocationContext;
pub use agent::{
    ActionResult, AgentBehavior, AgentDecision, AgentDecisionExplanationV1, AgentDecisionTrace,
    AgentQuery, AgentQueryResult, LlmChatMessageTrace, LlmChatRole, LlmDecisionDiagnostics,
    LlmEffectIntentTrace, LlmEffectReceiptTrace, LlmPromptSectionTrace, LlmStepTrace,
    MicroDepotQuoteRequest,
};
#[cfg(not(target_arch = "wasm32"))]
pub use async_agent_runner::CommittedFeedbackMemoryInput;
#[cfg(not(target_arch = "wasm32"))]
pub use async_agent_runner::project_receipt_memory;
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
    DecisionRequest, DecisionRequestContractError, DecisionResponse, DecisionResponseExplanation,
    FeedbackEnvelope, GoldenDecisionFixture, MemoryWriteIntent, MockDecisionProvider,
    MockDecisionProviderState, ObservationEnvelope, ProviderBackedAgentBehavior,
    ProviderCapabilityContext, ProviderDecision, ProviderDiagnostics, ProviderErrorEnvelope,
    ProviderExecutionMode, ProviderInteractionTarget, ProviderMissionContext,
    ProviderModuleCommand, ProviderNavigationNode, ProviderNearbyEntity, ProviderObservation,
    ProviderRecentEvent, ProviderSelfState, ProviderTokenUsage, ProviderTraceEnvelope,
    ProviderTranscriptEntry, golden_decision_provider_fixtures,
};
pub use oasis7_agent_api::intent_summary::{
    AGENT_INTENT_SUMMARY_MAX_CHARS, AGENT_INTENT_SUMMARY_SCHEMA_VERSION,
    AUTHORITATIVE_AGENT_INTENT_SOURCES, AgentIntentKindV1, AgentIntentSourceV1,
    AgentIntentStatusV1, AgentIntentSummaryError, AgentIntentSummaryTemplateV1,
    AgentIntentSummaryV1, canonical_agent_intent_summary,
};
pub use oasis7_agent_harness::BudgetContractV1;
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
pub use provider_material_capture::{
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderPendingAttemptPhaseV1,
    ProviderPendingAttemptSummaryV1, ProviderPendingRootsCursorV1, ProviderPendingRootsPageV1,
    ProviderPendingStateRootsV1,
};
#[cfg(not(target_arch = "wasm32"))]
pub use provider_material_capture::{
    ProviderMaterialAuthority, ProviderOriginalMaterialStore, ProviderSpoolReservation,
};

#[cfg(not(target_arch = "wasm32"))]
pub use llm_agent::{
    LlmAgentBehavior, LlmAgentBuildError, LlmAgentConfig, LlmClientError,
    OpenAiChatCompletionClient,
};
#[cfg(all(any(test, feature = "test_tier_required"), not(target_arch = "wasm32")))]
pub use llm_agent::{
    LlmCompletionClient, LlmCompletionRequest, LlmCompletionResult, LlmCompletionTurn,
    LlmPromptProfile,
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
pub use oasis7_agent_harness::llm_defaults::{
    DEFAULT_CONFIG_FILE_NAME, DEFAULT_LLM_FORCE_REPLAN_AFTER_SAME_ACTION,
    DEFAULT_LLM_LONG_TERM_GOAL, DEFAULT_LLM_MAX_DECISION_STEPS, DEFAULT_LLM_MAX_MODULE_CALLS,
    DEFAULT_LLM_MAX_REPAIR_ROUNDS, DEFAULT_LLM_MODEL, DEFAULT_LLM_PROMPT_MAX_HISTORY_ITEMS,
    DEFAULT_LLM_SHORT_TERM_GOAL, DEFAULT_LLM_SYSTEM_PROMPT, ENV_LLM_API_KEY, ENV_LLM_BASE_URL,
    ENV_LLM_FORCE_REPLAN_AFTER_SAME_ACTION, ENV_LLM_LONG_TERM_GOAL, ENV_LLM_MAX_DECISION_STEPS,
    ENV_LLM_MAX_MODULE_CALLS, ENV_LLM_MAX_REPAIR_ROUNDS, ENV_LLM_MODEL,
    ENV_LLM_PROMPT_MAX_HISTORY_ITEMS, ENV_LLM_PROMPT_PROFILE, ENV_LLM_SHORT_TERM_GOAL,
    ENV_LLM_SYSTEM_PROMPT, ENV_LLM_TIMEOUT_MS,
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

#[cfg(target_arch = "wasm32")]
pub mod archive;
#[cfg(not(target_arch = "wasm32"))]
#[path = "archive_host_facade.rs"]
pub mod archive;
pub use archive::WorldSnapshotExtensions;
pub use oasis7_simulator::{KernelEvent, KernelJournal, KernelSnapshot};

#[cfg(target_arch = "wasm32")]
mod kernel_host;
#[cfg(not(target_arch = "wasm32"))]
#[path = "kernel_host_host_facade.rs"]
mod kernel_host;
pub use archive::WorldEvent;
pub use kernel_host::{WorldEventRef, WorldJournalView};
pub use kernel_host::{WorldKernel, initialize_kernel};
pub use oasis7_simulator::WorldKernel as NativeKernel;
