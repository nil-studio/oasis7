use super::*;

#[test]
fn harvest_radiation_available_decreases_with_distance() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;
    config.physics.radiation_decay_k = 0.0;
    config.physics.max_harvest_per_tick = 10_000;
    config.physics.thermal_capacity = 10_000;
    config.physics.heat_factor = 0;
    let mut kernel = WorldKernel::with_config(config);

    let source_profile = LocationProfile {
        material: MaterialKind::Metal,
        radius_cm: 100_000,
        radiation_emission_per_tick: 200,
    };

    kernel.submit_action(Action::RegisterLocation {
        location_id: "source".to_string(),
        name: "source".to_string(),
        pos: pos(0, 0),
        profile: source_profile,
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "near-site".to_string(),
        name: "near-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "far-site".to_string(),
        name: "far-site".to_string(),
        pos: pos(200_000, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-near".to_string(),
        location_id: "near-site".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-far".to_string(),
        location_id: "far-site".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-near".to_string(),
        max_amount: 10_000,
    });
    let near_available = match kernel.step().expect("near harvest event").kind {
        WorldEventKind::RadiationHarvested { available, .. } => available,
        other => panic!("unexpected event: {other:?}"),
    };

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-far".to_string(),
        max_amount: 10_000,
    });
    let far_available = match kernel.step().expect("far harvest event").kind {
        WorldEventKind::RadiationHarvested { available, .. } => available,
        other => panic!("unexpected event: {other:?}"),
    };

    assert!(near_available > far_available);
    assert!(far_available > 0);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn harvest_radiation_amount_does_not_increase_when_overheated() {
    let mut config = WorldConfig::default();
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;
    config.physics.max_harvest_per_tick = 100;
    config.physics.thermal_capacity = 5;
    config.physics.thermal_dissipation = 0;
    config.physics.heat_factor = 1;
    let mut kernel = WorldKernel::with_config(config);

    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 100;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "site".to_string(),
        name: "site".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "site".to_string(),
    });
    kernel.step_until_empty();

    let mut harvested = Vec::new();
    for _ in 0..3 {
        kernel.submit_action(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 10,
        });
        let event = kernel.step().expect("harvest event");
        let amount = match event.kind {
            WorldEventKind::RadiationHarvested { amount, .. } => amount,
            other => panic!("unexpected event: {other:?}"),
        };
        harvested.push(amount);
    }

    assert_eq!(harvested[0], 10);
    assert!(harvested[1] <= harvested[0]);
    assert!(harvested[2] <= harvested[1]);
}

#[test]
fn movement_cost_is_non_decreasing_with_distance() {
    let config = WorldConfig::default();
    let sample_distances = [
        0,
        1,
        CM_PER_KM / 2,
        CM_PER_KM,
        CM_PER_KM + 1,
        CM_PER_KM * 2,
        CM_PER_KM * 5,
    ];

    let mut prev_cost = 0;
    for distance_cm in sample_distances {
        let cost = config.movement_cost(distance_cm);
        assert!(
            cost >= prev_cost,
            "movement cost should be non-decreasing: distance={} cost={} prev_cost={}",
            distance_cm,
            cost,
            prev_cost
        );
        prev_cost = cost;
    }
}
