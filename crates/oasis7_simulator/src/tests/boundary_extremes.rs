use super::*;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn asteroid_fragment_generator_handles_extreme_radius_density_and_spacing() {
    let space = SpaceConfig {
        width_cm: 100_000,
        depth_cm: 100_000,
        height_cm: 100_000,
    };

    let cases = [
        (1, 1, 0, 200.0),
        (500_000, 500_000, 0, 20.0),
        (5_000_000, 5_000_000, 0, 10.0),
    ];

    for (radius_min_cm, radius_max_cm, min_spacing_cm, density) in cases {
        let mut config = AsteroidFragmentConfig::default();
        config.base_density_per_km3 = density;
        config.voxel_size_km = 1;
        config.cluster_noise = 0.0;
        config.layer_scale_height_km = 0.0;
        config.radius_min_cm = radius_min_cm;
        config.radius_max_cm = radius_max_cm;
        config.min_fragment_spacing_cm = min_spacing_cm;

        let fragments = generate_fragments(2026, &space, &config);
        assert!(!fragments.is_empty());
        for fragment in fragments {
            assert!(space.contains(fragment.pos));
            assert!(fragment.profile.radius_cm >= radius_min_cm);
            assert!(fragment.profile.radius_cm <= radius_max_cm);
            assert!(fragment.profile.radiation_emission_per_tick > 0);
        }
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn world_init_survives_extreme_fragment_generation_with_budget_caps() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 500.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 1;
    config.asteroid_fragment.radius_max_cm = 5_000_000;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.max_fragments_per_chunk = 50;
    config.asteroid_fragment.max_blocks_per_fragment = 2;
    config.asteroid_fragment.max_blocks_per_chunk = 80;

    let mut init = WorldInitConfig::default();
    init.seed = 2026;
    init.agents.count = 0;

    let (model, report) = build_world_model(&config, &init).expect("build world model");
    assert!(report.locations > 0);

    for location in model
        .locations
        .values()
        .filter(|loc| loc.id.starts_with("frag-"))
    {
        assert!(config.space.contains(location.pos));
        assert!(location.profile.radius_cm >= 1);
        assert!(location.profile.radiation_emission_per_tick > 0);
    }
}
