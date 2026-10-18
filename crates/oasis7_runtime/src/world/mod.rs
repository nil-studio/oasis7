//! The World struct - core runtime implementation.
mod host_reads;
pub use host_reads::{JournalEventPosition, RecipeExecutionReadiness};

mod actions;
pub(crate) mod agent_claim_economic_publication;
pub(crate) mod agent_claim_light_lifecycle_publication;
pub(crate) mod agent_claim_terminal_publication;
mod agent_claims;
pub(crate) mod agent_delegation;
mod agent_intent;
mod source_compiler;
mod provider_outcome;
pub use provider_outcome::AgentIntentReference;
mod execution_identity;
pub use execution_identity::WorldExecutionStateIdentity;
#[cfg(test)]
pub(crate) use agent_intent::derive_agent_chat_request_digest;
pub(crate) mod agent_intent_publication;
mod agent_intent_terminal;
pub(crate) mod alliance_war_publication;
pub use agent_intent::{AgentIntentProviderFailureDisposition, AgentIntentRecordOutcome};
mod audit;
mod base_layer;
mod body;
mod bootstrap_economy;
mod bootstrap_gameplay;
mod bootstrap_power;
mod builtin_factory_quote;
pub use builtin_factory_quote::{BuiltinFactoryStartPreview, NativeBuiltinFactoryQuote};
mod builtin_recipe_quote;
pub use builtin_recipe_quote::{BuiltinRecipeStartPreview, NativeBuiltinRecipeQuote};
mod capability_authorization;
mod capability_authorization_admin;
mod capability_authorization_command;
mod capability_authorization_command_projection;
mod capability_authorization_command_stage;
mod capability_authorization_events;
mod capability_authorization_publication;
mod capability_authorization_state;
mod capability_authorization_transaction;
mod capability_authorization_validation;
mod capability_catalog;
mod capability_effect_receipt_projection;
#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
mod capability_test_fixture;
mod cognition_command;
mod cognition_economy;
#[path = "cognition_economy_world.rs"]
mod cognition_economy_world;
mod cognition_feedback;
mod cognition_gpd;
mod cognition_orchestration;
mod cognition_persistence;
mod cognition_persistence_validation;
pub(crate) mod economic_contract_publication;
mod economy;
pub(crate) mod economy_data_publication;
mod economy_product_validation;
mod effect_publication;
#[cfg(test)]
mod effect_publication_transaction_regressions;
mod effects;
mod event_processing;
mod factory_authority;
mod formal_execution_tick;
mod formal_retention;
mod formal_tick_consensus;
pub use formal_tick_consensus::FormalTickConsensusArchivePlanV1;
mod formal_retention_candidate_restore;
mod formal_retention_tick;
mod formal_retention_tick_reads;
mod runtime_support;
pub use formal_retention::FormalWorldRetentionPlanV1;
#[cfg(test)]
use oasis7_wasm_router::PreparedSubscription;
use runtime_support::PreparedSubscriptionCacheEntry;
pub(crate) use runtime_support::SharedFormalHistoryReader;
mod gameplay_layer;
mod gameplay_loop;
mod governance;
mod governance_identity_penalty;
pub(crate) mod governance_meta_publication;
#[cfg(test)]
mod governance_proposal_status_publication_transaction_regressions;
mod governance_publication;
mod governance_quote;
pub(crate) mod governance_registry_publication;
#[cfg(test)]
mod governance_registry_publication_transaction_regressions;
#[cfg(test)]
mod governed_module_lifecycle_transaction_regressions;
pub(crate) mod main_token_governance_monetary_publication;
pub(crate) mod main_token_monetary_publication;
pub(crate) mod main_token_restricted_claim_publication;
mod module_admission_freeze;
#[cfg(test)]
mod module_artifact_deployment_transaction_regressions;
#[cfg(test)]
mod module_artifact_retirement_transaction_regressions;
#[cfg(test)]
mod module_change_batch_transaction_regressions;
#[cfg(test)]
mod module_instance_publication_transaction_regressions;
#[cfg(test)]
mod module_marketplace_transaction_regressions;
#[cfg(test)]
mod module_metadata_publication_transaction_regressions;
#[cfg(test)]
mod module_output_publication_transaction_regressions;
#[cfg(test)]
mod module_release_publication_transaction_regressions;
#[cfg(test)]
mod module_release_review_publication_transaction_regressions;
#[cfg(test)]
mod module_store_load_transaction_regressions;
#[cfg(test)]
mod module_visual_publication_transaction_regressions;
pub(crate) mod node_points_settlement_publication;
#[cfg(test)]
mod power_publication_transaction_regressions;
pub(crate) mod power_redemption_publication;
#[cfg(test)]
#[path = "prepared_base_head_transaction_regressions.rs"]
mod prepared_base_head_transaction_regressions;
pub(crate) mod starter_oc_claim_publication;
mod war_declaration_quote;
pub use war_declaration_quote::WarDeclarationQuote;
mod logistics;
pub use logistics::LogisticsTransferQuote;
mod market_quote_decision_preview;
pub use market_quote_decision_preview::{MarketQuoteDecisionPreview, MarketQuoteSupplyDelta};
mod defaults;
mod local_test_provider_bootstrap;
mod main_token_economy_audit;
mod module_actions;
mod module_artifact_retirement;
#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
mod module_change_batch_publication;
mod module_marketplace_publication;
mod module_release_publication;
mod module_routing_runtime;
mod module_runtime;
mod module_runtime_labels;
mod module_runtime_metering;
mod module_runtime_publication;
mod module_tick_runtime;
mod observer_identity;
mod operability_release_gate;
mod persistence;
mod prepared_base_head;
mod provider_backed_bootstrap;
pub use persistence::{
    AuthoritativeRecoveryCommitError, AuthoritativeRecoveryCommitStatus,
    CommittedAuthoritativeRecoveryGeneration,
};
#[cfg(not(target_arch = "wasm32"))]
pub use persistence::{ObserverLoadControl, ObserverLoadError, ObserverReadLimits};
mod policy;
mod product_validation_quote;
pub use product_validation_quote::ProductValidationQuote;
mod release_manifest;
mod resources;
mod restricted_claim_grants;
mod rollback;
pub use rollback::{rollback_affected_census_digest, rollback_journal_commitment};
mod rules;
mod scheduling;
mod snapshot;
mod step;
mod tick_consensus;
mod tick_consensus_state_root;
#[cfg(test)]
mod tick_consensus_test_support;
mod transition;
mod world_service;
pub use cognition_economy::{
    COGNITION_ECONOMY_SCHEMA_VERSION, COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
    COGNITION_LEASE_SCHEMA_VERSION, COGNITION_PROVISIONING_EVENT_SCHEMA_VERSION,
    COGNITION_PROVISIONING_RECEIPT_SCHEMA_VERSION, COGNITION_PROVISIONING_SCHEMA_VERSION,
    COGNITION_RECEIPT_SCHEMA_VERSION, COGNITION_RESOURCE_VERSION_V1, CognitionEconomyError,
    CognitionEconomyEventV1, CognitionEconomyIdempotencyRecordV1,
    CognitionEconomyOperationRecordV1, CognitionEconomyStateV1, CognitionEconomyV1, CognitionLease,
    CognitionLeaseQuoteV1, CognitionLeaseRequestV1, CognitionLeaseStatusV1, CognitionLeaseV1,
    CognitionProvisioningEventV1, CognitionProvisioningReceiptV1, CognitionProvisioningRecordV1,
    CognitionProvisioningRequestV1, CognitionQuoteV1, CognitionReceipt, CognitionReceiptV1,
    CognitionResourceBalanceV1,
};
pub use local_test_provider_bootstrap::{
    LOCAL_TEST_PROVIDER_GRANT_RENEWAL_THRESHOLD_TICKS, LOCAL_TEST_PROVIDER_GRANT_TTL_TICKS,
    LocalTestProviderAuthorityConfig, LocalTestProviderAuthorityMode, LocalTestProviderGrantStatus,
    LocalTestProviderModuleArtifact, LocalTestProviderProvisioning, LocalTestProviderSessionMode,
};
pub use provider_backed_bootstrap::ProviderBackedBootstrapAuthorityV1;
pub use transition::{
    ExecutionTransaction, PreparedCommit, TransitionBaseHead, TransitionBuffer,
    TransitionCommitError, TransitionKernelEntriesView, TransitionKernelState,
    TransitionKernelView, TransitionPrepareError, TransitionRollbackError, TransitionSavepoint,
};

