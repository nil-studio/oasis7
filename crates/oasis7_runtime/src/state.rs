//! World state management.

use crate::agent_state::AgentState;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use oasis7_wasm_abi::{
    FactoryModuleSpec, FactoryProfileV1, MaterialProfileV1, MaterialStack, ModuleManifest,
    ProductProfileV1, ProductValidationDecision, RecipeProfileV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::agent_cell::{AgentCell, AgentIntentV2};
use super::error::WorldError;
use super::events::ModuleProfileChanges;
use super::events::{DomainEvent, IndustryStage, MaterialTransitPriority};
use super::formal_retained_history::FormalRetainedHistoryRootV1;
use super::gameplay_state::{
    AgentClaimState, AllianceState, CrisisState, EconomicContractState,
    GOVERNANCE_IDENTITY_DEFAULT_MAX_VOTE_WEIGHT, GameplayPolicyState,
    GovernanceIdentityProfileState, GovernanceIdentityStatus, GovernanceProposalState,
    GovernanceVoteState, GovernanceVoteWeightSnapshotState, MetaProgressState, StarterOcClaimState,
    WarState,
};
use super::governance::{
    GovernanceFinalitySignerRegistry, GovernanceMainTokenControllerRegistry,
    GovernanceValidatorAdmissionRecord,
};
use super::main_token::{
    MainTokenAccountBalance, MainTokenConfig, MainTokenEpochIssuanceRecord,
    MainTokenGenesisAllocationBucketState, MainTokenNodePointsBridgeEpochRecord,
    MainTokenScheduledPolicyUpdate, MainTokenSupplyState, MainTokenTreasuryDistributionRecord,
    RestrictedStarterClaimGrantState, RestrictedStarterClaimLiveopsPoolTopUpRecord,
};
use super::node_points::EpochSettlementReport;
use super::reward_asset::{
    NodeAssetBalance, NodeRewardMintRecord, ProtocolPowerReserve, REWARD_MINT_SIGNATURE_V1_PREFIX,
    REWARD_MINT_SIGNATURE_V2_PREFIX, RewardAssetConfig, RewardSignatureGovernancePolicy,
    SystemOrderPoolBudget, reward_mint_signature_v1, verify_reward_mint_signature_v2,
};
use super::types::{ActionId, MaterialLedgerId, ProposalId, WorldEventId, WorldTime};
use super::util::deserialize_btreemap_u64_keys;

mod apply_domain_event_core;
mod apply_domain_event_gameplay;
mod apply_domain_event_governance_meta;
mod apply_domain_event_industry;
mod apply_domain_event_industry_helpers;
#[cfg(test)]
mod apply_domain_event_industry_history_tests;
mod apply_domain_event_intent;
pub(crate) mod apply_domain_event_main_token;
mod body_projection;
mod command_projection;
pub(crate) mod core_policy_transition;
mod factory_authority;
mod formal_retention_projection;
mod governance_identity_projection;
pub(crate) mod industry_history_transition;
mod industry_state;
pub(crate) mod industry_transition;
mod logistics_path_authority;
mod module_admission_freeze;
pub(crate) mod module_instance_transition;
pub use module_admission_freeze::ModuleAdmissionFreeze;
pub(crate) mod module_marketplace_transition;
pub(crate) mod module_release_transition;
mod module_visual;
mod projection;
mod starter_industrial;
#[path = "state_defaults.rs"]
mod state_defaults;
pub(crate) mod support;

pub use self::starter_industrial::{
    IndustryProgressState, STARTER_ASSEMBLER_FACTORY_ID, STARTER_INDUSTRIAL_COMPLETION_BOUNDARY,
    STARTER_INDUSTRIAL_PROFILE_ID, STARTER_INDUSTRIAL_PROFILE_REVISION, STARTER_SMELTER_FACTORY_ID,
    STARTER_SMELTER_RECIPE_ID, StarterIndustrialFeasibilityResult,
    StarterIndustrialFeasibilityStatus, StarterIndustrialMilestoneV1,
    StarterIndustrialSettlementSummaryV1,
};
use self::support::*;
pub(crate) use command_projection::{
    CommandAgentMapProjection, CommandModuleStateMapProjection, CommandResourceMapProjection,
    CommandStateOverlay,
};
pub(super) use logistics_path_authority::LogisticsPathAuthorityV1;
pub use projection::{BodyOverlay, WorldStateProjection};

fn default_world_material_ledger() -> MaterialLedgerId {
    state_defaults::default_world_material_ledger()
}
fn default_logistics_route_available() -> bool {
    state_defaults::default_logistics_route_available()
}
fn default_logistics_capacity_units() -> i64 {
    state_defaults::default_logistics_capacity_units()
}
fn default_material_ledgers() -> BTreeMap<MaterialLedgerId, BTreeMap<String, i64>> {
    state_defaults::default_material_ledgers()
}
fn default_material_transit_priority() -> MaterialTransitPriority {
    state_defaults::default_material_transit_priority()
}
fn default_module_market_order_id() -> u64 {
    state_defaults::default_module_market_order_id()
}
fn default_module_market_sale_id() -> u64 {
    state_defaults::default_module_market_sale_id()
}
fn default_next_module_instance_id() -> u64 {
    state_defaults::default_next_module_instance_id()
}

fn default_next_module_release_request_id() -> u64 {
    state_defaults::default_next_module_release_request_id()
}

fn default_factory_durability_ppm() -> i64 {
    state_defaults::default_factory_durability_ppm()
}

fn default_factory_production_state() -> FactoryProductionState {
    state_defaults::default_factory_production_state()
}

fn default_module_release_required_roles() -> Vec<String> {
    state_defaults::default_module_release_required_roles()
}
pub use self::industry_state::{
    AgentLocationAuthorityV1, FACTORY_BUILD_STARTED_MODERN_VERSION, FactoryBuildPowerObligationV1,
    FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1, FactoryProductionSnapshot,
    FactoryProductionState, FactoryProductionStatus, FactoryRecycleReceiptV1,
    FactorySiteAuthorityV1, LocationAnchorV1, ProductValidationAttemptV1,
    ProductValidationReceiptV1, RecipeCompletionReceiptV1,
};

/// Persisted factory instance state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactoryState {
    pub factory_id: String,
    pub site_id: String,
    pub builder_agent_id: String,
    pub spec: FactoryModuleSpec,
    #[serde(default = "default_world_material_ledger")]
    pub input_ledger: MaterialLedgerId,
    #[serde(default = "default_world_material_ledger")]
    pub output_ledger: MaterialLedgerId,
    #[serde(default = "default_factory_durability_ppm")]
    pub durability_ppm: i64,
    #[serde(default = "default_factory_production_state")]
    pub production: FactoryProductionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_authority_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_location_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_anchor_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction_power_profile_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction_power_profile_revision: Option<u64>,
    pub built_at: WorldTime,
}

