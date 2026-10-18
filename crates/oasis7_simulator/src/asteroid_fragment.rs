//! Asteroid fragment belt generation utilities.

use crate::geometry::GeoPos;

use super::types::LocationProfile;
use super::world_model::{
    AsteroidFragmentConfig, Location, MaterialDistributionStrategy, MaterialWeights, SpaceConfig,
};

const MAX_PLACEMENT_ATTEMPTS: usize = 8;
const MAX_BACKFILL_ATTEMPTS_PER_FRAGMENT: usize = 24;

pub fn generate_fragments(
    seed: u64,
    space: &SpaceConfig,
    config: &AsteroidFragmentConfig,
) -> Vec<Location> {
    let mut rng = Lcg::new(seed);
    let voxel_cm = config.voxel_size_km.max(1) * 100_000;
    let voxels_x = ((space.width_cm + voxel_cm - 1) / voxel_cm).max(1);
    let voxels_y = ((space.depth_cm + voxel_cm - 1) / voxel_cm).max(1);
    let voxels_z = ((space.height_cm + voxel_cm - 1) / voxel_cm).max(1);

    let voxel_volume_km3 = (config.voxel_size_km as f64).powi(3).max(1e-6);

    let mut locations = Vec::new();
    let mut placements = Vec::new();
    let mut idx = 0usize;
    let mid_z_cm = space.height_cm as f64 / 2.0;
    let min_spacing_cm = config.min_fragment_spacing_cm.max(0) as f64;
    let core_radius_ratio = config.starter_core_radius_ratio.clamp(0.0, 1.0);
    let core_density_multiplier = config.starter_core_density_multiplier.max(1.0);
    let chunk_center_x_cm = space.width_cm as f64 / 2.0;
    let chunk_center_y_cm = space.depth_cm as f64 / 2.0;
    let max_planar_distance_cm = chunk_center_x_cm.hypot(chunk_center_y_cm).max(1.0);

    for ix in 0..voxels_x {
        let voxel_min_x = ix * voxel_cm;
        let voxel_max_x = ((ix + 1) * voxel_cm).min(space.width_cm);
        for iy in 0..voxels_y {
            let voxel_min_y = iy * voxel_cm;
            let voxel_max_y = ((iy + 1) * voxel_cm).min(space.depth_cm);
            for iz in 0..voxels_z {
                let voxel_min_z = iz * voxel_cm;
                let voxel_max_z = ((iz + 1) * voxel_cm).min(space.height_cm);
                let z_cm = (voxel_min_z as f64 + voxel_max_z as f64) / 2.0;
                let z_km = ((z_cm - mid_z_cm).abs() / 100_000.0).max(0.0);
                let layer = if config.layer_scale_height_km > 0.0 {
                    (-z_km / config.layer_scale_height_km).exp()
                } else {
                    1.0
                };

                let voxel_center_x = (voxel_min_x as f64 + voxel_max_x as f64) / 2.0;
                let voxel_center_y = (voxel_min_y as f64 + voxel_max_y as f64) / 2.0;
                let planar_distance_ratio = (voxel_center_x - chunk_center_x_cm)
                    .hypot(voxel_center_y - chunk_center_y_cm)
                    / max_planar_distance_cm;
                let core_density = if planar_distance_ratio <= core_radius_ratio {
                    core_density_multiplier
                } else {
                    1.0
                };

                let noise = (rng.next_f64() * 2.0 - 1.0) * config.cluster_noise;
                let density =
                    (config.base_density_per_km3 * (1.0 + noise).max(0.0)) * layer * core_density;
                let lambda = density * voxel_volume_km3;
                let count = sample_poisson(&mut rng, lambda);

                for _ in 0..count {
                    let placed = try_place_fragment(
                        &mut rng,
                        (voxel_min_x as f64, voxel_max_x as f64),
                        (voxel_min_y as f64, voxel_max_y as f64),
                        (voxel_min_z as f64, voxel_max_z as f64),
                        config,
                        &placements,
                        min_spacing_cm,
                        chunk_center_x_cm,
                        chunk_center_y_cm,
                        max_planar_distance_cm,
                        idx,
                    );
                    if let Some((location, placement)) = placed {
                        locations.push(location);
                        placements.push(placement);
                        idx += 1;
                    }
                }
            }
        }
    }

    let min_fragments_per_chunk = config.min_fragments_per_chunk.max(0) as usize;
    let max_fragments_per_chunk = config.max_fragments_per_chunk.max(0) as usize;
    let floor_target = if max_fragments_per_chunk == 0 {
        0
    } else {
        min_fragments_per_chunk.min(max_fragments_per_chunk)
    };
    if locations.len() < floor_target {
        let missing = floor_target - locations.len();
        let max_backfill_attempts = missing.saturating_mul(MAX_BACKFILL_ATTEMPTS_PER_FRAGMENT);
        for _ in 0..max_backfill_attempts {
            if locations.len() >= floor_target {
                break;
            }
            let placed = try_place_fragment(
                &mut rng,
                (0.0, space.width_cm as f64),
                (0.0, space.depth_cm as f64),
                (0.0, space.height_cm as f64),
                config,
                &placements,
                min_spacing_cm,
                chunk_center_x_cm,
                chunk_center_y_cm,
                max_planar_distance_cm,
                idx,
            );
            if let Some((location, placement)) = placed {
                locations.push(location);
                placements.push(placement);
                idx += 1;
            }
        }
    }

    locations
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable fragment-generation seam keeps deterministic placement inputs explicit."
)]
fn try_place_fragment(
    rng: &mut Lcg,
    x_range: (f64, f64),
    y_range: (f64, f64),
    z_range: (f64, f64),
    config: &AsteroidFragmentConfig,
    placements: &[(GeoPos, i64)],
    min_spacing_cm: f64,
    chunk_center_x_cm: f64,
    chunk_center_y_cm: f64,
    max_planar_distance_cm: f64,
    idx: usize,
) -> Option<(Location, (GeoPos, i64))> {
    for _ in 0..MAX_PLACEMENT_ATTEMPTS {
        let x = sample_in_range(rng, x_range.0, x_range.1);
        let y = sample_in_range(rng, y_range.0, y_range.1);
        let z = sample_in_range(rng, z_range.0, z_range.1);
        let radius_cm = sample_power_law(
            rng,
            config.radius_min_cm.max(1) as f64,
            config.radius_max_cm.max(1) as f64,
            config.size_powerlaw_q.max(1.0),
        );
        let radius_cm = radius_cm.round().max(1.0) as i64;
        let pos = GeoPos::new(x.round() as i64, y.round() as i64, z.round() as i64);
        if min_spacing_cm > 0.0 && !spacing_allows(&pos, radius_cm, placements, min_spacing_cm) {
            continue;
        }

        let planar_distance_ratio =
            (x - chunk_center_x_cm).hypot(y - chunk_center_y_cm) / max_planar_distance_cm;
        let material_weights = material_weights_for_ratio(config, planar_distance_ratio);
        let total_weights = material_weights.total().max(1);
        let roll = rng.next_u32() % total_weights;
        let material = material_weights.pick(roll);
        let material_factor = config.material_radiation_factors.factor_for(material);
        let emission = estimate_radiation_emission(
            radius_cm as f64,
            material_factor,
            config.radiation_emission_scale,
            config.radiation_radius_exponent,
        );
        let profile = LocationProfile {
            material,
            radius_cm,
            radiation_emission_per_tick: emission,
        };
        let location_id = format!("frag-{idx}");
        let name = location_id.clone();
        let location = Location::new_with_profile(location_id, name, pos, profile);
        return Some((location, (pos, radius_cm)));
    }
    None
}

