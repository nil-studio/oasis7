use super::*;

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn init_keeps_integer_location_seed_positions() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.origin.enabled = false;
    init.asteroid_fragment.enabled = false;
    init.agents.count = 0;

    let mut location_seed = LocationSeedConfig::default();
    location_seed.location_id = "integer".to_string();
    location_seed.name = "Integer".to_string();
    location_seed.pos = Some(GeoPos::new(10, 20, 31));
    init.locations.push(location_seed);

    let (model, _) = build_world_model(&config, &init).expect("init should succeed");
    let location = model.locations.get("integer").expect("location exists");

    assert_eq!(location.pos.x_cm, 10);
    assert_eq!(location.pos.y_cm, 20);
    assert_eq!(location.pos.z_cm, 31);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn init_generated_fragments_use_integer_centimeter_positions() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 10;
    config.asteroid_fragment.radius_max_cm = 10;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;

    let mut init = WorldInitConfig::default();
    init.origin.enabled = false;
    init.agents.count = 0;
    init.asteroid_fragment
        .bootstrap_chunks
        .push(ChunkCoord { x: 0, y: 0, z: 0 });

    let (model, _) = build_world_model(&config, &init).expect("init should succeed");
    let fragment = model
        .locations
        .values()
        .find(|location| location.id.starts_with("frag-"))
        .expect("fragment exists");

    assert!(config.space.contains(fragment.pos));
}
