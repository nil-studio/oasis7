use super::*;

fn event_kind_json(event: &WorldEvent) -> serde_json::Value {
    serde_json::to_value(&event.kind).expect("serialize event kind")
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn kernel_action_behavior_snapshot_stays_stable() {
    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 0;
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;

    let mut kernel = WorldKernel::with_config(config);

    let mut rich_profile = LocationProfile::default();
    rich_profile.radiation_emission_per_tick = 20;

    let mut kinds = Vec::new();

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-a".to_string(),
        name: "origin".to_string(),
        pos: pos(0, 0),
        profile: rich_profile,
    });
    kinds.push(event_kind_json(&kernel.step().expect("register loc-a")));

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-b".to_string(),
        name: "target".to_string(),
        pos: pos(100_000, 0),
        profile: LocationProfile::default(),
    });
    kinds.push(event_kind_json(&kernel.step().expect("register loc-b")));

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-a".to_string(),
    });
    kinds.push(event_kind_json(&kernel.step().expect("register agent")));

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 5,
    });
    kinds.push(event_kind_json(&kernel.step().expect("harvest")));

    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        ResourceKind::Data,
        3,
    );

    kernel.submit_action(Action::TransferResource {
        from: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        to: ResourceOwner::Location {
            location_id: "loc-a".to_string(),
        },
        kind: ResourceKind::Data,
        amount: 3,
    });
    kinds.push(event_kind_json(&kernel.step().expect("transfer")));
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        ResourceKind::Data,
        1_000,
    );

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        compound_mass_g: 1_000,
    });
    kinds.push(event_kind_json(&kernel.step().expect("refine")));

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-b".to_string(),
    });
    kinds.push(event_kind_json(&kernel.step().expect("move")));

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-b".to_string(),
    });
    kinds.push(event_kind_json(
        &kernel.step().expect("reject move to same location"),
    ));

    kernel.submit_action(Action::TransferResource {
        from: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        to: ResourceOwner::Location {
            location_id: "loc-b".to_string(),
        },
        kind: ResourceKind::Electricity,
        amount: 0,
    });
    kinds.push(event_kind_json(
        &kernel.step().expect("reject transfer invalid amount"),
    ));

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 0,
    });
    kinds.push(event_kind_json(
        &kernel.step().expect("reject harvest invalid amount"),
    ));

    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        compound_mass_g: 0,
    });
    kinds.push(event_kind_json(
        &kernel.step().expect("reject refine invalid amount"),
    ));

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/kernel_action_behavior_snapshot.json"
    ))
    .expect("parse fixed kernel behavior snapshot");

    let expected: Vec<serde_json::Value> = expected.as_array().expect("expected array").clone();
    assert_eq!(kinds, expected);

    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.location_id, "loc-b");
    assert_eq!(agent.resources.get(ResourceKind::Electricity), 3);
    assert_eq!(agent.resources.get(ResourceKind::Data), 1);

    let origin = kernel
        .model()
        .locations
        .get("loc-a")
        .expect("origin exists");
    assert_eq!(origin.resources.get(ResourceKind::Electricity), 0);
    assert_eq!(origin.resources.get(ResourceKind::Data), 3);
}
