#[test]
fn observe_triggers_chunk_generation_for_agent_chunk() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 100.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 10;
    config.asteroid_fragment.radius_max_cm = 10;

    // One 1km voxel bounds the fixture's high-density generation. The old
    // default 20×20×10km chunk requested roughly 400,000 candidates before
    // the later output cap, with quadratic spacing checks during placement.
    config.space.width_cm = 100_000;
    config.space.depth_cm = 100_000;
    config.space.height_cm = 100_000;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;

    // Construct the lazy state explicitly: initialize_kernel generates the
    // origin chunk itself, which cannot prove that observe caused generation.
    let coord = chunk_coords(&config.space).into_iter().next().unwrap();
    let location_pos = pos(50_000, 50_000);
    let mut model = WorldModel::default();
    model.chunks.insert(coord, ChunkState::Unexplored);
    model.locations.insert(
        "origin".into(),
        Location::new_with_profile("origin", "Origin", location_pos, LocationProfile::default()),
    );
    model.agents.insert(
        "agent-0".into(),
        Agent::new_with_power("agent-0", "origin", location_pos, &config.power),
    );
    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: 7,
        asteroid_fragment_enabled: true,
        asteroid_fragment_seed_offset: 1,
        min_fragment_spacing_cm: Some(0),
    };
    let mut kernel = WorldKernel::with_model_and_chunk_runtime(config, model, chunk_runtime);
    assert!(matches!(
        kernel.model().chunks.get(&coord),
        Some(ChunkState::Unexplored)
    ));
    let before = kernel.model().locations.len();
    let _ = kernel.observe("agent-0").expect("observe");
    assert!(matches!(
        kernel.model().chunks.get(&coord),
        Some(ChunkState::Generated)
    ));
    assert!(
        kernel.model().locations.len() > before,
        "observe must add generated locations"
    );
    assert!(kernel.journal().iter().any(|event| matches!(event.kind,
        WorldEventKind::ChunkGenerated { coord: generated, cause: ChunkGenerationCause::Observe, .. }
            if generated == coord)));
}

#[test]
fn observe_records_chunk_generated_event_with_observe_cause() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;

    let mut model = WorldModel::default();
    for coord in chunk_coords(&config.space) {
        model.chunks.insert(coord, ChunkState::Unexplored);
    }

    let location_pos = pos(100_000, 100_000);
    model.locations.insert(
        "origin".to_string(),
        Location::new_with_profile(
            "origin".to_string(),
            "Origin".to_string(),
            location_pos,
            LocationProfile::default(),
        ),
    );
    model.agents.insert(
        "agent-0".to_string(),
        Agent::new_with_power("agent-0", "origin", location_pos, &config.power),
    );

    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: 9,
        asteroid_fragment_enabled: true,
        asteroid_fragment_seed_offset: 1,
        min_fragment_spacing_cm: None,
    };
    let mut kernel = WorldKernel::with_model_and_chunk_runtime(config, model, chunk_runtime);

    let before = kernel.journal().len();
    let _ = kernel.observe("agent-0").expect("observe");
    assert!(kernel.journal().len() > before);
    assert!(kernel.journal().iter().any(|event| {
        matches!(
            event.kind,
            WorldEventKind::ChunkGenerated {
                cause: ChunkGenerationCause::Observe,
                ..
            }
        )
    }));
}