fn sample_in_range(rng: &mut Lcg, start: f64, end: f64) -> f64 {
    let min = start.min(end);
    let max = start.max(end);
    let width = max - min;
    if width <= f64::EPSILON {
        return min;
    }
    min + rng.next_f64() * width
}

fn material_weights_for_ratio(config: &AsteroidFragmentConfig, ratio: f64) -> MaterialWeights {
    match config.material_distribution_strategy {
        MaterialDistributionStrategy::Uniform => config.material_weights,
        MaterialDistributionStrategy::CoreMetalRimVolatile => zoned_weights(
            config.material_weights,
            ratio,
            config.starter_core_radius_ratio,
        ),
    }
}

fn zoned_weights(base: MaterialWeights, ratio: f64, core_radius_ratio: f64) -> MaterialWeights {
    let core_ratio = core_radius_ratio.clamp(0.0, 1.0);
    let mid_ratio = (core_ratio + 1.0) / 2.0;
    let r = ratio.clamp(0.0, 1.0);

    if r <= core_ratio {
        MaterialWeights {
            silicate: scale_weight(base.silicate, 9_000),
            metal: scale_weight(base.metal, 17_000),
            ice: scale_weight(base.ice, 7_000),
            carbon: scale_weight(base.carbon, 7_000),
            composite: scale_weight(base.composite, 15_000),
        }
        .sanitized()
    } else if r <= mid_ratio {
        MaterialWeights {
            silicate: scale_weight(base.silicate, 10_000),
            metal: scale_weight(base.metal, 12_000),
            ice: scale_weight(base.ice, 9_000),
            carbon: scale_weight(base.carbon, 9_000),
            composite: scale_weight(base.composite, 11_500),
        }
        .sanitized()
    } else {
        MaterialWeights {
            silicate: scale_weight(base.silicate, 10_000),
            metal: scale_weight(base.metal, 7_000),
            ice: scale_weight(base.ice, 16_000),
            carbon: scale_weight(base.carbon, 16_000),
            composite: scale_weight(base.composite, 8_000),
        }
        .sanitized()
    }
}

