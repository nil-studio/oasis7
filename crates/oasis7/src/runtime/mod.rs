//! Runtime module - the core world execution engine.
//!
//! This module contains the World struct and all supporting types for:
//! - World state management
//! - Event processing and journaling
//! - Snapshot persistence and recovery
//! - Effect system with capabilities and policies
//! - Governance and manifest management
//! - Agent scheduling

mod agent_cell;
mod agent_claims;
mod audit;
mod blob_store;
mod builtin_factory_catalog;
mod builtin_recipe_catalog;
mod builtin_wasm_identity_manifest;
mod builtin_wasm_materializer;
mod capability_authorization;
mod cognition;
mod cognition_feedback_contract;
mod cognition_receipt_binding;
mod cognition_recovery;
mod cognition_retention;
mod cognition_scheduler;
mod cognition_validation;
mod cognition_wake;
mod consensus;
pub mod development_node_config;
mod effect;
mod error;
mod events;
mod formal_retained_history;
mod gameplay;
mod gameplay_state;
mod governance;
mod governance_membership;
mod m1_builtin_wasm_artifact;
mod m4_builtin_wasm_artifact;
mod m5_builtin_wasm_artifact;
mod main_token;
mod major_world_event;
mod manifest;
mod module_source_compiler;
mod module_store;
mod modules;
mod node_points;
mod node_points_runtime;
mod operability;
mod policy;
mod reward_asset;
mod rules;
mod segmenter;
mod signer;
mod snapshot;
mod snapshot_delta;
pub use snapshot_delta::{
    MAX_SNAPSHOT_CHAIN_BYTES, MAX_SNAPSHOT_DELTA_BYTES, MAX_SNAPSHOT_DELTA_DEPTH,
    SnapshotArtifactV1, SnapshotDeltaV1, resolve_snapshot_artifact,
    resolve_snapshot_artifact_with_depth,
};
mod state;
mod types;
mod util;
mod world;
mod world_event;

#[cfg(test)]
mod tests;

pub use builtin_factory_catalog::builtin_factory_spec;
pub use builtin_recipe_catalog::builtin_recipe_plan;

// Re-export all public types
pub use oasis7_agent_api::CognitionLeaseConsumptionViewV1;

// Types
pub use types::{
    ActionId, IntentSeq, MaterialLedgerId, PatchPath, ProposalId, WorldEventId, WorldTime,
};

// Agent cell
pub use agent_cell::{
    AGENT_INTENT_V2_SCHEMA_VERSION, AgentActivityStatus, AgentActivityV1, AgentCell,
    AgentIntentAuthorityContext, AgentIntentReplayDisposition, AgentIntentV2,
};

// Chain resource schema
pub use crate::chain_resource_schema::{
    CHAIN_RESOURCE_DELTA_SCHEMA_V1, CHAIN_RESOURCE_MANIFEST_SCHEMA_V1, CHUNK_GENERATION_SCHEMA_V1,
    ChainChunkResourceManifestEntry, ChainChunkResourceStatus, ChainFragmentResourceRef,
    ChainResourceCommitRef, ChainResourceDelta, ChainResourceDeltaEntry, ChainResourceDeltaSource,
    ChainResourceDerivationContext, ChainResourceManifest, ChainResourceOrderingKey,
    ChainResourceReplayStatus,
};

// Audit
pub use audit::{AuditCausedBy, AuditEventKind, AuditFilter};

// Trusted module authorization (v2)
pub use capability_authorization::{
    CapabilityAgentIdentity, CapabilityAuthorityFinalityBinding, CapabilityAuthorityFinalityProof,
    CapabilityAuthorityRecord, CapabilityAuthorizationAuditReceipt,
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
    CapabilityInvocationContext, CapabilityRevocationState,
};

// Effect system
pub use effect::{
    CapabilityGrant, EffectIntent, EffectOrigin, EffectReceipt, OriginKind,
    ReceiptParticipantSignature, ReceiptSignature, SignatureAlgorithm,
};

// Error
pub use error::WorldError;

