#[test]
fn stable_line_byproduct_change_starts_fresh_candidate() {
    let factory_id = "factory.identity.byproduct";
    let recipe_id = "recipe.identity.byproduct";
    let mut world = stable_identity_fixture(factory_id);
    let original_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        vec![MaterialStack::new("slag", 1)],
        1,
        1,
    );
    let changed_byproduct_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        vec![MaterialStack::new("slag", 2)],
        1,
        1,
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed byproduct identity input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, original_plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, original_plan);
    complete_identity_recipe(&mut world, factory_id, recipe_id, changed_byproduct_plan);

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap,
        "changing the canonical byproduct bundle must not unlock scale-out"
    );
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("factory after byproduct change")
            .production
            .same_recipe_repeat_count,
        1,
        "changing the canonical byproduct bundle starts a fresh candidate"
    );
}

#[test]
fn conflicting_recipe_completion_replay_rejects_without_mutation_after_receipt() {
    let factory_id = "factory.identity.conflicting-completion";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.conflicting-completion".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            Vec::new(),
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    world.step().expect("complete recipe");
    let completion = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::RecipeCompleted { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("completion event");
    let mut conflicting = completion;
    if let DomainEvent::RecipeCompleted { produce, .. } = &mut conflicting {
        produce[0].amount += 1;
    }
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize settled state");
    let result = replay.apply_domain_event(&conflicting, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "same-job conflicting completion must be rejected: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize after conflicting completion"),
        before,
        "conflicting completion must not mutate settled state"
    );
}