/// In-flight factory construction tracked by job id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactoryBuildJobState {
    pub job_id: ActionId,
    pub builder_agent_id: String,
    pub site_id: String,
    pub spec: FactoryModuleSpec,
    #[serde(default = "default_world_material_ledger")]
    pub consume_ledger: MaterialLedgerId,
    pub ready_at: WorldTime,
    /// Build event contract discriminator; zero is legacy replay only.
    #[serde(default)]
    pub contract_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_authority_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_location_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_anchor_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction_power_obligation: Option<FactoryBuildPowerObligationV1>,
}

/// In-flight recipe execution tracked by job id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeJobState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_recipe_origin: Option<crate::CommittedRecipeOrigin>,
    pub job_id: ActionId,
    pub requester_agent_id: String,
    pub factory_id: String,
    pub recipe_id: String,
    pub accepted_batches: u32,
    pub consume: Vec<MaterialStack>,
    pub produce: Vec<MaterialStack>,
    pub byproducts: Vec<MaterialStack>,
    pub power_required: i64,
    /// Electricity payer committed at recipe start.  Optional for snapshot
    /// compatibility with pre-owner-power jobs; such legacy jobs are not
    /// newly admitted but can still be inspected/replayed safely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_owner_agent_id: Option<String>,
    pub duration_ticks: u32,
    #[serde(default = "default_world_material_ledger")]
    pub consume_ledger: MaterialLedgerId,
    #[serde(default = "default_world_material_ledger")]
    pub output_ledger: MaterialLedgerId,
    #[serde(default)]
    pub bottleneck_tags: Vec<String>,
    #[serde(default)]
    pub logistics_route_ids: Vec<String>,
    #[serde(default)]
    pub logistics_path_ids: Vec<String>,
    pub ready_at: WorldTime,
}

/// In-flight material transit tracked by job id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTransitJobState {
    pub job_id: ActionId,
    pub requester_agent_id: String,
    pub from_ledger: MaterialLedgerId,
    pub to_ledger: MaterialLedgerId,
    pub kind: String,
    pub amount: i64,
    pub distance_km: i64,
    pub loss_bps: i64,
    #[serde(default)]
    pub priority: MaterialTransitPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_id: Option<String>,
    #[serde(default)]
    pub route_ids: Vec<String>,
    #[serde(default)]
    pub tariff_electricity_total: i64,
    #[serde(default)]
    pub reroute_count: u32,
    pub ready_at: WorldTime,
}

/// Immutable material-transit route authority persisted by the world runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsRouteV1 {
    pub route_id: String,
    pub from_ledger: MaterialLedgerId,
    pub to_ledger: MaterialLedgerId,
    pub kind: String,
    pub distance_km: i64,
    pub priority: MaterialTransitPriority,
    #[serde(default)]
    pub owner_agent_id: String,
    #[serde(default = "default_logistics_route_available")]
    pub available: bool,
    #[serde(default = "default_logistics_capacity_units")]
    pub capacity_units: i64,
    #[serde(default)]
    pub reserved_capacity_units: i64,
    #[serde(default)]
    pub tariff_electricity_per_unit: i64,
}

/// Persisted one-time settlement receipt for a material transit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LogisticsSettlementReceiptV1 {
    pub job_id: ActionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_id: Option<String>,
    #[serde(default)]
    pub route_ids: Vec<String>,
    #[serde(default)]
    pub tariff_electricity_total: i64,
    #[serde(default)]
    pub owner_payouts: BTreeMap<String, i64>,
    #[serde(default)]
    pub governance_tax_electricity: i64,
}

/// Durable disposition for a product-validation failure after a recipe has
/// already sunk its committed inputs and electricity.
///
/// The action id is the stable identity. Keeping the exact consumed/lost
/// payload makes replay of the blocker idempotent while allowing a conflicting
/// same-id event to fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactoryProductionFailureDispositionV1 {
    pub action_id: ActionId,
    pub requester_agent_id: String,
    pub factory_id: String,
    pub recipe_id: String,
    pub blocker_kind: String,
    pub blocker_detail: String,
    pub disposition_kind: String,
    pub consumed_inputs: Vec<MaterialStack>,
    pub lost_inputs: Vec<MaterialStack>,
    pub consumed_power: i64,
    pub lost_power: i64,
    pub next_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_recheck: Option<WorldTime>,
}

