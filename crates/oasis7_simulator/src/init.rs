//! World initialization utilities.

use serde::{Deserialize, Serialize};

use crate::geometry::GeoPos;

use super::asteroid_fragment::generate_fragments;
use super::chunking::{ChunkCoord, chunk_coord_of, chunk_coords};
use super::frag_spawn::{FRAGMENT_LOCATION_PREFIX, fragment_spawn_pos};
use super::fragment_physics::{
    synthesize_fragment_budget, synthesize_fragment_profile, truncate_fragment_profile_blocks,
};
use super::init_module_visual::{ensure_module_visual_anchor_exists, insert_module_visual_entity};
use super::kernel::{ChunkGenerationCause, ChunkRuntimeConfig, WorldEventKind, WorldKernel};
use super::module_visual::{ModuleVisualAnchor, ModuleVisualEntity};
use super::power::{PlantStatus, PowerPlant};
use super::scenario::WorldScenario;
use super::types::{
    AgentId, ChunkResourceBudget, FacilityId, LocationId, LocationProfile, ResourceKind,
    ResourceOwner, ResourceStock,
};
use super::world_model::{
    Agent, BoundaryReservation, ChunkState, Location, SpaceConfig, WorldConfig, WorldModel,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct WorldInitConfig {
    pub seed: u64,
    pub origin: OriginLocationConfig,
    pub locations: Vec<LocationSeedConfig>,
    pub asteroid_fragment: AsteroidFragmentInitConfig,
    pub agents: AgentSpawnConfig,
    pub power_plants: Vec<PowerPlantSeedConfig>,
    pub module_visual_entities: Vec<ModuleVisualEntity>,
}

impl WorldInitConfig {
    pub fn sanitized(mut self) -> Self {
        self.origin = self.origin.sanitized();
        self.locations = self
            .locations
            .into_iter()
            .map(|location| location.sanitized())
            .collect();
        self.asteroid_fragment = self.asteroid_fragment.sanitized();
        self.agents = self.agents.sanitized();
        self.power_plants = self
            .power_plants
            .into_iter()
            .map(|plant| plant.sanitized())
            .collect();
        self.module_visual_entities = self
            .module_visual_entities
            .into_iter()
            .map(|entity| entity.sanitized())
            .collect();
        self
    }

    pub fn from_scenario(scenario: WorldScenario, config: &WorldConfig) -> Self {
        scenario.build_init(config)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OriginLocationConfig {
    pub enabled: bool,
    pub location_id: LocationId,
    pub name: String,
    pub pos: Option<GeoPos>,
    pub profile: LocationProfile,
    pub resources: ResourceStock,
}

impl Default for OriginLocationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            location_id: "origin".to_string(),
            name: "Origin".to_string(),
            pos: None,
            profile: LocationProfile::default(),
            resources: ResourceStock::default(),
        }
    }
}

