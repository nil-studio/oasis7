use super::*;

#[test]
fn init_defaults_create_origin_and_agents() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.asteroid_fragment.enabled = false;
    init.agents.count = 2;

    let (model, report) = build_world_model(&config, &init).expect("init should succeed");
    let origin = model.locations.get("origin").expect("origin exists");

    let center_x = config.space.width_cm / 2;
    let center_y = config.space.depth_cm / 2;
    let center_z = config.space.height_cm / 2;

    assert_eq!(origin.pos.x_cm, center_x);
    assert_eq!(origin.pos.y_cm, center_y);
    assert_eq!(origin.pos.z_cm, center_z);
    assert_eq!(report.locations, 1);
    assert_eq!(report.agents, 2);
    assert_eq!(model.chunks.len(), 25);
    assert!(model.chunks.values().all(|state| {
        matches!(
            state,
            ChunkState::Unexplored | ChunkState::Generated | ChunkState::Exhausted
        )
    }));
    assert!(model.agents.contains_key("agent-0"));
    assert!(model.agents.contains_key("agent-1"));
}

#[test]
fn init_default_fragment_radius_matches_story_scale() {
    let config = WorldConfig::default();
    assert_eq!(config.asteroid_fragment.radius_min_cm, 25_000);
    assert_eq!(config.asteroid_fragment.radius_max_cm, 500_000);
}

#[test]
fn init_default_fragment_radiation_distribution_is_conservative() {
    let fragment = WorldConfig::default().asteroid_fragment;
    let total = fragment.material_weights.total();
    assert!(total > 0);

    let high_radiation_share =
        fragment.material_weights.metal + fragment.material_weights.composite;
    assert!(high_radiation_share * 100 <= total * 15);
    assert!(fragment.radiation_emission_scale <= 1e-12);

    assert!(
        fragment.material_radiation_factors.metal_bps
            > fragment.material_radiation_factors.silicate_bps
    );
    assert!(
        fragment.material_radiation_factors.composite_bps
            >= fragment.material_radiation_factors.carbon_bps
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn init_is_deterministic_with_seed() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.5;

    let mut init = WorldInitConfig::default();
    init.seed = 42;
    init.agents.count = 0;

    let (model_a, report_a) = build_world_model(&config, &init).expect("init A");
    let (model_b, report_b) = build_world_model(&config, &init).expect("init B");

    assert_eq!(model_a, model_b);
    assert_eq!(report_a, report_b);
}

#[test]
fn init_requires_spawn_location() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.origin.enabled = false;
    init.agents.count = 1;

    let err = build_world_model(&config, &init).expect_err("should fail");
    assert!(matches!(err, WorldInitError::SpawnLocationMissing));
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn init_seeds_locations_and_resources() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.origin.enabled = false;
    init.asteroid_fragment.enabled = false;
    init.agents.count = 1;
    init.agents
        .resources
        .add(ResourceKind::Data, 5)
        .expect("seed agent resources");

    let mut location_seed = LocationSeedConfig::default();
    location_seed.location_id = "base".to_string();
    location_seed.name = "Base".to_string();
    location_seed.pos = Some(pos(10, 10));
    location_seed
        .resources
        .add(ResourceKind::Data, 3)
        .expect("seed location resources");
    init.locations.push(location_seed);

    let (model, _) = build_world_model(&config, &init).expect("init should succeed");
    let base = model.locations.get("base").expect("base exists");
    assert_eq!(base.resources.get(ResourceKind::Data), 3);
    let agent = model.agents.get("agent-0").expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 5);
}

#[test]
fn init_rejects_negative_resources() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.asteroid_fragment.enabled = false;
    init.agents.count = 0;
    init.origin
        .resources
        .amounts
        .insert(ResourceKind::Electricity, -5);

    let err = build_world_model(&config, &init).expect_err("should fail");
    assert!(matches!(
        err,
        WorldInitError::InvalidResourceAmount {
            kind: ResourceKind::Electricity,
            amount: -5
        }
    ));
}