/// Comparable one-time receipt for an immediate material transfer.
///
/// The transfer action id is the stable identity. Keeping the settled payload
/// alongside it lets replay treat an exact duplicate as a no-op while rejecting
/// a same-id event whose material effect was tampered with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialTransferReceiptV1 {
    pub transfer_id: ActionId,
    pub requester_agent_id: String,
    pub from_ledger: MaterialLedgerId,
    pub to_ledger: MaterialLedgerId,
    pub kind: String,
    pub amount: i64,
    pub distance_km: i64,
    #[serde(default = "default_material_transit_priority")]
    pub priority: MaterialTransitPriority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
}

/// Active market listing for one module artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleArtifactListingState {
    #[serde(default)]
    pub order_id: u64,
    pub seller_agent_id: String,
    pub price_kind: ResourceKind,
    pub price_amount: i64,
    pub listed_at: WorldTime,
}

/// Active bid order for one module artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleArtifactBidState {
    pub order_id: u64,
    pub bidder_agent_id: String,
    pub price_kind: ResourceKind,
    pub price_amount: i64,
    pub bid_at: WorldTime,
}

/// Installed module instance tracked independently from global module_id activation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInstanceState {
    pub instance_id: String,
    pub module_id: String,
    pub module_version: String,
    #[serde(default)]
    pub wasm_hash: String,
    pub owner_agent_id: String,
    #[serde(default)]
    pub install_target: ModuleInstallTarget,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub installed_at: WorldTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModuleReleaseRequestStatus {
    #[default]
    Requested,
    Shadowed,
    PartiallyApproved,
    Approved,
    Rejected,
    Applied,
}

/// One rebuild attestation submitted for a module release request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleReleaseAttestationState {
    pub request_id: u64,
    pub signer_node_id: String,
    pub platform: String,
    pub submitted_by_agent_id: String,
    pub build_manifest_hash: String,
    pub source_hash: String,
    pub wasm_hash: String,
    pub proof_cid: String,
    #[serde(default)]
    pub builder_image_digest: String,
    #[serde(default)]
    pub container_platform: String,
    #[serde(default)]
    pub canonicalizer_version: String,
    #[serde(default)]
    pub submitted_at: WorldTime,
}

/// Module release request tracked through governance closure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleReleaseRequestState {
    pub request_id: u64,
    pub requester_agent_id: String,
    pub manifest: ModuleManifest,
    pub activate: bool,
    #[serde(default)]
    pub install_target: ModuleInstallTarget,
    #[serde(default)]
    pub profile_changes: ModuleProfileChanges,
    #[serde(default = "default_module_release_required_roles")]
    pub required_roles: Vec<String>,
    #[serde(default)]
    pub role_approvals: BTreeMap<String, String>,
    #[serde(default)]
    pub attestations: BTreeMap<String, ModuleReleaseAttestationState>,
    #[serde(default)]
    pub status: ModuleReleaseRequestStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_proposal_id: Option<ProposalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejected_reason: Option<String>,
    #[serde(default)]
    pub created_at: WorldTime,
    #[serde(default)]
    pub updated_at: WorldTime,
}

/// Durable truth for the latest accepted product validation. Player-facing
/// presentation joins this with the current governed profile and industry stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastProductValidationState {
    pub product_id: String,
    pub tradable: bool,
}

/// Durable high-water mark for product-validation event routing. Validation
/// events are generated in journal order, so a scalar cursor keeps recovery
/// metadata bounded independently from the number of events.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationDeliveryCursor {
    #[serde(default)]
    pub routed_through_event_id: WorldEventId,
    /// Era-aware bounded high-water mark; old snapshots default this era to 0.
    #[serde(default)]
    pub event_id_era: u64,
}

impl ProductValidationDeliveryCursor {
    pub fn has_routed(&self, event_id_era: u64, event_id: WorldEventId) -> bool {
        (self.event_id_era, self.routed_through_event_id) >= (event_id_era, event_id)
    }
    pub fn advance_to(&mut self, event_id_era: u64, event_id: WorldEventId) {
        if (event_id_era, event_id) > (self.event_id_era, self.routed_through_event_id) {
            self.event_id_era = event_id_era;
            self.routed_through_event_id = event_id;
        }
    }
}

/// Persistent mapping from module release request lifecycle to release manifest lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleReleaseManifestMappingState {
    pub request_id: u64,
    pub release_id: String,
    pub module_id: String,
    #[serde(default)]
    pub attestation_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_wasm_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_build_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_builder_image_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_container_platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_canonicalizer_version: Option<String>,
    #[serde(default)]
    pub attestation_platforms: Vec<String>,
    #[serde(default)]
    pub attestation_proof_cids: Vec<String>,
    #[serde(default)]
    pub receipt_evidence_conflict: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_proposal_id: Option<ProposalId>,
    #[serde(default)]
    pub status: ModuleReleaseRequestStatus,
    #[serde(default)]
    pub created_at: WorldTime,
    #[serde(default)]
    pub updated_at: WorldTime,
}

