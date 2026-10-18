use super::*;

#[test]
fn power_idle_consumption_depletes_agent() {
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

    let events = kernel.process_power_tick();
    assert!(!events.is_empty());
}

#[test]
fn power_tick_preserves_agent_event_order() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-b".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let consumed_agent_ids = kernel
        .process_power_tick()
        .into_iter()
        .filter_map(|event| match event.kind {
            WorldEventKind::Power(PowerEvent::PowerConsumed { agent_id, .. }) => Some(agent_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(consumed_agent_ids, vec!["agent-a", "agent-b"]);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn power_tick_dissipates_more_heat_when_hotter() {
    let mut config = WorldConfig::default();
    config.power.idle_cost_per_tick = 0;
    config.physics.radiation_floor = 0;
    config.physics.max_harvest_per_tick = 100;
    config.physics.thermal_capacity = 100;
    config.physics.thermal_dissipation = 10;
    config.physics.thermal_dissipation_gradient_bps = 10_000;
    let mut kernel = WorldKernel::with_config(config);

    let mut low_profile = LocationProfile::default();
    low_profile.radiation_emission_per_tick = 5;
    let mut high_profile = LocationProfile::default();
    high_profile.radiation_emission_per_tick = 40;

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-low".to_string(),
        name: "low".to_string(),
        pos: pos(0, 0),
        profile: low_profile,
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-high".to_string(),
        name: "high".to_string(),
        pos: pos(10_000_000, 0),
        profile: high_profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-low".to_string(),
        location_id: "loc-low".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-high".to_string(),
        location_id: "loc-high".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-low".to_string(),
        max_amount: 100,
    });
    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-high".to_string(),
        max_amount: 100,
    });
    kernel.step_until_empty();

    let low_before = kernel
        .model()
        .agents
        .get("agent-low")
        .expect("agent-low exists")
        .thermal
        .heat;
    let high_before = kernel
        .model()
        .agents
        .get("agent-high")
        .expect("agent-high exists")
        .thermal
        .heat;

    assert!(high_before > low_before);

    kernel.process_power_tick();

    let low_after = kernel
        .model()
        .agents
        .get("agent-low")
        .expect("agent-low exists")
        .thermal
        .heat;
    let high_after = kernel
        .model()
        .agents
        .get("agent-high")
        .expect("agent-high exists")
        .thermal
        .heat;

    let low_drop = low_before.saturating_sub(low_after);
    let high_drop = high_before.saturating_sub(high_after);

    assert!(high_drop > low_drop);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn power_tick_thermal_dissipation_never_makes_heat_negative() {
    let mut config = WorldConfig::default();
    config.power.idle_cost_per_tick = 0;
    config.physics.radiation_floor = 0;
    config.physics.max_harvest_per_tick = 100;
    config.physics.thermal_capacity = 100;
    config.physics.thermal_dissipation = 50;
    config.physics.thermal_dissipation_gradient_bps = 20_000;
    let mut kernel = WorldKernel::with_config(config);

    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 1;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-hot".to_string(),
        name: "hot".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-hot".to_string(),
        location_id: "loc-hot".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-hot".to_string(),
        max_amount: 100,
    });
    kernel.step_until_empty();

    kernel.process_power_tick();

    let heat = kernel
        .model()
        .agents
        .get("agent-hot")
        .expect("agent-hot exists")
        .thermal
        .heat;
    assert_eq!(heat, 0);
}

#[test]
fn power_shutdown_agent_cannot_move() {
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
        pos: pos(0, 1),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let ticks = kernel
        .config()
        .power
        .default_power_level
        .saturating_add(1)
        .max(1) as usize;
    for _ in 0..ticks {
        kernel.process_power_tick();
    }

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let event = kernel.step().unwrap();
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::AgentShutdown { .. }
        }
    ));
}

