use super::*;
use crate::geometry::{DEFAULT_CLOUD_WIDTH_CM, GeoPos, space_distance_cm};
use std::sync::{Arc, Mutex};

#[test]
fn kernel_registers_and_moves_agent() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 1),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step().unwrap();
    let starting_energy = 500;
    kernel.model().agents.get("agent-1").unwrap();
    let mut kernel2 = WorldKernel::new();
    kernel2.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel2.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 1),
        profile: LocationProfile::default(),
    });
    kernel2.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel2.step_until_empty();

    kernel2.submit_action(Action::TransferResource {
        from: ResourceOwner::Location {
            location_id: "loc-1".to_string(),
        },
        to: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        kind: ResourceKind::Electricity,
        amount: starting_energy,
    });
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM,
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 1),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::AgentMoved {
            agent_id,
            from,
            to,
            distance_cm,
            electricity_cost,
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(from, "loc-1");
            assert_eq!(to, "loc-2");
            assert!(distance_cm > 0);
            assert_eq!(electricity_cost, 0);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel.model().agents.get("agent-1").unwrap();
    assert_eq!(agent.location_id, "loc-2");
    assert_eq!(agent.pos, pos(1, 1));
}

#[test]
fn kernel_move_requires_energy() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(reason, RejectReason::InsufficientResource { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn register_location_rejects_out_of_bounds() {
    let mut kernel = WorldKernel::new();
    let out_of_bounds = GeoPos {
        x_cm: DEFAULT_CLOUD_WIDTH_CM + 1,
        y_cm: 0,
        z_cm: 0,
    };
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-oob".to_string(),
        name: "void".to_string(),
        pos: out_of_bounds,
        profile: LocationProfile::default(),
    });
    let event = kernel.step().unwrap();
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::PositionOutOfBounds { .. }
        }
    ));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn harvest_radiation_adds_electricity() {
    let mut kernel = WorldKernel::new();
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 50;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-rad".to_string(),
        name: "rad".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-rad".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 20,
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::RadiationHarvested {
            amount, available, ..
        } => {
            assert_eq!(amount, 20);
            assert_eq!(available, 51);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let agent = kernel.model().agents.get("agent-1").unwrap();
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 20);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn harvest_radiation_respects_max_per_tick() {
    let mut config = WorldConfig::default();
    config.physics.max_harvest_per_tick = 5;
    let mut kernel = WorldKernel::with_config(config);

    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 50;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-rad".to_string(),
        name: "rad".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-rad".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 20,
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::RadiationHarvested { amount, .. } => {
            assert_eq!(amount, 5);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn harvest_radiation_applies_thermal_penalty() {
    let mut config = WorldConfig::default();
    config.physics.thermal_capacity = 5;
    config.physics.heat_factor = 1;
    config.physics.max_harvest_per_tick = 100;
    let mut kernel = WorldKernel::with_config(config);

    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 50;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-rad".to_string(),
        name: "rad".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-rad".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10,
    });
    let _ = kernel.step().unwrap();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10,
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::RadiationHarvested { amount, .. } => {
            assert!(amount < 10);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn harvest_radiation_includes_nearby_sources_and_distance_decay() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 0;
    config.physics.radiation_decay_k = 0.0;
    config.physics.max_harvest_per_tick = 10_000;
    let mut kernel = WorldKernel::with_config(config);

    let source_near = LocationProfile {
        material: MaterialKind::Metal,
        radius_cm: 100,
        radiation_emission_per_tick: 90,
    };
    let source_far = LocationProfile {
        material: MaterialKind::Metal,
        radius_cm: 100,
        radiation_emission_per_tick: 90,
    };

    kernel.submit_action(Action::RegisterLocation {
        location_id: "harvest-site".to_string(),
        name: "harvest-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "source-near".to_string(),
        name: "source-near".to_string(),
        pos: pos(100, 0),
        profile: source_near,
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "source-far".to_string(),
        name: "source-far".to_string(),
        pos: pos(2_000, 0),
        profile: source_far,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "harvest-site".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10_000,
    });

    let event = kernel.step().unwrap();
    let available = match event.kind {
        WorldEventKind::RadiationHarvested { available, .. } => available,
        other => panic!("unexpected event: {other:?}"),
    };

    assert!(available > 0);
    assert!(available < 180);
}

#[test]
fn harvest_radiation_uses_background_floor_when_no_source() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 3;
    config.physics.max_harvest_per_tick = 10;
    let mut kernel = WorldKernel::with_config(config);

    kernel.submit_action(Action::RegisterLocation {
        location_id: "site".to_string(),
        name: "site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "site".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10,
    });

    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::RadiationHarvested {
            amount, available, ..
        } => {
            assert_eq!(available, 3);
            assert_eq!(amount, 3);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn harvest_radiation_caps_background_floor_when_no_source() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 20;
    config.physics.radiation_floor_cap_per_tick = 4;
    config.physics.max_harvest_per_tick = 10;
    let mut kernel = WorldKernel::with_config(config);

    kernel.submit_action(Action::RegisterLocation {
        location_id: "site".to_string(),
        name: "site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "site".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10,
    });

    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::RadiationHarvested {
            amount, available, ..
        } => {
            assert_eq!(available, 4);
            assert_eq!(amount, 4);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn kernel_rejects_move_beyond_max_distance_per_tick() {
    let mut config = WorldConfig::default();
    config.physics.max_move_distance_cm_per_tick = 100;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "far".to_string(),
        pos: pos(101, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::MoveDistanceExceeded {
                    distance_cm: 101,
                    max_distance_cm: 100,
                }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn kernel_rejects_move_beyond_max_speed() {
    let mut config = WorldConfig::default();
    config.physics.time_step_s = 1;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = 100;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "fast".to_string(),
        pos: pos(101, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::MoveSpeedExceeded {
                    required_speed_cm_per_s: 101,
                    max_speed_cm_per_s: 100,
                    time_step_s: 1,
                }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn kernel_rejects_move_to_same_location() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-1".to_string(),
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(
                reason,
                RejectReason::AgentAlreadyAtLocation { .. }
            ));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn kernel_move_agent_progresses_over_multiple_ticks_when_speed_is_low() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;

    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "target".to_string(),
        pos: pos(300, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let mut snapshot = kernel.kernel_snapshot();
    snapshot
        .model
        .agents
        .get_mut("agent-1")
        .expect("agent exists")
        .kinematics
        .speed_cm_per_tick = 100;
    kernel = WorldKernel::from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
        .expect("restore");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });

    let first = kernel.step().expect("first move segment");
    assert!(matches!(
        first.kind,
        WorldEventKind::AgentMoved {
            distance_cm: 100,
            electricity_cost: 0,
            ..
        }
    ));
    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.location_id, "loc-1");

    let second = kernel.step().expect("second move segment");
    assert!(matches!(
        second.kind,
        WorldEventKind::AgentMoved {
            distance_cm: 100,
            electricity_cost: 0,
            ..
        }
    ));
    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.location_id, "loc-1");

    let third = kernel.step().expect("arrival segment");
    assert!(matches!(
        third.kind,
        WorldEventKind::AgentMoved {
            distance_cm: 100,
            electricity_cost: 0,
            ..
        }
    ));
    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.location_id, "loc-2");
    assert_eq!(agent.pos, pos(300, 0));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn kernel_rejects_move_with_non_positive_agent_speed() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;

    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "target".to_string(),
        pos: pos(10, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let mut snapshot = kernel.kernel_snapshot();
    snapshot
        .model
        .agents
        .get_mut("agent-1")
        .expect("agent exists")
        .kinematics
        .speed_cm_per_tick = 0;
    kernel = WorldKernel::from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
        .expect("restore");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().expect("reject event");
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::InvalidAmount { amount: 0 }
        }
    ));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn kernel_segmented_move_keeps_agent_on_centimeter_grid() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    config.physics.time_step_s = 1;

    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "origin".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "diagonal".to_string(),
        pos: pos(300, 300),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let mut snapshot = kernel.kernel_snapshot();
    snapshot
        .model
        .agents
        .get_mut("agent-1")
        .expect("agent exists")
        .kinematics
        .speed_cm_per_tick = 100;
    kernel = WorldKernel::from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
        .expect("restore");

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });

    let _ = kernel.step().expect("first move segment");
    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert!(agent.pos.x_cm > 0 || agent.pos.y_cm > 0);
    assert_eq!(agent.pos.z_cm, 0);
    assert!(space_distance_cm(pos(0, 0), agent.pos) <= 100);
}

