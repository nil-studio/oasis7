use super::*;
use crate::geometry::space_distance_cm;

#[test]
fn agents_prefer_fragment_spawn_locations_when_fragments_exist() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let agent = model.agents.get("agent-0").expect("agent exists");

    assert!(agent.location_id.starts_with("frag-"));
}

#[test]
fn fragment_spawn_positions_stand_off_above_surface() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let agent = model.agents.get("agent-0").expect("agent exists");
    let fragment = model
        .locations
        .get(&agent.location_id)
        .expect("fragment location exists");

    let center_distance_cm = space_distance_cm(agent.pos, fragment.pos);
    let standoff_cm = center_distance_cm.saturating_sub(fragment.profile.radius_cm.max(1));

    assert!(agent.pos.z_cm > fragment.pos.z_cm);
    assert_eq!(agent.pos.x_cm, fragment.pos.x_cm);
    assert_eq!(agent.pos.y_cm, fragment.pos.y_cm);
    assert!(standoff_cm >= 2_000);
    assert!(standoff_cm <= 5_100);
    assert!(config.space.contains(agent.pos));
}