#[test]
fn power_charge_recovers_agent() {
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

    let ticks = kernel
        .config()
        .power
        .default_power_level
        .saturating_add(1)
        .max(1) as usize;
    for _ in 0..ticks {
        kernel.process_power_tick();
    }

    assert!(kernel.is_agent_shutdown(&"agent-1".to_string()));
    kernel.charge_agent_power(&"agent-1".to_string(), 100);
    assert!(!kernel.is_agent_shutdown(&"agent-1".to_string()));
}

#[test]
fn power_consume_for_decision() {
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

    let event = kernel.consume_agent_power(
        &"agent-1".to_string(),
        kernel.config().power.decision_cost,
        ConsumeReason::Decision,
    );
    assert!(event.is_some());
}

#[test]
fn shutdown_agents_list() {
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

    let ticks = kernel
        .config()
        .power
        .default_power_level
        .saturating_add(1)
        .max(1) as usize;
    for _ in 0..ticks {
        kernel.process_power_tick();
    }

    let shutdown = kernel.shutdown_agents();
    assert!(shutdown.contains(&"agent-1".to_string()));
}

#[test]
fn power_generation_creates_electricity() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "plant".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.submit_action(Action::RegisterPowerPlant {
        facility_id: "plant-1".to_string(),
        location_id: "loc-1".to_string(),
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        capacity_per_tick: 50,
        fuel_cost_per_pu: 0,
        maintenance_cost: 0,
        efficiency: 1.0,
        degradation: 0.0,
    });
    kernel.step_until_empty();

    let events = kernel.process_power_generation_tick();
    assert!(!events.is_empty());
    let level = kernel
        .model()
        .agents
        .get("agent-1")
        .expect("agent exists")
        .resources
        .get(ResourceKind::Electricity);
    assert_eq!(level, 50);
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn build_radiation_power_factory_registers_plant_and_generates_to_owner() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.radiation_power_plant_output_per_tick = 12;
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;
    let mut kernel = WorldKernel::with_config(config);

    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 100;
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "plant".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        location_id: "loc-1".to_string(),
        factory_id: "factory.power.alpha".to_string(),
        factory_kind: "factory.power.radiation.mk1".to_string(),
    });
    let event = kernel.step().expect("build radiation power factory");
    assert!(matches!(event.kind, WorldEventKind::FactoryBuilt { .. }));
    assert!(kernel.model().factories.contains_key("factory.power.alpha"));
    assert!(
        kernel
            .model()
            .power_plants
            .contains_key("factory.power.alpha")
    );

    let events = kernel.process_power_generation_tick();
    assert!(!events.is_empty());
    let level = kernel
        .model()
        .agents
        .get("agent-1")
        .expect("agent exists")
        .resources
        .get(ResourceKind::Electricity);
    assert_eq!(level, 12);
    let location_level = kernel
        .model()
        .locations
        .get("loc-1")
        .expect("location exists")
        .resources
        .get(ResourceKind::Electricity);
    assert_eq!(location_level, 0);
}

#[test]
fn power_buy_rejects_when_location_owner_involved() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "source".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "sink".to_string(),
        pos: pos(0, CM_PER_KM),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::BuyPower {
        buyer: ResourceOwner::Location {
            location_id: "loc-2".to_string(),
        },
        seller: ResourceOwner::Location {
            location_id: "loc-1".to_string(),
        },
        amount: 10,
        price_per_pu: 1,
    });
    let event = kernel.step().unwrap();
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { .. }
        }
    ));
}

#[test]
fn power_buy_allows_between_colocated_agents() {
    let mut kernel = WorldKernel::new();
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
        20,
    );

    kernel.submit_action(Action::BuyPower {
        buyer: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        seller: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        amount: 10,
        price_per_pu: 1,
    });
    let event = kernel.step().unwrap();
    assert!(matches!(
        event.kind,
        WorldEventKind::Power(PowerEvent::PowerTransferred { loss: 0, .. })
    ));

    let seller = kernel.model().agents.get("seller").expect("seller exists");
    let buyer = kernel.model().agents.get("buyer").expect("buyer exists");
    assert_eq!(seller.resources.get(ResourceKind::Electricity), 10);
    assert_eq!(buyer.resources.get(ResourceKind::Electricity), 10);
}

