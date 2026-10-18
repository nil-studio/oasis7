// The path-authority tests exercise the public action/event surface so that
// persisted quantity and allocation behavior cannot be satisfied by a test-only
// implementation map.

fn completed_recipe_path_id(world: &World, route_id: &str) -> String {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted {
                path_id: Some(path_id),
                route_ids,
                ..
            }) if route_ids.len() == 1 && route_ids[0] == route_id => Some(path_id.clone()),
            _ => None,
        })
        .expect("completed recipe path id")
}

#[test]
fn recipe_binding_cannot_exceed_completed_path_settled_quantity() {
    let factory_id = "factory.recipe.path.quantity";
    let recipe_id = "recipe.recipe.path.quantity";
    let mut world = recipe_route_fixture(factory_id);
    let route_id = register_recipe_route(&mut world, "source-quantity", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_id,
        "source-quantity",
        "site-1",
        "iron_ingot",
        1,
    );
    let path_id = completed_recipe_path_id(&world, &route_id);

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed destination material above path settlement");
    let destination_ledger = MaterialLedgerId::site("site-1");
    let material_before = world.ledger_material_balance(&destination_ledger, "iron_ingot");
    let electricity_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder electricity before under-quantity rejection");
    let production_before = world
        .state()
        .factories
        .get(factory_id)
        .expect("quantity test factory")
        .production
        .clone();
    let progress_before = world.state().industry_progress.clone();
    let journal_before = world.journal().events.len();

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 2)],
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![path_id],
    });
    world.step().expect("reject recipe above path settlement");

    assert!(
        world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )),
        "under-quantity path binding must reject the recipe"
    );
    assert!(
        !world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
            )),
        "under-quantity path binding must not start a recipe"
    );
    assert_eq!(
        world.ledger_material_balance(&destination_ledger, "iron_ingot"),
        material_before,
        "under-quantity rejection must not debit destination material"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity after under-quantity rejection"),
        electricity_before,
        "under-quantity rejection must not debit electricity"
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(world.state().industry_progress, progress_before);
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("quantity test factory after rejection")
            .production,
        production_before,
        "under-quantity rejection must not mutate factory production"
    );
}

#[test]
fn exact_completed_path_quantity_succeeds_once_and_reuse_rejects_atomically() {
    let factory_id = "factory.recipe.path.single_use";
    let recipe_id = "recipe.recipe.path.single_use";
    let mut world = recipe_route_fixture(factory_id);
    let route_id = register_recipe_route(&mut world, "source-single-use", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_id,
        "source-single-use",
        "site-1",
        "iron_ingot",
        2,
    );
    let path_id = completed_recipe_path_id(&world, &route_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![path_id.clone()],
    });
    world.step().expect("exact path quantity starts recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        0,
        "exact path quantity should be consumed once at recipe start"
    );
    world.step().expect("complete exact path-bound recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 1);

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("replenish material for reuse attempt");
    let material_before = world.ledger_material_balance(
        &MaterialLedgerId::site("site-1"),
        "iron_ingot",
    );
    let electricity_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder electricity before reuse rejection");
    let production_before = world
        .state()
        .factories
        .get(factory_id)
        .expect("single-use test factory")
        .production
        .clone();
    let progress_before = world.state().industry_progress.clone();
    let journal_before = world.journal().events.len();

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![path_id],
    });
    world.step().expect("reject reused completed path");

    assert!(
        world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )),
        "a settled path must reject a second exact recipe binding"
    );
    assert!(
        !world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
            )),
        "reused path must not start a second recipe"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        material_before,
        "reused-path rejection must not debit material"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity after reuse rejection"),
        electricity_before,
        "reused-path rejection must not debit electricity"
    );
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(world.state().industry_progress, progress_before);
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("single-use test factory after rejection")
            .production,
        production_before,
        "reused-path rejection must not mutate factory production"
    );
}

#[test]
fn completed_recipe_path_authority_roundtrips_after_partial_consumption_and_legacy_defaults_fail_closed()
{
    let factory_id = "factory.recipe.path.serde";
    let recipe_id = "recipe.recipe.path.serde";
    let mut world = recipe_route_fixture(factory_id);
    let route_id = register_recipe_route(&mut world, "source-serde", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_id,
        "source-serde",
        "site-1",
        "iron_ingot",
        3,
    );
    let path_id = completed_recipe_path_id(&world, &route_id);

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![path_id.clone()],
    });
    world.step().expect("start partial path-bound recipe");
    assert_eq!(
        world
            .state()
            .completed_logistics_paths
            .get(&path_id)
            .expect("completed serde path")
            .remaining_recipe_amount,
        2
    );

    let encoded = serde_json::to_vec(world.state()).expect("serialize consumed path authority");
    let restored: crate::WorldState =
        serde_json::from_slice(&encoded).expect("deserialize consumed path authority");
    let restored_authority = restored
        .completed_logistics_paths
        .get(&path_id)
        .expect("restored completed path authority");
    assert_eq!(restored_authority.settled_amount, 3);
    assert_eq!(restored_authority.remaining_recipe_amount, 2);
    assert_eq!(restored_authority.route_ids, vec![route_id.clone()]);

    let mut legacy_json = serde_json::to_value(&restored).expect("encode legacy path state");
    let legacy_path = legacy_json
        .get_mut("completed_logistics_paths")
        .and_then(|paths| paths.get_mut(&path_id))
        .and_then(serde_json::Value::as_object_mut)
        .expect("legacy path authority object");
    legacy_path.remove("settled_amount");
    legacy_path.remove("remaining_recipe_amount");
    let legacy: crate::WorldState =
        serde_json::from_value(legacy_json).expect("deserialize legacy path authority");
    let legacy_authority = legacy
        .completed_logistics_paths
        .get(&path_id)
        .expect("legacy completed path authority");
    assert_eq!(legacy_authority.settled_amount, 0);
    assert_eq!(legacy_authority.remaining_recipe_amount, 0);
    assert!(
        legacy
            .allocate_recipe_path_amounts(
                &MaterialLedgerId::site("site-1"),
                &[],
                std::slice::from_ref(&path_id),
                &[MaterialStack::new("iron_ingot", 1)],
            )
            .is_err(),
        "legacy omitted quantity fields must fail closed for recipe binding"
    );
}