impl OriginLocationConfig {
    pub fn sanitized(mut self) -> Self {
        if self.location_id.is_empty() {
            self.location_id = "origin".to_string();
        }
        if self.name.is_empty() {
            self.name = "Origin".to_string();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LocationSeedConfig {
    pub location_id: LocationId,
    pub name: String,
    pub pos: Option<GeoPos>,
    pub profile: LocationProfile,
    pub resources: ResourceStock,
}

impl LocationSeedConfig {
    pub fn sanitized(mut self) -> Self {
        if self.name.is_empty() {
            self.name = self.location_id.clone();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AsteroidFragmentInitConfig {
    pub enabled: bool,
    pub seed_offset: u64,
    pub min_fragment_spacing_cm: Option<i64>,
    pub bootstrap_chunks: Vec<ChunkCoord>,
}

impl Default for AsteroidFragmentInitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            seed_offset: 1,
            min_fragment_spacing_cm: None,
            bootstrap_chunks: Vec::new(),
        }
    }
}

impl AsteroidFragmentInitConfig {
    pub fn sanitized(mut self) -> Self {
        if let Some(spacing) = self.min_fragment_spacing_cm
            && spacing < 0
        {
            self.min_fragment_spacing_cm = Some(0);
        }
        self.bootstrap_chunks.sort();
        self.bootstrap_chunks.dedup();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSpawnConfig {
    pub count: usize,
    pub id_prefix: String,
    pub start_index: u32,
    pub resources: ResourceStock,
}

impl Default for AgentSpawnConfig {
    fn default() -> Self {
        Self {
            count: 1,
            id_prefix: "agent-".to_string(),
            start_index: 0,
            resources: ResourceStock::default(),
        }
    }
}

impl AgentSpawnConfig {
    pub fn sanitized(mut self) -> Self {
        if self.id_prefix.is_empty() {
            self.id_prefix = "agent-".to_string();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerPlantSeedConfig {
    pub facility_id: FacilityId,
    pub location_id: LocationId,
    pub owner: ResourceOwner,
    pub capacity_per_tick: i64,
    pub fuel_cost_per_pu: i64,
    pub maintenance_cost: i64,
    pub efficiency: f64,
    pub degradation: f64,
}

impl Default for PowerPlantSeedConfig {
    fn default() -> Self {
        Self {
            facility_id: String::new(),
            location_id: String::new(),
            owner: ResourceOwner::Location {
                location_id: String::new(),
            },
            capacity_per_tick: 0,
            fuel_cost_per_pu: 0,
            maintenance_cost: 0,
            efficiency: 1.0,
            degradation: 0.0,
        }
    }
}

impl PowerPlantSeedConfig {
    pub fn sanitized(self) -> Self {
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldInitReport {
    pub seed: u64,
    pub asteroid_fragment_seed: Option<u64>,
    pub locations: usize,
    pub agents: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldInitError {
    OriginOutOfBounds {
        pos: GeoPos,
    },
    LocationOutOfBounds {
        location_id: LocationId,
        pos: GeoPos,
    },
    InvalidLocationId {
        location_id: LocationId,
    },
    LocationIdConflict {
        location_id: LocationId,
    },
    AgentIdConflict {
        agent_id: AgentId,
    },
    InvalidFacilityId {
        facility_id: FacilityId,
    },
    FacilityIdConflict {
        facility_id: FacilityId,
    },
    FacilityLocationNotFound {
        location_id: LocationId,
    },
    FacilityOwnerNotFound {
        owner: ResourceOwner,
    },
    InvalidModuleVisualEntityId {
        entity_id: String,
    },
    ModuleVisualEntityIdConflict {
        entity_id: String,
    },
    ModuleVisualEntityAnchorNotFound {
        entity_id: String,
        anchor: ModuleVisualAnchor,
    },
    SpawnLocationMissing,
    SpawnLocationNotFound {
        location_id: LocationId,
    },
    InvalidResourceAmount {
        kind: ResourceKind,
        amount: i64,
    },
    InvalidFacilityAmount {
        field: String,
        amount: i64,
    },
    InvalidFacilityRatio {
        field: String,
        value: f64,
    },
}

pub fn build_world_model(
    config: &WorldConfig,
    init: &WorldInitConfig,
) -> Result<(WorldModel, WorldInitReport), WorldInitError> {
    let config = config.clone().sanitized();
    let init = init.clone().sanitized();
    let mut model = WorldModel::default();
    initialize_chunk_index(&mut model, &config);

    if init.origin.enabled {
        let pos = init.origin.pos.unwrap_or_else(|| center_pos(&config.space));
        if !config.space.contains(pos) {
            return Err(WorldInitError::OriginOutOfBounds { pos });
        }
        ensure_valid_stock(&init.origin.resources)?;
        let mut location = Location::new_with_profile(
            init.origin.location_id.clone(),
            init.origin.name.clone(),
            pos,
            init.origin.profile.clone(),
        );
        location.resources = init.origin.resources.clone();
        strip_location_electricity(&mut location.resources);
        insert_location(&mut model, location)?;
    }

    for location_seed in &init.locations {
        if location_seed.location_id.is_empty() {
            return Err(WorldInitError::InvalidLocationId {
                location_id: location_seed.location_id.clone(),
            });
        }
        let pos = location_seed
            .pos
            .unwrap_or_else(|| center_pos(&config.space));
        if !config.space.contains(pos) {
            return Err(WorldInitError::LocationOutOfBounds {
                location_id: location_seed.location_id.clone(),
                pos,
            });
        }
        ensure_valid_stock(&location_seed.resources)?;
        let name = if location_seed.name.is_empty() {
            location_seed.location_id.clone()
        } else {
            location_seed.name.clone()
        };
        let mut location = Location::new_with_profile(
            location_seed.location_id.clone(),
            name,
            pos,
            location_seed.profile.clone(),
        );
        location.resources = location_seed.resources.clone();
        strip_location_electricity(&mut location.resources);
        insert_location(&mut model, location)?;
    }

    let asteroid_fragment_seed = if init.asteroid_fragment.enabled {
        Some(init.seed.wrapping_add(init.asteroid_fragment.seed_offset))
    } else {
        None
    };

    if init.asteroid_fragment.enabled {
        let seed_positions = gather_seed_positions(&model);
        ensure_chunk_generated_at_positions(
            &mut model,
            &config,
            &init,
            seed_positions,
            asteroid_fragment_seed,
        )?;
        ensure_chunk_generated_at_coords(
            &mut model,
            &config,
            &init,
            init.asteroid_fragment.bootstrap_chunks.clone(),
            asteroid_fragment_seed,
        )?;
    }

    if init.agents.count > 0 {
        let mut spawn_candidates: Vec<LocationId> = model
            .locations
            .keys()
            .filter(|location_id| location_id.starts_with(FRAGMENT_LOCATION_PREFIX))
            .cloned()
            .collect();
        if spawn_candidates.is_empty() {
            spawn_candidates = model
                .locations
                .keys()
                .filter(|location_id| !location_id.starts_with(FRAGMENT_LOCATION_PREFIX))
                .cloned()
                .collect();
        }
        if spawn_candidates.is_empty() {
            spawn_candidates = model.locations.keys().cloned().collect();
        }
        if spawn_candidates.is_empty() {
            return Err(WorldInitError::SpawnLocationMissing);
        }
        spawn_candidates.sort();

        ensure_valid_stock(&init.agents.resources)?;
        for offset in 0..init.agents.count {
            let idx = init.agents.start_index as u64 + offset as u64;
            let spawn_roll = splitmix64(
                init.seed
                    .wrapping_add(0xA2E4_4B5D_1974_3377)
                    .wrapping_add(idx),
            );
            let spawn_pick = spawn_roll as usize % spawn_candidates.len();
            let spawn_location_id = spawn_candidates[spawn_pick].clone();
            let spawn_pos = model
                .locations
                .get(&spawn_location_id)
                .map(|location| {
                    if !location.id.starts_with(FRAGMENT_LOCATION_PREFIX) {
                        return location.pos;
                    }
                    fragment_spawn_pos(location, &config.space, spawn_roll)
                })
                .ok_or_else(|| WorldInitError::SpawnLocationNotFound {
                    location_id: spawn_location_id.clone(),
                })?;

            let agent_id = format!("{}{}", init.agents.id_prefix, idx);
            let mut agent = Agent::new_with_power(
                agent_id.clone(),
                spawn_location_id,
                spawn_pos,
                &config.power,
            );
            agent.resources = init.agents.resources.clone();
            insert_agent(&mut model, agent)?;
        }
    }

    if init.asteroid_fragment.enabled {
        let agent_positions: Vec<GeoPos> = model.agents.values().map(|agent| agent.pos).collect();
        ensure_chunk_generated_at_positions(
            &mut model,
            &config,
            &init,
            agent_positions,
            asteroid_fragment_seed,
        )?;
    }

    for plant_seed in &init.power_plants {
        if plant_seed.facility_id.is_empty() {
            return Err(WorldInitError::InvalidFacilityId {
                facility_id: plant_seed.facility_id.clone(),
            });
        }
        if !model.locations.contains_key(&plant_seed.location_id) {
            return Err(WorldInitError::FacilityLocationNotFound {
                location_id: plant_seed.location_id.clone(),
            });
        }
        ensure_owner_exists(&model, &plant_seed.owner)?;
        ensure_non_negative_amount("capacity_per_tick", plant_seed.capacity_per_tick)?;
        ensure_non_negative_amount("fuel_cost_per_pu", plant_seed.fuel_cost_per_pu)?;
        ensure_non_negative_amount("maintenance_cost", plant_seed.maintenance_cost)?;
        ensure_valid_ratio("efficiency", plant_seed.efficiency)?;
        ensure_valid_ratio("degradation", plant_seed.degradation)?;

        let plant = PowerPlant {
            id: plant_seed.facility_id.clone(),
            location_id: plant_seed.location_id.clone(),
            owner: plant_seed.owner.clone(),
            capacity_per_tick: plant_seed.capacity_per_tick,
            current_output: 0,
            fuel_cost_per_pu: plant_seed.fuel_cost_per_pu,
            maintenance_cost: plant_seed.maintenance_cost,
            status: PlantStatus::Running,
            efficiency: plant_seed.efficiency,
            degradation: plant_seed.degradation,
        };
        insert_power_plant(&mut model, plant)?;
    }

    for module_entity in &init.module_visual_entities {
        if module_entity.entity_id.trim().is_empty() || module_entity.module_id.trim().is_empty() {
            return Err(WorldInitError::InvalidModuleVisualEntityId {
                entity_id: module_entity.entity_id.clone(),
            });
        }
        ensure_module_visual_anchor_exists(&model, &config, module_entity)?;
        insert_module_visual_entity(&mut model, module_entity.clone())?;
    }

    let report = WorldInitReport {
        seed: init.seed,
        asteroid_fragment_seed,
        locations: model
            .locations
            .values()
            .filter(|location| !location.id.starts_with("frag-"))
            .count(),
        agents: model.agents.len(),
    };

    Ok((model, report))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChunkGenerationSummary {
    pub coord: ChunkCoord,
    pub seed: u64,
    pub fragment_count: u32,
    pub block_count: u32,
    pub chunk_budget: ChunkResourceBudget,
}

fn empty_chunk_generation_summary(coord: ChunkCoord, seed: u64) -> ChunkGenerationSummary {
    ChunkGenerationSummary {
        coord,
        seed,
        fragment_count: 0,
        block_count: 0,
        chunk_budget: ChunkResourceBudget::default(),
    }
}

pub fn summarize_chunk_generation(
    model: &WorldModel,
    config: &WorldConfig,
    coord: ChunkCoord,
    seed: u64,
) -> ChunkGenerationSummary {
    let fragment_prefix = format!("frag-{}-{}-{}-", coord.x, coord.y, coord.z);
    let mut fragment_count = 0u32;
    let mut block_count = 0u32;

    for location in model.locations.values() {
        if !location.id.starts_with(&fragment_prefix) {
            continue;
        }
        if chunk_coord_of(location.pos, &config.space) != Some(coord) {
            continue;
        }
        fragment_count = fragment_count.saturating_add(1);
        if let Some(profile) = &location.fragment_profile {
            block_count = block_count.saturating_add(profile.blocks.blocks.len() as u32);
        }
    }

    let chunk_budget = model
        .chunk_resource_budgets
        .get(&coord)
        .cloned()
        .unwrap_or_default();

    ChunkGenerationSummary {
        coord,
        seed,
        fragment_count,
        block_count,
        chunk_budget,
    }
}

pub fn ensure_chunk_generated_at_positions(
    model: &mut WorldModel,
    config: &WorldConfig,
    init: &WorldInitConfig,
    positions: Vec<GeoPos>,
    asteroid_fragment_seed: Option<u64>,
) -> Result<(), WorldInitError> {
    let coords = positions
        .into_iter()
        .filter_map(|pos| chunk_coord_of(pos, &config.space))
        .collect::<Vec<_>>();
    ensure_chunk_generated_at_coords(model, config, init, coords, asteroid_fragment_seed)
}

pub fn ensure_chunk_generated_at_coords(
    model: &mut WorldModel,
    config: &WorldConfig,
    init: &WorldInitConfig,
    coords: Vec<ChunkCoord>,
    asteroid_fragment_seed: Option<u64>,
) -> Result<(), WorldInitError> {
    for coord in coords {
        if model
            .chunks
            .get(&coord)
            .is_some_and(|state| matches!(state, ChunkState::Generated | ChunkState::Exhausted))
        {
            continue;
        }
        generate_chunk_fragments(model, config, init, coord, asteroid_fragment_seed)?;
    }
    Ok(())
}

pub fn generate_chunk_fragments(
    model: &mut WorldModel,
    config: &WorldConfig,
    init: &WorldInitConfig,
    coord: super::chunking::ChunkCoord,
    asteroid_fragment_seed: Option<u64>,
) -> Result<ChunkGenerationSummary, WorldInitError> {
    let base_seed = asteroid_fragment_seed
        .unwrap_or_else(|| init.seed.wrapping_add(init.asteroid_fragment.seed_offset));
    let seed = super::chunking::chunk_seed(base_seed, coord);

    if !init.asteroid_fragment.enabled {
        model.chunks.insert(coord, ChunkState::Generated);
        model
            .chunk_resource_budgets
            .insert(coord, ChunkResourceBudget::default());
        return Ok(summarize_chunk_generation(model, config, coord, seed));
    }

    if !model.chunks.contains_key(&coord) {
        return Ok(empty_chunk_generation_summary(coord, seed));
    }
    if model
        .chunks
        .get(&coord)
        .is_some_and(|state| matches!(state, ChunkState::Generated | ChunkState::Exhausted))
    {
        return Ok(summarize_chunk_generation(model, config, coord, seed));
    }

    let Some(bounds) = super::chunking::chunk_bounds(coord, &config.space) else {
        return Ok(empty_chunk_generation_summary(coord, seed));
    };

    let mut asteroid_fragment_config = config.asteroid_fragment.clone();
    if let Some(spacing) = init.asteroid_fragment.min_fragment_spacing_cm {
        asteroid_fragment_config.min_fragment_spacing_cm = spacing;
    }
    let spacing_cm = asteroid_fragment_config.min_fragment_spacing_cm.max(0);
    let max_fragments_per_chunk = asteroid_fragment_config.max_fragments_per_chunk.max(0) as usize;
    let max_blocks_per_fragment = asteroid_fragment_config.max_blocks_per_fragment.max(0) as usize;
    let max_blocks_per_chunk = asteroid_fragment_config.max_blocks_per_chunk.max(0) as usize;

    let local_space = chunk_local_space(bounds);
    if local_space.width_cm <= 0 || local_space.depth_cm <= 0 || local_space.height_cm <= 0 {
        model.chunks.insert(coord, ChunkState::Generated);
        model
            .chunk_resource_budgets
            .insert(coord, ChunkResourceBudget::default());
        return Ok(summarize_chunk_generation(model, config, coord, seed));
    }

    let incoming_boundary_reservations = model
        .chunk_boundary_reservations
        .remove(&coord)
        .unwrap_or_default();
    let neighbor_fragments = gather_neighbor_fragments(model, &config.space, coord);

    let fragments = generate_fragments(seed, &local_space, &asteroid_fragment_config);
    let mut chunk_budget = ChunkResourceBudget::default();
    let mut accepted_fragments = Vec::<AcceptedFragment>::new();
    let mut accepted_fragment_count = 0usize;
    let mut accepted_block_count = 0usize;

    for (idx, mut frag) in fragments.into_iter().enumerate() {
        if accepted_fragment_count >= max_fragments_per_chunk {
            break;
        }
        frag.id = format!("frag-{}-{}-{}-{}", coord.x, coord.y, coord.z, idx);
        frag.name = frag.id.clone();
        frag.pos.x_cm += bounds.min.x_cm;
        frag.pos.y_cm += bounds.min.y_cm;
        frag.pos.z_cm += bounds.min.z_cm;

        if model.locations.contains_key(&frag.id) {
            continue;
        }

        let fragment_radius_cm = frag.profile.radius_cm.max(0);
        if conflicts_with_neighbor_fragments(
            frag.pos,
            fragment_radius_cm,
            spacing_cm,
            &neighbor_fragments,
        ) {
            continue;
        }
        if conflicts_with_boundary_reservations(
            frag.pos,
            fragment_radius_cm,
            spacing_cm,
            &incoming_boundary_reservations,
        ) {
            continue;
        }

        let profile_seed = seed.wrapping_add((idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut fragment_profile = synthesize_fragment_profile(
            profile_seed,
            frag.profile.radius_cm,
            frag.profile.material,
        );

        let remaining_chunk_blocks = max_blocks_per_chunk.saturating_sub(accepted_block_count);
        if remaining_chunk_blocks == 0 {
            break;
        }
        let block_limit_for_fragment = max_blocks_per_fragment.min(remaining_chunk_blocks);
        if block_limit_for_fragment == 0 {
            break;
        }
        truncate_fragment_profile_blocks(&mut fragment_profile, block_limit_for_fragment);
        let kept_blocks = fragment_profile.blocks.blocks.len();
        if kept_blocks == 0 {
            continue;
        }

        let fragment_budget = synthesize_fragment_budget(&fragment_profile);
        chunk_budget.accumulate_fragment(&fragment_budget);
        frag.fragment_profile = Some(fragment_profile);
        frag.fragment_budget = Some(fragment_budget);

        accepted_fragment_count = accepted_fragment_count.saturating_add(1);
        accepted_block_count = accepted_block_count.saturating_add(kept_blocks);

        accepted_fragments.push(AcceptedFragment {
            fragment_id: frag.id.clone(),
            pos: frag.pos,
            radius_cm: fragment_radius_cm,
        });
        insert_location(model, frag)?;
    }

    model.chunks.insert(coord, ChunkState::Generated);
    model.chunk_resource_budgets.insert(coord, chunk_budget);

    for fragment in accepted_fragments {
        append_boundary_reservations_for_fragment(model, config, coord, fragment, spacing_cm);
    }

    Ok(summarize_chunk_generation(model, config, coord, seed))
}

#[derive(Debug, Clone)]
struct NeighborFragment {
    fragment_id: LocationId,
    source_chunk: ChunkCoord,
    pos: GeoPos,
    radius_cm: i64,
}

#[derive(Debug, Clone)]
struct AcceptedFragment {
    fragment_id: LocationId,
    pos: GeoPos,
    radius_cm: i64,
}

fn gather_neighbor_fragments(
    model: &WorldModel,
    space: &SpaceConfig,
    coord: ChunkCoord,
) -> Vec<NeighborFragment> {
    let neighbors = neighboring_chunk_coords(coord);
    model
        .locations
        .values()
        .filter(|location| location.id.starts_with("frag-"))
        .filter_map(|location| {
            let source_chunk = chunk_coord_of(location.pos, space)?;
            if !neighbors.contains(&source_chunk) {
                return None;
            }
            Some(NeighborFragment {
                fragment_id: location.id.clone(),
                source_chunk,
                pos: location.pos,
                radius_cm: location.profile.radius_cm.max(0),
            })
        })
        .collect()
}

fn neighboring_chunk_coords(coord: ChunkCoord) -> Vec<ChunkCoord> {
    let mut out = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                out.push(ChunkCoord {
                    x: coord.x + dx,
                    y: coord.y + dy,
                    z: coord.z + dz,
                });
            }
        }
    }
    out
}

fn conflicts_with_neighbor_fragments(
    pos: GeoPos,
    radius_cm: i64,
    spacing_cm: i64,
    neighbors: &[NeighborFragment],
) -> bool {
    neighbors.iter().any(|neighbor| {
        let _tie_break_hint = (&neighbor.source_chunk, &neighbor.fragment_id);
        spacing_conflict(pos, radius_cm, neighbor.pos, neighbor.radius_cm, spacing_cm)
    })
}

fn conflicts_with_boundary_reservations(
    pos: GeoPos,
    radius_cm: i64,
    spacing_cm: i64,
    reservations: &[BoundaryReservation],
) -> bool {
    reservations.iter().any(|reservation| {
        let required_spacing = spacing_cm.max(reservation.min_spacing_cm.max(0));
        spacing_conflict(
            pos,
            radius_cm,
            reservation.source_pos,
            reservation.source_radius_cm.max(0),
            required_spacing,
        )
    })
}

fn spacing_conflict(
    a_pos: GeoPos,
    a_radius_cm: i64,
    b_pos: GeoPos,
    b_radius_cm: i64,
    spacing_cm: i64,
) -> bool {
    let dx = a_pos.x_cm - b_pos.x_cm;
    let dy = a_pos.y_cm - b_pos.y_cm;
    let dz = a_pos.z_cm - b_pos.z_cm;
    let min_dist = a_radius_cm.max(0) + b_radius_cm.max(0) + spacing_cm.max(0);
    (dx * dx + dy * dy + dz * dz) < (min_dist * min_dist)
}

fn append_boundary_reservations_for_fragment(
    model: &mut WorldModel,
    config: &WorldConfig,
    source_chunk: ChunkCoord,
    fragment: AcceptedFragment,
    spacing_cm: i64,
) {
    let influence_distance_cm = (fragment.radius_cm.max(0) + spacing_cm.max(0)) as f64;

    for neighbor in neighboring_chunk_coords(source_chunk) {
        let Some(state) = model.chunks.get(&neighbor) else {
            continue;
        };
        if !matches!(state, ChunkState::Unexplored) {
            continue;
        }
        let Some(neighbor_bounds) = super::chunking::chunk_bounds(neighbor, &config.space) else {
            continue;
        };
        if point_to_chunk_distance_cm(fragment.pos, neighbor_bounds) > influence_distance_cm {
            continue;
        }

        let reservation = BoundaryReservation {
            source_chunk,
            source_fragment_id: fragment.fragment_id.clone(),
            source_pos: fragment.pos,
            source_radius_cm: fragment.radius_cm,
            min_spacing_cm: spacing_cm.max(0),
        };
        let entry = model
            .chunk_boundary_reservations
            .entry(neighbor)
            .or_default();
        entry.push(reservation);
        entry.sort_by(|left, right| {
            (left.source_chunk, left.source_fragment_id.as_str())
                .cmp(&(right.source_chunk, right.source_fragment_id.as_str()))
        });
    }
}

fn point_to_chunk_distance_cm(pos: GeoPos, bounds: super::chunking::ChunkBounds) -> f64 {
    let dx = if pos.x_cm < bounds.min.x_cm {
        bounds.min.x_cm - pos.x_cm
    } else if pos.x_cm > bounds.max.x_cm {
        pos.x_cm - bounds.max.x_cm
    } else {
        0
    };
    let dy = if pos.y_cm < bounds.min.y_cm {
        bounds.min.y_cm - pos.y_cm
    } else if pos.y_cm > bounds.max.y_cm {
        pos.y_cm - bounds.max.y_cm
    } else {
        0
    };
    let dz = if pos.z_cm < bounds.min.z_cm {
        bounds.min.z_cm - pos.z_cm
    } else if pos.z_cm > bounds.max.z_cm {
        pos.z_cm - bounds.max.z_cm
    } else {
        0
    };
    ((dx * dx + dy * dy + dz * dz) as f64).sqrt()
}

fn chunk_local_space(bounds: super::chunking::ChunkBounds) -> SpaceConfig {
    SpaceConfig {
        width_cm: bounds.max.x_cm - bounds.min.x_cm,
        depth_cm: bounds.max.y_cm - bounds.min.y_cm,
        height_cm: bounds.max.z_cm - bounds.min.z_cm,
    }
}

fn initialize_chunk_index(model: &mut WorldModel, config: &WorldConfig) {
    for coord in chunk_coords(&config.space) {
        model.chunks.insert(coord, ChunkState::Unexplored);
    }
}

fn gather_seed_positions(model: &WorldModel) -> Vec<GeoPos> {
    let mut positions: Vec<GeoPos> = model
        .locations
        .values()
        .map(|location| location.pos)
        .collect();
    positions.extend(model.agents.values().map(|agent| agent.pos));
    positions
}

pub fn initialize_kernel(
    config: WorldConfig,
    init: WorldInitConfig,
) -> Result<(WorldKernel, WorldInitReport), WorldInitError> {
    let (model, report) = build_world_model(&config, &init)?;
    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: init.seed,
        asteroid_fragment_enabled: init.asteroid_fragment.enabled,
        asteroid_fragment_seed_offset: init.asteroid_fragment.seed_offset,
        min_fragment_spacing_cm: init.asteroid_fragment.min_fragment_spacing_cm,
    };
    let mut kernel = WorldKernel::with_model_and_chunk_runtime(config, model, chunk_runtime);

    if init.asteroid_fragment.enabled {
        let base_seed = init.seed.wrapping_add(init.asteroid_fragment.seed_offset);
        let generated_coords = kernel
            .model()
            .chunks
            .iter()
            .filter_map(|(coord, state)| {
                if matches!(state, ChunkState::Generated | ChunkState::Exhausted) {
                    Some(*coord)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for coord in generated_coords {
            let seed = super::chunking::chunk_seed(base_seed, coord);
            let summary = summarize_chunk_generation(kernel.model(), kernel.config(), coord, seed);
            kernel.record_event(WorldEventKind::ChunkGenerated {
                coord: summary.coord,
                seed: summary.seed,
                fragment_count: summary.fragment_count,
                block_count: summary.block_count,
                chunk_budget: summary.chunk_budget,
                cause: ChunkGenerationCause::Init,
            });
        }
    }

    Ok((kernel, report))
}

fn center_pos(space: &super::world_model::SpaceConfig) -> GeoPos {
    GeoPos::new(space.width_cm / 2, space.depth_cm / 2, space.height_cm / 2)
}

fn insert_location(model: &mut WorldModel, location: Location) -> Result<(), WorldInitError> {
    if model.locations.contains_key(&location.id) {
        return Err(WorldInitError::LocationIdConflict {
            location_id: location.id,
        });
    }
    model.locations.insert(location.id.clone(), location);
    Ok(())
}

fn insert_agent(model: &mut WorldModel, agent: Agent) -> Result<(), WorldInitError> {
    if model.agents.contains_key(&agent.id) {
        return Err(WorldInitError::AgentIdConflict { agent_id: agent.id });
    }
    model.agents.insert(agent.id.clone(), agent);
    Ok(())
}

fn ensure_valid_stock(stock: &ResourceStock) -> Result<(), WorldInitError> {
    for (kind, amount) in &stock.amounts {
        if *amount < 0 {
            return Err(WorldInitError::InvalidResourceAmount {
                kind: *kind,
                amount: *amount,
            });
        }
    }
    Ok(())
}

fn strip_location_electricity(stock: &mut ResourceStock) {
    stock.amounts.remove(&ResourceKind::Electricity);
}

fn ensure_owner_exists(model: &WorldModel, owner: &ResourceOwner) -> Result<(), WorldInitError> {
    match owner {
        ResourceOwner::Agent { agent_id } => {
            if model.agents.contains_key(agent_id) {
                Ok(())
            } else {
                Err(WorldInitError::FacilityOwnerNotFound {
                    owner: owner.clone(),
                })
            }
        }
        ResourceOwner::Location { location_id } => {
            if model.locations.contains_key(location_id) {
                Ok(())
            } else {
                Err(WorldInitError::FacilityOwnerNotFound {
                    owner: owner.clone(),
                })
            }
        }
    }
}

fn ensure_non_negative_amount(field: &str, amount: i64) -> Result<(), WorldInitError> {
    if amount < 0 {
        return Err(WorldInitError::InvalidFacilityAmount {
            field: field.to_string(),
            amount,
        });
    }
    Ok(())
}

fn ensure_valid_ratio(field: &str, value: f64) -> Result<(), WorldInitError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(WorldInitError::InvalidFacilityRatio {
            field: field.to_string(),
            value,
        });
    }
    Ok(())
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn insert_power_plant(model: &mut WorldModel, plant: PowerPlant) -> Result<(), WorldInitError> {
    if model.power_plants.contains_key(&plant.id) {
        return Err(WorldInitError::FacilityIdConflict {
            facility_id: plant.id,
        });
    }
    model.power_plants.insert(plant.id.clone(), plant);
    Ok(())
}
