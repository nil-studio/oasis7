//! Core type definitions: IDs, constants, and resource types.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::chunking::ChunkCoord;

// ============================================================================
// Type Aliases
// ============================================================================

pub type AgentId = String;
pub type LocationId = String;
pub type AssetId = String;
pub type FacilityId = String;
pub type WorldTime = u64;
pub type WorldEventId = u64;
pub type ActionId = u64;

// ============================================================================
// Constants
// ============================================================================

pub const CM_PER_KM: i64 = 100_000;
pub const DEFAULT_VISIBILITY_RANGE_CM: i64 = 10_000_000;
pub const DEFAULT_MOVE_COST_PER_KM_ELECTRICITY: i64 = 1;
pub const SNAPSHOT_VERSION: u32 = 3;
pub const JOURNAL_VERSION: u32 = 3;
pub const CHUNK_GENERATION_SCHEMA_VERSION: u32 = 1;
pub const PPM_BASE: i64 = 1_000_000;
pub const DEFAULT_ELEMENT_RECOVERABILITY_PPM: i64 = 850_000;

// ============================================================================
// Resource Types
// ============================================================================

pub use oasis7_agent_api::world_action::ResourceKind;

// ============================================================================
// Location Physical Profile
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    Silicate,
    Metal,
    Ice,
    Carbon,
    Composite,
}

