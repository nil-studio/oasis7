#[test]
fn due_recipe_jobs_prioritize_survival_over_expansion() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.priority");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: "factory.priority".to_string(),
            tier: 1,
            recipe_slots: 2,
            tags: vec!["assembly".to_string()],
        })
        .expect("set two-slot factory profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.priority", 1, 2),
    });
    world.step().expect("start factory build");
    world.step().expect("factory ready");

    world
        .set_material_balance("iron_ingot", 4)
        .expect("seed recipe input");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed site recipe input");
    seed_builder_electricity(&mut world, 20);
    world.set_resource_balance(ResourceKind::Electricity, 20);

    let expansion_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("outpost_kit", 1)],
        Vec::new(),
        2,
        1,
    );
    let survival_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("oxygen_pack", 1)],
        Vec::new(),
        2,
        1,
    );

    // Submit expansion first to verify due-job completion still prioritizes survival.
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.priority".to_string(),
        recipe_id: "recipe.expand.outpost".to_string(),
        plan: expansion_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.priority".to_string(),
        recipe_id: "recipe.survival.oxygen".to_string(),
        plan: survival_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipes");
    assert_eq!(world.pending_recipe_jobs_len(), 2);

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared =
        world.prepared_due_economy_event_bodies_for_test(world.state().time.saturating_add(1));
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    let prepared_recipe_ids = prepared
        .iter()
        .filter_map(|body| match body {
            WorldEventBody::Domain(DomainEvent::RecipeCompleted { recipe_id, .. }) => {
                Some(recipe_id.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepared_recipe_ids,
        ["recipe.survival.oxygen", "recipe.expand.outpost"]
    );

    let before = world.journal().events.len();
    world.step().expect("complete recipes");

    let completed_recipe_ids: Vec<String> = world.journal().events[before..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::RecipeCompleted { recipe_id, .. }) => {
                Some(recipe_id.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        completed_recipe_ids,
        vec![
            "recipe.survival.oxygen".to_string(),
            "recipe.expand.outpost".to_string(),
        ]
    );
}
#[test]
fn bottleneck_pressure_bumps_recipe_completion_priority() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.bottleneck");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: "factory.bottleneck".to_string(),
            tier: 1,
            recipe_slots: 2,
            tags: vec!["assembly".to_string()],
        })
        .expect("set two-slot bottleneck profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.bottleneck", 1, 2),
    });
    world.step().expect("start factory build");
    world.step().expect("factory ready");

    world
        .set_material_balance("gear", 4)
        .expect("seed non-bottleneck material");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "gear", 4)
        .expect("seed site non-bottleneck material");
    world
        .set_material_balance("control_chip", 2)
        .expect("seed bottleneck material");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "control_chip", 2)
        .expect("seed site bottleneck material");
    seed_builder_electricity(&mut world, 20);
    world.set_resource_balance(ResourceKind::Electricity, 20);

    let non_bottleneck_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("gear", 2)],
        vec![MaterialStack::new("factory_frame", 1)],
        Vec::new(),
        2,
        1,
    );
    let bottleneck_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("control_chip", 2)],
        vec![MaterialStack::new("factory_frame", 1)],
        Vec::new(),
        2,
        1,
    );

    // Submit non-bottleneck first. Bottleneck should still complete first under low-stock pressure.
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.bottleneck".to_string(),
        recipe_id: "recipe.production.frame.normal".to_string(),
        plan: non_bottleneck_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.bottleneck".to_string(),
        recipe_id: "recipe.production.frame.control_chip".to_string(),
        plan: bottleneck_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipes");
    assert_eq!(world.pending_recipe_jobs_len(), 2);

    let before = world.journal().events.len();
    world.step().expect("complete recipes");

    let completed: Vec<(String, Vec<String>)> = world.journal().events[before..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                recipe_id,
                bottleneck_tags,
                ..
            }) => Some((recipe_id.clone(), bottleneck_tags.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(completed.len(), 2);
    assert_eq!(completed[0].0, "recipe.production.frame.control_chip");
    assert_eq!(completed[0].1, vec!["control_chip".to_string()]);
    assert!(completed[1].1.is_empty());
}

#[test]
fn logistics_sla_metrics_and_priority_are_observable_after_transit_completion() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 100)
        .expect("seed source");
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 50,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start transit");

    let started_priority = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { priority, .. }) => {
                Some(*priority)
            }
            _ => None,
        })
        .expect("material transit started with priority");
    assert_eq!(started_priority, MaterialTransitPriority::Standard);

    world.step().expect("complete transit");

    let completed_priority = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted { priority, .. }) => {
                Some(*priority)
            }
            _ => None,
        })
        .expect("material transit completed with priority");
    assert_eq!(completed_priority, MaterialTransitPriority::Standard);

    let metrics = world.logistics_sla_metrics();
    assert_eq!(metrics.completed_transits, 1);
    assert_eq!(metrics.fulfilled_transits, 1);
    assert_eq!(metrics.breached_transits, 0);
    assert_eq!(metrics.total_delay_ticks, 0);
    assert_eq!(metrics.urgent_completed_transits, 0);
    assert_eq!(metrics.urgent_fulfilled_transits, 0);
    assert_eq!(metrics.urgent_breached_transits, 0);
    assert_eq!(metrics.urgent_total_delay_ticks, 0);
    assert_eq!(metrics.breach_rate(), 0.0);
    assert_eq!(metrics.fulfillment_rate(), 1.0);
}