use super::CrisisStatus;
use super::capability_authorization::{
    CapabilityAuthorizationAuditReceipt, CapabilityAuthorizationNonceRecord,
    CapabilityBudgetAccount, CapabilityEffectReceiptLink, CapabilityInvocationContext,
    CapabilityRevocationState,
};
use super::cognition_recovery::default_cognition_persistence_projection;
use super::consensus::{TickConsensusRecord, TickConsensusRejectionAuditEvent};
use super::effect::{CapabilityGrant, EffectIntent};
use super::error::WorldError;
use super::events::{ActionEnvelope, MaterialTransitPriority};
use super::governance::{
    GovernanceExecutionPolicy, GovernanceFinalityEpochSnapshot, GovernanceFinalitySignerRegistry,
    GovernanceIdentityPenaltyMonitorStats, GovernanceIdentityPenaltyRecord,
    GovernanceMainTokenControllerRegistry, GovernanceValidatorAdmissionRecord, Proposal,
};
use super::main_token::main_token_account_id_from_node_public_key;
use super::manifest::Manifest;
use super::modules::{ModuleCache, ModuleLimits, ModuleRegistry};
use super::policy::PolicySet;
use super::signer::ReceiptSigner;
use super::snapshot::{Journal, SnapshotCatalog};
use super::state::WorldState;
use super::types::{ActionId, IntentSeq, ProposalId, WorldEventId, WorldTime};
use oasis7_agent_api::world_resource::{ChainResourceDelta, ChainResourceManifest};
#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) use bootstrap_economy::m4_bootstrap_module_ids;
pub use bootstrap_power::M1ScenarioBootstrapConfig;
use module_tick_runtime::ModuleTickRoutingMetrics;
pub use module_tick_runtime::{
    ModuleTickRoutingDeterministicSnapshot, ModuleTickRoutingDurationBuckets,
    ModuleTickRoutingMetricsSnapshot,
};
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::{cell::RefCell, path::PathBuf, sync::Arc};

const DEFAULT_MAX_PENDING_ACTIONS: usize = 8_192;
const DEFAULT_MAX_PENDING_EFFECTS: usize = 8_192;
const DEFAULT_MAX_INFLIGHT_EFFECTS: usize = 8_192;
const DEFAULT_MAX_JOURNAL_EVENTS: usize = 65_536;
pub(super) const BUILTIN_MODULE_SIGNER_NODE_ID: &str = "builtin.module.release.signer";
pub(super) const BUILTIN_MODULE_SIGNER_PUBLIC_KEY_HEX: &str =
    "4b97aa20b3abd613401d4f5778eab8b6c019bd2ea912d1ce2234868536389ebb";