// Authenticated compacted history committed by an anchored Snapshot.
pub(crate) use formal_retained_history::{
    AgentChatIntentLookupV1, FeedbackPublicationLookupV1, GameTaskControlLookupV1,
    digest_text as retained_history_digest_text,
};
pub use formal_retained_history::{
    FormalRetainedHistoryRootV1, RetainedHistoryReadError, VerifiedFormalRetainedHistoryParentV1,
};
pub use world::{FormalTickConsensusArchivePlanV1, FormalWorldRetentionPlanV1};

// Consensus
pub use consensus::{
    RuntimeCommittedTickContext, TICK_BLOCK_HEADER_SCHEMA_V1, TICK_BLOCK_HEADER_SCHEMA_V2,
    TickBlock, TickBlockHeader, TickCertificate, TickConsensusDriftReport, TickConsensusRecord,
    TickConsensusRejectionAuditEvent, TickConsensusSubmissionRole, TickExecutionDigest,
};

// Agent cognition admission boundary
pub use cognition::{
    AGENT_DECISION_ENVELOPE_V1_SCHEMA, AgentCognitionDisposition, AgentCognitionMailbox,
    AgentDecisionEnvelopeV1, CognitionCommitRejectReasonV1, CognitionValidationError,
    MvccValidator, PreconditionSubjectV1, PreconditionV1, classify_cognition_commit_error,
};

// Durable cognition commit/recovery boundary
pub(crate) use cognition_recovery::response_artifact_digest;
pub use cognition_recovery::{
    CognitionCrashPrefix, CognitionReceiptViewV1, CognitionRecovery, CognitionRecoveryError,
    CognitionRecoveryFixture, CognitionRecoveryProbe, CognitionRecoveryReport,
    CognitionResponseRecordV1, CognitionSnapshotV1, RuntimeCognitionBaseBindingV1,
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1,
    RuntimeFeedbackOutboxRecordV1, RuntimeFeedbackProjectionV1, RuntimeFeedbackRequestV1,
    RuntimeReceiptLineageV1, WorldCommitRecordV1, WorldRootViewV1,
};
pub use world::{
    BuiltinFactoryStartPreview, BuiltinRecipeStartPreview, COGNITION_ECONOMY_SCHEMA_VERSION,
    COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION, COGNITION_LEASE_SCHEMA_VERSION,
    COGNITION_PROVISIONING_EVENT_SCHEMA_VERSION, COGNITION_PROVISIONING_RECEIPT_SCHEMA_VERSION,
    COGNITION_PROVISIONING_SCHEMA_VERSION, COGNITION_RECEIPT_SCHEMA_VERSION,
    COGNITION_RESOURCE_VERSION_V1, CognitionEconomyError, CognitionEconomyEventV1,
    CognitionEconomyIdempotencyRecordV1, CognitionEconomyOperationRecordV1,
    CognitionEconomyStateV1, CognitionEconomyV1, CognitionLease, CognitionLeaseQuoteV1,
    CognitionLeaseRequestV1, CognitionLeaseStatusV1, CognitionLeaseV1,
    CognitionProvisioningEventV1, CognitionProvisioningReceiptV1, CognitionProvisioningRecordV1,
    CognitionProvisioningRequestV1, CognitionQuoteV1, CognitionReceipt, CognitionReceiptV1,
    CognitionResourceBalanceV1, LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS,
    LOCAL_TEST_PROVIDER_GRANT_TTL_TICKS, LocalTestProviderAuthorityConfig,
    LocalTestProviderAuthorityMode, LocalTestProviderGrantStatus, LocalTestProviderModuleArtifact,
    LocalTestProviderProvisioning, LocalTestProviderSessionMode, NativeBuiltinFactoryQuote,
    NativeBuiltinRecipeQuote, ProviderBackedBootstrapAuthorityV1,
};