#[test]
fn kernel_observe_visibility_range() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "far".to_string(),
        pos: pos(DEFAULT_VISIBILITY_RANGE_CM + 1, 0),
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

    let obs = kernel.observe("agent-1").unwrap();
    assert!(obs.visible_agents.is_empty());
    assert!(
        obs.visible_locations
            .iter()
            .any(|loc| loc.location_id == "loc-1")
    );
    assert!(
        !obs.visible_locations
            .iter()
            .any(|loc| loc.location_id == "loc-2")
    );
}

#[test]
fn kernel_observe_includes_module_power_and_social_snapshots() {
    let config = WorldConfig::default();
    let mut model = WorldModel::default();
    let origin = pos(0, 0);
    model.locations.insert(
        "loc-1".to_string(),
        Location::new_with_profile(
            "loc-1".to_string(),
            "origin".to_string(),
            origin,
            LocationProfile::default(),
        ),
    );
    model.agents.insert(
        "agent-1".to_string(),
        Agent::new_with_power("agent-1", "loc-1", origin, &config.power),
    );
    model.module_artifacts.insert(
        "hash-z".to_string(),
        ModuleArtifactState {
            wasm_hash: "hash-z".to_string(),
            publisher_agent_id: "agent-1".to_string(),
            module_id_hint: Some("m.z".to_string()),
            wasm_bytes: vec![1, 2, 3],
            deployed_at_tick: 8,
        },
    );
    model.module_artifacts.insert(
        "hash-a".to_string(),
        ModuleArtifactState {
            wasm_hash: "hash-a".to_string(),
            publisher_agent_id: "agent-1".to_string(),
            module_id_hint: Some("m.a".to_string()),
            wasm_bytes: vec![4, 5],
            deployed_at_tick: 7,
        },
    );
    model.installed_modules.insert(
        "m.z".to_string(),
        InstalledModuleState {
            module_id: "m.z".to_string(),
            module_version: "0.2.0".to_string(),
            wasm_hash: "hash-z".to_string(),
            installer_agent_id: "agent-1".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: true,
            installed_at_tick: 9,
        },
    );
    model.installed_modules.insert(
        "m.a".to_string(),
        InstalledModuleState {
            module_id: "m.a".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "hash-a".to_string(),
            installer_agent_id: "agent-1".to_string(),
            install_target: ModuleInstallTarget::SelfAgent,
            active: false,
            installed_at_tick: 10,
        },
    );
    model.module_artifact_listings.insert(
        "5".to_string(),
        ModuleArtifactListingState {
            order_id: 5,
            wasm_hash: "hash-z".to_string(),
            seller_agent_id: "agent-1".to_string(),
            price_kind: ResourceKind::Data,
            price_amount: 9,
            listed_at_tick: 11,
        },
    );
    model.module_artifact_listings.insert(
        "2".to_string(),
        ModuleArtifactListingState {
            order_id: 2,
            wasm_hash: "hash-a".to_string(),
            seller_agent_id: "agent-1".to_string(),
            price_kind: ResourceKind::Electricity,
            price_amount: 3,
            listed_at_tick: 12,
        },
    );
    model.module_artifact_bids.insert(
        "hash-a".to_string(),
        vec![ModuleArtifactBidState {
            order_id: 6,
            wasm_hash: "hash-a".to_string(),
            bidder_agent_id: "agent-2".to_string(),
            price_kind: ResourceKind::Data,
            price_amount: 4,
            placed_at_tick: 13,
        }],
    );
    model.module_artifact_bids.insert(
        "hash-z".to_string(),
        vec![ModuleArtifactBidState {
            order_id: 4,
            wasm_hash: "hash-z".to_string(),
            bidder_agent_id: "agent-3".to_string(),
            price_kind: ResourceKind::Electricity,
            price_amount: 2,
            placed_at_tick: 14,
        }],
    );
    model.power_order_book.next_order_id = 12;
    model.power_order_book.open_orders = vec![
        PowerOrderState {
            order_id: 9,
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            side: PowerOrderSide::Buy,
            remaining_amount: 5,
            limit_price_per_pu: 2,
            created_at: 15,
        },
        PowerOrderState {
            order_id: 1,
            owner: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            side: PowerOrderSide::Sell,
            remaining_amount: 8,
            limit_price_per_pu: 3,
            created_at: 16,
        },
    ];
    model.social_facts.insert(
        2,
        SocialFactState {
            fact_id: 2,
            actor: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            schema_id: "s.alliance".to_string(),
            subject: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            object: None,
            claim: "allied".to_string(),
            confidence_ppm: 900_000,
            evidence_event_ids: vec![1],
            ttl_ticks: None,
            expires_at_tick: None,
            stake: None,
            challenge: None,
            lifecycle: SocialFactLifecycleState::Active,
            created_at_tick: 17,
            updated_at_tick: 17,
        },
    );
    model.social_facts.insert(
        1,
        SocialFactState {
            fact_id: 1,
            actor: ResourceOwner::Agent {
                agent_id: "agent-3".to_string(),
            },
            schema_id: "s.risk".to_string(),
            subject: ResourceOwner::Agent {
                agent_id: "agent-4".to_string(),
            },
            object: None,
            claim: "risk".to_string(),
            confidence_ppm: 500_000,
            evidence_event_ids: vec![2],
            ttl_ticks: None,
            expires_at_tick: None,
            stake: None,
            challenge: None,
            lifecycle: SocialFactLifecycleState::Revoked,
            created_at_tick: 18,
            updated_at_tick: 18,
        },
    );
    model.social_edges.insert(
        8,
        SocialEdgeState {
            edge_id: 8,
            declarer: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            schema_id: "s.network".to_string(),
            relation_kind: "ally".to_string(),
            from: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            weight_bps: 5_000,
            backing_fact_ids: vec![2],
            ttl_ticks: None,
            expires_at_tick: None,
            lifecycle: SocialEdgeLifecycleState::Active,
            created_at_tick: 19,
            updated_at_tick: 19,
        },
    );
    model.social_edges.insert(
        3,
        SocialEdgeState {
            edge_id: 3,
            declarer: ResourceOwner::Agent {
                agent_id: "agent-3".to_string(),
            },
            schema_id: "s.network".to_string(),
            relation_kind: "former-ally".to_string(),
            from: ResourceOwner::Agent {
                agent_id: "agent-3".to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: "agent-4".to_string(),
            },
            weight_bps: 2_500,
            backing_fact_ids: vec![1],
            ttl_ticks: None,
            expires_at_tick: None,
            lifecycle: SocialEdgeLifecycleState::Expired,
            created_at_tick: 20,
            updated_at_tick: 20,
        },
    );

    let mut kernel = WorldKernel::with_model(config, model);
    let observation = kernel.observe("agent-1").expect("observe");

    assert_eq!(observation.module_lifecycle.artifacts.len(), 2);
    assert_eq!(
        observation.module_lifecycle.artifacts[0].wasm_hash,
        "hash-a"
    );
    assert_eq!(
        observation.module_lifecycle.artifacts[1].wasm_hash,
        "hash-z"
    );
    assert_eq!(observation.module_lifecycle.installed_modules.len(), 2);
    assert_eq!(
        observation.module_lifecycle.installed_modules[0].module_id,
        "m.a"
    );
    assert_eq!(
        observation.module_lifecycle.installed_modules[1].module_id,
        "m.z"
    );

    assert_eq!(observation.module_market.listings.len(), 2);
    assert_eq!(observation.module_market.listings[0].order_id, 2);
    assert_eq!(observation.module_market.listings[1].order_id, 5);
    assert_eq!(observation.module_market.bids.len(), 2);
    assert_eq!(observation.module_market.bids[0].order_id, 4);
    assert_eq!(observation.module_market.bids[1].order_id, 6);

    assert_eq!(observation.power_market.next_order_id, 12);
    assert_eq!(observation.power_market.open_orders.len(), 2);
    assert_eq!(observation.power_market.open_orders[0].order_id, 1);
    assert_eq!(observation.power_market.open_orders[1].order_id, 9);

    assert_eq!(observation.social_state.facts.len(), 2);
    assert_eq!(observation.social_state.facts[0].fact_id, 1);
    assert_eq!(observation.social_state.facts[1].fact_id, 2);
    assert_eq!(observation.social_state.edges.len(), 2);
    assert_eq!(observation.social_state.edges[0].edge_id, 3);
    assert_eq!(observation.social_state.edges[1].edge_id, 8);
}