pub use oasis7_agent_api::world_resource::{
    ChunkResourceBudget, ElementBudgetError, ElementComposition, FragmentElementKind,
    FragmentResourceBudget,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocationProfile {
    pub material: MaterialKind,
    pub radius_cm: i64,
    pub radiation_emission_per_tick: i64,
}

impl Default for LocationProfile {
    fn default() -> Self {
        Self {
            material: MaterialKind::Silicate,
            radius_cm: 100,
            radiation_emission_per_tick: 0,
        }
    }
}

pub use oasis7_agent_api::world_resource::{ResourceStock, StockError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ResourceOwner {
    Agent { agent_id: AgentId },
    Location { location_id: LocationId },
}

pub use oasis7_agent_api::world_action::ModuleInstallTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerOrderSide {
    Buy,
    Sell,
}

// ============================================================================
// Action Types
// ============================================================================

use super::module_visual::ModuleVisualEntity;
use super::social::{SocialAdjudicationDecision, SocialStake};
use crate::geometry::GeoPos;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub id: ActionId,
    pub action: Action,
    #[serde(default)]
    pub submitter: ActionSubmitter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleQuote {
    pub owner: ResourceOwner,
    pub factory_id: FacilityId,
    pub recipe_id: String,
    pub batches: i64,
    pub base_duration_ticks: i64,
    pub electricity_cost: i64,
    pub electricity_after: i64,
    pub hardware_cost: i64,
    pub data_output: i64,
    pub finished_product_id: String,
    pub finished_product_units: i64,
    pub local_shortage_delay_ticks: i64,
    pub shortage_reason: String,
    pub recommended_pre_step: String,
    pub runway_before_ticks: i64,
    pub runway_after_ticks: i64,
    pub downtime_threshold_ppm: i64,
    pub continue_production_risk: String,
    pub maintenance_pressure_delta: String,
    pub recommended_maintenance_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefineQuote {
    pub owner: ResourceOwner,
    pub compound_mass_g: i64,
    pub electricity_cost: i64,
    pub hardware_output: i64,
    pub electricity_after: i64,
    pub hardware_shortfall_before: i64,
    pub hardware_shortfall_after: i64,
    pub first_goal_relevance: String,
    pub recommended_refine_amount: i64,
    pub refine_value_class: String,
}

/// Read-only forecast of the existing asteroid-fragment replenishment cadence.
///
/// This deliberately describes a chunk-level opportunity, rather than a future
/// generated fragment: placement and material composition remain subject to the
/// normal deterministic replenishment path when its tick is actually processed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FragmentRefillPreview {
    pub chunk_coord: ChunkCoord,
    pub target_frag_id: Option<LocationId>,
    pub current_frag_remaining_summary: String,
    pub chunk_remaining_summary: String,
    pub remaining_by_element_g: BTreeMap<FragmentElementKind, i64>,
    pub replenishment_enabled: bool,
    pub replenishment_due: bool,
    pub next_replenish_tick: Option<WorldTime>,
    pub ticks_until_replenish: Option<WorldTime>,
    pub wait_cost_ticks: WorldTime,
    pub estimated_replenished_frag_count: i64,
    pub estimated_replenished_resource_hint: String,
    pub next_industrial_goal_relevance: String,
    pub wait_cost_summary: String,
    pub recommended_resource_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionSubmitter {
    #[default]
    System,
    Agent {
        agent_id: AgentId,
    },
    Player {
        player_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Action {
    RegisterLocation {
        location_id: LocationId,
        name: String,
        pos: GeoPos,
        profile: LocationProfile,
    },
    RegisterAgent {
        agent_id: AgentId,
        location_id: LocationId,
    },
    RegisterPowerPlant {
        facility_id: FacilityId,
        location_id: LocationId,
        owner: ResourceOwner,
        capacity_per_tick: i64,
        fuel_cost_per_pu: i64,
        maintenance_cost: i64,
        efficiency: f64,
        degradation: f64,
    },
    UpsertModuleVisualEntity {
        entity: ModuleVisualEntity,
    },
    RemoveModuleVisualEntity {
        entity_id: String,
    },
    MoveAgent {
        agent_id: AgentId,
        to: LocationId,
    },
    SpeakToNearby {
        agent_id: AgentId,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_agent_id: Option<AgentId>,
    },
    InspectTarget {
        agent_id: AgentId,
        target_kind: String,
        target_id: String,
    },
    SimpleInteract {
        agent_id: AgentId,
        target_kind: String,
        target_id: String,
        interaction: String,
    },
    HarvestRadiation {
        agent_id: AgentId,
        max_amount: i64,
    },
    BuyPower {
        buyer: ResourceOwner,
        seller: ResourceOwner,
        amount: i64,
        price_per_pu: i64,
    },
    SellPower {
        seller: ResourceOwner,
        buyer: ResourceOwner,
        amount: i64,
        price_per_pu: i64,
    },
    PlacePowerOrder {
        owner: ResourceOwner,
        side: PowerOrderSide,
        amount: i64,
        limit_price_per_pu: i64,
    },
    CancelPowerOrder {
        owner: ResourceOwner,
        order_id: u64,
    },
    TransferResource {
        from: ResourceOwner,
        to: ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    },
    DebugGrantResource {
        owner: ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    },
    MineCompound {
        owner: ResourceOwner,
        location_id: LocationId,
        compound_mass_g: i64,
    },
    RefineCompound {
        owner: ResourceOwner,
        compound_mass_g: i64,
    },
    BuildFactory {
        owner: ResourceOwner,
        location_id: LocationId,
        factory_id: FacilityId,
        factory_kind: String,
    },
    ScheduleRecipe {
        owner: ResourceOwner,
        factory_id: FacilityId,
        recipe_id: String,
        batches: i64,
    },
    CompileModuleArtifactFromSource {
        publisher_agent_id: AgentId,
        module_id: String,
        manifest_path: String,
        source_files: BTreeMap<String, Vec<u8>>,
    },
    DeployModuleArtifact {
        publisher_agent_id: AgentId,
        wasm_hash: String,
        wasm_bytes: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        module_id_hint: Option<String>,
    },
    InstallModuleFromArtifact {
        installer_agent_id: AgentId,
        module_id: String,
        module_version: String,
        wasm_hash: String,
        activate: bool,
    },
    InstallModuleToTargetFromArtifact {
        installer_agent_id: AgentId,
        module_id: String,
        module_version: String,
        wasm_hash: String,
        activate: bool,
        install_target: ModuleInstallTarget,
    },
    InstallMicroDepot {
        installer_agent_id: AgentId,
        facility_id: FacilityId,
        location_id: LocationId,
        owner_claim_id: String,
        regional_blocker_receipt_id: String,
        module_id: String,
        module_version: String,
        wasm_hash: String,
        entrypoint: String,
        service_radius_cm: i64,
        #[serde(default)]
        supported_resource_kinds: Vec<String>,
    },
    ServiceMicroDepotRepair {
        agent_id: AgentId,
        facility_id: FacilityId,
        target_id: String,
        base_cost_class: crate::MicroDepotPressureClass,
        base_risk_class: crate::MicroDepotPressureClass,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blocker_type: Option<String>,
    },
    ServiceMicroDepotLogistics {
        agent_id: AgentId,
        facility_id: FacilityId,
        target_id: String,
        base_cost_class: crate::MicroDepotPressureClass,
        base_risk_class: crate::MicroDepotPressureClass,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blocker_type: Option<String>,
    },
    PayMicroDepotUpkeep {
        agent_id: AgentId,
        facility_id: FacilityId,
    },
    SuspendMicroDepot {
        agent_id: AgentId,
        facility_id: FacilityId,
    },
    ReclaimMicroDepot {
        agent_id: AgentId,
        facility_id: FacilityId,
    },
    ListModuleArtifactForSale {
        seller_agent_id: AgentId,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
    },
    BuyModuleArtifact {
        buyer_agent_id: AgentId,
        wasm_hash: String,
    },
    DelistModuleArtifact {
        seller_agent_id: AgentId,
        wasm_hash: String,
    },
    DestroyModuleArtifact {
        owner_agent_id: AgentId,
        wasm_hash: String,
        reason: String,
    },
    PlaceModuleArtifactBid {
        bidder_agent_id: AgentId,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
    },
    CancelModuleArtifactBid {
        bidder_agent_id: AgentId,
        wasm_hash: String,
        bid_order_id: u64,
    },
    PublishSocialFact {
        actor: ResourceOwner,
        schema_id: String,
        subject: ResourceOwner,
        object: Option<ResourceOwner>,
        claim: String,
        confidence_ppm: i64,
        evidence_event_ids: Vec<WorldEventId>,
        ttl_ticks: Option<u64>,
        stake: Option<SocialStake>,
    },
    ChallengeSocialFact {
        challenger: ResourceOwner,
        fact_id: u64,
        reason: String,
        stake: Option<SocialStake>,
    },
    AdjudicateSocialFact {
        adjudicator: ResourceOwner,
        fact_id: u64,
        decision: SocialAdjudicationDecision,
        notes: String,
    },
    RevokeSocialFact {
        actor: ResourceOwner,
        fact_id: u64,
        reason: String,
    },
    DeclareSocialEdge {
        declarer: ResourceOwner,
        schema_id: String,
        relation_kind: String,
        from: ResourceOwner,
        to: ResourceOwner,
        weight_bps: i64,
        backing_fact_ids: Vec<u64>,
        ttl_ticks: Option<u64>,
    },
    FormAlliance {
        proposer_agent_id: AgentId,
        alliance_id: String,
        members: Vec<AgentId>,
        charter: String,
    },
    JoinAlliance {
        operator_agent_id: AgentId,
        alliance_id: String,
        member_agent_id: AgentId,
    },
    LeaveAlliance {
        operator_agent_id: AgentId,
        alliance_id: String,
        member_agent_id: AgentId,
    },
    DissolveAlliance {
        operator_agent_id: AgentId,
        alliance_id: String,
        reason: String,
    },
    DeclareWar {
        initiator_agent_id: AgentId,
        war_id: String,
        aggressor_alliance_id: String,
        defender_alliance_id: String,
        objective: String,
        intensity: u32,
    },
    OpenGovernanceProposal {
        proposer_agent_id: AgentId,
        proposal_key: String,
        title: String,
        description: String,
        options: Vec<String>,
        voting_window_ticks: u64,
        quorum_weight: u64,
        pass_threshold_bps: u16,
    },
    CastGovernanceVote {
        voter_agent_id: AgentId,
        proposal_key: String,
        option: String,
        weight: u32,
    },
    ResolveCrisis {
        resolver_agent_id: AgentId,
        crisis_id: String,
        strategy: String,
        success: bool,
    },
    GrantMetaProgress {
        operator_agent_id: AgentId,
        target_agent_id: AgentId,
        track: String,
        points: i64,
        achievement_id: Option<String>,
    },
    OpenEconomicContract {
        creator_agent_id: AgentId,
        contract_id: String,
        counterparty_agent_id: AgentId,
        settlement_kind: ResourceKind,
        settlement_amount: i64,
        reputation_stake: i64,
        expires_at: u64,
        description: String,
    },
    AcceptEconomicContract {
        accepter_agent_id: AgentId,
        contract_id: String,
    },
    SettleEconomicContract {
        operator_agent_id: AgentId,
        contract_id: String,
        success: bool,
        notes: String,
    },
}