// Durable cognition scheduler, wake and continuation projections
pub use cognition_retention::{
    CognitionRetentionStore, RetentionError, RetentionExecutionProbe, RetentionGcReport,
    RetentionRecordV1, RetentionReplayRequestV1, RetentionReplayResult,
};
pub use cognition_scheduler::{
    CognitionScheduler, SchedulerCrashPoint, SchedulerCursorRecoveryFixture,
    SchedulerCursorRecoveryReport, SchedulerCursorV1, SchedulerEnqueueOutcome, SchedulerError,
    SchedulerExecutionMetrics, SchedulerPolicyV1, SchedulerWakeV1,
};
pub use cognition_wake::{
    AgentContinuation, CognitionBudgetConsumptionV1, CognitionContextDigestsV1,
    CognitionContinuationProposalV1, CognitionContinuationResumeRequestV1,
    CognitionWakeDispositionV1, CognitionWakeHandoffResultV1, ContinuationBudgetV1,
    ContinuationReorgReport, ContinuationStatusV1, ContinuationTransition, WakeConditionError,
    WakeConditionV1, WakeConditionValidator, WakeEvaluation, WakeEvaluationContext,
};

// Events
pub use events::{
    Action, ActionEnvelope, CausedBy, CommittedRecipeOrigin, DomainEvent, GameplaySubmissionOrigin,
    IndustryStage, MainTokenFeeKind, MaterialMarketQuote, MaterialTransitPriority,
    ModuleProfileChanges, ModuleSourcePackage, RejectReason,
};

// Governance
pub use governance::{
    AgentSchedule, GovernanceEvent, GovernanceExecutionPolicy, GovernanceFinalityCertificate,
    GovernanceFinalityEpochSnapshot, GovernanceFinalitySignerRegistry,
    GovernanceIdentityPenaltyMonitorStats, GovernanceIdentityPenaltyRecord,
    GovernanceIdentityPenaltyStatus, GovernanceMainTokenControllerRegistry,
    GovernanceThresholdSignerPolicy, GovernanceValidatorAdmissionRecord,
    GovernanceValidatorAdmissionStatus, Proposal, ProposalDecision, ProposalStatus,
};

// Manifest
pub use manifest::{
    ConflictKind, Manifest, ManifestPatch, ManifestPatchOp, ManifestUpdate, PatchConflict,
    PatchMergeResult, PatchOpKind, PatchOpSummary, apply_manifest_patch, diff_manifest,
    merge_manifest_patches, merge_manifest_patches_with_conflicts,
};

// Modules
pub(crate) use agent_claims::auto_restricted_starter_claim_amount;
pub use agent_claims::{
    AgentClaimCostQuote, agent_claim_cap_for_tier, agent_claim_quote, agent_claim_reputation_tier,
};
pub use gameplay::{
    ActiveGameplayModule, GAMEPLAY_BASELINE_KINDS, GameplayKindCoverage, GameplayModeReadiness,
};
pub use gameplay_state::{
    AgentClaimState, AllianceState, CrisisState, CrisisStatus, EconomicContractFulfillmentKind,
    EconomicContractState, EconomicContractStatus, GOVERNANCE_IDENTITY_DEFAULT_MAX_VOTE_WEIGHT,
    GameplayPolicyState, GovernanceIdentityProfileState, GovernanceIdentityStatus,
    GovernanceProposalState, GovernanceProposalStatus, GovernanceVoteBallotState,
    GovernanceVoteState, GovernanceVoteWeightSnapshotState, MetaProgressState,
    WarParticipantOutcome, WarState,
};
pub use modules::{
    EconomyModuleKind, FactoryBuildDecision, FactoryBuildRequest, FactoryModuleApi,
    FactoryModuleSpec, FactoryProfileV1, GameplayContract, GameplayModuleKind,
    MaterialDefaultPriority, MaterialProfileV1, MaterialStack, MaterialTransportLossClass,
    ModuleAbiContract, ModuleActivation, ModuleArtifact, ModuleArtifactIdentity, ModuleCache,
    ModuleChangeSet, ModuleCommandCatalogEntry, ModuleDeactivation, ModuleEvent, ModuleEventKind,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRecord, ModuleRegistry, ModuleRole,
    ModuleSubscription, ModuleSubscriptionStage, ModuleUpgrade, ProductModuleApi,
    ProductModuleSpec, ProductProfileV1, ProductValidationDecision, ProductValidationRequest,
    RecipeExecutionPlan, RecipeExecutionRequest, RecipeModuleApi, RecipeModuleSpec,
    RecipeProfileV1, module_command_catalog,
};

