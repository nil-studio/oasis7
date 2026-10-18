//! World model entities: Agent, Location, Asset, WorldConfig, WorldModel.

use crate::geometry::GeoPos;
use oasis7_agent_api::world_entity::RobotBodySpec;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

mod module_market;
#[path = "world_model_config.rs"]
mod world_model_config;
mod world_model_physics_specs;

use super::ResourceOwner;
use super::chunking::{ChunkCoord, chunk_coord_of};
use super::fragment_physics::FragmentPhysicalProfile;
use super::memory_entry::LongTermMemoryEntry;
use super::module_visual::ModuleVisualEntity;
use super::power::{AgentPowerStatus, PowerConfig, PowerPlant};
use super::social::{
    SocialEdgeState, SocialFactState, default_next_social_edge_id, default_next_social_fact_id,
};
use super::types::{
    AgentId, AssetId, ChunkResourceBudget, ElementBudgetError, FacilityId, FragmentElementKind,
    FragmentResourceBudget, LocationId, LocationProfile, PowerOrderSide, ResourceKind,
    ResourceStock, WorldTime,
};
pub use module_market::{
    InstalledModuleState, ModuleArtifactBidState, ModuleArtifactListingState, ModuleArtifactState,
};
use module_market::{default_next_module_market_order_id, default_next_module_market_sale_id};
pub use world_model_config::{
    AsteroidFragmentConfig, EconomyConfig, MaterialDistributionStrategy, MaterialRadiationFactors,
    MaterialWeights, PhysicsConfig, SpaceConfig, ThermalStatus, WorldConfig,
};
pub use world_model_physics_specs::{PhysicsParameterSpec, physics_parameter_specs};

const DEFAULT_AGENT_SPEED_CM_PER_TICK: i64 = world_model_config::DEFAULT_AGENT_SPEED_CM_PER_TICK;

// ============================================================================
// World Entities
// ============================================================================

/// Player-facing projection of an authoritative runtime agent activity.
///
/// This is deliberately a separate simulator shape: the runtime keeps its
/// persisted field names and typed action id, while the simulator model
/// exposes the compact labels consumed by the Viewer.  `None` means the
/// runtime has no activity record; it must not be synthesized as idle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivityProjectionV1 {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub updated_at: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentKinematics {
    /// Planned movement speed used by time-based movement semantics.
    pub speed_cm_per_tick: i64,
    /// Optional movement target location id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_target_location_id: Option<LocationId>,
    /// Optional movement target in absolute world position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_target: Option<GeoPos>,
    /// Tick index when the current move started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_started_at_tick: Option<WorldTime>,
    /// Tick index when the current move is expected to arrive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_eta_tick: Option<WorldTime>,
    /// Remaining distance in centimeters for the current move.
    pub move_remaining_cm: i64,
}

impl Default for AgentKinematics {
    fn default() -> Self {
        Self {
            speed_cm_per_tick: DEFAULT_AGENT_SPEED_CM_PER_TICK,
            move_target_location_id: None,
            move_target: None,
            move_started_at_tick: None,
            move_eta_tick: None,
            move_remaining_cm: 0,
        }
    }
}

impl AgentKinematics {
    pub fn clear_motion_state(&mut self) {
        self.move_target_location_id = None;
        self.move_target = None;
        self.move_started_at_tick = None;
        self.move_eta_tick = None;
        self.move_remaining_cm = 0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentExecutionDebugContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_check_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_check_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_schema_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_schema_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_class: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_action_sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_reported_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provider_reported_supported_action_sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_check_fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_check_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_config_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub pos: GeoPos,
    pub body: RobotBodySpec,
    pub location_id: LocationId,
    pub resources: ResourceStock,
    /// Optional authoritative activity projection from the runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<AgentActivityProjectionV1>,
    /// Power status for M4 power system.
    pub power: AgentPowerStatus,
    /// Movement state for upcoming time-based move execution.
    #[serde(default)]
    pub kinematics: AgentKinematics,
    /// Thermal status (heat accumulation).
    #[serde(default)]
    pub thermal: ThermalStatus,
}

impl Agent {
    pub fn new(id: impl Into<String>, location_id: impl Into<String>, pos: GeoPos) -> Self {
        Self {
            id: id.into(),
            pos,
            body: RobotBodySpec::default(),
            location_id: location_id.into(),
            resources: ResourceStock::default(),
            activity: None,
            power: AgentPowerStatus::default(),
            kinematics: AgentKinematics::default(),
            thermal: ThermalStatus::default(),
        }
    }