#[test]
fn due_transits_prioritize_urgent_before_standard_with_same_ready_at() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 50)
        .expect("seed standard source");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-c"), "oxygen_pack", 50)
        .expect("seed urgent source");

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 20,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-c"),
        to_ledger: MaterialLedgerId::site("site-d"),
        kind: "oxygen_pack".to_string(),
        amount: 20,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start transits");
    assert_eq!(world.pending_material_transits_len(), 2);

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let metrics_before_prepare = world.logistics_sla_metrics().clone();
    let (prepared, projected_metrics) =
        world.prepared_due_material_transits_for_test(world.state().time.saturating_add(1));
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert_eq!(world.logistics_sla_metrics(), &metrics_before_prepare);
    assert_eq!(
        prepared.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        [
            MaterialTransitPriority::Urgent,
            MaterialTransitPriority::Standard,
        ]
    );
    assert_eq!(projected_metrics.completed_transits, 2);
    assert_eq!(projected_metrics.fulfilled_transits, 2);
    assert_eq!(projected_metrics.urgent_completed_transits, 1);
    assert_eq!(projected_metrics.urgent_fulfilled_transits, 1);

    let before = world.journal().events.len();
    world.step().expect("complete transits");

    let completion_priorities: Vec<MaterialTransitPriority> = world.journal().events[before..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted { priority, .. }) => {
                Some(*priority)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        completion_priorities,
        vec![
            MaterialTransitPriority::Urgent,
            MaterialTransitPriority::Standard,
        ]
    );

    let metrics = world.logistics_sla_metrics();
    assert_eq!(metrics.completed_transits, 2);
    assert_eq!(metrics.fulfilled_transits, 2);
    assert_eq!(metrics.urgent_completed_transits, 1);
    assert_eq!(metrics.urgent_fulfilled_transits, 1);
}

#[test]
fn due_transits_allow_explicit_priority_override_for_non_urgent_material() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 60)
        .expect("seed source");

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 20,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-c"),
        kind: "copper_wire".to_string(),
        amount: 20,
        distance_km: 100,
        priority: Some(MaterialTransitPriority::Urgent),
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start transits");
    assert_eq!(world.pending_material_transits_len(), 2);

    let before = world.journal().events.len();
    world.step().expect("complete transits");
    let completion_priorities: Vec<MaterialTransitPriority> = world.journal().events[before..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted { priority, .. }) => {
                Some(*priority)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        completion_priorities,
        vec![
            MaterialTransitPriority::Urgent,
            MaterialTransitPriority::Standard,
        ]
    );
}

#[test]
fn transfer_material_uses_profile_priority_and_loss_class() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    world
        .upsert_material_profile(MaterialProfileV1 {
            kind: "copper_wire".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::High,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Urgent,
        })
        .expect("insert profile");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 100)
        .expect("seed source");

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 20,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start transit");

    let (started_priority, started_loss_bps) = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitStarted {
                priority,
                loss_bps,
                ..
            }) => Some((*priority, *loss_bps)),
            _ => None,
        })
        .expect("material transit started");
    assert_eq!(started_priority, MaterialTransitPriority::Urgent);
    assert_eq!(started_loss_bps, 20);

    world.step().expect("complete transit");
    let completed_priority = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted { priority, .. }) => {
                Some(*priority)
            }
            _ => None,
        })
        .expect("material transit completed");
    assert_eq!(completed_priority, MaterialTransitPriority::Urgent);
}