/// The mutable state of the world.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorldState {
    pub time: WorldTime,
    /// Independent authenticated archive index. It is committed by the
    /// enclosing Snapshot's state bytes and is trusted only when those exact
    /// bytes are verified under a qualified formal anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_history_root: Option<FormalRetainedHistoryRootV1>,
    pub agents: BTreeMap<String, AgentCell>,
    /// Latest durable intent disposition used for idempotency after replacement.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_intent_ledger: BTreeMap<String, AgentIntentV2>,
    #[serde(default)]
    pub resources: BTreeMap<ResourceKind, i64>,
    #[serde(default)]
    pub materials: BTreeMap<String, i64>,
    #[serde(default = "default_material_ledgers")]
    pub material_ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    #[serde(default)]
    pub material_profiles: BTreeMap<String, MaterialProfileV1>,
    #[serde(default)]
    pub logistics_routes: BTreeMap<String, LogisticsRouteV1>,
    #[serde(default)]
    pub completed_logistics_route_ids: BTreeSet<String>,
    #[serde(default)]
    pub completed_logistics_paths: BTreeMap<String, LogisticsPathAuthorityV1>,
    #[serde(default)]
    pub settled_logistics_transit_ids: BTreeSet<ActionId>,
    #[serde(default)]
    pub logistics_settlement_receipts: BTreeMap<ActionId, LogisticsSettlementReceiptV1>,
    /// Product-validation failure dispositions are keyed by the recipe
    /// action id so a post-start sink is recorded exactly once across replay.
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub factory_production_failure_dispositions:
        BTreeMap<ActionId, FactoryProductionFailureDispositionV1>,
    /// Direct material transfer action ids are single-use. The comparable
    /// receipt distinguishes an exact replay from a tampered same-id event.
    #[serde(default)]
    pub direct_material_transfer_receipts: BTreeMap<ActionId, MaterialTransferReceiptV1>,
    /// Product-module decisions are scoped to a production job/output and
    /// survive snapshots so retries do not invoke the module twice.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_btreemap_u64_keys"
    )]
    pub product_validation_receipts: BTreeMap<ActionId, Vec<ProductValidationReceiptV1>>,
    #[serde(default)]
    pub product_profiles: BTreeMap<String, ProductProfileV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_product_validation: Option<LastProductValidationState>,
    #[serde(default)]
    pub recipe_profiles: BTreeMap<String, RecipeProfileV1>,
    #[serde(default)]
    pub factory_profiles: BTreeMap<String, FactoryProfileV1>,
    /// Canonical agent-to-location assignments. Old snapshots decode this as
    /// an empty registry and cannot thereby grant new site admission.
    #[serde(default)]
    pub agent_location_authorities: BTreeMap<String, AgentLocationAuthorityV1>,
    /// Runtime-owned exact location identities. Old snapshots decode this as
    /// an empty registry and cannot thereby grant new authority or admission.
    #[serde(default)]
    pub location_anchors: BTreeMap<String, LocationAnchorV1>,
    /// Canonical factory-site admission records.
    #[serde(default)]
    pub factory_site_authorities: BTreeMap<String, FactorySiteAuthorityV1>,
    /// M4-governed construction electricity profiles keyed by exact factory id.
    #[serde(default)]
    pub factory_construction_power_profiles: BTreeMap<String, FactoryConstructionPowerProfileV1>,
    #[serde(default)]
    pub factories: BTreeMap<String, FactoryState>,
    /// Factory identities are single-use.  Recycling leaves a durable
    /// tombstone so stale build/recycle events cannot resurrect or mutate the
    /// same identity after replay or recovery.
    #[serde(default)]
    pub retired_factory_ids: BTreeSet<String>,
    /// Factory build action identities are single-use.  Once a build has
    /// settled, replaying the same completion event must not recreate or
    /// overwrite the factory; the receipt also lets the reducer distinguish
    /// a settled duplicate from an unknown/reordered completion.
    #[serde(default)]
    pub settled_factory_build_ids: BTreeSet<ActionId>,
    /// Construction obligations remain auditable after FactoryBuilt settles
    /// the pending job; old snapshots decode this as an empty map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub factory_construction_receipts: BTreeMap<String, FactoryBuildPowerObligationV1>,
    /// Pre-call intents prevent a crash between module side effects and the
    /// validation receipt from causing a second validator invocation.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_btreemap_u64_keys"
    )]
    pub product_validation_attempts: BTreeMap<ActionId, Vec<ProductValidationAttemptV1>>,
    /// Runtime-owned validation delivery progress is separate from reducer
    /// module state so a module cannot shadow or overwrite the cursor.
    #[serde(default)]
    pub product_validation_delivery_cursor: ProductValidationDeliveryCursor,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub pending_factory_builds: BTreeMap<ActionId, FactoryBuildJobState>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub pending_recipe_jobs: BTreeMap<ActionId, RecipeJobState>,
    /// Recipe action identities are single-use.  Once a recipe reaches a
    /// terminal settlement (including a product-validation block), replaying
    /// the same completion/start identity must not sink or credit it again.
    #[serde(default)]
    pub settled_recipe_job_ids: BTreeSet<ActionId>,
    /// Monotonic settlement position for new industry terminal outcomes.
    #[serde(default = "state_defaults::default_next_industry_settlement_order")]
    pub next_industry_settlement_order: u64,
    /// Durable insertion order for bounded industry history compaction.
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub industry_settlement_orders: BTreeMap<ActionId, u64>,
    /// Full completion payloads for new recipe settlements.  The ID set above
    /// remains a compatibility projection for pre-receipt snapshots.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_btreemap_u64_keys"
    )]
    pub recipe_completion_receipts: BTreeMap<ActionId, RecipeCompletionReceiptV1>,
    /// Full recycle payloads for new retired-factory tombstones.  The legacy
    /// ID set remains authoritative only when no receipt exists.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub factory_recycle_receipts: BTreeMap<String, FactoryRecycleReceiptV1>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub pending_material_transits: BTreeMap<ActionId, MaterialTransitJobState>,
    #[serde(default)]
    pub industry_progress: IndustryProgressState,
    #[serde(default)]
    pub alliances: BTreeMap<String, AllianceState>,
    #[serde(default)]
    pub gameplay_policy: GameplayPolicyState,
    #[serde(default)]
    pub data_access_permissions: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub economic_contracts: BTreeMap<String, EconomicContractState>,
    #[serde(default)]
    pub agent_claims: BTreeMap<String, AgentClaimState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub starter_oc_claims: BTreeMap<String, StarterOcClaimState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub authenticated_collect_data_last_nonces: BTreeMap<String, BTreeMap<String, u64>>,
    #[serde(default)]
    pub agent_claim_last_processed_epoch: u64,
    #[serde(default)]
    pub contract_pair_last_success_settled_at: BTreeMap<String, WorldTime>,
    #[serde(default)]
    pub reputation_reward_window_started_at: BTreeMap<String, WorldTime>,
    #[serde(default)]
    pub reputation_reward_window_accumulated: BTreeMap<String, i64>,
    #[serde(default)]
    pub reputation_scores: BTreeMap<String, i64>,
    #[serde(default)]
    pub wars: BTreeMap<String, WarState>,
    #[serde(default)]
    pub governance_votes: BTreeMap<String, GovernanceVoteState>,
    #[serde(default)]
    pub governance_proposals: BTreeMap<String, GovernanceProposalState>,
    #[serde(default)]
    pub governance_identity_profiles: BTreeMap<String, GovernanceIdentityProfileState>,
    #[serde(default)]
    pub crises: BTreeMap<String, CrisisState>,
    #[serde(default)]
    pub meta_progress: BTreeMap<String, MetaProgressState>,
    #[serde(default)]
    pub module_states: BTreeMap<String, Vec<u8>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub module_visual_entities: BTreeMap<String, ModuleVisualEntity>,
    #[serde(default)]
    pub module_artifact_owners: BTreeMap<String, String>,
    #[serde(default)]
    pub module_artifact_listings: BTreeMap<String, ModuleArtifactListingState>,
    #[serde(default)]
    pub module_artifact_bids: BTreeMap<String, Vec<ModuleArtifactBidState>>,
    #[serde(default)]
    pub module_instances: BTreeMap<String, ModuleInstanceState>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub module_admission_freezes: BTreeMap<String, ModuleAdmissionFreeze>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub module_release_requests: BTreeMap<u64, ModuleReleaseRequestState>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub module_release_manifest_mappings: BTreeMap<u64, ModuleReleaseManifestMappingState>,
    #[serde(default = "default_next_module_release_request_id")]
    pub next_module_release_request_id: u64,
    #[serde(default)]
    pub module_release_role_bindings: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    pub installed_module_targets: BTreeMap<String, ModuleInstallTarget>,
    #[serde(default = "default_next_module_instance_id")]
    pub next_module_instance_id: u64,
    #[serde(default = "default_module_market_order_id")]
    pub next_module_market_order_id: u64,
    #[serde(default = "default_module_market_sale_id")]
    pub next_module_market_sale_id: u64,
    #[serde(default)]
    pub main_token_config: MainTokenConfig,
    #[serde(default)]
    pub main_token_supply: MainTokenSupplyState,
    #[serde(default)]
    pub main_token_balances: BTreeMap<String, MainTokenAccountBalance>,
    #[serde(default)]
    pub restricted_starter_claim_grants: BTreeMap<String, RestrictedStarterClaimGrantState>,
    #[serde(default)]
    pub main_token_genesis_buckets: BTreeMap<String, MainTokenGenesisAllocationBucketState>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub main_token_epoch_issuance_records: BTreeMap<u64, MainTokenEpochIssuanceRecord>,
    #[serde(default)]
    pub main_token_treasury_balances: BTreeMap<String, u64>,
    #[serde(default)]
    pub main_token_claim_nonces: BTreeMap<String, u64>,
    #[serde(default)]
    pub main_token_transfer_nonces: BTreeMap<String, u64>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub main_token_scheduled_policy_updates: BTreeMap<u64, MainTokenScheduledPolicyUpdate>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub main_token_node_points_bridge_records: BTreeMap<u64, MainTokenNodePointsBridgeEpochRecord>,
    #[serde(default)]
    pub main_token_treasury_distribution_records:
        BTreeMap<String, MainTokenTreasuryDistributionRecord>,
    #[serde(default)]
    pub restricted_starter_claim_liveops_pool_top_up_records:
        BTreeMap<String, RestrictedStarterClaimLiveopsPoolTopUpRecord>,
    #[serde(default)]
    pub reward_asset_config: RewardAssetConfig,
    #[serde(default)]
    pub node_asset_balances: BTreeMap<String, NodeAssetBalance>,
    #[serde(default)]
    pub protocol_power_reserve: ProtocolPowerReserve,
    #[serde(default)]
    pub reward_mint_records: Vec<NodeRewardMintRecord>,
    #[serde(default)]
    pub node_redeem_nonces: BTreeMap<String, u64>,
    #[serde(default, deserialize_with = "deserialize_btreemap_u64_keys")]
    pub system_order_pool_budgets: BTreeMap<u64, SystemOrderPoolBudget>,
    #[serde(default)]
    pub node_identity_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub node_main_token_account_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub governance_finality_signer_registry: Option<GovernanceFinalitySignerRegistry>,
    #[serde(default)]
    pub governance_validator_admissions: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    #[serde(default)]
    pub governance_main_token_controller_registry: Option<GovernanceMainTokenControllerRegistry>,
    #[serde(default)]
    pub reward_signature_governance_policy: RewardSignatureGovernancePolicy,
}