    /// Create a new agent with custom power configuration.
    pub fn new_with_power(
        id: impl Into<String>,
        location_id: impl Into<String>,
        pos: GeoPos,
        power_config: &PowerConfig,
    ) -> Self {
        Self {
            id: id.into(),
            pos,
            body: RobotBodySpec::default(),
            location_id: location_id.into(),
            resources: ResourceStock::default(),
            activity: None,
            power: AgentPowerStatus::from_config(power_config),
            kinematics: AgentKinematics::default(),
            thermal: ThermalStatus::default(),
        }
    }

    /// Check if the agent is shut down due to power depletion.
    pub fn is_shutdown(&self) -> bool {
        self.power.is_shutdown()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub id: LocationId,
    pub name: String,
    pub pos: GeoPos,
    pub profile: LocationProfile,
    pub resources: ResourceStock,
    #[serde(default)]
    pub mined_compound_g: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_profile: Option<FragmentPhysicalProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_budget: Option<FragmentResourceBudget>,
}

impl Location {
    pub fn new(id: impl Into<String>, name: impl Into<String>, pos: GeoPos) -> Self {
        Self::new_with_profile(id, name, pos, LocationProfile::default())
    }

    pub fn new_with_profile(
        id: impl Into<String>,
        name: impl Into<String>,
        pos: GeoPos,
        profile: LocationProfile,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pos,
            profile,
            resources: ResourceStock::default(),
            mined_compound_g: 0,
            fragment_profile: None,
            fragment_budget: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub owner: ResourceOwner,
    pub kind: AssetKind,
    pub quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AssetKind {
    Resource { kind: ResourceKind },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Factory {
    pub id: FacilityId,
    pub owner: ResourceOwner,
    pub location_id: LocationId,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionalInfrastructure {
    pub facility_id: FacilityId,
    pub kind: String,
    pub location_id: LocationId,
    pub owner: ResourceOwner,
    pub owner_claim_id: String,
    pub regional_blocker_receipt_id: String,
    pub status: String,
    pub module_id: String,
    pub module_version: String,
    pub wasm_hash: String,
    pub entrypoint: String,
    pub service_radius_cm: i64,
    #[serde(default)]
    pub supported_resource_kinds: Vec<String>,
    #[serde(default)]
    pub measured_supply_schema_version: u8,
    #[serde(default)]
    pub inventory_revision: u64,
    #[serde(default)]
    pub available_units_by_kind: BTreeMap<String, i64>,
    #[serde(default)]
    pub throughput_limit_units_per_epoch: i64,
    #[serde(default)]
    pub throughput_epoch: u64,
    #[serde(default)]
    pub throughput_remaining_units: i64,
    pub upkeep_paid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_proposal_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_serviced_target_id: Option<String>,
}

fn default_next_power_order_id() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerOrderState {
    pub order_id: u64,
    pub owner: ResourceOwner,
    pub side: PowerOrderSide,
    pub remaining_amount: i64,
    pub limit_price_per_pu: i64,
    pub created_at: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerOrderBookState {
    #[serde(default = "default_next_power_order_id")]
    pub next_order_id: u64,
    #[serde(default)]
    pub open_orders: Vec<PowerOrderState>,
}

impl Default for PowerOrderBookState {
    fn default() -> Self {
        Self {
            next_order_id: default_next_power_order_id(),
            open_orders: Vec::new(),
        }
    }
}

// ============================================================================
// World Model (aggregate)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldModel {
    pub agents: BTreeMap<AgentId, Agent>,
    #[serde(default)]
    pub agent_prompt_profiles: BTreeMap<AgentId, AgentPromptProfile>,
    #[serde(default)]
    pub agent_player_bindings: BTreeMap<AgentId, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_execution_debug_contexts: BTreeMap<AgentId, AgentExecutionDebugContext>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_player_public_key_bindings: BTreeMap<AgentId, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub player_auth_last_nonce: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_long_term_memories: BTreeMap<AgentId, Vec<LongTermMemoryEntry>>,
    pub locations: BTreeMap<LocationId, Location>,
    pub assets: BTreeMap<AssetId, Asset>,
    #[serde(default)]
    pub module_visual_entities: BTreeMap<String, ModuleVisualEntity>,
    #[serde(default)]
    pub module_artifacts: BTreeMap<String, ModuleArtifactState>,
    #[serde(default)]
    pub installed_modules: BTreeMap<String, InstalledModuleState>,
    #[serde(default)]
    pub module_artifact_listings: BTreeMap<String, ModuleArtifactListingState>,
    #[serde(default)]
    pub module_artifact_bids: BTreeMap<String, Vec<ModuleArtifactBidState>>,
    #[serde(default = "default_next_module_market_order_id")]
    pub next_module_market_order_id: u64,
    #[serde(default = "default_next_module_market_sale_id")]
    pub next_module_market_sale_id: u64,
    #[serde(default)]
    pub power_plants: BTreeMap<FacilityId, PowerPlant>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub regional_infrastructure: BTreeMap<FacilityId, RegionalInfrastructure>,
    #[serde(default)]
    pub power_order_book: PowerOrderBookState,
    #[serde(default = "default_next_social_fact_id")]
    pub next_social_fact_id: u64,
    #[serde(default)]
    pub social_facts: BTreeMap<u64, SocialFactState>,
    #[serde(default = "default_next_social_edge_id")]
    pub next_social_edge_id: u64,
    #[serde(default)]
    pub social_edges: BTreeMap<u64, SocialEdgeState>,
    #[serde(default)]
    pub social_stake_pool: ResourceStock,
    #[serde(default)]
    pub factories: BTreeMap<FacilityId, Factory>,
    #[serde(
        default,
        serialize_with = "serialize_chunk_states",
        deserialize_with = "deserialize_chunk_states"
    )]
    pub chunks: BTreeMap<ChunkCoord, ChunkState>,
    #[serde(
        default,
        serialize_with = "serialize_chunk_resource_budgets",
        deserialize_with = "deserialize_chunk_resource_budgets"
    )]
    pub chunk_resource_budgets: BTreeMap<ChunkCoord, ChunkResourceBudget>,
    #[serde(
        default,
        serialize_with = "serialize_chunk_boundary_reservations",
        deserialize_with = "deserialize_chunk_boundary_reservations"
    )]
    pub chunk_boundary_reservations: BTreeMap<ChunkCoord, Vec<BoundaryReservation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentPromptProfile {
    pub agent_id: AgentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_term_goal_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_term_goal_override: Option<String>,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub updated_at_tick: WorldTime,
    #[serde(default)]
    pub updated_by: String,
}

impl AgentPromptProfile {
    pub fn for_agent(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChunkState {
    #[default]
    Unexplored,
    Generated,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryReservation {
    pub source_chunk: ChunkCoord,
    pub source_fragment_id: LocationId,
    pub source_pos: GeoPos,
    pub source_radius_cm: i64,
    pub min_spacing_cm: i64,
}

fn serialize_chunk_states<S>(
    chunks: &BTreeMap<ChunkCoord, ChunkState>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded: BTreeMap<String, ChunkState> = chunks
        .iter()
        .map(|(coord, state)| (encode_chunk_coord(*coord), *state))
        .collect();
    encoded.serialize(serializer)
}

fn deserialize_chunk_states<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<ChunkCoord, ChunkState>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = BTreeMap::<String, ChunkState>::deserialize(deserializer)?;
    let mut decoded = BTreeMap::new();
    for (key, state) in encoded {
        let coord = decode_chunk_coord(&key).map_err(serde::de::Error::custom)?;
        decoded.insert(coord, state);
    }
    Ok(decoded)
}

fn serialize_chunk_resource_budgets<S>(
    budgets: &BTreeMap<ChunkCoord, ChunkResourceBudget>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded: BTreeMap<String, ChunkResourceBudget> = budgets
        .iter()
        .map(|(coord, budget)| (encode_chunk_coord(*coord), budget.clone()))
        .collect();
    encoded.serialize(serializer)
}

fn deserialize_chunk_resource_budgets<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<ChunkCoord, ChunkResourceBudget>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = BTreeMap::<String, ChunkResourceBudget>::deserialize(deserializer)?;
    let mut decoded = BTreeMap::new();
    for (key, budget) in encoded {
        let coord = decode_chunk_coord(&key).map_err(serde::de::Error::custom)?;
        decoded.insert(coord, budget);
    }
    Ok(decoded)
}

fn serialize_chunk_boundary_reservations<S>(
    reservations: &BTreeMap<ChunkCoord, Vec<BoundaryReservation>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let encoded: BTreeMap<String, Vec<BoundaryReservation>> = reservations
        .iter()
        .map(|(coord, entries)| (encode_chunk_coord(*coord), entries.clone()))
        .collect();
    encoded.serialize(serializer)
}

fn deserialize_chunk_boundary_reservations<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<ChunkCoord, Vec<BoundaryReservation>>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = BTreeMap::<String, Vec<BoundaryReservation>>::deserialize(deserializer)?;
    let mut decoded = BTreeMap::new();
    for (key, entries) in encoded {
        let coord = decode_chunk_coord(&key).map_err(serde::de::Error::custom)?;
        decoded.insert(coord, entries);
    }
    Ok(decoded)
}

fn encode_chunk_coord(coord: ChunkCoord) -> String {
    format!("{}:{}:{}", coord.x, coord.y, coord.z)
}

fn decode_chunk_coord(encoded: &str) -> Result<ChunkCoord, String> {
    let mut parts = encoded.split(':');
    let x = parts
        .next()
        .ok_or_else(|| format!("invalid chunk coord key: {encoded}"))?
        .parse::<i32>()
        .map_err(|_| format!("invalid chunk coord x: {encoded}"))?;
    let y = parts
        .next()
        .ok_or_else(|| format!("invalid chunk coord key: {encoded}"))?
        .parse::<i32>()
        .map_err(|_| format!("invalid chunk coord y: {encoded}"))?;
    let z = parts
        .next()
        .ok_or_else(|| format!("invalid chunk coord key: {encoded}"))?
        .parse::<i32>()
        .map_err(|_| format!("invalid chunk coord z: {encoded}"))?;
    if parts.next().is_some() {
        return Err(format!("invalid chunk coord key: {encoded}"));
    }
    Ok(ChunkCoord { x, y, z })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentResourceError {
    LocationNotFound { location_id: LocationId },
    FragmentBudgetMissing { location_id: LocationId },
    ChunkCoordUnavailable { location_id: LocationId },
    ChunkBudgetMissing { coord: ChunkCoord },
    Budget(ElementBudgetError),
}

impl WorldModel {
    pub fn consume_fragment_resource(
        &mut self,
        location_id: &str,
        space: &SpaceConfig,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<i64, FragmentResourceError> {
        if amount_g <= 0 {
            return Err(FragmentResourceError::Budget(
                ElementBudgetError::InvalidAmount { amount_g },
            ));
        }

        let location_id_owned = location_id.to_string();
        let (location_pos, location_remaining) = {
            let location = self.locations.get(location_id).ok_or_else(|| {
                FragmentResourceError::LocationNotFound {
                    location_id: location_id_owned.clone(),
                }
            })?;
            let budget = location.fragment_budget.as_ref().ok_or_else(|| {
                FragmentResourceError::FragmentBudgetMissing {
                    location_id: location_id_owned.clone(),
                }
            })?;
            (location.pos, budget.get_remaining(kind))
        };

        if location_remaining < amount_g {
            return Err(FragmentResourceError::Budget(
                ElementBudgetError::Insufficient {
                    kind,
                    requested_g: amount_g,
                    remaining_g: location_remaining,
                },
            ));
        }

        let coord = chunk_coord_of(location_pos, space).ok_or_else(|| {
            FragmentResourceError::ChunkCoordUnavailable {
                location_id: location_id_owned.clone(),
            }
        })?;

        let chunk_remaining = self
            .chunk_resource_budgets
            .get(&coord)
            .ok_or(FragmentResourceError::ChunkBudgetMissing { coord })?
            .get_remaining(kind);
        if chunk_remaining < amount_g {
            return Err(FragmentResourceError::Budget(
                ElementBudgetError::Insufficient {
                    kind,
                    requested_g: amount_g,
                    remaining_g: chunk_remaining,
                },
            ));
        }

        {
            let location = self.locations.get_mut(location_id).ok_or_else(|| {
                FragmentResourceError::LocationNotFound {
                    location_id: location_id_owned.clone(),
                }
            })?;
            let fragment_budget = location.fragment_budget.as_mut().ok_or_else(|| {
                FragmentResourceError::FragmentBudgetMissing {
                    location_id: location_id_owned.clone(),
                }
            })?;
            fragment_budget
                .consume(kind, amount_g)
                .map_err(FragmentResourceError::Budget)?;
        }

        let chunk_budget = self
            .chunk_resource_budgets
            .get_mut(&coord)
            .ok_or(FragmentResourceError::ChunkBudgetMissing { coord })?;
        chunk_budget
            .consume(kind, amount_g)
            .map_err(FragmentResourceError::Budget)?;

        if chunk_budget.is_exhausted() {
            self.chunks.insert(coord, ChunkState::Exhausted);
        }

        Ok(amount_g)
    }
}