#[test]
fn power_buy_zero_price_uses_dynamic_market_quote() {
    let mut config = WorldConfig::default();
    config.power.market_base_price_per_pu = 2;
    config.power.market_price_min_per_pu = 1;
    config.power.market_price_max_per_pu = 50;
    config.power.market_scarcity_price_max_bps = 30_000;
    config.power.market_distance_price_per_km_bps = 0;
    config.power.market_price_band_bps = 20_000;
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
        20,
    );

    kernel.submit_action(Action::BuyPower {
        buyer: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        seller: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        amount: 10,
        price_per_pu: 0,
    });
    let event = kernel.step().expect("buy power");
    let WorldEventKind::Power(PowerEvent::PowerTransferred {
        amount,
        loss,
        quoted_price_per_pu,
        price_per_pu,
        settlement_amount,
        ..
    }) = event.kind
    else {
        panic!("expected power transferred event");
    };
    assert_eq!(amount, 10);
    assert_eq!(loss, 0);
    assert_eq!(quoted_price_per_pu, 2);
    assert_eq!(price_per_pu, 2);
    assert_eq!(settlement_amount, 20);
}

#[test]
fn power_buy_zero_price_equal_supply_keeps_base_price() {
    let mut config = WorldConfig::default();
    config.power.market_base_price_per_pu = 7;
    config.power.market_price_min_per_pu = 1;
    config.power.market_price_max_per_pu = 50;
    config.power.market_scarcity_price_max_bps = 30_000;
    config.power.market_price_band_bps = 20_000;
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
        20,
    );

    kernel.submit_action(Action::BuyPower {
        buyer: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        seller: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        amount: 20,
        price_per_pu: 0,
    });
    let event = kernel.step().expect("buy power");
    let WorldEventKind::Power(PowerEvent::PowerTransferred {
        quoted_price_per_pu,
        price_per_pu,
        settlement_amount,
        ..
    }) = event.kind
    else {
        panic!("expected power transferred event");
    };
    assert_eq!(quoted_price_per_pu, 7);
    assert_eq!(price_per_pu, 7);
    assert_eq!(settlement_amount, 140);
}

#[test]
fn power_buy_rejects_price_outside_market_band() {
    let mut config = WorldConfig::default();
    config.power.market_base_price_per_pu = 10;
    config.power.market_price_min_per_pu = 1;
    config.power.market_price_max_per_pu = 100;
    config.power.market_scarcity_price_max_bps = 10_000;
    config.power.market_distance_price_per_km_bps = 0;
    config.power.market_price_band_bps = 500;
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
        100,
    );

    kernel.submit_action(Action::BuyPower {
        buyer: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        seller: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        amount: 10,
        price_per_pu: 20,
    });
    let event = kernel.step().expect("buy power");
    let WorldEventKind::ActionRejected {
        reason: RejectReason::RuleDenied { notes },
    } = event.kind
    else {
        panic!("expected action rejected for out-of-band price");
    };
    assert!(
        notes.iter().any(|note| note.contains("out of band")),
        "expected out-of-band note, got {notes:?}"
    );
}

#[test]
fn power_transfer_rejects_when_location_owner_involved() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "source".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );

    kernel.submit_action(Action::SellPower {
        seller: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        buyer: ResourceOwner::Location {
            location_id: "loc-1".to_string(),
        },
        amount: 10,
        price_per_pu: 1,
    });
    let event = kernel.step().unwrap();
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { .. }
        }
    ));
}