// Node points
pub use main_token::{
    FROZEN_MAIN_TOKEN_INITIAL_SUPPLY, MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE, MAIN_TOKEN_TREASURY_BUCKET_MODULE_FEE,
    MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD,
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE, MAIN_TOKEN_TREASURY_BUCKET_SLASH,
    MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD, MainTokenAccountBalance, MainTokenBurnPolicy,
    MainTokenConfig, MainTokenEconomyAnomalyAlert, MainTokenEconomyAuditReport,
    MainTokenEconomyAuditThresholds, MainTokenEpochIssuanceRecord,
    MainTokenGenesisAllocationBucketState, MainTokenGenesisAllocationPlan,
    MainTokenInflationPolicy, MainTokenIssuanceSplitPolicy, MainTokenNodePointsBridgeDistribution,
    MainTokenNodePointsBridgeEpochRecord, MainTokenScheduledPolicyUpdate, MainTokenSupplyState,
    MainTokenTreasuryDistribution, MainTokenTreasuryDistributionRecord,
    RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY, RestrictedStarterClaimGrantState,
    RestrictedStarterClaimGrantStatus, RestrictedStarterClaimLiveopsPoolTopUpRecord,
    RestrictedStarterClaimRefundSink, main_token_account_id_from_node_public_key,
    main_token_bucket_unlocked_amount, production_hardened_main_token_config,
};
pub use node_points::{
    EpochSettlementReport, NodeContributionSample, NodePointsConfig, NodePointsError,
    NodePointsLedger, NodePointsLedgerSnapshot, NodeSettlement,
};
pub use node_points_runtime::{
    NodePointsRuntimeAccumulatorSnapshot, NodePointsRuntimeCollector,
    NodePointsRuntimeCollectorSnapshot, NodePointsRuntimeCursorSnapshot,
    NodePointsRuntimeHeuristics, NodePointsRuntimeObservation, measure_directory_storage_bytes,
};
pub use operability::{
    LongRunOperabilityGateViolation, LongRunOperabilityReleaseGateReport,
    LongRunOperabilityReleaseGateThresholds, LongRunReleaseStage,
};
pub use reward_asset::{
    NodeAssetBalance, NodeRewardMintRecord, ProtocolPowerReserve, RewardAssetConfig,
    RewardAssetInvariantReport, RewardAssetInvariantViolation, RewardSignatureGovernancePolicy,
    SystemOrderPoolBudget, reward_redeem_signature_v1,
};

// Blob store
pub use blob_store::{BlobStore, HashAlgorithm, LocalCasStore, blake3_hex};
pub(crate) use builtin_wasm_materializer::load_builtin_wasm_with_fetch_fallback;

#[cfg(test)]
pub(crate) use m1_builtin_wasm_artifact::m1_builtin_manifest_hash_tokens;
pub(crate) use m1_builtin_wasm_artifact::m1_builtin_module_artifact_identity;
#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) use m1_builtin_wasm_artifact::{
    m1_builtin_module_ids_manifest, register_m1_builtin_wasm_module_artifact,
};
#[cfg(test)]
pub(crate) use m4_builtin_wasm_artifact::m4_builtin_manifest_hash_tokens;
pub(crate) use m4_builtin_wasm_artifact::m4_builtin_module_artifact_identity;
#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) use m4_builtin_wasm_artifact::m4_builtin_module_ids_manifest;
#[cfg(test)]
pub(crate) use m5_builtin_wasm_artifact::m5_builtin_manifest_hash_tokens;
pub(crate) use m5_builtin_wasm_artifact::m5_builtin_module_artifact_identity;
#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) use m5_builtin_wasm_artifact::m5_builtin_module_ids_manifest;
#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) use world::m4_bootstrap_module_ids;

