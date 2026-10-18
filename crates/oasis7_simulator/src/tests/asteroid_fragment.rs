use super::*;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_generator_produces_locations_within_bounds() {
    let space = SpaceConfig {
        width_cm: 100_000,
        depth_cm: 100_000,
        height_cm: 100_000,
    };
    let mut asteroid_fragment = AsteroidFragmentConfig::default();
    asteroid_fragment.base_density_per_km3 = 50.0;
    asteroid_fragment.voxel_size_km = 1;
    asteroid_fragment.cluster_noise = 0.0;
    asteroid_fragment.layer_scale_height_km = 0.0;
    asteroid_fragment.radius_min_cm = 10;
    asteroid_fragment.radius_max_cm = 10;

    let fragments = generate_fragments(42, &space, &asteroid_fragment);
    assert!(!fragments.is_empty());

    for frag in fragments {
        assert!(space.contains(frag.pos));
        assert_eq!(frag.profile.radius_cm, 10);
        assert!(frag.profile.radiation_emission_per_tick > 0);
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_generator_respects_min_fragment_spacing() {
    let space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    let mut asteroid_fragment = AsteroidFragmentConfig::default();
    asteroid_fragment.base_density_per_km3 = 5.0;
    asteroid_fragment.voxel_size_km = 1;
    asteroid_fragment.cluster_noise = 0.0;
    asteroid_fragment.layer_scale_height_km = 0.0;
    asteroid_fragment.radius_min_cm = 10;
    asteroid_fragment.radius_max_cm = 10;
    asteroid_fragment.min_fragment_spacing_cm = 50_000;

    let fragments = generate_fragments(7, &space, &asteroid_fragment);
    assert!(fragments.len() > 1);

    for i in 0..fragments.len() {
        for j in (i + 1)..fragments.len() {
            let a = &fragments[i];
            let b = &fragments[j];
            let dx = a.pos.x_cm - b.pos.x_cm;
            let dy = a.pos.y_cm - b.pos.y_cm;
            let dz = a.pos.z_cm - b.pos.z_cm;
            let min_dist = (a.profile.radius_cm
                + b.profile.radius_cm
                + asteroid_fragment.min_fragment_spacing_cm) as f64;
            assert!(((dx * dx + dy * dy + dz * dz) as f64) >= (min_dist * min_dist));
        }
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_emission_scales_with_radius_exponent() {
    let space = SpaceConfig {
        width_cm: 20_000,
        depth_cm: 20_000,
        height_cm: 20_000,
    };

    let mut small = AsteroidFragmentConfig::default();
    small.base_density_per_km3 = 10.0;
    small.voxel_size_km = 1;
    small.cluster_noise = 0.0;
    small.layer_scale_height_km = 0.0;
    small.radius_min_cm = 100;
    small.radius_max_cm = 100;
    small.min_fragment_spacing_cm = 0;
    small.min_fragments_per_chunk = 0;
    small.starter_core_radius_ratio = 0.0;
    small.starter_core_density_multiplier = 1.0;
    small.radiation_emission_scale = 1e-6;
    small.radiation_radius_exponent = 3.0;

    let mut large = small.clone();
    large.radius_min_cm = 200;
    large.radius_max_cm = 200;

    let small_frags = generate_fragments(11, &space, &small);
    let large_frags = generate_fragments(11, &space, &large);

    assert!(!small_frags.is_empty());
    assert!(!large_frags.is_empty());

    let small_emission = small_frags[0].profile.radiation_emission_per_tick;
    let large_emission = large_frags[0].profile.radiation_emission_per_tick;

    assert!(large_emission >= small_emission * 7);
}

#[test]
fn asteroid_fragment_default_mix_is_conservative_for_high_radiation_materials() {
    let config = AsteroidFragmentConfig::default();
    let total = config.material_weights.total();
    assert!(total > 0);

    let high_radiation_share = config.material_weights.metal + config.material_weights.composite;
    assert!(high_radiation_share * 100 <= total * 15);
    assert!(config.radiation_emission_scale <= 1e-12);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_default_calibration_keeps_small_silicate_non_extreme() {
    let space = SpaceConfig {
        width_cm: 100_000,
        depth_cm: 100_000,
        height_cm: 100_000,
    };
    let mut config = AsteroidFragmentConfig::default();
    config.base_density_per_km3 = 20.0;
    config.voxel_size_km = 1;
    config.cluster_noise = 0.0;
    config.layer_scale_height_km = 0.0;
    config.min_fragment_spacing_cm = 0;
    config.radius_min_cm = 25_000;
    config.radius_max_cm = 25_000;
    config.material_weights = MaterialWeights {
        silicate: 1,
        metal: 0,
        ice: 0,
        carbon: 0,
        composite: 0,
    };

    let fragments = generate_fragments(17, &space, &config);
    assert!(!fragments.is_empty());

    let emission = fragments[0].profile.radiation_emission_per_tick;
    assert!(emission > 0);
    assert!(emission <= 50);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_default_calibration_preserves_high_radiation_outliers() {
    let space = SpaceConfig {
        width_cm: 100_000,
        depth_cm: 100_000,
        height_cm: 100_000,
    };
    let mut config = AsteroidFragmentConfig::default();
    config.base_density_per_km3 = 20.0;
    config.voxel_size_km = 1;
    config.cluster_noise = 0.0;
    config.layer_scale_height_km = 0.0;
    config.min_fragment_spacing_cm = 0;
    config.radius_min_cm = 500_000;
    config.radius_max_cm = 500_000;
    config.material_weights = MaterialWeights {
        silicate: 0,
        metal: 1,
        ice: 0,
        carbon: 0,
        composite: 0,
    };

    let fragments = generate_fragments(29, &space, &config);
    assert!(!fragments.is_empty());

    let emission = fragments[0].profile.radiation_emission_per_tick;
    assert!(emission > 50);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_generator_enforces_min_fragments_floor() {
    let space = SpaceConfig {
        width_cm: 100_000,
        depth_cm: 100_000,
        height_cm: 100_000,
    };
    let mut config = AsteroidFragmentConfig::default();
    config.base_density_per_km3 = 0.0;
    config.voxel_size_km = 1;
    config.cluster_noise = 0.0;
    config.layer_scale_height_km = 0.0;
    config.radius_min_cm = 100;
    config.radius_max_cm = 100;
    config.min_fragment_spacing_cm = 0;
    config.min_fragments_per_chunk = 5;
    config.max_fragments_per_chunk = 10;

    let fragments = generate_fragments(123, &space, &config);
    assert!(fragments.len() >= 5);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_generator_biases_distribution_toward_core_zone() {
    let space = SpaceConfig {
        width_cm: 400_000,
        depth_cm: 400_000,
        height_cm: 100_000,
    };
    let mut baseline = AsteroidFragmentConfig::default();
    baseline.base_density_per_km3 = 20.0;
    baseline.voxel_size_km = 1;
    baseline.cluster_noise = 0.0;
    baseline.layer_scale_height_km = 0.0;
    baseline.radius_min_cm = 100;
    baseline.radius_max_cm = 100;
    baseline.min_fragment_spacing_cm = 0;
    baseline.min_fragments_per_chunk = 0;
    baseline.starter_core_radius_ratio = 0.5;
    baseline.starter_core_density_multiplier = 1.0;

    let mut boosted = baseline.clone();
    boosted.starter_core_density_multiplier = 6.0;

    let baseline_fragments = generate_fragments(99, &space, &baseline);
    let boosted_fragments = generate_fragments(99, &space, &boosted);
    assert!(!baseline_fragments.is_empty());
    assert!(!boosted_fragments.is_empty());

    let baseline_core = count_fragments_in_core_zone(
        &baseline_fragments,
        &space,
        baseline.starter_core_radius_ratio,
    );
    let boosted_core = count_fragments_in_core_zone(
        &boosted_fragments,
        &space,
        boosted.starter_core_radius_ratio,
    );
    let baseline_core_share = baseline_core as f64 / baseline_fragments.len() as f64;
    let boosted_core_share = boosted_core as f64 / boosted_fragments.len() as f64;

    assert!(boosted_core > baseline_core);
    assert!(boosted_core_share > baseline_core_share);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_config_sanitize_clamps_starter_balance_fields() {
    let mut config = AsteroidFragmentConfig::default();
    config.max_fragments_per_chunk = 12;
    config.min_fragments_per_chunk = 99;
    config.starter_core_radius_ratio = 2.0;
    config.starter_core_density_multiplier = 0.2;
    config.replenish_interval_ticks = -10;
    config.replenish_percent_ppm = 2_000_000;

    let sanitized = config.sanitized();
    assert_eq!(sanitized.min_fragments_per_chunk, 12);
    assert_eq!(sanitized.starter_core_radius_ratio, 1.0);
    assert_eq!(sanitized.starter_core_density_multiplier, 1.0);
    assert_eq!(sanitized.replenish_interval_ticks, 0);
    assert_eq!(sanitized.replenish_percent_ppm, 1_000_000);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_material_distribution_core_metal_rim_volatile_biases_zones() {
    let space = SpaceConfig {
        width_cm: 800_000,
        depth_cm: 800_000,
        height_cm: 100_000,
    };
    let mut config = AsteroidFragmentConfig::default();
    config.base_density_per_km3 = 40.0;
    config.voxel_size_km = 1;
    config.cluster_noise = 0.0;
    config.layer_scale_height_km = 0.0;
    config.radius_min_cm = 100;
    config.radius_max_cm = 100;
    config.min_fragment_spacing_cm = 0;
    config.min_fragments_per_chunk = 0;
    config.material_distribution_strategy = MaterialDistributionStrategy::CoreMetalRimVolatile;
    config.starter_core_radius_ratio = 0.35;

    let fragments = generate_fragments(212, &space, &config);
    assert!(fragments.len() > 20);

    let stats = zone_material_stats(&fragments, &space, config.starter_core_radius_ratio);
    assert!(stats.core_total > 0);
    assert!(stats.rim_total > 0);

    let core_metal_share = stats.core_metal_like as f64 / stats.core_total as f64;
    let rim_metal_share = stats.rim_metal_like as f64 / stats.rim_total as f64;
    let core_volatile_share = stats.core_volatile as f64 / stats.core_total as f64;
    let rim_volatile_share = stats.rim_volatile as f64 / stats.rim_total as f64;

    assert!(core_metal_share > rim_metal_share);
    assert!(rim_volatile_share > core_volatile_share);
}

fn count_fragments_in_core_zone(fragments: &[Location], space: &SpaceConfig, ratio: f64) -> usize {
    let center_x = space.width_cm as f64 / 2.0;
    let center_y = space.depth_cm as f64 / 2.0;
    let max_distance = center_x.hypot(center_y).max(1.0);
    let core_ratio = ratio.clamp(0.0, 1.0);

    fragments
        .iter()
        .filter(|frag| {
            let distance_ratio = ((frag.pos.x_cm as f64) - center_x)
                .hypot((frag.pos.y_cm as f64) - center_y)
                / max_distance;
            distance_ratio <= core_ratio
        })
        .count()
}

struct ZoneMaterialStats {
    core_total: usize,
    rim_total: usize,
    core_metal_like: usize,
    rim_metal_like: usize,
    core_volatile: usize,
    rim_volatile: usize,
}

fn zone_material_stats(
    fragments: &[Location],
    space: &SpaceConfig,
    core_ratio: f64,
) -> ZoneMaterialStats {
    let center_x = space.width_cm as f64 / 2.0;
    let center_y = space.depth_cm as f64 / 2.0;
    let max_distance = center_x.hypot(center_y).max(1.0);
    let core = core_ratio.clamp(0.0, 1.0);
    let rim_threshold = (core + 1.0) / 2.0;

    let mut out = ZoneMaterialStats {
        core_total: 0,
        rim_total: 0,
        core_metal_like: 0,
        rim_metal_like: 0,
        core_volatile: 0,
        rim_volatile: 0,
    };

    for fragment in fragments {
        let ratio = ((fragment.pos.x_cm as f64) - center_x)
            .hypot((fragment.pos.y_cm as f64) - center_y)
            / max_distance;
        if ratio <= core {
            out.core_total = out.core_total.saturating_add(1);
            if matches!(
                fragment.profile.material,
                MaterialKind::Metal | MaterialKind::Composite
            ) {
                out.core_metal_like = out.core_metal_like.saturating_add(1);
            }
            if matches!(
                fragment.profile.material,
                MaterialKind::Ice | MaterialKind::Carbon
            ) {
                out.core_volatile = out.core_volatile.saturating_add(1);
            }
        } else if ratio >= rim_threshold {
            out.rim_total = out.rim_total.saturating_add(1);
            if matches!(
                fragment.profile.material,
                MaterialKind::Metal | MaterialKind::Composite
            ) {
                out.rim_metal_like = out.rim_metal_like.saturating_add(1);
            }
            if matches!(
                fragment.profile.material,
                MaterialKind::Ice | MaterialKind::Carbon
            ) {
                out.rim_volatile = out.rim_volatile.saturating_add(1);
            }
        }
    }

    out
}
