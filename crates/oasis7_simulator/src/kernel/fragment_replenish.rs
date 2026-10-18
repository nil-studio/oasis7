use crate::geometry::GeoPos;

use super::super::asteroid_fragment::generate_fragments;
use super::super::chunking::{ChunkBounds, ChunkCoord, chunk_bounds, chunk_coord_of, chunk_seed};
use super::super::fragment_physics::{synthesize_fragment_budget, synthesize_fragment_profile};
use super::super::types::PPM_BASE;
use super::super::world_model::{ChunkState, SpaceConfig, WorldModel};
use super::{FragmentReplenishedEntry, WorldEventKind, WorldKernel};

const PROFILE_SEED_STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

impl WorldKernel {
    pub(super) fn maybe_replenish_fragments(&mut self) {
        if !self.chunk_runtime.asteroid_fragment_enabled {
            return;
        }

        let config = self.config.asteroid_fragment.clone();
        let interval_ticks = config.replenish_interval_ticks.max(0) as u64;
        if interval_ticks == 0 || self.time == 0 || !self.time.is_multiple_of(interval_ticks) {
            return;
        }

        let max_fragments_per_chunk = config.max_fragments_per_chunk.max(0) as usize;
        if max_fragments_per_chunk == 0 || config.replenish_percent_ppm <= 0 {
            return;
        }

        let coords: Vec<ChunkCoord> = self
            .model
            .chunks
            .iter()
            .filter_map(|(coord, state)| {
                if matches!(state, ChunkState::Generated | ChunkState::Exhausted) {
                    Some(*coord)
                } else {
                    None
                }
            })
            .collect();

        let mut entries = Vec::new();
        for coord in coords {
            let Some(bounds) = chunk_bounds(coord, &self.config.space) else {
                continue;
            };
            let current = count_chunk_fragments(&self.model, &self.config.space, coord);
            if current >= max_fragments_per_chunk {
                continue;
            }

            let missing = max_fragments_per_chunk.saturating_sub(current);
            if missing == 0 {
                continue;
            }

            let replenish_target = compute_replenish_target(
                max_fragments_per_chunk,
                config.replenish_percent_ppm,
                missing,
            );
            if replenish_target == 0 {
                continue;
            }

            let base_seed = self
                .chunk_runtime
                .asteroid_fragment_seed()
                .wrapping_add(self.time)
                .wrapping_add(0xA11C_E5E7_5EED_1234);
            let replenish_seed = chunk_seed(base_seed, coord);

            let mut replenish_config = config.clone();
            replenish_config.min_fragments_per_chunk = replenish_target as i64;
            replenish_config.max_fragments_per_chunk = max_fragments_per_chunk as i64;

            let local_space = chunk_local_space(bounds);
            if local_space.width_cm <= 0 || local_space.depth_cm <= 0 || local_space.height_cm <= 0
            {
                continue;
            }

            let candidates = generate_fragments(replenish_seed, &local_space, &replenish_config);
            if candidates.is_empty() {
                continue;
            }

            let mut placements =
                gather_neighboring_fragment_placements(&self.model, &self.config.space, coord);
            let spacing_cm = config.min_fragment_spacing_cm.max(0);
            let mut accepted = 0usize;
            let mut sequence = 0usize;

            for (idx, mut candidate) in candidates.into_iter().enumerate() {
                if accepted >= replenish_target {
                    break;
                }

                candidate.pos.x_cm += bounds.min.x_cm;
                candidate.pos.y_cm += bounds.min.y_cm;
                candidate.pos.z_cm += bounds.min.z_cm;

                let radius_cm = candidate.profile.radius_cm.max(0);
                if conflicts_with_existing_fragments(
                    candidate.pos,
                    radius_cm,
                    spacing_cm,
                    &placements,
                ) {
                    continue;
                }

                let profile_seed =
                    replenish_seed.wrapping_add((idx as u64).wrapping_mul(PROFILE_SEED_STRIDE));
                let fragment_profile = synthesize_fragment_profile(
                    profile_seed,
                    candidate.profile.radius_cm,
                    candidate.profile.material,
                );
                let fragment_budget = synthesize_fragment_budget(&fragment_profile);
                candidate.fragment_profile = Some(fragment_profile);
                candidate.fragment_budget = Some(fragment_budget);
                candidate.id = next_replenish_fragment_id(&self.model, coord, self.time, sequence);
                candidate.name = candidate.id.clone();

                placements.push((candidate.pos, radius_cm));
                entries.push(FragmentReplenishedEntry {
                    coord,
                    location: candidate,
                });
                accepted = accepted.saturating_add(1);
                sequence = sequence.saturating_add(1);
            }
        }

        if entries.is_empty() {
            return;
        }
        if self.apply_fragment_replenished_entries(&entries).is_ok() {
            self.record_event(WorldEventKind::FragmentsReplenished { entries });
        }
    }