#[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
pub(super) const TEST_MODULE_SIGNER_NODE_ID: &str = "test.module.release.signer";

#[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
use runtime_support::test_module_signer_public_key_hex;

fn default_next_governance_identity_penalty_id() -> u64 {
    1
}

fn default_tick_consensus_authority_source() -> String {
    BUILTIN_MODULE_SIGNER_NODE_ID.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuntimeMemoryLimits {
    pub max_pending_actions: usize,
    pub max_pending_effects: usize,
    pub max_inflight_effects: usize,
    pub max_journal_events: usize,
}

impl Default for WorldRuntimeMemoryLimits {
    fn default() -> Self {
        Self {
            max_pending_actions: DEFAULT_MAX_PENDING_ACTIONS,
            max_pending_effects: DEFAULT_MAX_PENDING_EFFECTS,
            max_inflight_effects: DEFAULT_MAX_INFLIGHT_EFFECTS,
            max_journal_events: DEFAULT_MAX_JOURNAL_EVENTS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuntimeBackpressureStats {
    pub pending_actions_evicted: u64,
    pub pending_effects_evicted: u64,
    pub inflight_effects_evicted: u64,
    pub inflight_effect_dispatch_blocked: u64,
    pub journal_events_evicted: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsSlaMetrics {
    pub completed_transits: u64,
    pub fulfilled_transits: u64,
    pub breached_transits: u64,
    pub total_delay_ticks: u64,
    pub urgent_completed_transits: u64,
    pub urgent_fulfilled_transits: u64,
    pub urgent_breached_transits: u64,
    pub urgent_total_delay_ticks: u64,
}

impl LogisticsSlaMetrics {
    fn record_completion(
        &mut self,
        expected_ready_at: WorldTime,
        completed_at: WorldTime,
        priority: MaterialTransitPriority,
    ) {
        self.completed_transits = self.completed_transits.saturating_add(1);
        if priority == MaterialTransitPriority::Urgent {
            self.urgent_completed_transits = self.urgent_completed_transits.saturating_add(1);
        }
        if completed_at > expected_ready_at {
            let delay = completed_at.saturating_sub(expected_ready_at);
            self.breached_transits = self.breached_transits.saturating_add(1);
            self.total_delay_ticks = self.total_delay_ticks.saturating_add(delay);
            if priority == MaterialTransitPriority::Urgent {
                self.urgent_breached_transits = self.urgent_breached_transits.saturating_add(1);
                self.urgent_total_delay_ticks = self.urgent_total_delay_ticks.saturating_add(delay);
            }
        } else {
            self.fulfilled_transits = self.fulfilled_transits.saturating_add(1);
            if priority == MaterialTransitPriority::Urgent {
                self.urgent_fulfilled_transits = self.urgent_fulfilled_transits.saturating_add(1);
            }
        }
    }

    pub fn breach_rate(&self) -> f64 {
        if self.completed_transits == 0 {
            return 0.0;
        }
        self.breached_transits as f64 / self.completed_transits as f64
    }

    pub fn fulfillment_rate(&self) -> f64 {
        if self.completed_transits == 0 {
            return 1.0;
        }
        self.fulfilled_transits as f64 / self.completed_transits as f64
    }

    pub fn average_delay_ticks(&self) -> f64 {
        if self.completed_transits == 0 {
            return 0.0;
        }
        self.total_delay_ticks as f64 / self.completed_transits as f64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseSecurityPolicy {
    #[serde(default = "default_allow_builtin_manifest_fallback")]
    pub allow_builtin_manifest_fallback: bool,
    #[serde(default = "default_allow_identity_hash_signature")]
    pub allow_identity_hash_signature: bool,
    #[serde(default = "default_allow_local_finality_signing")]
    pub allow_local_finality_signing: bool,
    #[serde(default = "default_allow_runtime_source_compile")]
    pub allow_runtime_source_compile: bool,
}

impl ReleaseSecurityPolicy {
    pub fn production_hardened() -> Self {
        Self {
            allow_builtin_manifest_fallback: false,
            allow_identity_hash_signature: false,
            allow_local_finality_signing: false,
            allow_runtime_source_compile: false,
        }
    }

    pub fn is_production_hardened(&self) -> bool {
        !self.allow_builtin_manifest_fallback
            && !self.allow_identity_hash_signature
            && !self.allow_local_finality_signing
            && !self.allow_runtime_source_compile
    }
}

impl Default for ReleaseSecurityPolicy {
    fn default() -> Self {
        Self {
            allow_builtin_manifest_fallback: default_allow_builtin_manifest_fallback(),
            allow_identity_hash_signature: default_allow_identity_hash_signature(),
            allow_local_finality_signing: default_allow_local_finality_signing(),
            allow_runtime_source_compile: default_allow_runtime_source_compile(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BuiltinReleaseManifestEntry {
    #[serde(default)]
    pub hash_tokens: Vec<String>,
    #[serde(default)]
    pub artifact_identities: BTreeMap<String, crate::ModuleArtifactIdentity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BuiltinReleaseManifestState {
    #[serde(default)]
    pub module_sets: BTreeMap<String, BTreeMap<String, BuiltinReleaseManifestEntry>>,
}

fn default_allow_builtin_manifest_fallback() -> bool {
    true
}

fn default_allow_identity_hash_signature() -> bool {
    true
}

fn default_allow_local_finality_signing() -> bool {
    true
}

fn default_allow_runtime_source_compile() -> bool {
    true
}

/// The main World runtime that orchestrates the simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    #[serde(skip)]
    source_compiler: super::module_source_compiler::RuntimeSourceCompiler,
    manifest: Manifest,
    #[serde(default = "super::cognition_recovery::default_cognition_persistence_projection")]
    cognition: JsonValue,
    module_registry: ModuleRegistry,
    module_artifacts: BTreeSet<String>,
    #[serde(skip)]
    module_artifact_bytes: BTreeMap<String, Arc<[u8]>>,
    #[serde(skip)]
    module_cache: ModuleCache,
    #[serde(skip)]
    prepared_subscription_cache: BTreeMap<String, PreparedSubscriptionCacheEntry>,
    module_limits_max: ModuleLimits,
    snapshot_catalog: SnapshotCatalog,
    #[serde(default)]
    chain_resource_manifest: ChainResourceManifest,
    #[serde(default)]
    latest_chain_resource_delta: Option<ChainResourceDelta>,
    state: WorldState,
    /// Process-local authenticated CAS access. It is intentionally absent from
    /// snapshots and survives only in-memory World clones.
    #[serde(skip, default)]
    formal_history_reader: Option<formal_retention::AttachedFormalHistory>,
    journal: Journal,
    next_event_id: WorldEventId,
    #[serde(default)]
    next_event_id_era: u64,
    next_action_id: ActionId,
    #[serde(default)]
    next_action_id_era: u64,
    next_intent_id: IntentSeq,
    #[serde(default)]
    next_intent_id_era: u64,
    next_proposal_id: ProposalId,
    #[serde(default)]
    next_proposal_id_era: u64,
    pending_actions: VecDeque<ActionEnvelope>,
    pending_effects: VecDeque<EffectIntent>,
    inflight_effects: BTreeMap<String, EffectIntent>,
    #[serde(default)]
    module_tick_schedule: BTreeMap<String, u64>,
    #[serde(default)]
    module_tick_routing_metrics: ModuleTickRoutingMetrics,
    capabilities: BTreeMap<String, CapabilityGrant>,
    /// Immutable v2 grant bodies keyed by stable grant id.  The ABI owns the
    /// typed wire DTO; JSON here keeps snapshots forward-compatible with ABI
    /// additions while retaining the exact values used for authorization.
    #[serde(default)]
    capability_grants_v2: BTreeMap<String, JsonValue>,
    #[serde(default)]
    capability_revocation_state: CapabilityRevocationState,
    #[serde(default)]
    capability_nonce_records: BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    #[serde(default)]
    capability_authorization_receipts: BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    #[serde(default)]
    capability_invocation_contexts: BTreeMap<String, CapabilityInvocationContext>,
    #[serde(default)]
    capability_authorization_root: String,
    #[serde(default)]
    capability_budget_accounts: BTreeMap<String, CapabilityBudgetAccount>,
    /// Pending provider receipt association for module effects.  This remains
    /// durable across restart and is resolved only by an actual receipt.
    #[serde(default)]
    capability_effect_receipt_links: BTreeMap<String, CapabilityEffectReceiptLink>,
    policies: PolicySet,
    proposals: BTreeMap<ProposalId, Proposal>,
    scheduler_cursor: Option<String>,
    #[serde(skip)]
    receipt_signer: Option<ReceiptSigner>,
    #[serde(skip, default)]
    persistence_dir: RefCell<Option<PathBuf>>,
    #[serde(default)]
    runtime_memory_limits: WorldRuntimeMemoryLimits,
    #[serde(default)]
    runtime_backpressure_stats: WorldRuntimeBackpressureStats,
    #[serde(default)]
    logistics_sla_metrics: LogisticsSlaMetrics,
    #[serde(default)]
    threat_heatmap: BTreeMap<String, i64>,
    #[serde(default)]
    tick_consensus_records: Vec<TickConsensusRecord>,
    #[serde(skip)]
    formal_archived_tick_consensus_count: u64,
    #[serde(skip)]
    formal_execution_tick: Option<(super::RuntimeCommittedTickContext, bool)>,
    #[serde(default = "default_tick_consensus_authority_source")]
    tick_consensus_authority_source: String,
    #[serde(default)]
    tick_consensus_rejection_audit_events: Vec<TickConsensusRejectionAuditEvent>,
    #[serde(default)]
    governance_execution_policy: GovernanceExecutionPolicy,
    #[serde(default)]
    governance_finality_epoch_snapshots: BTreeMap<u64, GovernanceFinalityEpochSnapshot>,
    #[serde(default)]
    governance_emergency_brake_until_tick: Option<WorldTime>,
    #[serde(default)]
    governance_identity_penalties: BTreeMap<u64, GovernanceIdentityPenaltyRecord>,
    #[serde(default = "default_next_governance_identity_penalty_id")]
    next_governance_identity_penalty_id: u64,
    #[serde(default)]
    builtin_release_manifest: BuiltinReleaseManifestState,
    #[serde(default)]
    release_security_policy: ReleaseSecurityPolicy,
    #[serde(default)]
    rollback_authority_registry: super::RollbackAuthorityRegistry,
    #[serde(default)]
    consumed_rollback_nonces: BTreeSet<String>,
    rollback_nonce_outcomes: BTreeMap<String, super::RollbackNonceOutcome>,
    #[cfg(test)]
    fail_next_append_after_reducer: bool,
    #[cfg(test)]
    fail_next_append_after_publication_prepare: bool,
    #[cfg(test)]
    fail_append_after_publication_prepare_countdown: Option<usize>,
}

impl World {
    pub fn new() -> Self {
        Self::new_with_state(WorldState::default())
    }

    pub fn new_with_release_security_policy(policy: ReleaseSecurityPolicy) -> Self {
        Self::new().with_release_security_policy(policy)
    }

    pub fn new_production_hardened() -> Self {
        Self::new_with_release_security_policy(ReleaseSecurityPolicy::production_hardened())
    }

    pub fn new_production_hardened_with_cognition_binding(
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Result<Self, WorldError> {
        let mut world = Self::new_production_hardened();
        world.bind_cognition_runtime(
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
        )?;
        Ok(world)
    }

    pub fn new_with_state(mut state: WorldState) -> Self {
        state.migrate_compat_material_ledgers();
        state
            .node_identity_bindings
            .entry(BUILTIN_MODULE_SIGNER_NODE_ID.to_string())
            .or_insert_with(|| BUILTIN_MODULE_SIGNER_PUBLIC_KEY_HEX.to_string());
        if let Some(registry) = state.governance_finality_signer_registry.clone() {
            for (node_id, public_key_hex) in registry.signer_bindings {
                state
                    .node_identity_bindings
                    .entry(node_id)
                    .or_insert(public_key_hex);
            }
        }
        for record in state.governance_validator_admissions.values() {
            if record.node_id.trim().is_empty()
                || record.finality_signer_public_key.trim().is_empty()
            {
                continue;
            }
            state
                .node_identity_bindings
                .entry(record.node_id.clone())
                .or_insert(record.finality_signer_public_key.clone());
            state
                .node_main_token_account_bindings
                .entry(record.node_id.clone())
                .or_insert_with(|| {
                    main_token_account_id_from_node_public_key(
                        record.finality_signer_public_key.as_str(),
                    )
                });
        }
        #[cfg(any(test, feature = "test_tier_required", feature = "test_tier_full"))]
        state
            .node_identity_bindings
            .entry(TEST_MODULE_SIGNER_NODE_ID.to_string())
            .or_insert_with(test_module_signer_public_key_hex);
        if state.governance_finality_signer_registry.is_none() {
            for (node_id, public_key_hex) in
                governance::local_governance_finality_signer_public_keys()
            {
                state
                    .node_identity_bindings
                    .entry(node_id)
                    .or_insert(public_key_hex);
            }
        }
        let mut world = Self {
            source_compiler: super::module_source_compiler::RuntimeSourceCompiler::default(),
            manifest: Manifest::default(),
            cognition: default_cognition_persistence_projection(),
            module_registry: ModuleRegistry::default(),
            module_artifacts: BTreeSet::new(),
            module_artifact_bytes: BTreeMap::new(),
            module_cache: ModuleCache::default(),
            prepared_subscription_cache: BTreeMap::new(),
            module_limits_max: ModuleLimits::unbounded(),
            snapshot_catalog: SnapshotCatalog::default(),
            chain_resource_manifest: ChainResourceManifest::default(),
            latest_chain_resource_delta: None,
            state,
            formal_history_reader: None,
            journal: Journal::new(),
            next_event_id: 1,
            next_event_id_era: 0,
            next_action_id: 1,
            next_action_id_era: 0,
            next_intent_id: 1,
            next_intent_id_era: 0,
            next_proposal_id: 1,
            next_proposal_id_era: 0,
            pending_actions: VecDeque::new(),
            pending_effects: VecDeque::new(),
            inflight_effects: BTreeMap::new(),
            module_tick_schedule: BTreeMap::new(),
            module_tick_routing_metrics: ModuleTickRoutingMetrics::default(),
            capabilities: BTreeMap::new(),
            capability_grants_v2: BTreeMap::new(),
            capability_revocation_state: CapabilityRevocationState::default(),
            capability_nonce_records: BTreeMap::new(),
            capability_authorization_receipts: BTreeMap::new(),
            capability_invocation_contexts: BTreeMap::new(),
            capability_authorization_root: String::new(),
            capability_budget_accounts: BTreeMap::new(),
            capability_effect_receipt_links: BTreeMap::new(),
            policies: PolicySet::default(),
            proposals: BTreeMap::new(),
            scheduler_cursor: None,
            receipt_signer: None,
            persistence_dir: RefCell::new(None),
            runtime_memory_limits: WorldRuntimeMemoryLimits::default(),
            runtime_backpressure_stats: WorldRuntimeBackpressureStats::default(),
            logistics_sla_metrics: LogisticsSlaMetrics::default(),
            threat_heatmap: BTreeMap::new(),
            tick_consensus_records: Vec::new(),
            formal_archived_tick_consensus_count: 0,
            formal_execution_tick: None,
            tick_consensus_authority_source: default_tick_consensus_authority_source(),
            tick_consensus_rejection_audit_events: Vec::new(),
            governance_execution_policy: GovernanceExecutionPolicy::default(),
            governance_finality_epoch_snapshots: BTreeMap::new(),
            governance_emergency_brake_until_tick: None,
            governance_identity_penalties: BTreeMap::new(),
            next_governance_identity_penalty_id: default_next_governance_identity_penalty_id(),
            builtin_release_manifest: BuiltinReleaseManifestState::default(),
            release_security_policy: ReleaseSecurityPolicy::default(),
            rollback_authority_registry: super::RollbackAuthorityRegistry::default(),
            consumed_rollback_nonces: BTreeSet::new(),
            rollback_nonce_outcomes: BTreeMap::new(),
            #[cfg(test)]
            fail_next_append_after_reducer: false,
            #[cfg(test)]
            fail_next_append_after_publication_prepare: false,
            #[cfg(test)]
            fail_append_after_publication_prepare_countdown: None,
        };
        world
            .refresh_capability_authorization_root()
            .expect("empty capability authorization root is serializable");
        world
    }

    pub fn with_release_security_policy(mut self, policy: ReleaseSecurityPolicy) -> Self {
        self.release_security_policy = policy;
        self
    }

    pub fn state(&self) -> &WorldState {
        &self.state
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn module_registry(&self) -> &ModuleRegistry {
        &self.module_registry
    }

    pub fn module_limits_max(&self) -> &ModuleLimits {
        &self.module_limits_max
    }

    pub fn module_cache_len(&self) -> usize {
        self.module_cache.len()
    }

    pub fn snapshot_catalog(&self) -> &SnapshotCatalog {
        &self.snapshot_catalog
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn policies(&self) -> &PolicySet {
        &self.policies
    }

    pub fn capabilities(&self) -> &BTreeMap<String, CapabilityGrant> {
        &self.capabilities
    }

    pub fn capability_grants_v2(&self) -> &BTreeMap<String, JsonValue> {
        &self.capability_grants_v2
    }

    pub fn capability_revocation_state(&self) -> &CapabilityRevocationState {
        &self.capability_revocation_state
    }

    pub fn chain_resource_manifest(&self) -> &ChainResourceManifest {
        &self.chain_resource_manifest
    }

    pub fn capability_nonce_records(
        &self,
    ) -> &BTreeMap<String, CapabilityAuthorizationNonceRecord> {
        &self.capability_nonce_records
    }

    pub fn capability_authorization_receipts(
        &self,
    ) -> &BTreeMap<String, CapabilityAuthorizationAuditReceipt> {
        &self.capability_authorization_receipts
    }

    pub fn capability_invocation_contexts(&self) -> &BTreeMap<String, CapabilityInvocationContext> {
        &self.capability_invocation_contexts
    }

    pub fn capability_authorization_root(&self) -> &str {
        &self.capability_authorization_root
    }

    pub fn capability_budget_accounts(&self) -> &BTreeMap<String, CapabilityBudgetAccount> {
        &self.capability_budget_accounts
    }

    pub fn capability_effect_receipt_links(
        &self,
    ) -> &BTreeMap<String, CapabilityEffectReceiptLink> {
        &self.capability_effect_receipt_links
    }

    pub fn proposals(&self) -> &BTreeMap<ProposalId, Proposal> {
        &self.proposals
    }

    pub fn runtime_backpressure_stats(&self) -> &WorldRuntimeBackpressureStats {
        &self.runtime_backpressure_stats
    }

    pub fn logistics_sla_metrics(&self) -> &LogisticsSlaMetrics {
        &self.logistics_sla_metrics
    }

    pub fn threat_heatmap(&self) -> &BTreeMap<String, i64> {
        &self.threat_heatmap
    }

    pub fn tick_consensus_authority_source(&self) -> &str {
        self.tick_consensus_authority_source.as_str()
    }

    pub fn tick_consensus_rejection_audit_events(&self) -> &[TickConsensusRejectionAuditEvent] {
        self.tick_consensus_rejection_audit_events.as_slice()
    }

    pub fn governance_execution_policy(&self) -> &GovernanceExecutionPolicy {
        &self.governance_execution_policy
    }

    pub fn governance_finality_epoch_snapshots(
        &self,
    ) -> &BTreeMap<u64, GovernanceFinalityEpochSnapshot> {
        &self.governance_finality_epoch_snapshots
    }

    pub fn governance_finality_signer_registry(&self) -> Option<&GovernanceFinalitySignerRegistry> {
        self.state.governance_finality_signer_registry.as_ref()
    }

    pub fn governance_validator_admissions(
        &self,
    ) -> &BTreeMap<String, GovernanceValidatorAdmissionRecord> {
        &self.state.governance_validator_admissions
    }

    pub fn governance_main_token_controller_registry(
        &self,
    ) -> Option<&GovernanceMainTokenControllerRegistry> {
        self.state
            .governance_main_token_controller_registry
            .as_ref()
    }

    pub fn governance_emergency_brake_until_tick(&self) -> Option<WorldTime> {
        self.governance_emergency_brake_until_tick
    }

    pub fn governance_identity_penalties(&self) -> &BTreeMap<u64, GovernanceIdentityPenaltyRecord> {
        &self.governance_identity_penalties
    }

    pub fn governance_identity_penalty_monitor_stats(
        &self,
        high_risk_threshold: i64,
    ) -> GovernanceIdentityPenaltyMonitorStats {
        let mut stats = GovernanceIdentityPenaltyMonitorStats::default();
        for record in self.governance_identity_penalties.values() {
            stats.total_penalties = stats.total_penalties.saturating_add(1);
            if record.status != super::GovernanceIdentityPenaltyStatus::Applied {
                stats.appealed_penalties = stats.appealed_penalties.saturating_add(1);
            }
            if record.status == super::GovernanceIdentityPenaltyStatus::Appealed
                || record.status == super::GovernanceIdentityPenaltyStatus::AppealAccepted
                || record.status == super::GovernanceIdentityPenaltyStatus::AppealRejected
            {
                stats.resolved_appeals = stats.resolved_appeals.saturating_add(u64::from(
                    record.status != super::GovernanceIdentityPenaltyStatus::Appealed,
                ));
                if record.status == super::GovernanceIdentityPenaltyStatus::AppealAccepted {
                    stats.appeal_accepted_penalties =
                        stats.appeal_accepted_penalties.saturating_add(1);
                }
            }
            if record.status == super::GovernanceIdentityPenaltyStatus::Applied
                && record.detection_risk_score >= high_risk_threshold
            {
                stats.high_risk_open_penalties = stats.high_risk_open_penalties.saturating_add(1);
            }
        }
        if stats.resolved_appeals > 0 {
            stats.false_positive_rate_bps = (stats
                .appeal_accepted_penalties
                .saturating_mul(10_000)
                .checked_div(stats.resolved_appeals)
                .unwrap_or_default())
            .min(10_000) as u16;
        }
        stats
    }

    pub fn release_security_policy(&self) -> &ReleaseSecurityPolicy {
        &self.release_security_policy
    }

    pub fn set_release_security_policy(&mut self, policy: ReleaseSecurityPolicy) {
        self.release_security_policy = policy;
    }

    pub fn enable_production_release_policy(&mut self) {
        self.release_security_policy = ReleaseSecurityPolicy::production_hardened();
    }

    #[cfg(test)]
    pub(crate) fn fail_next_append_after_reducer_for_test(&mut self) {
        self.fail_next_append_after_reducer = true;
    }

    #[cfg(test)]
    fn take_fail_next_append_after_reducer_for_test(&mut self) -> bool {
        std::mem::take(&mut self.fail_next_append_after_reducer)
    }

    #[cfg(not(test))]
    fn take_fail_next_append_after_reducer_for_test(&mut self) -> bool {
        false
    }

    #[cfg(test)]
    pub(crate) fn fail_next_append_after_publication_prepare_for_test(&mut self) {
        self.fail_next_append_after_publication_prepare = true;
    }

    #[cfg(test)]
    pub(crate) fn fail_append_after_publication_prepare_on_nth_for_test(&mut self, nth: usize) {
        assert!(nth > 0);
        self.fail_append_after_publication_prepare_countdown = Some(nth);
    }

    #[cfg(test)]
    pub(crate) fn append_event_for_test(
        &mut self,
        body: crate::WorldEventBody,
        caused_by: Option<crate::CausedBy>,
    ) -> Result<WorldEventId, WorldError> {
        self.append_event(body, caused_by)
    }

    #[cfg(test)]
    pub(crate) fn seed_capability_grant_for_test(&mut self, grant_id: String, encoded: JsonValue) {
        self.capability_grants_v2.insert(grant_id, encoded);
    }

    #[cfg(test)]
    pub(crate) fn seed_capability_budget_account_for_test(
        &mut self,
        key: String,
        account: crate::CapabilityBudgetAccount,
    ) {
        self.capability_budget_accounts.insert(key, account);
    }

    #[cfg(test)]
    pub(crate) fn remove_module_release_mapping_for_test(&mut self, request_id: u64) {
        self.state
            .module_release_manifest_mappings
            .remove(&request_id);
    }

    #[cfg(test)]
    fn take_fail_next_append_after_publication_prepare_for_test(&mut self) -> bool {
        if std::mem::take(&mut self.fail_next_append_after_publication_prepare) {
            return true;
        }
        match self.fail_append_after_publication_prepare_countdown {
            Some(1) => {
                self.fail_append_after_publication_prepare_countdown = None;
                true
            }
            Some(remaining) => {
                self.fail_append_after_publication_prepare_countdown = Some(remaining - 1);
                false
            }
            None => false,
        }
    }

    #[cfg(not(test))]
    fn take_fail_next_append_after_publication_prepare_for_test(&mut self) -> bool {
        false
    }

    pub fn with_runtime_memory_limits(mut self, limits: WorldRuntimeMemoryLimits) -> Self {
        self.runtime_memory_limits = limits;
        self.enforce_runtime_memory_limits();
        self
    }

    pub(super) fn preview_next_event_id(
        next_id: WorldEventId,
        era: u64,
    ) -> (WorldEventId, WorldEventId, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }

    pub(super) fn allocate_next_action_id(&mut self) -> ActionId {
        Self::allocate_rolling_sequence_id(&mut self.next_action_id, &mut self.next_action_id_era)
    }

    pub(super) fn allocate_next_intent_seq(&mut self) -> IntentSeq {
        Self::allocate_rolling_sequence_id(&mut self.next_intent_id, &mut self.next_intent_id_era)
    }

    pub(super) fn allocate_next_proposal_id(&mut self) -> ProposalId {
        Self::allocate_rolling_sequence_id(
            &mut self.next_proposal_id,
            &mut self.next_proposal_id_era,
        )
    }

    pub(super) fn preview_next_intent_seq(
        next_id: IntentSeq,
        era: u64,
    ) -> (IntentSeq, IntentSeq, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }

    pub(super) fn preview_next_proposal_id(
        next_id: ProposalId,
        era: u64,
    ) -> (ProposalId, ProposalId, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }

    fn allocate_rolling_sequence_id(next_id: &mut u64, era: &mut u64) -> u64 {
        let (allocated, next_id_after, era_after) =
            Self::preview_rolling_sequence_id(*next_id, *era);
        *next_id = next_id_after;
        *era = era_after;
        allocated
    }

    fn preview_rolling_sequence_id(next_id: u64, era: u64) -> (u64, u64, u64) {
        let allocated = next_id.max(1);
        if allocated == u64::MAX {
            (allocated, 1, era.saturating_add(1))
        } else {
            (allocated, allocated + 1, era)
        }
    }

    pub(super) fn enforce_pending_action_limit(&mut self) {
        let max_len = self.runtime_memory_limits.max_pending_actions.max(1);
        while self.pending_actions.len() > max_len {
            let _ = self.pending_actions.pop_front();
            self.runtime_backpressure_stats.pending_actions_evicted = self
                .runtime_backpressure_stats
                .pending_actions_evicted
                .saturating_add(1);
        }
    }

    pub(super) fn push_pending_effect_bounded(
        &mut self,
        intent: EffectIntent,
    ) -> Result<(), WorldError> {
        let max_len = self.runtime_memory_limits.max_pending_effects.max(1);
        if self.pending_effects.len() >= max_len
            && !self.pending_effects.iter().any(|pending| {
                !self
                    .capability_effect_receipt_links
                    .contains_key(&pending.identity_key())
            })
        {
            // An authorization-linked intent represents a durable budget debit;
            // refuse the new queue event rather than stranding its recovery receipt.
            return Err(WorldError::CapabilityAuthorizationDenied {
                reason: "effect queue is full of authorization-linked intents".to_string(),
            });
        }
        self.pending_effects.push_back(intent);
        self.enforce_pending_effect_limit();
        Ok(())
    }

    pub(super) fn prepare_threat_heatmap(&self) -> BTreeMap<String, i64> {
        let mut next = BTreeMap::new();
        for war in self.state.wars.values() {
            if !war.active {
                continue;
            }
            let war_risk = (war.intensity as i64).saturating_mul(10).max(10);
            *next
                .entry(format!("alliance:{}", war.aggressor_alliance_id))
                .or_insert(0) += war_risk;
            *next
                .entry(format!("alliance:{}", war.defender_alliance_id))
                .or_insert(0) += war_risk;
            *next.entry("global:war".to_string()).or_insert(0) += war_risk;
        }
        for crisis in self.state.crises.values() {
            if !matches!(crisis.status, CrisisStatus::Active) {
                continue;
            }
            let crisis_risk = (crisis.severity as i64).saturating_mul(12).max(12);
            *next.entry(format!("crisis:{}", crisis.kind)).or_insert(0) += crisis_risk;
            *next.entry("global:crisis".to_string()).or_insert(0) += crisis_risk;
        }
        next
    }

    pub(super) fn refresh_threat_heatmap(&mut self) {
        self.threat_heatmap = self.prepare_threat_heatmap();
    }

    pub(super) fn enforce_pending_effect_limit(&mut self) {
        let max_len = self.runtime_memory_limits.max_pending_effects.max(1);
        while self.pending_effects.len() > max_len {
            let Some(eviction_index) = self.pending_effects.iter().position(|intent| {
                !self
                    .capability_effect_receipt_links
                    .contains_key(&intent.identity_key())
            }) else {
                // Keep linked effects durable below a lowered limit; a real
                // provider receipt can still close them after restart.
                break;
            };
            let _ = self.pending_effects.remove(eviction_index);
            self.runtime_backpressure_stats.pending_effects_evicted = self
                .runtime_backpressure_stats
                .pending_effects_evicted
                .saturating_add(1);
        }
    }

    pub(super) fn inflight_effect_capacity_reached(&self) -> bool {
        self.inflight_effects.len() >= self.runtime_memory_limits.max_inflight_effects.max(1)
    }

    pub(super) fn record_inflight_effect_dispatch_blocked(&mut self) {
        self.runtime_backpressure_stats
            .inflight_effect_dispatch_blocked = self
            .runtime_backpressure_stats
            .inflight_effect_dispatch_blocked
            .saturating_add(1);
    }

    pub(super) fn enforce_inflight_effect_limit(&mut self) {
        let max_len = self.runtime_memory_limits.max_inflight_effects.max(1);
        while self.inflight_effects.len() > max_len {
            if let Some(first_key) = self
                .inflight_effects
                .keys()
                .find(|intent_id| {
                    !self
                        .capability_effect_receipt_links
                        .contains_key(*intent_id)
                })
                .cloned()
            {
                self.inflight_effects.remove(first_key.as_str());
                self.runtime_backpressure_stats.inflight_effects_evicted = self
                    .runtime_backpressure_stats
                    .inflight_effects_evicted
                    .saturating_add(1);
            } else {
                // Never drop an in-flight intent whose authorization budget
                // is already debited and whose receipt link is still open.
                break;
            }
        }
    }

    pub(super) fn enforce_journal_event_limit(&mut self) {
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = self.journal.events.len().saturating_sub(max_len);
        if overflow > 0 {
            self.journal.events.drain(0..overflow);
            self.runtime_backpressure_stats.journal_events_evicted = self
                .runtime_backpressure_stats
                .journal_events_evicted
                .saturating_add(overflow as u64);
        }
    }

    pub(super) fn enforce_runtime_memory_limits(&mut self) {
        self.enforce_pending_action_limit();
        self.enforce_pending_effect_limit();
        self.enforce_inflight_effect_limit();
        self.enforce_journal_event_limit();
    }
}
mod effect_birth;

pub use observer_identity::ObserverAuthorityIdentity;

mod owner_control_admission;
pub use owner_control_admission::OwnerControlHostAuthority;

mod provider_causal_commit;
pub use provider_causal_commit::{ProviderDecisionEvidenceV1, ProviderCognitionCommitError};