#[test]
fn init_seeds_power_facilities() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.asteroid_fragment.enabled = false;
    init.agents.count = 1;

    let plant_seed = PowerPlantSeedConfig {
        facility_id: "plant-1".to_string(),
        location_id: "origin".to_string(),
        owner: ResourceOwner::Agent {
            agent_id: "agent-0".to_string(),
        },
        capacity_per_tick: 5,
        fuel_cost_per_pu: 1,
        maintenance_cost: 1,
        efficiency: 1.0,
        degradation: 0.0,
    };
    init.power_plants.push(plant_seed);

    let (model, _) = build_world_model(&config, &init).expect("init should succeed");
    assert!(model.power_plants.contains_key("plant-1"));
}

#[test]
fn init_rejects_facility_with_missing_owner() {
    let config = WorldConfig::default();
    let mut init = WorldInitConfig::default();
    init.asteroid_fragment.enabled = false;
    init.agents.count = 0;

    let plant_seed = PowerPlantSeedConfig {
        facility_id: "plant-1".to_string(),
        location_id: "origin".to_string(),
        owner: ResourceOwner::Agent {
            agent_id: "missing-agent".to_string(),
        },
        capacity_per_tick: 5,
        fuel_cost_per_pu: 1,
        maintenance_cost: 1,
        efficiency: 1.0,
        degradation: 0.0,
    };
    init.power_plants.push(plant_seed);

    let err = build_world_model(&config, &init).expect_err("should fail");
    assert!(matches!(err, WorldInitError::FacilityOwnerNotFound { .. }));
}

#[test]
fn scenario_templates_build_models() {
    let config = WorldConfig::default();
    let scenarios = [
        WorldScenario::Minimal,
        WorldScenario::TwoBases,
        WorldScenario::LlmBootstrap,
        WorldScenario::PowerBootstrap,
        WorldScenario::ResourceBootstrap,
        WorldScenario::TwinRegionBootstrap,
        WorldScenario::TriadRegionBootstrap,
        WorldScenario::TriadP2pBootstrap,
        WorldScenario::AsteroidFragmentBootstrap,
        WorldScenario::AsteroidFragmentDetailBootstrap,
        WorldScenario::AsteroidFragmentTwinRegionBootstrap,
        WorldScenario::AsteroidFragmentTriadRegionBootstrap,
    ];

    for scenario in scenarios {
        let init = WorldInitConfig::from_scenario(scenario, &config);
        let (model, _) = build_world_model(&config, &init).expect("scenario init");
        assert!(!model.locations.is_empty());
    }
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn scenario_asteroid_fragment_min_spacing_overrides_world_config() {
    let spec_json = r#"{
        "id": "spacing_override",
        "name": "Spacing Override",
        "seed": 7,
        "asteroid_fragment": { "enabled": true, "seed_offset": 0, "min_fragment_spacing_cm": 50000 },
        "agents": { "count": 0 },
        "location_generator": { "count": 0 }
    }"#;
    let spec: WorldScenarioSpec = serde_json::from_str(spec_json).expect("parse spec");

    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 10;
    config.asteroid_fragment.radius_max_cm = 10;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;

    let init = spec.into_init_config(&config);
    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let fragments: Vec<_> = model
        .locations
        .values()
        .filter(|loc| loc.id.starts_with("frag-"))
        .collect();
    assert!(fragments.len() > 1);

    let spacing_cm = init
        .asteroid_fragment
        .min_fragment_spacing_cm
        .expect("spacing override");
    for i in 0..fragments.len() {
        for j in (i + 1)..fragments.len() {
            let a = fragments[i];
            let b = fragments[j];
            let dx = a.pos.x_cm - b.pos.x_cm;
            let dy = a.pos.y_cm - b.pos.y_cm;
            let dz = a.pos.z_cm - b.pos.z_cm;
            let min_dist = (a.profile.radius_cm + b.profile.radius_cm + spacing_cm) as f64;
            assert!(((dx * dx + dy * dy + dz * dz) as f64) >= (min_dist * min_dist));
        }
    }
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn chunk_generated_fragments_include_fragment_profile() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 19;
    init.agents.count = 0;

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let fragments: Vec<_> = model
        .locations
        .values()
        .filter(|loc| loc.id.starts_with("frag-"))
        .collect();

    assert!(!fragments.is_empty());
    assert_eq!(model.chunk_resource_budgets.len(), 1);

    let mut expected_total_by_element = std::collections::BTreeMap::new();
    for fragment in fragments {
        let profile = fragment
            .fragment_profile
            .as_ref()
            .expect("generated fragment profile");
        let budget = fragment
            .fragment_budget
            .as_ref()
            .expect("generated fragment budget");

        assert!(!profile.blocks.blocks.is_empty());
        assert_eq!(profile.total_volume_cm3, profile.blocks.total_volume_cm3());
        assert_eq!(profile.total_mass_g, profile.blocks.total_mass_g());
        assert!(profile.bulk_density_kg_per_m3 > 0);
        assert!(profile.compounds.total_ppm() > 0);
        assert!(profile.elements.total_ppm() > 0);

        assert!(!budget.total_by_element_g.is_empty());
        assert_eq!(budget.total_by_element_g, budget.remaining_by_element_g);
        for (kind, amount) in &budget.total_by_element_g {
            let entry = expected_total_by_element.entry(*kind).or_insert(0i64);
            *entry = entry.saturating_add(*amount);
        }
    }

    let chunk_budget = model
        .chunk_resource_budgets
        .values()
        .next()
        .expect("chunk budget exists");
    assert_eq!(chunk_budget.total_by_element_g, expected_total_by_element);
    assert_eq!(
        chunk_budget.total_by_element_g,
        chunk_budget.remaining_by_element_g
    );
}