fn scale_weight(weight: u32, bps: u32) -> u32 {
    if weight == 0 || bps == 0 {
        return 0;
    }
    ((weight as u64)
        .saturating_mul(bps as u64)
        .saturating_add(9_999)
        / 10_000) as u32
}

fn spacing_allows(
    pos: &GeoPos,
    radius_cm: i64,
    existing: &[(GeoPos, i64)],
    min_spacing_cm: f64,
) -> bool {
    if min_spacing_cm <= 0.0 {
        return true;
    }
    let radius_cm = radius_cm as f64;
    for (other_pos, other_radius_cm) in existing {
        let dx = pos.x_cm - other_pos.x_cm;
        let dy = pos.y_cm - other_pos.y_cm;
        let dz = pos.z_cm - other_pos.z_cm;
        let min_dist = radius_cm + (*other_radius_cm as f64) + min_spacing_cm;
        if ((dx * dx + dy * dy + dz * dz) as f64) < (min_dist * min_dist) {
            return false;
        }
    }
    true
}

fn estimate_radiation_emission(
    radius_cm: f64,
    material_factor: f64,
    emission_scale: f64,
    radius_exponent: f64,
) -> i64 {
    let radius_term = radius_cm.max(1.0).powf(radius_exponent.max(0.0));
    let base = radius_term * emission_scale.max(0.0) * material_factor.max(0.0);
    base.round().max(1.0) as i64
}

fn sample_power_law(rng: &mut Lcg, r_min: f64, r_max: f64, q: f64) -> f64 {
    if (q - 1.0).abs() < f64::EPSILON {
        let u = rng.next_f64().max(1e-9);
        return (r_min.ln() + u * (r_max.ln() - r_min.ln())).exp();
    }
    let u = rng.next_f64().max(1e-9);
    let one_minus_q = 1.0 - q;
    let min_term = r_min.powf(one_minus_q);
    let max_term = r_max.powf(one_minus_q);
    let value = min_term + u * (max_term - min_term);
    value.max(0.0).powf(1.0 / one_minus_q)
}

fn sample_poisson(rng: &mut Lcg, lambda: f64) -> usize {
    if lambda <= 0.0 {
        return 0;
    }
    let l = (-lambda).exp();
    let mut k = 0usize;
    let mut p = 1.0;
    while p > l {
        k += 1;
        p *= rng.next_f64();
    }
    k.saturating_sub(1)
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    fn next_f64(&mut self) -> f64 {
        let val = (self.next_u64() >> 11) as f64;
        val / ((1u64 << 53) as f64)
    }
}