    pub(super) fn apply_fragment_replenished_entries(
        &mut self,
        entries: &[FragmentReplenishedEntry],
    ) -> Result<(), String> {
        for entry in entries {
            if self.model.locations.contains_key(&entry.location.id) {
                return Err(format!(
                    "fragment replenish location already exists: {}",
                    entry.location.id
                ));
            }

            self.model
                .chunks
                .entry(entry.coord)
                .or_insert(ChunkState::Generated);
            self.model
                .locations
                .insert(entry.location.id.clone(), entry.location.clone());

            let chunk_budget = self
                .model
                .chunk_resource_budgets
                .entry(entry.coord)
                .or_default();
            if let Some(fragment_budget) = &entry.location.fragment_budget {
                chunk_budget.accumulate_fragment(fragment_budget);
            }

            if !chunk_budget.is_exhausted() {
                self.model.chunks.insert(entry.coord, ChunkState::Generated);
            }
        }
        Ok(())
    }
}

pub(super) fn compute_replenish_target(
    max_fragments_per_chunk: usize,
    percent_ppm: i64,
    missing: usize,
) -> usize {
    if max_fragments_per_chunk == 0 || percent_ppm <= 0 {
        return 0;
    }
    let ceil_target = ((max_fragments_per_chunk as i64)
        .saturating_mul(percent_ppm)
        .saturating_add(PPM_BASE - 1)
        .saturating_div(PPM_BASE)) as usize;
    ceil_target.max(1).min(missing)
}

pub(super) fn count_chunk_fragments(
    model: &WorldModel,
    space: &SpaceConfig,
    coord: ChunkCoord,
) -> usize {
    model
        .locations
        .values()
        .filter(|location| {
            location.id.starts_with("frag-")
                && chunk_coord_of(location.pos, space).is_some_and(|chunk| chunk == coord)
        })
        .count()
}

fn gather_neighboring_fragment_placements(
    model: &WorldModel,
    space: &SpaceConfig,
    coord: ChunkCoord,
) -> Vec<(GeoPos, i64)> {
    let mut relevant_coords = neighboring_chunk_coords(coord);
    relevant_coords.push(coord);

    model
        .locations
        .values()
        .filter(|location| location.id.starts_with("frag-"))
        .filter_map(|location| {
            let chunk = chunk_coord_of(location.pos, space)?;
            if !relevant_coords.contains(&chunk) {
                return None;
            }
            Some((location.pos, location.profile.radius_cm.max(0)))
        })
        .collect()
}

fn conflicts_with_existing_fragments(
    pos: GeoPos,
    radius_cm: i64,
    spacing_cm: i64,
    existing: &[(GeoPos, i64)],
) -> bool {
    existing.iter().any(|(other_pos, other_radius_cm)| {
        spacing_conflict(pos, radius_cm, *other_pos, *other_radius_cm, spacing_cm)
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

fn chunk_local_space(bounds: ChunkBounds) -> SpaceConfig {
    SpaceConfig {
        width_cm: (bounds.max.x_cm - bounds.min.x_cm).max(0),
        depth_cm: (bounds.max.y_cm - bounds.min.y_cm).max(0),
        height_cm: (bounds.max.z_cm - bounds.min.z_cm).max(0),
    }
}

fn next_replenish_fragment_id(
    model: &WorldModel,
    coord: ChunkCoord,
    time: u64,
    sequence: usize,
) -> String {
    let base = format!(
        "frag-{}-{}-{}-r{}-{}",
        coord.x, coord.y, coord.z, time, sequence
    );
    if !model.locations.contains_key(&base) {
        return base;
    }

    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !model.locations.contains_key(&candidate) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}