#[test]
fn resource_bootstrap_seeds_stock() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::ResourceBootstrap, &config);
    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let origin = model.locations.get("origin").expect("origin exists");
    let agent = model.agents.get("agent-0").expect("agent exists");

    assert_eq!(origin.resources.get(ResourceKind::Electricity), 0);
    assert_eq!(origin.resources.get(ResourceKind::Data), 20);
    assert_eq!(agent.resources.get(ResourceKind::Data), 10);
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 25);
}

#[test]
fn twin_region_bootstrap_seeds_regions() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::TwinRegionBootstrap, &config);
    let (model, _) = build_world_model(&config, &init).expect("scenario init");

    assert!(model.locations.contains_key("region-0"));
    assert!(model.locations.contains_key("region-1"));
    assert!(model.power_plants.is_empty());
    assert!(model.agents.contains_key("agent-0"));
    assert!(model.agents.contains_key("agent-1"));
}

#[test]
fn triad_region_bootstrap_seeds_regions() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::TriadRegionBootstrap, &config);
    let (model, _) = build_world_model(&config, &init).expect("scenario init");

    assert!(model.locations.contains_key("region-0"));
    assert!(model.locations.contains_key("region-1"));
    assert!(model.locations.contains_key("region-2"));
    assert!(model.power_plants.is_empty());
    assert!(model.agents.contains_key("agent-0"));
    assert!(model.agents.contains_key("agent-1"));
    assert!(model.agents.contains_key("agent-2"));
}

#[test]
fn triad_p2p_bootstrap_seeds_nodes_and_agents() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::TriadP2pBootstrap, &config);
    let (model, _) = build_world_model(&config, &init).expect("scenario init");

    assert!(model.locations.contains_key("node-0"));
    assert!(model.locations.contains_key("node-1"));
    assert!(model.locations.contains_key("node-2"));

    let agent_a = model.agents.get("agent-0").expect("agent-0 exists");
    let agent_b = model.agents.get("agent-1").expect("agent-1 exists");
    let agent_c = model.agents.get("agent-2").expect("agent-2 exists");

    assert!(model.locations.contains_key(&agent_a.location_id));
    assert!(model.locations.contains_key(&agent_b.location_id));
    assert!(model.locations.contains_key(&agent_c.location_id));
}

#[test]
fn asteroid_fragment_bootstrap_seeds_fragments_and_resources() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);
    let (model, report) = build_world_model(&config, &init).expect("scenario init");

    assert!(report.asteroid_fragment_seed.is_some());
    assert!(!model.locations.is_empty());
    assert!(model.power_plants.is_empty());
    assert!(model.agents.contains_key("agent-0"));
    assert!(!init.asteroid_fragment.bootstrap_chunks.is_empty());
    for coord in &init.asteroid_fragment.bootstrap_chunks {
        assert!(
            model.chunks.get(coord).is_some_and(|state| matches!(
                state,
                ChunkState::Generated | ChunkState::Exhausted
            ))
        );
        assert!(model.chunk_resource_budgets.contains_key(coord));
    }
}