#[test]
fn detached_harvest_quote_generates_ungenerated_chunk_without_mutating_live_world() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;
    config.physics.radiation_decay_k = 0.0;
    config.asteroid_fragment.base_density_per_km3 = 2.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 1_000_000;
    config.asteroid_fragment.radius_max_cm = 1_000_000;
    config.asteroid_fragment.radiation_emission_scale = 1_000_000.0;

    let location_pos = pos(100_000, 100_000);
    let mut model = WorldModel::default();
    for coord in chunk_coords(&config.space) {
        model.chunks.insert(coord, ChunkState::Unexplored);
    }
    model.locations.insert(
        "origin".to_string(),
        Location::new_with_profile(
            "origin".to_string(),
            "Origin".to_string(),
            location_pos,
            LocationProfile::default(),
        ),
    );
    model.agents.insert(
        "agent-0".to_string(),
        Agent::new_with_power("agent-0", "origin", location_pos, &config.power),
    );
    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: 9,
        asteroid_fragment_enabled: true,
        asteroid_fragment_seed_offset: 1,
        min_fragment_spacing_cm: None,
    };
    let live = WorldKernel::with_model_and_chunk_runtime(config.clone(), model, chunk_runtime);
    let live_before = live.clone();
    let mut submitted = live.clone();
    submitted.submit_action(Action::HarvestRadiation {
        agent_id: "agent-0".to_string(),
        max_amount: 20,
    });
    let event = submitted.step().expect("submit harvest");
    let submitted_gain = match event.kind {
        WorldEventKind::RadiationHarvested { amount, .. } if amount > 0 => amount,
        other => panic!("expected positive radiation harvest, got {other:?}"),
    };
    let projection = live.clone();
    let quote = projection
        .quote_harvest_radiation_survival(
            &ResourceOwner::Agent {
                agent_id: "agent-0".to_string(),
            },
            20,
        )
        .expect("detached quote generates the action chunk before estimating harvest");
    assert_eq!(quote.power_gain_estimate, submitted_gain);
    assert_eq!(live_before, live);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn action_chunk_generation_consumes_boundary_reservations() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.space = SpaceConfig {
        width_cm: 4_000_000,
        depth_cm: 2_000_000,
        height_cm: 1_000_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 0.005;
    config.asteroid_fragment.voxel_size_km = 20;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 1_000;
    config.asteroid_fragment.radius_max_cm = 1_000;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;

    let mut init = WorldInitConfig::default();
    init.seed = 1337;
    init.origin.enabled = false;
    init.agents.count = 0;
    init.asteroid_fragment.min_fragment_spacing_cm = Some(2_000_000);
    init.asteroid_fragment.bootstrap_chunks = vec![ChunkCoord { x: 0, y: 0, z: 0 }];

    let (mut kernel, _) = initialize_kernel(config, init).expect("init kernel");
    let right_coord = ChunkCoord { x: 1, y: 0, z: 0 };
    assert!(
        kernel
            .model()
            .chunk_boundary_reservations
            .get(&right_coord)
            .is_some_and(|entries| !entries.is_empty())
    );

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-left".to_string(),
        name: "left".to_string(),
        pos: GeoPos {
            x_cm: 100_000,
            y_cm: 1_000_000,
            z_cm: 500_000,
        },
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register left location");

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-right".to_string(),
        name: "right".to_string(),
        pos: GeoPos {
            x_cm: 3_000_000,
            y_cm: 1_000_000,
            z_cm: 500_000,
        },
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register right location");

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-0".to_string(),
        location_id: "loc-left".to_string(),
    });
    kernel.step().expect("register agent");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-0".to_string(),
        to: "loc-right".to_string(),
    });
    let event = kernel.step().expect("move action");
    assert!(matches!(event.kind, WorldEventKind::AgentMoved { .. }));

    assert!(
        kernel
            .model()
            .chunks
            .get(&right_coord)
            .is_some_and(|state| matches!(state, ChunkState::Generated | ChunkState::Exhausted))
    );
    assert!(
        !kernel
            .model()
            .chunk_boundary_reservations
            .contains_key(&right_coord)
    );
    assert!(kernel.journal().iter().any(|entry| {
        matches!(
            entry.kind,
            WorldEventKind::ChunkGenerated {
                cause: ChunkGenerationCause::Action,
                coord,
                ..
            } if coord == right_coord
        )
    }));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn step_replenishes_fragments_every_hundred_ticks_at_one_percent() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;
    config.asteroid_fragment.min_fragments_per_chunk = 0;
    config.asteroid_fragment.max_fragments_per_chunk = 100;
    config.asteroid_fragment.replenish_interval_ticks = 100;
    config.asteroid_fragment.replenish_percent_ppm = 10_000;

    let target_chunk = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut init = WorldInitConfig::default();
    init.seed = 123;
    init.agents.count = 0;
    init.asteroid_fragment.bootstrap_chunks = vec![target_chunk];

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let before_count = kernel
        .model()
        .locations
        .values()
        .filter(|location| {
            location.id.starts_with("frag-")
                && chunk_coord_of(location.pos, &config.space)
                    .is_some_and(|coord| coord == target_chunk)
        })
        .count();
    assert_eq!(before_count, 0);

    for i in 0..100 {
        kernel.submit_action(Action::RegisterLocation {
            location_id: format!("tick-loc-{i}"),
            name: format!("tick-loc-{i}"),
            pos: GeoPos {
                x_cm: 1000 + i as i64,
                y_cm: 2000 + i as i64,
                z_cm: 3000,
            },
            profile: LocationProfile::default(),
        });
        kernel.step().expect("step");
    }

    let replenish_events: Vec<&WorldEventKind> = kernel
        .journal()
        .iter()
        .filter_map(|event| match &event.kind {
            WorldEventKind::FragmentsReplenished { .. } => Some(&event.kind),
            _ => None,
        })
        .collect();
    assert_eq!(replenish_events.len(), 1);

    let after_count = kernel
        .model()
        .locations
        .values()
        .filter(|location| {
            location.id.starts_with("frag-")
                && chunk_coord_of(location.pos, &config.space)
                    .is_some_and(|coord| coord == target_chunk)
        })
        .count();
    assert_eq!(after_count, 1);
}