impl WorldState {
    pub fn migrate_compat_material_ledgers(&mut self) {
        self.material_ledgers
            .entry(MaterialLedgerId::world())
            .or_default();

        let world_ledger = self
            .material_ledgers
            .get(&MaterialLedgerId::world())
            .cloned()
            .unwrap_or_default();
        if world_ledger.is_empty() && !self.materials.is_empty() {
            self.material_ledgers
                .insert(MaterialLedgerId::world(), self.materials.clone());
        }

        sync_compat_world_materials(&self.material_ledgers, &mut self.materials);
    }

    pub fn has_data_access_permission(
        &self,
        owner_agent_id: &str,
        accessor_agent_id: &str,
    ) -> bool {
        if owner_agent_id == accessor_agent_id {
            return true;
        }
        self.data_access_permissions
            .get(owner_agent_id)
            .is_some_and(|grantees| grantees.contains(accessor_agent_id))
    }

    pub fn economic_contract_pair_key(left_agent_id: &str, right_agent_id: &str) -> String {
        if left_agent_id <= right_agent_id {
            format!("{left_agent_id}|{right_agent_id}")
        } else {
            format!("{right_agent_id}|{left_agent_id}")
        }
    }

    pub fn economic_contract_pair_cooldown_ready_at(
        &self,
        left_agent_id: &str,
        right_agent_id: &str,
        cooldown_ticks: u64,
    ) -> Option<WorldTime> {
        let pair_key = Self::economic_contract_pair_key(left_agent_id, right_agent_id);
        self.contract_pair_last_success_settled_at
            .get(&pair_key)
            .map(|last| last.saturating_add(cooldown_ticks))
    }