#[test]
fn kernel_config_overrides_defaults() {
    let config = WorldConfig {
        visibility_range_cm: DEFAULT_VISIBILITY_RANGE_CM * 2,
        move_cost_per_km_electricity: DEFAULT_MOVE_COST_PER_KM_ELECTRICITY * 2,
        ..Default::default()
    };
    let kernel = WorldKernel::with_config(config);
    assert_eq!(
        kernel.config().visibility_range_cm,
        DEFAULT_VISIBILITY_RANGE_CM * 2
    );
    assert_eq!(
        kernel.config().move_cost_per_km_electricity,
        DEFAULT_MOVE_COST_PER_KM_ELECTRICITY * 2
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn movement_cost_scales_with_time_step_and_power_unit() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 2;

    assert_eq!(config.movement_cost(CM_PER_KM), 2);

    config.physics.time_step_s = 20;
    assert_eq!(config.movement_cost(CM_PER_KM), 4);

    config.physics.power_unit_j = 2_000;
    assert_eq!(config.movement_cost(CM_PER_KM), 2);

    config.physics.power_unit_j = 500;
    assert_eq!(config.movement_cost(CM_PER_KM), 8);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn movement_cost_uses_calibrated_per_km_in_move_action() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 2;
    config.physics.time_step_s = 20;
    config.physics.power_unit_j = 2_000;
    config.physics.max_move_distance_cm_per_tick = i64::MAX;
    config.physics.max_move_speed_cm_per_s = i64::MAX;
    config.physics.max_harvest_per_tick = 50;
    let mut kernel = WorldKernel::with_config(config);

    let mut source_profile = LocationProfile::default();
    source_profile.radiation_emission_per_tick = 100;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: source_profile,
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(CM_PER_KM, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 10,
    });
    let _ = kernel.step().unwrap();

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });

    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::AgentMoved {
            electricity_cost, ..
        } => {
            assert_eq!(electricity_cost, 2);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn kernel_transfer_requires_colocation() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(10, 10),
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

    kernel.submit_action(Action::TransferResource {
        from: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        to: ResourceOwner::Agent {
            agent_id: "agent-2".to_string(),
        },
        kind: ResourceKind::Electricity,
        amount: 10,
    });
    let event = kernel.step().unwrap();
    match event.kind {
        WorldEventKind::ActionRejected { reason } => {
            assert!(matches!(reason, RejectReason::AgentsNotCoLocated { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