#[test]
fn kernel_closed_loop_example() {
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM,
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    let loc1_pos = pos(0, 0);
    let loc2_pos = pos(2, 2);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "plant".to_string(),
        pos: loc1_pos,
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "lab".to_string(),
        pos: loc2_pos,
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        location_id: "loc-2".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    kernel.step().unwrap();

    let agent = kernel.model().agents.get("agent-1").unwrap();
    assert_eq!(agent.location_id, "loc-2");
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn kernel_consume_fragment_resource_keeps_chunk_budget_in_sync() {
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
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 77;
    init.agents.count = 0;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let fragment = kernel
        .model()
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
        .expect("fragment element");

    let before_fragment = fragment
        .fragment_budget
        .as_ref()
        .expect("fragment budget")
        .get_remaining(element);
    let before_chunk = kernel
        .model()
        .chunk_resource_budgets
        .get(&coord)
        .expect("chunk budget")
        .get_remaining(element);
    let amount = before_fragment.clamp(1, 30);

    kernel
        .consume_fragment_resource(&fragment.id, element, amount)
        .expect("consume by kernel api");

    let after_fragment = kernel
        .model()
        .locations
        .get(&fragment.id)
        .and_then(|loc| loc.fragment_budget.as_ref())
        .expect("fragment budget after")
        .get_remaining(element);
    let after_chunk = kernel
        .model()
        .chunk_resource_budgets
        .get(&coord)
        .expect("chunk budget after")
        .get_remaining(element);

    assert_eq!(after_fragment, before_fragment - amount);
    assert_eq!(after_chunk, before_chunk - amount);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn mine_compound_consumes_fragment_budget_and_awards_owner_compound() {
    let mut config = WorldConfig::default();
    config.economy.mine_electricity_cost_per_kg = 2;
    config.economy.mine_compound_max_per_action_g = 2_000;
    config.economy.mine_compound_max_per_location_g = 10_000;
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 91;
    init.agents.count = 0;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let fragment = kernel
        .model()
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .cloned()
        .expect("fragment exists");
    let location_id = fragment.id.clone();
    let before_remaining_total = fragment
        .fragment_budget
        .as_ref()
        .expect("fragment budget")
        .remaining_by_element_g
        .values()
        .copied()
        .sum::<i64>();

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-miner".to_string(),
        location_id: location_id.clone(),
    });
    kernel.step().expect("register miner");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        ResourceKind::Electricity,
        50,
    );

    kernel.submit_action(Action::MineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        location_id: location_id.clone(),
        compound_mass_g: 2_000,
    });
    let event = kernel.step().expect("mine compound");
    match event.kind {
        WorldEventKind::CompoundMined {
            owner,
            location_id: mined_location,
            compound_mass_g,
            electricity_cost,
            extracted_elements,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "agent-miner".to_string()
                }
            );
            assert_eq!(mined_location, location_id);
            assert_eq!(compound_mass_g, 2_000);
            assert_eq!(electricity_cost, 4);
            assert_eq!(
                extracted_elements.values().copied().sum::<i64>(),
                compound_mass_g
            );
            assert!(!extracted_elements.is_empty());
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel
        .model()
        .agents
        .get("agent-miner")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 2_000);
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 46);

    let after_fragment = kernel
        .model()
        .locations
        .get(&location_id)
        .and_then(|location| location.fragment_budget.as_ref())
        .expect("fragment budget after");
    let after_remaining_total = after_fragment
        .remaining_by_element_g
        .values()
        .copied()
        .sum::<i64>();
    assert_eq!(after_remaining_total, before_remaining_total - 2_000);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn mine_compound_enforces_location_cap() {
    let mut config = WorldConfig::default();
    config.economy.mine_electricity_cost_per_kg = 1;
    config.economy.mine_compound_max_per_action_g = 1_000;
    config.economy.mine_compound_max_per_location_g = 1_500;
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 92;
    init.agents.count = 0;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("init kernel");
    let location_id = kernel
        .model()
        .locations
        .values()
        .find(|loc| loc.id.starts_with("frag-"))
        .map(|loc| loc.id.clone())
        .expect("fragment exists");

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-miner".to_string(),
        location_id: location_id.clone(),
    });
    kernel.step().expect("register miner");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );

    kernel.submit_action(Action::MineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        location_id: location_id.clone(),
        compound_mass_g: 1_000,
    });
    let first = kernel.step().expect("first mining");
    assert!(matches!(first.kind, WorldEventKind::CompoundMined { .. }));

    kernel.submit_action(Action::MineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-miner".to_string(),
        },
        location_id: location_id.clone(),
        compound_mass_g: 600,
    });
    let second = kernel.step().expect("second mining");
    match second.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::InsufficientResource {
                    owner: ResourceOwner::Location { .. },
                    kind: ResourceKind::Data,
                    requested: 600,
                    available: 500,
                }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn debug_grant_resource_adds_requested_amount_to_owner_stock() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-debug".to_string(),
        name: "debug".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-debug".to_string(),
        location_id: "loc-debug".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::DebugGrantResource {
        owner: ResourceOwner::Agent {
            agent_id: "agent-debug".to_string(),
        },
        kind: ResourceKind::Data,
        amount: 123,
    });
    let event = kernel.step().expect("debug grant");
    match event.kind {
        WorldEventKind::DebugResourceGranted {
            owner,
            kind,
            amount,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "agent-debug".to_string(),
                }
            );
            assert_eq!(kind, ResourceKind::Data);
            assert_eq!(amount, 123);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel
        .model()
        .agents
        .get("agent-debug")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 123);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn refine_compound_consumes_electricity_and_outputs_hardware() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 3;
    config.economy.refine_hardware_yield_ppm = 2_000;

    let mut kernel = WorldKernel::with_config(config);
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 120;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-refiner".to_string(),
        max_amount: 50,
    });
    kernel.step().expect("seed electricity");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        ResourceKind::Data,
        2_500,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        compound_mass_g: 2_500,
    });

    let event = kernel.step().expect("refine action");
    match event.kind {
        WorldEventKind::CompoundRefined {
            owner,
            compound_mass_g,
            electricity_cost,
            hardware_output,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "agent-refiner".to_string()
                }
            );
            assert_eq!(compound_mass_g, 2_500);
            assert_eq!(electricity_cost, 9);
            assert_eq!(hardware_output, 5);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel
        .model()
        .agents
        .get("agent-refiner")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 41);
    assert_eq!(agent.resources.get(ResourceKind::Data), 5);
}