    pub fn available_reputation_reward_budget(
        &self,
        agent_id: &str,
        now: WorldTime,
        window_ticks: u64,
        window_cap: i64,
    ) -> i64 {
        if window_cap <= 0 {
            return 0;
        }
        let in_window = self
            .reputation_reward_window_started_at
            .get(agent_id)
            .is_some_and(|window_started_at| now.saturating_sub(*window_started_at) < window_ticks);
        if !in_window {
            return window_cap;
        }
        let accumulated = self
            .reputation_reward_window_accumulated
            .get(agent_id)
            .copied()
            .unwrap_or(0)
            .max(0);
        window_cap.saturating_sub(accumulated).max(0)
    }

    pub fn record_successful_contract_pair_settlement(
        &mut self,
        left_agent_id: &str,
        right_agent_id: &str,
        now: WorldTime,
    ) {
        let pair_key = Self::economic_contract_pair_key(left_agent_id, right_agent_id);
        self.contract_pair_last_success_settled_at
            .insert(pair_key, now);
    }

    pub fn record_reputation_reward_window_gain(
        &mut self,
        agent_id: &str,
        reward_delta: i64,
        now: WorldTime,
        window_ticks: u64,
    ) {
        if reward_delta <= 0 {
            return;
        }
        let in_window = self
            .reputation_reward_window_started_at
            .get(agent_id)
            .is_some_and(|window_started_at| now.saturating_sub(*window_started_at) < window_ticks);
        if in_window {
            let current = self
                .reputation_reward_window_accumulated
                .get(agent_id)
                .copied()
                .unwrap_or(0);
            self.reputation_reward_window_accumulated
                .insert(agent_id.to_string(), current.saturating_add(reward_delta));
        } else {
            self.reputation_reward_window_started_at
                .insert(agent_id.to_string(), now);
            self.reputation_reward_window_accumulated
                .insert(agent_id.to_string(), reward_delta);
        }
    }