#[test]
fn place_power_order_matches_buy_and_sell_and_clears_book() {
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
        20,
    );

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 2,
    });
    let sell_event = kernel.step().expect("place sell order");
    let sell_order_id = match sell_event.kind {
        WorldEventKind::PowerOrderPlaced {
            order_id,
            remaining_amount,
            fills,
            ..
        } => {
            assert_eq!(remaining_amount, 10);
            assert!(fills.is_empty());
            order_id
        }
        other => panic!("expected power order placed, got {other:?}"),
    };

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 10,
        limit_price_per_pu: 5,
    });
    let buy_event = kernel.step().expect("place buy order");
    match buy_event.kind {
        WorldEventKind::PowerOrderPlaced {
            order_id,
            remaining_amount,
            fills,
            auto_cancelled_order_ids,
            ..
        } => {
            assert_eq!(remaining_amount, 0);
            assert!(auto_cancelled_order_ids.is_empty());
            assert_eq!(fills.len(), 1);
            let fill = &fills[0];
            assert_eq!(fill.buy_order_id, order_id);
            assert_eq!(fill.sell_order_id, sell_order_id);
            assert_eq!(fill.amount, 10);
            assert!(fill.price_per_pu >= 2 && fill.price_per_pu <= 5);
        }
        other => panic!("expected matched power order placed, got {other:?}"),
    }

    assert!(kernel.model().power_order_book.open_orders.is_empty());
    let seller = kernel.model().agents.get("seller").expect("seller exists");
    let buyer = kernel.model().agents.get("buyer").expect("buyer exists");
    assert_eq!(seller.resources.get(ResourceKind::Electricity), 10);
    assert_eq!(buyer.resources.get(ResourceKind::Electricity), 10);
}

#[test]
fn power_order_keeps_open_when_quote_below_sell_limit() {
    let mut kernel = WorldKernel::new();
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
        20,
    );

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 2,
    });
    kernel.step().expect("place sell order");

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 10,
        limit_price_per_pu: 5,
    });
    let buy_event = kernel.step().expect("place buy order");
    match buy_event.kind {
        WorldEventKind::PowerOrderPlaced {
            remaining_amount,
            fills,
            ..
        } => {
            assert_eq!(remaining_amount, 10);
            assert!(fills.is_empty());
        }
        other => panic!("expected power order placed, got {other:?}"),
    }
    assert_eq!(kernel.model().power_order_book.open_orders.len(), 2);
}

#[test]
fn cancel_power_order_removes_open_order() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "hub".to_string(),
        name: "hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 6,
        limit_price_per_pu: 3,
    });
    let placed = kernel.step().expect("place buy order");
    let order_id = match placed.kind {
        WorldEventKind::PowerOrderPlaced { order_id, .. } => order_id,
        other => panic!("expected power order placed, got {other:?}"),
    };
    assert_eq!(kernel.model().power_order_book.open_orders.len(), 1);

    kernel.submit_action(Action::CancelPowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        order_id,
    });
    let cancelled = kernel.step().expect("cancel order");
    match cancelled.kind {
        WorldEventKind::PowerOrderCancelled {
            owner,
            order_id: cancelled_order_id,
            side,
            remaining_amount,
        } => {
            assert_eq!(
                owner,
                ResourceOwner::Agent {
                    agent_id: "buyer".to_string()
                }
            );
            assert_eq!(cancelled_order_id, order_id);
            assert_eq!(side, PowerOrderSide::Buy);
            assert_eq!(remaining_amount, 6);
        }
        other => panic!("expected power order cancelled, got {other:?}"),
    }
    assert!(kernel.model().power_order_book.open_orders.is_empty());
}

#[test]
fn power_order_match_prefers_lower_ask_price() {
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
        agent_id: "seller-high".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller-low".to_string(),
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
            agent_id: "seller-high".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller-low".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller-high".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 4,
    });
    let high_ask_event = kernel.step().expect("place high ask");
    let high_ask_order_id = match high_ask_event.kind {
        WorldEventKind::PowerOrderPlaced { order_id, .. } => order_id,
        other => panic!("expected high ask order event, got {other:?}"),
    };

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller-low".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 2,
    });
    let low_ask_event = kernel.step().expect("place low ask");
    let low_ask_order_id = match low_ask_event.kind {
        WorldEventKind::PowerOrderPlaced { order_id, .. } => order_id,
        other => panic!("expected low ask order event, got {other:?}"),
    };

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 10,
        limit_price_per_pu: 5,
    });
    let buy_event = kernel.step().expect("place taker buy");
    match buy_event.kind {
        WorldEventKind::PowerOrderPlaced { fills, .. } => {
            assert_eq!(fills.len(), 1);
            assert_eq!(fills[0].sell_order_id, low_ask_order_id);
        }
        other => panic!("expected power order placed event, got {other:?}"),
    }

    assert!(
        kernel
            .model()
            .power_order_book
            .open_orders
            .iter()
            .any(|entry| entry.order_id == high_ask_order_id)
    );
}