// Built-in module constants
pub use oasis7_wasm_store::{
    M1_AGENT_DEFAULT_MODULE_VERSION, M1_BODY_ACTION_COST_ELECTRICITY, M1_BODY_MODULE_ID,
    M1_MEMORY_MAX_ENTRIES, M1_MEMORY_MODULE_ID, M1_MOBILITY_MODULE_ID, M1_MOVE_RULE_MODULE_ID,
    M1_POWER_HARVEST_BASE_PER_TICK, M1_POWER_HARVEST_DISTANCE_BONUS_CAP,
    M1_POWER_HARVEST_DISTANCE_STEP_CM, M1_POWER_MODULE_VERSION, M1_POWER_STORAGE_CAPACITY,
    M1_POWER_STORAGE_INITIAL_LEVEL, M1_POWER_STORAGE_MOVE_COST_PER_KM,
    M1_RADIATION_POWER_MODULE_ID, M1_SENSOR_MODULE_ID, M1_STORAGE_CARGO_MODULE_ID,
    M1_STORAGE_POWER_MODULE_ID, M1_TRANSFER_RULE_MODULE_ID, M1_VISIBILITY_RULE_MODULE_ID,
    M4_ECONOMY_MODULE_VERSION, M4_FACTORY_ASSEMBLER_MODULE_ID, M4_FACTORY_MINER_MODULE_ID,
    M4_FACTORY_SMELTER_MODULE_ID, M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
    M4_PRODUCT_CONTROL_CHIP_MODULE_ID, M4_PRODUCT_FACTORY_CORE_MODULE_ID,
    M4_PRODUCT_IRON_INGOT_MODULE_ID, M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID,
    M4_PRODUCT_MODULE_RACK_MODULE_ID, M4_PRODUCT_MOTOR_MODULE_ID, M4_PRODUCT_SENSOR_PACK_MODULE_ID,
    M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID, M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID,
    M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID, M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID,
    M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID, M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID,
    M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID, M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID,
    M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID, M4_RECIPE_SMELT_IRON_MODULE_ID,
    M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID, M5_GAMEPLAY_CRISIS_MODULE_ID,
    M5_GAMEPLAY_ECONOMIC_MODULE_ID, M5_GAMEPLAY_GOVERNANCE_MODULE_ID, M5_GAMEPLAY_META_MODULE_ID,
    M5_GAMEPLAY_MODULE_VERSION, M5_GAMEPLAY_WAR_MODULE_ID,
};

// Module store
pub use module_store::ModuleStore;

// Segmenter
pub use segmenter::{JournalSegmentRef, SegmentConfig, segment_journal, segment_snapshot};

// Policy
pub use policy::{PolicyDecision, PolicyDecisionRecord, PolicyRule, PolicySet, PolicyWhen};

// Rules
pub use rules::{
    ActionOverrideRecord, ResourceBalanceError, ResourceDelta, RuleDecision,
    RuleDecisionMergeError, RuleDecisionRecord, RuleVerdict, merge_rule_decisions,
};

// Signer
pub use signer::ReceiptSigner;

// Snapshot
pub use snapshot::{
    Journal, RollbackApprovalSignature, RollbackAuthorityRecord, RollbackAuthorityRegistry,
    RollbackAuthorityRole, RollbackAuthorizationEnvelope, RollbackCompensationCaseRef,
    RollbackCompensationState, RollbackDispositionStatus, RollbackEvent, RollbackEventDisposition,
    RollbackIntent, RollbackNonceOutcome, RollbackOutcomeRecoveryMetadata, RollbackReadiness,
    RollbackReadinessEvidence, RollbackReceiptProjection, RollbackSourceEventIdentity, Snapshot,
    SnapshotCatalog, SnapshotMeta, SnapshotRecord, SnapshotRetentionPolicy,
};