    pub fn set_reputation_score(
        &mut self,
        agent_id: &str,
        reputation_score: i64,
    ) -> Result<(), WorldError> {
        if !self.agents.contains_key(agent_id) {
            return Err(WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            });
        }
        self.reputation_scores
            .insert(agent_id.to_string(), reputation_score);
        Ok(())
    }

    pub fn set_governance_identity_profile(
        &mut self,
        profile: GovernanceIdentityProfileState,
    ) -> Result<(), WorldError> {
        if !self.agents.contains_key(profile.agent_id.as_str()) {
            return Err(WorldError::AgentNotFound {
                agent_id: profile.agent_id.clone(),
            });
        }
        self.governance_identity_profiles
            .insert(profile.agent_id.clone(), profile);
        Ok(())
    }

    pub fn governance_identity_snapshot_for_agent(
        &self,
        agent_id: &str,
        snapshot_tick: WorldTime,
    ) -> GovernanceVoteWeightSnapshotState {
        let reputation_score = self.reputation_scores.get(agent_id).copied().unwrap_or(0);
        if let Some(profile) = self.governance_identity_profiles.get(agent_id) {
            let vote_weight_cap = self.governance_vote_weight_cap_from_profile(
                reputation_score,
                profile,
                snapshot_tick,
            );
            return GovernanceVoteWeightSnapshotState {
                agent_id: agent_id.to_string(),
                reputation_score,
                stake_locked: profile.stake_locked,
                status: profile.status,
                vote_weight_cap,
            };
        }
        GovernanceVoteWeightSnapshotState {
            agent_id: agent_id.to_string(),
            reputation_score,
            stake_locked: 0,
            status: GovernanceIdentityStatus::Active,
            vote_weight_cap: GOVERNANCE_IDENTITY_DEFAULT_MAX_VOTE_WEIGHT,
        }
    }

    pub fn governance_effective_vote_weight_for_agent(
        &self,
        proposal: &GovernanceProposalState,
        voter_agent_id: &str,
        requested_weight: u32,
    ) -> Result<u32, WorldError> {
        if proposal.vote_weight_snapshot.is_empty() {
            return Ok(requested_weight);
        }
        let Some(snapshot) = proposal.vote_weight_snapshot.get(voter_agent_id) else {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "voter is not in governance snapshot: proposal={} voter={}",
                    proposal.proposal_key, voter_agent_id
                ),
            });
        };
        if snapshot.status != GovernanceIdentityStatus::Active {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "governance identity is not active: proposal={} voter={} status={:?}",
                    proposal.proposal_key, voter_agent_id, snapshot.status
                ),
            });
        }
        if snapshot.vote_weight_cap == 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "governance vote weight cap is zero: proposal={} voter={}",
                    proposal.proposal_key, voter_agent_id
                ),
            });
        }
        if requested_weight > snapshot.vote_weight_cap {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "vote weight {} exceeds snapshot cap {} for voter {} in proposal {}",
                    requested_weight,
                    snapshot.vote_weight_cap,
                    voter_agent_id,
                    proposal.proposal_key
                ),
            });
        }
        Ok(requested_weight)
    }

    fn governance_vote_weight_cap_from_profile(
        &self,
        reputation_score: i64,
        profile: &GovernanceIdentityProfileState,
        snapshot_tick: WorldTime,
    ) -> u32 {
        if profile.status != GovernanceIdentityStatus::Active {
            return 0;
        }
        if snapshot_tick < profile.warmup_until_tick {
            return 0;
        }
        let stake_component = integer_sqrt_u64(profile.stake_locked);
        let reputation_component = reputation_score.max(0).saturating_div(10) as u64;
        let raw_weight = stake_component
            .saturating_add(reputation_component)
            .max(1)
            .min(u64::from(GOVERNANCE_IDENTITY_DEFAULT_MAX_VOTE_WEIGHT));
        raw_weight as u32
    }

    pub fn apply_domain_event(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        self.apply_domain_event_at(event, now, None, None)
    }

    pub(crate) fn apply_domain_event_at(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
        envelope_event_seq: Option<WorldEventId>,
        committed_receipt_event_id: Option<WorldEventId>,
    ) -> Result<(), WorldError> {
        match event {
            DomainEvent::AgentIntentProposed { .. }
            | DomainEvent::AgentIntentSubmitted { .. }
            | DomainEvent::AgentIntentAccepted { .. }
            | DomainEvent::AgentIntentReplaced { .. }
            | DomainEvent::AgentIntentTransitioned { .. } => self.apply_domain_event_intent(
                event,
                now,
                envelope_event_seq,
                committed_receipt_event_id,
            )?,
            DomainEvent::AgentRegistered { .. }
            | DomainEvent::AgentMoved { .. }
            | DomainEvent::ActionAccepted { .. }
            | DomainEvent::ActionRejected { .. }
            | DomainEvent::Observation { .. }
            | DomainEvent::BodyAttributesUpdated { .. }
            | DomainEvent::BodyAttributesRejected { .. }
            | DomainEvent::BodyInterfaceExpanded { .. }
            | DomainEvent::BodyInterfaceExpandRejected { .. }
            | DomainEvent::ModuleArtifactDeployed { .. }
            | DomainEvent::ModuleInstalled { .. }
            | DomainEvent::ModuleUpgraded { .. }
            | DomainEvent::ModuleReleaseRequested { .. }
            | DomainEvent::ModuleReleaseShadowed { .. }
            | DomainEvent::ModuleReleaseAttested { .. }
            | DomainEvent::ModuleReleaseRoleApproved { .. }
            | DomainEvent::ModuleReleaseRolesBound { .. }
            | DomainEvent::ModuleReleaseRejected { .. }
            | DomainEvent::ModuleReleaseApplied { .. }
            | DomainEvent::ModuleRollbackApplied { .. }
            | DomainEvent::ModuleArtifactListed { .. }
            | DomainEvent::ModuleArtifactDelisted { .. }
            | DomainEvent::ModuleArtifactDestroyed { .. }
            | DomainEvent::ModuleArtifactBidPlaced { .. }
            | DomainEvent::ModuleArtifactBidCancelled { .. }
            | DomainEvent::ModuleArtifactSaleCompleted { .. }
            | DomainEvent::ResourceTransferred { .. }
            | DomainEvent::DataCollected { .. }
            | DomainEvent::DataCollectedAuthenticated { .. }
            | DomainEvent::DataAccessGranted { .. }
            | DomainEvent::DataAccessRevoked { .. }
            | DomainEvent::PowerRedeemed { .. }
            | DomainEvent::PowerRedeemRejected { .. }
            | DomainEvent::NodePointsSettlementApplied { .. }
            | DomainEvent::LogisticsRouteRegistered { .. }
            | DomainEvent::LogisticsRouteAvailabilityChanged { .. }
            | DomainEvent::LogisticsPathRerouted { .. }
            | DomainEvent::MaterialTransferred { .. }
            | DomainEvent::MaterialTransitStarted { .. }
            | DomainEvent::MaterialTransitCompleted { .. }
            | DomainEvent::FactoryBuildStarted { .. }
            | DomainEvent::FactoryBuilt { .. }
            | DomainEvent::FactoryDurabilityChanged { .. }
            | DomainEvent::FactoryMaintained { .. }
            | DomainEvent::FactoryRecycled { .. }
            | DomainEvent::RecipeStarted { .. }
            | DomainEvent::RecipeCompleted { .. }
            | DomainEvent::FactoryProductionBlocked { .. }
            | DomainEvent::FactoryProductionResumed { .. }
            | DomainEvent::FactoryProductionPaused { .. }
            | DomainEvent::MaterialProfileGoverned { .. }
            | DomainEvent::ProductProfileGoverned { .. }
            | DomainEvent::RecipeProfileGoverned { .. }
            | DomainEvent::FactoryProfileGoverned { .. } => {
                self.apply_domain_event_core(event, now)?
            }
            DomainEvent::MainTokenGenesisInitialized { .. }
            | DomainEvent::MainTokenVestingClaimed { .. }
            | DomainEvent::MainTokenTransferred { .. }
            | DomainEvent::MainTokenEpochIssued { .. }
            | DomainEvent::MainTokenFeeSettled { .. }
            | DomainEvent::MainTokenPolicyUpdateScheduled { .. }
            | DomainEvent::MainTokenTreasuryDistributed { .. }
            | DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
            | DomainEvent::RestrictedStarterClaimGrantIssued { .. }
            | DomainEvent::RestrictedStarterClaimGrantExpired { .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { .. } => {
                self.apply_domain_event_main_token(event, now)?
            }
            DomainEvent::GameplayPolicyUpdated { .. } => {
                self.apply_domain_event_core(event, now)?
            }
            DomainEvent::EconomicContractOpened { .. }
            | DomainEvent::EconomicContractAccepted { .. }
            | DomainEvent::EconomicContractSettled { .. }
            | DomainEvent::EconomicContractExpired { .. }
            | DomainEvent::StarterOcClaimed { .. }
            | DomainEvent::AgentClaimed { .. }
            | DomainEvent::AgentClaimReleaseRequested { .. }
            | DomainEvent::AgentClaimUpkeepSettled { .. }
            | DomainEvent::AgentClaimEnteredGrace { .. }
            | DomainEvent::AgentClaimIdleWarning { .. }
            | DomainEvent::AgentClaimReleased { .. }
            | DomainEvent::AgentClaimReclaimed { .. }
            | DomainEvent::AllianceFormed { .. }
            | DomainEvent::AllianceJoined { .. }
            | DomainEvent::AllianceLeft { .. }
            | DomainEvent::AllianceDissolved { .. }
            | DomainEvent::WarDeclared { .. }
            | DomainEvent::WarConcluded { .. } => self.apply_domain_event_gameplay(event, now)?,
            DomainEvent::GovernanceProposalOpened { .. }
            | DomainEvent::GovernanceVoteCast { .. }
            | DomainEvent::GovernanceProposalFinalized { .. }
            | DomainEvent::CrisisSpawned { .. }
            | DomainEvent::CrisisResolved { .. }
            | DomainEvent::CrisisTimedOut { .. }
            | DomainEvent::MetaProgressGranted { .. }
            | DomainEvent::ProductValidated { .. } => {
                self.apply_domain_event_governance_meta(event, now)?
            }
            DomainEvent::AgentLocationAuthorityUpdated { .. }
            | DomainEvent::LocationAnchorUpdated { .. }
            | DomainEvent::FactorySiteAuthorityUpdated { .. }
            | DomainEvent::FactoryConstructionPowerProfileUpdated { .. }
            | DomainEvent::ProductValidationRecorded { .. }
            | DomainEvent::ProductValidationAttemptStarted { .. } => {
                industry_history_transition::PreparedIndustryHistoryEvent::prepare(
                    self, event, now,
                )?
                .install(self)
            }
        }
        self.migrate_compat_material_ledgers();
        Ok(())
    }

    pub fn route_domain_event(&mut self, event: &DomainEvent) {
        match event {
            DomainEvent::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                ..
            } => {
                if let Some(cell) = self.agents.get_mut(from_agent_id) {
                    cell.mailbox.push_back(event.clone());
                }
                if from_agent_id != to_agent_id
                    && let Some(cell) = self.agents.get_mut(to_agent_id)
                {
                    cell.mailbox.push_back(event.clone());
                }
            }
            DomainEvent::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            }
            | DomainEvent::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            } => {
                if let Some(cell) = self.agents.get_mut(owner_agent_id) {
                    cell.mailbox.push_back(event.clone());
                }
                if owner_agent_id != grantee_agent_id
                    && let Some(cell) = self.agents.get_mut(grantee_agent_id)
                {
                    cell.mailbox.push_back(event.clone());
                }
            }
            _ => {
                let Some(agent_id) = event.agent_id() else {
                    return;
                };
                if let Some(cell) = self.agents.get_mut(agent_id) {
                    cell.mailbox.push_back(event.clone());
                }
            }
        }
    }
}