#[test]
fn asteroid_fragment_twin_region_bootstrap_seeds_fragments_and_regions() {
    let config = WorldConfig::default();
    let init =
        WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentTwinRegionBootstrap, &config);
    let (model, report) = build_world_model(&config, &init).expect("scenario init");

    assert!(report.asteroid_fragment_seed.is_some());
    assert!(model.locations.contains_key("region-0"));
    assert!(model.locations.contains_key("region-1"));
    assert!(model.power_plants.is_empty());
    assert!(model.agents.contains_key("agent-0"));
    assert!(model.agents.contains_key("agent-1"));
    assert!(!init.asteroid_fragment.bootstrap_chunks.is_empty());
    for coord in &init.asteroid_fragment.bootstrap_chunks {
        assert!(
            model.chunks.get(coord).is_some_and(|state| matches!(
                state,
                ChunkState::Generated | ChunkState::Exhausted
            ))
        );
        assert!(model.chunk_resource_budgets.contains_key(coord));
    }
}

#[test]
fn asteroid_fragment_detail_bootstrap_seeds_dense_fragments_for_viewer() {
    let config = WorldConfig::default();
    let init =
        WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentDetailBootstrap, &config);
    let (model, report) = build_world_model(&config, &init).expect("scenario init");

    assert!(report.asteroid_fragment_seed.is_some());
    assert!(model.agents.is_empty());
    assert!(!model.locations.contains_key("origin"));
    assert!(
        model
            .locations
            .values()
            .any(|loc| loc.id.starts_with("frag-"))
    );
    assert!(init.asteroid_fragment.min_fragment_spacing_cm.is_some());
    assert!(!init.asteroid_fragment.bootstrap_chunks.is_empty());
    for coord in &init.asteroid_fragment.bootstrap_chunks {
        assert!(
            model.chunks.get(coord).is_some_and(|state| matches!(
                state,
                ChunkState::Generated | ChunkState::Exhausted
            ))
        );
        assert!(model.chunk_resource_budgets.contains_key(coord));
    }
}

#[test]
fn asteroid_fragment_triad_region_bootstrap_seeds_fragments_and_regions() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(
        WorldScenario::AsteroidFragmentTriadRegionBootstrap,
        &config,
    );
    let (model, report) = build_world_model(&config, &init).expect("scenario init");

    assert!(report.asteroid_fragment_seed.is_some());
    assert!(model.locations.contains_key("region-0"));
    assert!(model.locations.contains_key("region-1"));
    assert!(model.locations.contains_key("region-2"));
    assert!(model.power_plants.is_empty());
    assert!(model.agents.contains_key("agent-0"));
    assert!(model.agents.contains_key("agent-1"));
    assert!(model.agents.contains_key("agent-2"));
    assert!(!init.asteroid_fragment.bootstrap_chunks.is_empty());
    for coord in &init.asteroid_fragment.bootstrap_chunks {
        assert!(
            model.chunks.get(coord).is_some_and(|state| matches!(
                state,
                ChunkState::Generated | ChunkState::Exhausted
            ))
        );
        assert!(model.chunk_resource_budgets.contains_key(coord));
    }
}

#[test]
fn scenario_aliases_parse() {
    let cases = [
        ("two-bases", WorldScenario::TwoBases),
        ("llm", WorldScenario::LlmBootstrap),
        ("bootstrap", WorldScenario::PowerBootstrap),
        ("resources", WorldScenario::ResourceBootstrap),
        ("twin-regions", WorldScenario::TwinRegionBootstrap),
        ("triad-regions", WorldScenario::TriadRegionBootstrap),
        ("p2p-triad", WorldScenario::TriadP2pBootstrap),
        (
            "asteroid_fragment",
            WorldScenario::AsteroidFragmentBootstrap,
        ),
        (
            "asteroid_fragment_detail",
            WorldScenario::AsteroidFragmentDetailBootstrap,
        ),
        (
            "asteroid-fragment-regions",
            WorldScenario::AsteroidFragmentTwinRegionBootstrap,
        ),
        (
            "asteroid-fragment-triad",
            WorldScenario::AsteroidFragmentTriadRegionBootstrap,
        ),
    ];

    for (input, expected) in cases {
        let parsed = WorldScenario::parse(input).expect("parse scenario");
        assert_eq!(parsed, expected);
    }
}

