use super::*;

#[test]
fn factory_depreciation_counts_only_jobs_for_each_factory() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.target", 1, 2, 3),
    );
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-2",
        factory_spec("factory.other", 1, 2, 3),
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed target recipe inputs");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-2"), "iron_ingot", 2)
        .expect("seed other recipe inputs");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 40)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 40);
    for factory_id in ["factory.target", "factory.other"] {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: factory_id.to_string(),
            recipe_id: format!("recipe.{factory_id}"),
            plan: RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 1)],
                vec![MaterialStack::new("control_chip", 1)],
                Vec::new(),
                1,
                3,
            ),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
    }
    world.step().expect("start recipes");
    assert_eq!(world.pending_recipe_jobs_len(), 2);

    let durability_before_loaded_tick = world
        .snapshot()
        .state
        .factories
        .get("factory.target")
        .expect("target factory exists")
        .durability_ppm;

    world
        .step()
        .expect("depreciation under independent factory loads");

    let durability_after_loaded_tick = world
        .snapshot()
        .state
        .factories
        .get("factory.target")
        .expect("target factory exists")
        .durability_ppm;
    assert_eq!(
        durability_before_loaded_tick - durability_after_loaded_tick,
        4_500
    );
}

#[test]
fn maintain_factory_consumes_hardware_part_and_recovers_durability() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.alpha", 1, 1, 4),
    );
    world.step().expect("depreciate once");
    world
        .set_material_balance("hardware_part", 10)
        .expect("seed hardware part");

    world.submit_action(Action::MaintainFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.alpha".to_string(),
        parts: 2,
    });
    world.step().expect("maintain factory");

    let snapshot = world.snapshot();
    let durability_after = snapshot
        .state
        .factories
        .get("factory.alpha")
        .expect("factory exists")
        .durability_ppm;
    assert_eq!(durability_after, 1_000_000);
    assert_eq!(world.material_balance("hardware_part"), 9);

    let last = world.journal().events.last().expect("maintain event");
    match &last.body {
        WorldEventBody::Domain(DomainEvent::FactoryMaintained {
            factory_id,
            consumed_parts,
            durability_ppm,
            ..
        }) => {
            assert_eq!(factory_id, "factory.alpha");
            assert_eq!(*consumed_parts, 1);
            assert_eq!(*durability_ppm, 1_000_000);
        }
        other => panic!("expected FactoryMaintained, got {other:?}"),
    }
}

#[test]
fn industrial_integrity_factory_maintained_wrong_operator_rejects_before_debit() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-maintain-owner",
        factory_spec("factory.maintain-owner", 1, 1, 1),
    );
    world
        .set_material_balance("hardware_part", 2)
        .expect("seed maintenance parts");

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before wrong operator");
    let event = DomainEvent::FactoryMaintained {
        operator_agent_id: "operator-not-builder".to_string(),
        factory_id: "factory.maintain-owner".to_string(),
        consume_ledger: MaterialLedgerId::world(),
        consumed_parts: 1,
        durability_ppm: 1_000_000,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "maintenance by a non-builder must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after wrong operator"),
        before,
        "wrong-operator maintenance must not debit hardware or mutate factory state"
    );
}

#[test]
fn industrial_integrity_factory_maintained_unknown_factory_rejects_before_debit() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    world
        .set_material_balance("hardware_part", 2)
        .expect("seed maintenance parts");

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before unknown factory");
    let event = DomainEvent::FactoryMaintained {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.unknown-maintenance".to_string(),
        consume_ledger: MaterialLedgerId::world(),
        consumed_parts: 1,
        durability_ppm: 1_000_000,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "maintenance for an unknown factory must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after unknown factory"),
        before,
        "unknown-factory maintenance must not debit hardware or mutate state"
    );
}