#[test]
fn reversed_multi_path_binding_allocates_deterministically_and_exhausts_remainder()
{
    let factory_id = "factory.recipe.path.multi";
    let recipe_id = "recipe.recipe.path.multi";
    let mut world = recipe_route_fixture(factory_id);
    let route_a = register_recipe_route(&mut world, "source-multi-a", "site-1", "iron_ingot");
    let route_b = register_recipe_route(&mut world, "source-multi-b", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_a,
        "source-multi-a",
        "site-1",
        "iron_ingot",
        2,
    );
    complete_recipe_route_transfer(
        &mut world,
        &route_b,
        "source-multi-b",
        "site-1",
        "iron_ingot",
        3,
    );
    let path_a = completed_recipe_path_id(&world, &route_a);
    let path_b = completed_recipe_path_id(&world, &route_b);
    let mut sorted_paths = vec![path_a.clone(), path_b.clone()];
    sorted_paths.sort();
    let reversed_paths = vec![sorted_paths[1].clone(), sorted_paths[0].clone()];

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 4)],
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: reversed_paths,
    });
    let journal_before = world.journal().events.len();
    world.step().expect("start reversed multi-path recipe");
    assert!(world.journal().events[journal_before..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                logistics_path_ids,
                ..
            }) if logistics_path_ids == &sorted_paths
        )
    }));
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world
            .state()
            .completed_logistics_paths
            .get(&sorted_paths[0])
            .expect("first sorted path")
            .remaining_recipe_amount,
        0,
        "sorted allocation must exhaust the first path before the second"
    );
    assert_eq!(
        world
            .state()
            .completed_logistics_paths
            .get(&sorted_paths[1])
            .expect("second sorted path")
            .remaining_recipe_amount,
        1,
        "sorted allocation must preserve the second path remainder"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        1
    );
    world.step().expect("complete first multi-path recipe");

    let remainder_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 1)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: remainder_plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![sorted_paths[1].clone()],
    });
    world.step().expect("consume second path remainder");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world
            .state()
            .completed_logistics_paths
            .get(&sorted_paths[1])
            .expect("exhausted second path")
            .remaining_recipe_amount,
        0
    );
    world.step().expect("complete remainder recipe");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 1)
        .expect("replenish aggregate material for exhaustion check");
    let material_before = world.ledger_material_balance(
        &MaterialLedgerId::site("site-1"),
        "iron_ingot",
    );
    let electricity_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("electricity before exhausted path rejection");
    let journal_before = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: remainder_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: vec![sorted_paths[1].clone()],
    });
    world.step().expect("reject exhausted second path");
    assert!(world.journal().events[journal_before..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
        )
    }));
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        material_before
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("electricity after exhausted path rejection"),
        electricity_before
    );
}

#[test]
fn recipe_binding_rejects_path_and_route_from_different_completed_authorities() {
    let factory_id = "factory.recipe.path.route.mismatch";
    let recipe_id = "recipe.recipe.path.route.mismatch";
    let mut world = recipe_route_fixture(factory_id);
    let route_a = register_recipe_route(
        &mut world,
        "source-path-route-a",
        "site-1",
        "iron_ingot",
    );
    let route_b = register_recipe_route(
        &mut world,
        "source-path-route-b",
        "site-1",
        "iron_ingot",
    );
    complete_recipe_route_transfer(
        &mut world,
        &route_a,
        "source-path-route-a",
        "site-1",
        "iron_ingot",
        1,
    );
    complete_recipe_route_transfer(
        &mut world,
        &route_b,
        "source-path-route-b",
        "site-1",
        "iron_ingot",
        1,
    );
    let path_a = completed_recipe_path_id(&world, &route_a);
    let journal_before = world.journal().events.len();

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: vec![route_b],
        logistics_path_ids: vec![path_a],
    });
    world
        .step()
        .expect("reject recipe whose completed path does not belong to bound route");

    assert!(
        world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )),
        "a completed path must not authorize a different completed route"
    );
    assert!(
        !world.journal().events[journal_before..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
            )),
        "path/route authority mismatch must not start a recipe"
    );
}