// State
pub use state::{
    AgentLocationAuthorityV1, BodyOverlay, FACTORY_BUILD_STARTED_MODERN_VERSION,
    FactoryBuildJobState, FactoryBuildPowerObligationV1, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProductionFailureDispositionV1,
    FactoryProductionState, FactoryProductionStatus, FactoryRecycleReceiptV1,
    FactorySiteAuthorityV1, FactoryState, IndustryProgressState, LocationAnchorV1,
    LogisticsRouteV1, MaterialTransferReceiptV1, MaterialTransitJobState, ModuleInstanceState,
    ModuleReleaseAttestationState, ModuleReleaseManifestMappingState, ProductValidationAttemptV1,
    ProductValidationDeliveryCursor, ProductValidationReceiptV1, RecipeCompletionReceiptV1,
    RecipeJobState, STARTER_ASSEMBLER_FACTORY_ID, STARTER_INDUSTRIAL_COMPLETION_BOUNDARY,
    STARTER_INDUSTRIAL_PROFILE_ID, STARTER_INDUSTRIAL_PROFILE_REVISION, STARTER_SMELTER_FACTORY_ID,
    STARTER_SMELTER_RECIPE_ID, StarterIndustrialFeasibilityResult,
    StarterIndustrialFeasibilityStatus, StarterIndustrialMilestoneV1,
    StarterIndustrialSettlementSummaryV1, WorldState, WorldStateProjection,
};
pub(crate) use state::{ModuleVisualMutation, parse_module_visual_emit};

// World
#[cfg(test)]
pub(crate) use world::agent_delegation::AgentDelegationDecisionV1;
pub use world::agent_delegation::{
    AgentCausalReceiptV1, AgentDecisionCausalContextV1, AgentDelegationAuthorizationV1,
    AgentDelegationGrantV1, AgentOwnerControlKindV1, AgentOwnerControlV1,
};
pub use world::{
    AgentIntentProviderFailureDisposition, AgentIntentRecordOutcome,
    AuthoritativeRecoveryCommitError, AuthoritativeRecoveryCommitStatus,
    BuiltinReleaseManifestEntry, BuiltinReleaseManifestState,
    CommittedAuthoritativeRecoveryGeneration, ExecutionTransaction, LogisticsTransferQuote,
    M1ScenarioBootstrapConfig, MarketQuoteDecisionPreview, MarketQuoteSupplyDelta,
    ModuleTickRoutingDeterministicSnapshot, ModuleTickRoutingDurationBuckets,
    ModuleTickRoutingMetricsSnapshot, PreparedCommit, ProductValidationQuote,
    ReleaseSecurityPolicy, TransitionBaseHead, TransitionBuffer, TransitionCommitError,
    TransitionKernelEntriesView, TransitionKernelState, TransitionKernelView,
    TransitionPrepareError, TransitionRollbackError, TransitionSavepoint, WarDeclarationQuote,
    World, WorldRuntimeBackpressureStats, WorldRuntimeMemoryLimits,
};
#[cfg(not(target_arch = "wasm32"))]
pub use world::{ObserverLoadError, ObserverReadLimits};
pub use world::{rollback_affected_census_digest, rollback_journal_commitment};

// World event
#[cfg(test)]
pub(crate) use major_world_event::project_major_world_events;
pub use major_world_event::{
    MAJOR_WORLD_EVENT_PROJECTION_SCHEMA_VERSION, MajorWorldEventAnchor, MajorWorldEventAnchorScope,
    MajorWorldEventCategory, MajorWorldEventCausalReference, MajorWorldEventFreshness,
    MajorWorldEventIdentity, MajorWorldEventLifecycle, MajorWorldEventProjection,
    MajorWorldEventProjectionContext, MajorWorldEventSource, MajorWorldEventSourceAuthority,
    MajorWorldEventStreamState, MajorWorldEventVisibilityPermission,
    project_major_world_events_with_canonical_state,
};
pub use world_event::{
    CapabilityAuthorizationEvent, ModuleRuntimeChargeEvent, WorldEvent, WorldEventBody,
};

// Committed governance queries and explicit runtime adoption status.
pub use governance_membership::{RuntimeMembershipApplicationView, WorldGovernanceMembershipView};

pub mod chain_resource;

pub(crate) mod service_context;

pub mod cognition_disposition;

pub mod service_admission;

pub mod agent_state;
