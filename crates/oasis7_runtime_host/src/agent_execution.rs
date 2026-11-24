//! Native owner of archive-bearing agent execution and trusted receipt/lease boundaries.
pub mod agent;
pub(crate) mod agent_api_compat;
mod async_agent_pilot;
pub mod async_agent_runner;
pub mod cognition_policy;
pub mod cognition_response_identity;
pub mod continuous_agent_harness;
pub mod decision_provider;
pub mod provider_json_admission;
pub mod provider_loopback_adapter;
pub mod provider_loopback_http;
pub mod provider_material_capture;
pub mod runner;
pub mod turn_engine;
pub(crate) mod world_service_feedback_host;

pub use crate::simulator_archive::archive;
pub use crate::simulator_archive::archive::WorldEvent;
pub use crate::simulator_archive::kernel_host::{WorldKernel, initialize_kernel};
pub use crate::simulator_archive::persist::{WorldJournal, WorldSnapshot};
pub use oasis7_agent_api::CapabilityInvocationContext;
mod runtime_perf {
    pub use oasis7_simulator::{
        RuntimePerfBottleneck, RuntimePerfCollector, RuntimePerfHealth, RuntimePerfSeriesSnapshot,
        RuntimePerfSnapshot, unsupported_runtime_perf_snapshot,
    };
}

pub use agent::{
    AgentBehavior, AgentDecision, AgentDecisionExplanationV1, AgentDecisionTrace, AgentQuery,
    AgentQueryResult, LlmChatMessageTrace, LlmChatRole, LlmDecisionDiagnostics,
    LlmEffectIntentTrace, LlmEffectReceiptTrace, LlmPromptSectionTrace, LlmStepTrace,
    MicroDepotQuoteRequest,
};
pub use async_agent_runner::CommittedFeedbackMemoryInput;
pub use async_agent_runner::project_receipt_memory;
pub(crate) use async_agent_runner::validated_cognition_lease_consumption_view;
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
    AgentPowerState, AgentPowerStatus, ConsumeReason, PlantStatus, PowerConfig, PowerEvent,
    PowerPlant, PowerSaleQuote, PowerSurvivalQuote,
};
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
pub use oasis7_simulator::{
    CmMappingRule, NativeResolutionDeclaration, NativeResolutionKind, NativeResolutionValue,
    RUNTIME_NATIVE_RESOLUTIONS, RoundingRule, fragment_block_native_resolution,
    native_resolution_by_subsystem, runtime_native_resolutions,
};
pub use oasis7_simulator::{KernelEvent, KernelJournal, KernelSnapshot};
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
pub use provider_loopback_adapter::ProviderLoopbackAdapter;
pub use provider_loopback_adapter::{RuntimeTrustedModuleExecutor, TrustedModuleCommandExecutor};
pub use provider_loopback_http::{
    LOOPBACK_HTTP_PROVIDER_TRANSPORT, PROVIDER_PHASE1_ACTION_SET_ALIAS, ProviderAgentChatRequest,
    ProviderAgentChatResponse, ProviderCompatibilityReport, ProviderCompatibilityStatus,
    ProviderFeedbackAck, ProviderHealth, ProviderInfo, ProviderLoopbackHttpClient,
    ProviderLoopbackHttpError, REMOTE_HTTPS_PROVIDER_TRANSPORT, evaluate_provider_compatibility,
    provider_agent_chat_log_key, provider_phase1_required_actions,
    provider_phase1_required_capabilities, validate_provider_http_base_url,
    validate_provider_loopback_http_base_url,
};
pub use provider_material_capture::{
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderPendingAttemptPhaseV1,
    ProviderPendingAttemptSummaryV1, ProviderPendingRootsCursorV1, ProviderPendingRootsPageV1,
    ProviderPendingStateRootsV1,
};
pub use provider_material_capture::{
    ProviderMaterialAuthority, ProviderOriginalMaterialStore, ProviderSpoolReservation,
};
pub use runner::{
    AgentQuota, AgentRunner, AgentStats, AgentTickResult, RateLimitPolicy, RateLimitState,
    RegisteredAgent, RunnerLogEntry, RunnerLogKind, RunnerMetrics, SkippedReason,
};
pub use runtime_perf::{
    RuntimePerfBottleneck, RuntimePerfHealth, RuntimePerfSeriesSnapshot, RuntimePerfSnapshot,
    unsupported_runtime_perf_snapshot,
};
pub use turn_engine::{
    RuntimeReadbackProof, TurnEngine, TurnEngineError, TurnEngineEvent, TurnEnginePhase,
    TurnRequest,
};

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// Result of an action execution, providing feedback to the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    /// The action that was executed.
    pub action: Action,
    /// The action ID assigned by the kernel.
    pub action_id: ActionId,
    /// Whether the action succeeded.
    pub success: bool,
    /// The resulting event (success or rejection).
    pub event: WorldEvent,
}

impl ActionResult {
    /// Returns true if the action was rejected.
    pub fn is_rejected(&self) -> bool {
        matches!(self.event.kind, WorldEventKind::ActionRejected { .. })
    }

    /// Returns the rejection reason if the action was rejected.
    pub fn reject_reason(&self) -> Option<&RejectReason> {
        match &self.event.kind {
            WorldEventKind::ActionRejected { reason } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod action_result_tests {
    use super::*;

    #[test]
    fn action_result_helpers() {
        let action = Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        };

        let success_event = WorldEvent {
            id: 1,
            time: 1,
            kind: WorldEventKind::AgentMoved {
                agent_id: "agent-1".to_string(),
                from: "loc-1".to_string(),
                to: "loc-2".to_string(),
                distance_cm: 1000,
                electricity_cost: 1,
            },
            runtime_event: None,
        };
        let success_result = ActionResult {
            action: action.clone(),
            action_id: 1,
            success: true,
            event: success_event,
        };
        assert!(!success_result.is_rejected());
        assert!(success_result.reject_reason().is_none());

        let reject_event = WorldEvent {
            id: 2,
            time: 2,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: "agent-1".to_string(),
                },
            },
            runtime_event: None,
        };
        let reject_result = ActionResult {
            action,
            action_id: 2,
            success: false,
            event: reject_event,
        };
        assert!(reject_result.is_rejected());
        assert!(matches!(
            reject_result.reject_reason(),
            Some(RejectReason::AgentNotFound { .. })
        ));
    }
}
