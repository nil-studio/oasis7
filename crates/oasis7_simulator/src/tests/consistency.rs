use super::*;

fn run_action_sequence(kernel: &mut WorldKernel, actions: &[Action]) {
    for action in actions {
        kernel.submit_action(action.clone());
        kernel.step().expect("action event");
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn replay_from_snapshot_matches_same_seed_and_action_sequence() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 1;
    config.physics.heat_factor = 0;

    let mut init = WorldInitConfig::default();
    init.seed = 123;
    init.agents.count = 1;
    init.asteroid_fragment.enabled = false;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("initialize kernel");

    let actions = [
        Action::HarvestRadiation {
            agent_id: "agent-0".to_string(),
            max_amount: 10,
        },
        Action::HarvestRadiation {
            agent_id: "agent-0".to_string(),
            max_amount: 10,
        },
        Action::RefineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-0".to_string(),
            },
            compound_mass_g: 1_000,
        },
    ];

    run_action_sequence(&mut kernel, &actions[..1]);
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-0".to_string(),
        },
        ResourceKind::Data,
        1_000,
    );
    let mid_snapshot = kernel.kernel_snapshot();

    run_action_sequence(&mut kernel, &actions[1..]);

    let journal = kernel.kernel_journal_snapshot();
    let replayed = WorldKernel::replay_from_kernel_snapshot(mid_snapshot, journal.clone())
        .expect("replay from snapshot");
    assert_eq!(replayed.model(), kernel.model());

    let restored = WorldKernel::from_kernel_snapshot(kernel.kernel_snapshot(), journal)
        .expect("restore snapshot");
    assert_eq!(restored.model(), kernel.model());
}

#[test]
fn replay_from_snapshot_matches_power_orderbook_sequence() {
    let mut config = WorldConfig::default();
    config.power.market_base_price_per_pu = 2;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "hub".to_string(),
        name: "hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        ResourceKind::Electricity,
        30,
    );

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 20,
        limit_price_per_pu: 2,
    });
    let placed_event = kernel.step().expect("place sell order");
    let sell_order_id = match placed_event.kind {
        WorldEventKind::PowerOrderPlaced { order_id, .. } => order_id,
        other => panic!("expected power order placed, got {other:?}"),
    };

    let mid_snapshot = kernel.kernel_snapshot();

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 7,
        limit_price_per_pu: 5,
    });
    kernel.step().expect("first buy");

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 5,
        limit_price_per_pu: 5,
    });
    kernel.step().expect("second buy");

    kernel.submit_action(Action::CancelPowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        order_id: sell_order_id,
    });
    kernel.step().expect("cancel remainder");

    let journal = kernel.kernel_journal_snapshot();
    let replayed = WorldKernel::replay_from_kernel_snapshot(mid_snapshot, journal.clone())
        .expect("replay from snapshot with orderbook events");
    assert_eq!(replayed.model(), kernel.model());

    let restored = WorldKernel::from_kernel_snapshot(kernel.kernel_snapshot(), journal)
        .expect("restore snapshot orderbook");
    assert_eq!(restored.model(), kernel.model());
}