#[test]
fn logistics_route_registration_persists_and_duplicate_tuple_is_rejected() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    let register = || Action::RegisterLogisticsRoute {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        distance_km: 100,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 0,
    };
    world.submit_action(register());
    world.step().expect("register logistics route");

    let route_id = match &world.journal().events.last().expect("route event").body {
        WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered {
            requester_agent_id,
            route_id,
            from_ledger,
            to_ledger,
            kind,
            distance_km,
            priority,
            ..
        }) => {
            assert_eq!(requester_agent_id, "operator-a");
            assert_eq!(from_ledger, &MaterialLedgerId::site("site-a"));
            assert_eq!(to_ledger, &MaterialLedgerId::site("site-b"));
            assert_eq!(kind, "copper_wire");
            assert_eq!(*distance_km, 100);
            assert_eq!(*priority, MaterialTransitPriority::Standard);
            route_id.clone().expect("registered route id")
        }
        other => panic!("expected LogisticsRouteRegistered, got {other:?}"),
    };

    world.submit_action(register());
    world.step().expect("reject duplicate route");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { .. },
            ..
        }))
    ));
    let registrations = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered { .. })
            )
        })
        .count();
    assert_eq!(registrations, 1);
    assert!(!route_id.is_empty());
}

#[test]
fn route_bound_transit_carries_route_id_through_start_pending_and_completion() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 100)
        .expect("seed route source");

    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        distance_km: 100,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 0,
    });
    world.step().expect("register route");
    let route_id = match &world.journal().events.last().expect("route event").body {
        WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered { route_id, .. }) => {
            route_id.clone().expect("route id")
        }
        other => panic!("expected LogisticsRouteRegistered, got {other:?}"),
    };

    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 20,
        distance_km: 100,
        priority: Some(MaterialTransitPriority::Standard),
        route_id: Some(route_id.clone()),
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start route-bound transit");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(
            DomainEvent::MaterialTransitStarted {
                route_id: Some(started_route_id),
                ..
            }
        )) if started_route_id == &route_id
    ));
    assert_eq!(
        world
            .state()
            .pending_material_transits
            .values()
            .next()
            .and_then(|job| job.route_id.clone()),
        Some(route_id.clone())
    );

    world.step().expect("complete route-bound transit");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(
            DomainEvent::MaterialTransitCompleted {
                route_id: Some(completed_route_id),
                ..
            }
        )) if completed_route_id == &route_id
    ));
}

#[test]
fn incompatible_or_unreachable_route_registration_and_use_are_rejected_without_mutation() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 500)
        .expect("seed route validation source");
    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        distance_km: 100,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 0,
    });
    world.step().expect("register route");
    let route_id = match &world.journal().events.last().expect("route event").body {
        WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered { route_id, .. }) => {
            route_id.clone().expect("route id")
        }
        other => panic!("expected LogisticsRouteRegistered, got {other:?}"),
    };

    let mismatches = [
        (
            MaterialLedgerId::site("site-other"),
            MaterialLedgerId::site("site-b"),
            "copper_wire",
            100,
            MaterialTransitPriority::Standard,
        ),
        (
            MaterialLedgerId::site("site-a"),
            MaterialLedgerId::site("site-b"),
            "iron_ingot",
            100,
            MaterialTransitPriority::Standard,
        ),
        (
            MaterialLedgerId::site("site-a"),
            MaterialLedgerId::site("site-b"),
            "copper_wire",
            200,
            MaterialTransitPriority::Standard,
        ),
        (
            MaterialLedgerId::site("site-a"),
            MaterialLedgerId::site("site-b"),
            "copper_wire",
            100,
            MaterialTransitPriority::Urgent,
        ),
    ];
    for (from_ledger, to_ledger, kind, distance_km, priority) in mismatches {
        let pending_before = world.pending_material_transits_len();
        world.submit_action(Action::TransferMaterial {
            requester_agent_id: "operator-a".to_string(),
            from_ledger,
            to_ledger,
            kind: kind.to_string(),
            amount: 10,
            distance_km,
            priority: Some(priority),
            route_id: Some(route_id.clone()),
            route_ids: Vec::new(),
            auto_reroute: false,
        });
        world.step().expect("reject mismatched route use");
        assert!(matches!(
            world.journal().events.last().map(|event| &event.body),
            Some(WorldEventBody::Domain(DomainEvent::ActionRejected { .. }))
        ));
        assert_eq!(world.pending_material_transits_len(), pending_before);
    }

    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-far"),
        kind: "copper_wire".to_string(),
        distance_km: 20_001,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 0,
    });
    world.step().expect("reject over-max route");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected { .. }))
    ));

    let pending_before = world.pending_material_transits_len();
    let journal_before = world.journal().events.len();
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "copper_wire".to_string(),
        amount: 10,
        distance_km: 100,
        priority: Some(MaterialTransitPriority::Standard),
        route_id: Some("route-missing".to_string()),
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("reject unreachable route use");
    assert!(
        world.journal().events[journal_before..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                )
            })
    );
    assert!(
        !world.journal().events[journal_before..]
            .iter()
            .any(|event| {
                matches!(
                    &event.body,
                    WorldEventBody::Domain(DomainEvent::MaterialTransitStarted { .. })
                )
            })
    );
    assert_eq!(world.pending_material_transits_len(), pending_before);
}