#[test]
fn refine_compound_rejects_when_compound_insufficient() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 4;
    config.economy.refine_hardware_yield_ppm = 1_000;

    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        ResourceKind::Electricity,
        100,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        compound_mass_g: 1_500,
    });

    let event = kernel.step().expect("refine rejected");
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::InsufficientResource {
                    kind: ResourceKind::Data,
                    requested: 1_500,
                    available: 0,
                    ..
                }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn refine_compound_rejects_when_electricity_insufficient() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 4;
    config.economy.refine_hardware_yield_ppm = 1_000;

    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        ResourceKind::Data,
        1_500,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-refiner".to_string(),
        },
        compound_mass_g: 1_500,
    });

    let event = kernel.step().expect("refine rejected");
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::InsufficientResource {
                    kind: ResourceKind::Electricity,
                    requested: 8,
                    available: 0,
                    ..
                }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn build_factory_consumes_resources_and_persists_factory_state() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 7;
    config.economy.factory_build_hardware_cost = 3;
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;

    let mut kernel = WorldKernel::with_config(config);
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 120;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-factory".to_string(),
        name: "factory-site".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-builder".to_string(),
        location_id: "loc-factory".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-builder".to_string(),
        max_amount: 30,
    });
    kernel.step().expect("harvest for factory build");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        ResourceKind::Data,
        3_000,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        compound_mass_g: 3_000,
    });
    kernel.step().expect("refine hardware for factory build");

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        location_id: "loc-factory".to_string(),
        factory_id: "factory.alpha".to_string(),
        factory_kind: "factory.assembler.mk1".to_string(),
    });

    let event = kernel.step().expect("build factory");
    match event.kind {
        WorldEventKind::FactoryBuilt {
            owner,
            location_id,
            factory_id,
            factory_kind,
            electricity_cost,
            hardware_cost,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "agent-builder".to_string()
                }
            );
            assert_eq!(location_id, "loc-factory");
            assert_eq!(factory_id, "factory.alpha");
            assert_eq!(factory_kind, "factory.assembler.mk1");
            assert_eq!(electricity_cost, 7);
            assert_eq!(hardware_cost, 3);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel
        .model()
        .agents
        .get("agent-builder")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 20);
    assert_eq!(agent.resources.get(ResourceKind::Data), 0);
    assert!(kernel.model().factories.contains_key("factory.alpha"));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn schedule_recipe_consumes_inputs_and_outputs_data() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;
    config.economy.recipe_electricity_cost_per_batch = 5;
    config.economy.recipe_hardware_cost_per_batch = 2;
    config.economy.recipe_data_output_per_batch = 3;
    config.physics.max_harvest_per_tick = 100;

    let mut kernel = WorldKernel::with_config(config);
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 200;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-factory".to_string(),
        name: "factory-site".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-builder".to_string(),
        location_id: "loc-factory".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-builder".to_string(),
        max_amount: 80,
    });
    kernel.step().expect("harvest for recipe");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        ResourceKind::Data,
        16_000,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        compound_mass_g: 16_000,
    });
    kernel.step().expect("refine hardware for recipe");

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        location_id: "loc-factory".to_string(),
        factory_id: "factory.alpha".to_string(),
        factory_kind: "factory.assembler.mk1".to_string(),
    });
    kernel.step().expect("build factory");

    kernel.submit_action(Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        batches: 2,
    });
    let event = kernel.step().expect("schedule recipe");
    match event.kind {
        WorldEventKind::RecipeScheduled {
            owner,
            factory_id,
            recipe_id,
            batches,
            electricity_cost,
            hardware_cost,
            data_output,
            finished_product_id,
            finished_product_units,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "agent-builder".to_string()
                }
            );
            assert_eq!(factory_id, "factory.alpha");
            assert_eq!(recipe_id, "recipe.assembler.logistics_drone");
            assert_eq!(batches, 2);
            assert_eq!(electricity_cost, 40);
            assert_eq!(hardware_cost, 16);
            assert_eq!(data_output, 24);
            assert_eq!(finished_product_id, "logistics_drone");
            assert_eq!(finished_product_units, 2);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel
        .model()
        .agents
        .get("agent-builder")
        .expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 24);
    assert_eq!(agent.resources.get(ResourceKind::Data), 24);
}