#[test]
fn scenarios_are_stable() {
    struct ScenarioExpectation<'a> {
        scenario: WorldScenario,
        expected_agents: usize,
        expect_origin: bool,
        required_locations: &'a [&'a str],
        required_plants: &'a [&'a str],
        expect_asteroid_fragment: bool,
    }

    let expectations = [
        ScenarioExpectation {
            scenario: WorldScenario::Minimal,
            expected_agents: 1,
            expect_origin: true,
            required_locations: &["origin"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::TwoBases,
            expected_agents: 2,
            expect_origin: true,
            required_locations: &["origin", "base-0", "base-1"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::LlmBootstrap,
            expected_agents: 5,
            expect_origin: true,
            required_locations: &["origin", "llm-site-0", "llm-site-1"],
            required_plants: &[],
            expect_asteroid_fragment: true,
        },
        ScenarioExpectation {
            scenario: WorldScenario::PowerBootstrap,
            expected_agents: 1,
            expect_origin: true,
            required_locations: &["origin"],
            required_plants: &["plant-1"],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::ResourceBootstrap,
            expected_agents: 1,
            expect_origin: true,
            required_locations: &["origin"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::TwinRegionBootstrap,
            expected_agents: 2,
            expect_origin: false,
            required_locations: &["region-0", "region-1"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::TriadRegionBootstrap,
            expected_agents: 3,
            expect_origin: false,
            required_locations: &["region-0", "region-1", "region-2"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::TriadP2pBootstrap,
            expected_agents: 3,
            expect_origin: false,
            required_locations: &["node-0", "node-1", "node-2"],
            required_plants: &[],
            expect_asteroid_fragment: false,
        },
        ScenarioExpectation {
            scenario: WorldScenario::AsteroidFragmentBootstrap,
            expected_agents: 1,
            expect_origin: true,
            required_locations: &["origin"],
            required_plants: &[],
            expect_asteroid_fragment: true,
        },
        ScenarioExpectation {
            scenario: WorldScenario::AsteroidFragmentDetailBootstrap,
            expected_agents: 0,
            expect_origin: false,
            required_locations: &[],
            required_plants: &[],
            expect_asteroid_fragment: true,
        },
        ScenarioExpectation {
            scenario: WorldScenario::AsteroidFragmentTwinRegionBootstrap,
            expected_agents: 2,
            expect_origin: false,
            required_locations: &["region-0", "region-1"],
            required_plants: &[],
            expect_asteroid_fragment: true,
        },
        ScenarioExpectation {
            scenario: WorldScenario::AsteroidFragmentTriadRegionBootstrap,
            expected_agents: 3,
            expect_origin: false,
            required_locations: &["region-0", "region-1", "region-2"],
            required_plants: &[],
            expect_asteroid_fragment: true,
        },
    ];

    let config = WorldConfig::default();
    for expectation in expectations {
        let init = WorldInitConfig::from_scenario(expectation.scenario, &config);
        let (model, report) = build_world_model(&config, &init).expect("scenario init");

        assert_eq!(report.agents, expectation.expected_agents);
        assert_eq!(model.agents.len(), expectation.expected_agents);
        assert_eq!(
            model.locations.contains_key("origin"),
            expectation.expect_origin
        );

        for location_id in expectation.required_locations {
            assert!(model.locations.contains_key(*location_id));
        }
        for plant_id in expectation.required_plants {
            assert!(model.power_plants.contains_key(*plant_id));
        }

        assert_eq!(
            report.asteroid_fragment_seed.is_some(),
            expectation.expect_asteroid_fragment
        );
    }
}

#[test]
fn world_model_chunk_states_roundtrip_json_keys() {
    let mut model = WorldModel::default();
    model
        .chunks
        .insert(ChunkCoord { x: 0, y: 0, z: 0 }, ChunkState::Unexplored);
    model
        .chunks
        .insert(ChunkCoord { x: 1, y: 2, z: 0 }, ChunkState::Generated);

    let mut chunk_budget = ChunkResourceBudget::default();
    chunk_budget
        .total_by_element_g
        .insert(FragmentElementKind::Iron, 1200);
    chunk_budget
        .remaining_by_element_g
        .insert(FragmentElementKind::Iron, 1200);
    model
        .chunk_resource_budgets
        .insert(ChunkCoord { x: 1, y: 2, z: 0 }, chunk_budget);
    model.chunk_boundary_reservations.insert(
        ChunkCoord { x: 0, y: 1, z: 0 },
        vec![BoundaryReservation {
            source_chunk: ChunkCoord { x: 0, y: 0, z: 0 },
            source_fragment_id: "frag-0-0-0-0".to_string(),
            source_pos: GeoPos {
                x_cm: 10,
                y_cm: 20,
                z_cm: 30,
            },
            source_radius_cm: 100,
            min_spacing_cm: 500,
        }],
    );

    let json = serde_json::to_string(&model).expect("serialize world model");
    assert!(json.contains("\"0:0:0\""));
    assert!(json.contains("\"1:2:0\""));

    let decoded: WorldModel = serde_json::from_str(&json).expect("deserialize world model");
    assert_eq!(decoded.chunks, model.chunks);
    assert_eq!(decoded.chunk_resource_budgets, model.chunk_resource_budgets);
    assert_eq!(
        decoded.chunk_boundary_reservations,
        model.chunk_boundary_reservations
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn boundary_reservations_are_created_for_unexplored_neighbor_chunks() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 4_000_000,
        depth_cm: 2_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 0.005;
    config.asteroid_fragment.voxel_size_km = 10;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 1_000;
    config.asteroid_fragment.radius_max_cm = 1_000;

    let mut init = WorldInitConfig::default();
    init.seed = 1337;
    init.origin.enabled = false;
    init.agents.count = 0;
    init.asteroid_fragment.min_fragment_spacing_cm = Some(2_000_000);
    init.asteroid_fragment.bootstrap_chunks = vec![ChunkCoord { x: 0, y: 0, z: 0 }];

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    assert!(
        model
            .chunks
            .get(&ChunkCoord { x: 1, y: 0, z: 0 })
            .is_some_and(|state| matches!(state, ChunkState::Unexplored))
    );
    assert!(
        model
            .chunk_boundary_reservations
            .get(&ChunkCoord { x: 1, y: 0, z: 0 })
            .is_some_and(|entries| !entries.is_empty())
    );
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn cross_chunk_generation_respects_spacing_with_neighbor_checks() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 4_000_000,
        depth_cm: 2_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 0.003;
    config.asteroid_fragment.voxel_size_km = 20;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 1_000;
    config.asteroid_fragment.radius_max_cm = 1_000;

    let mut init = WorldInitConfig::default();
    init.seed = 2026;
    init.origin.enabled = false;
    init.agents.count = 0;
    init.asteroid_fragment.min_fragment_spacing_cm = Some(500_000);
    init.asteroid_fragment.bootstrap_chunks = vec![
        ChunkCoord { x: 0, y: 0, z: 0 },
        ChunkCoord { x: 1, y: 0, z: 0 },
    ];

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let left: Vec<_> = model
        .locations
        .values()
        .filter(|loc| {
            chunk_coord_of(loc.pos, &config.space) == Some(ChunkCoord { x: 0, y: 0, z: 0 })
        })
        .filter(|loc| loc.id.starts_with("frag-"))
        .collect();
    let right: Vec<_> = model
        .locations
        .values()
        .filter(|loc| {
            chunk_coord_of(loc.pos, &config.space) == Some(ChunkCoord { x: 1, y: 0, z: 0 })
        })
        .filter(|loc| loc.id.starts_with("frag-"))
        .collect();

    assert!(!left.is_empty());
    assert!(!right.is_empty());

    let spacing_cm = init
        .asteroid_fragment
        .min_fragment_spacing_cm
        .expect("spacing configured") as f64;

    for a in &left {
        for b in &right {
            let dx = a.pos.x_cm - b.pos.x_cm;
            let dy = a.pos.y_cm - b.pos.y_cm;
            let dz = a.pos.z_cm - b.pos.z_cm;
            let min_dist = (a.profile.radius_cm + b.profile.radius_cm) as f64 + spacing_cm;
            assert!(((dx * dx + dy * dy + dz * dz) as f64) >= (min_dist * min_dist));
        }
    }
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn world_model_roundtrip_preserves_fragment_profile() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 23;
    init.agents.count = 0;

    let (model, _) = build_world_model(&config, &init).expect("scenario init");
    let fragment_before = model
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .expect("fragment before serialization");
    let frag_before = fragment_before
        .fragment_profile
        .clone()
        .expect("fragment profile before serialization");
    let budget_before = fragment_before
        .fragment_budget
        .clone()
        .expect("fragment budget before serialization");
    let chunk_budget_before = model.chunk_resource_budgets.clone();

    let json = serde_json::to_string(&model).expect("serialize world model");
    let restored: WorldModel = serde_json::from_str(&json).expect("deserialize world model");
    let fragment_after = restored
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .expect("fragment after serialization");
    let frag_after = fragment_after
        .fragment_profile
        .clone()
        .expect("fragment profile after serialization");
    let budget_after = fragment_after
        .fragment_budget
        .clone()
        .expect("fragment budget after serialization");

    assert_eq!(frag_after, frag_before);
    assert_eq!(budget_after, budget_before);
    assert_eq!(restored.chunk_resource_budgets, chunk_budget_before);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn consume_fragment_resource_keeps_fragment_and_chunk_conservation() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 27;
    init.agents.count = 0;

    let (mut model, _) = build_world_model(&config, &init).expect("scenario init");
    let fragment = model
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .cloned()
        .expect("fragment exists");
    let coord = chunk_coord_of(fragment.pos, &config.space).expect("fragment chunk coord");
    let element = fragment
        .fragment_budget
        .as_ref()
        .and_then(|budget| budget.remaining_by_element_g.keys().next().copied())
        .expect("fragment has element budget");

    let fragment_remaining_before = fragment
        .fragment_budget
        .as_ref()
        .expect("fragment budget")
        .get_remaining(element);
    let chunk_remaining_before = model
        .chunk_resource_budgets
        .get(&coord)
        .expect("chunk budget")
        .get_remaining(element);
    let consume_amount = fragment_remaining_before.clamp(1, 50);

    model
        .consume_fragment_resource(&fragment.id, &config.space, element, consume_amount)
        .expect("consume fragment resource");

    let fragment_remaining_after = model
        .locations
        .get(&fragment.id)
        .and_then(|loc| loc.fragment_budget.as_ref())
        .expect("fragment budget after")
        .get_remaining(element);
    let chunk_remaining_after = model
        .chunk_resource_budgets
        .get(&coord)
        .expect("chunk budget after")
        .get_remaining(element);

    assert_eq!(
        fragment_remaining_after,
        fragment_remaining_before - consume_amount
    );
    assert_eq!(
        chunk_remaining_after,
        chunk_remaining_before - consume_amount
    );

    let overdraw = model.consume_fragment_resource(
        &fragment.id,
        &config.space,
        element,
        fragment_remaining_after + 1,
    );
    assert!(matches!(
        overdraw,
        Err(FragmentResourceError::Budget(
            ElementBudgetError::Insufficient { .. }
        ))
    ));
}

#[test]
fn scenario_asteroid_fragment_bootstrap_chunks_generate_without_seed_locations() {
    let spec_json = r#"{
        "id": "chunk_bootstrap_only",
        "name": "Chunk Bootstrap Only",
        "seed": 91,
        "origin": { "enabled": false },
        "asteroid_fragment": {
            "enabled": true,
            "seed_offset": 5,
            "bootstrap_chunks": [
                { "x": 0, "y": 0, "z": 0 },
                { "x": 1, "y": 0, "z": 0 }
            ]
        },
        "agents": { "count": 0 },
        "location_generator": { "count": 0 }
    }"#;

    let spec: WorldScenarioSpec = serde_json::from_str(spec_json).expect("parse spec");
    let config = WorldConfig::default();
    let init = spec.into_init_config(&config);

    assert_eq!(init.asteroid_fragment.bootstrap_chunks.len(), 2);

    let (model, report) = build_world_model(&config, &init).expect("scenario init");
    assert!(report.asteroid_fragment_seed.is_some());

    for coord in &init.asteroid_fragment.bootstrap_chunks {
        assert!(
            model.chunks.get(coord).is_some_and(|state| matches!(
                state,
                ChunkState::Generated | ChunkState::Exhausted
            ))
        );
        assert!(model.chunk_resource_budgets.contains_key(coord));
    }
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn chunk_generation_respects_max_fragments_per_chunk() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 2_000_000,
        depth_cm: 2_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 10.0;
    config.asteroid_fragment.voxel_size_km = 20;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 1_000;
    config.asteroid_fragment.radius_max_cm = 1_000;
    config.asteroid_fragment.max_fragments_per_chunk = 2;

    let target = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut observed_non_empty = false;

    for seed in 320..352 {
        let mut init = WorldInitConfig::default();
        init.seed = seed;
        init.origin.enabled = false;
        init.agents.count = 0;
        init.asteroid_fragment.bootstrap_chunks = vec![target];

        let (model, _) = build_world_model(&config, &init).expect("build world model");

        let fragment_count = model
            .locations
            .values()
            .filter(|location| location.id.starts_with("frag-"))
            .filter(|location| {
                chunk_coord_of(location.pos, &config.space).is_some_and(|coord| coord == target)
            })
            .count();

        if fragment_count > 0 {
            observed_non_empty = true;
        }
        assert!(fragment_count <= 2);
    }

    assert!(observed_non_empty);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn chunk_generation_respects_block_budgets() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 2_000_000,
        depth_cm: 2_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 200.0;
    config.asteroid_fragment.voxel_size_km = 20;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 3_000;
    config.asteroid_fragment.radius_max_cm = 3_000;
    config.asteroid_fragment.max_fragments_per_chunk = 10;
    config.asteroid_fragment.max_blocks_per_fragment = 3;
    config.asteroid_fragment.max_blocks_per_chunk = 5;

    let target = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut observed_non_empty = false;

    for seed in 321..353 {
        let mut init = WorldInitConfig::default();
        init.seed = seed;
        init.origin.enabled = false;
        init.agents.count = 0;
        init.asteroid_fragment.bootstrap_chunks = vec![target];

        let (model, _) = build_world_model(&config, &init).expect("build world model");

        let mut total_blocks = 0usize;
        let mut fragment_count = 0usize;
        for location in model
            .locations
            .values()
            .filter(|loc| loc.id.starts_with("frag-"))
        {
            if chunk_coord_of(location.pos, &config.space).is_none_or(|coord| coord != target) {
                continue;
            }
            let profile = location
                .fragment_profile
                .as_ref()
                .expect("fragment profile exists");
            let blocks = profile.blocks.blocks.len();
            assert!(blocks <= 3);
            total_blocks = total_blocks.saturating_add(blocks);
            fragment_count = fragment_count.saturating_add(1);
        }

        if fragment_count > 0 {
            observed_non_empty = true;
        }
        assert!(total_blocks <= 5);
    }

    assert!(observed_non_empty);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn multi_chunk_generation_respects_budget_caps() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 4_000_000,
        depth_cm: 4_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 80.0;
    config.asteroid_fragment.voxel_size_km = 10;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 2_500;
    config.asteroid_fragment.radius_max_cm = 2_500;
    config.asteroid_fragment.max_fragments_per_chunk = 2;
    config.asteroid_fragment.max_blocks_per_fragment = 2;
    config.asteroid_fragment.max_blocks_per_chunk = 3;

    let chunks = vec![
        ChunkCoord { x: 0, y: 0, z: 0 },
        ChunkCoord { x: 1, y: 0, z: 0 },
        ChunkCoord { x: 0, y: 1, z: 0 },
        ChunkCoord { x: 1, y: 1, z: 0 },
    ];

    let mut init = WorldInitConfig::default();
    init.seed = 410;
    init.origin.enabled = false;
    init.agents.count = 0;
    init.asteroid_fragment.bootstrap_chunks = chunks.clone();

    let (model, _) = build_world_model(&config, &init).expect("build world model");

    for coord in chunks {
        let mut fragment_count = 0usize;
        let mut block_count = 0usize;
        for location in model
            .locations
            .values()
            .filter(|loc| loc.id.starts_with("frag-"))
        {
            if chunk_coord_of(location.pos, &config.space).is_none_or(|c| c != coord) {
                continue;
            }
            let profile = location
                .fragment_profile
                .as_ref()
                .expect("fragment profile exists");
            let blocks = profile.blocks.blocks.len();
            assert!(blocks <= 2);
            block_count = block_count.saturating_add(blocks);
            fragment_count = fragment_count.saturating_add(1);
        }
        assert!(fragment_count <= 2);
        assert!(block_count <= 3);
    }
}