#[test]
fn power_order_match_prefers_earlier_order_at_same_price() {
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
        agent_id: "seller-a".to_string(),
        location_id: "hub".to_string(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "seller-b".to_string(),
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
            agent_id: "seller-a".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "seller-b".to_string(),
        },
        ResourceKind::Electricity,
        20,
    );

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller-a".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 2,
    });
    let first_ask_event = kernel.step().expect("place first ask");
    let first_ask_order_id = match first_ask_event.kind {
        WorldEventKind::PowerOrderPlaced { order_id, .. } => order_id,
        other => panic!("expected first ask order event, got {other:?}"),
    };

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "seller-b".to_string(),
        },
        side: PowerOrderSide::Sell,
        amount: 10,
        limit_price_per_pu: 2,
    });
    kernel.step().expect("place second ask");

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 5,
        limit_price_per_pu: 5,
    });
    let buy_event = kernel.step().expect("place taker buy");
    match buy_event.kind {
        WorldEventKind::PowerOrderPlaced { fills, .. } => {
            assert_eq!(fills.len(), 1);
            assert_eq!(fills[0].sell_order_id, first_ask_order_id);
        }
        other => panic!("expected power order placed event, got {other:?}"),
    }
}

#[test]
fn power_order_match_sweeps_many_resting_orders_in_priority_order() {
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
        agent_id: "buyer".to_string(),
        location_id: "hub".to_string(),
    });
    for index in 0..32 {
        kernel.submit_action(Action::RegisterAgent {
            agent_id: format!("seller-{index}"),
            location_id: "hub".to_string(),
        });
    }
    kernel.step_until_empty();

    let mut sell_order_ids = Vec::new();
    for index in 0..32 {
        let seller_id = format!("seller-{index}");
        seed_owner_resource(
            &mut kernel,
            ResourceOwner::Agent {
                agent_id: seller_id.clone(),
            },
            ResourceKind::Electricity,
            1,
        );
        kernel.submit_action(Action::PlacePowerOrder {
            owner: ResourceOwner::Agent {
                agent_id: seller_id,
            },
            side: PowerOrderSide::Sell,
            amount: 1,
            limit_price_per_pu: 2,
        });
        let sell_event = kernel.step().expect("place resting sell order");
        match sell_event.kind {
            WorldEventKind::PowerOrderPlaced {
                order_id,
                remaining_amount,
                fills,
                ..
            } => {
                assert_eq!(remaining_amount, 1);
                assert!(fills.is_empty());
                sell_order_ids.push(order_id);
            }
            other => panic!("expected power order placed event, got {other:?}"),
        }
    }

    kernel.submit_action(Action::PlacePowerOrder {
        owner: ResourceOwner::Agent {
            agent_id: "buyer".to_string(),
        },
        side: PowerOrderSide::Buy,
        amount: 32,
        limit_price_per_pu: 5,
    });
    let buy_event = kernel.step().expect("place sweeping buy order");
    match buy_event.kind {
        WorldEventKind::PowerOrderPlaced {
            remaining_amount,
            fills,
            auto_cancelled_order_ids,
            ..
        } => {
            assert_eq!(remaining_amount, 0);
            assert!(auto_cancelled_order_ids.is_empty());
            assert_eq!(fills.len(), sell_order_ids.len());
            let filled_sell_order_ids: Vec<u64> =
                fills.iter().map(|fill| fill.sell_order_id).collect();
            assert_eq!(filled_sell_order_ids, sell_order_ids);
        }
        other => panic!("expected power order placed event, got {other:?}"),
    }
    assert!(kernel.model().power_order_book.open_orders.is_empty());
}
