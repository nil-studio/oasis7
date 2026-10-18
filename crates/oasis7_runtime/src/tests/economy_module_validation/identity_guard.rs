use super::*;

#[test]
fn industrial_integrity_product_validation_blocker_rejects_tampered_identity_before_mutation() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 10)
        .expect("seed steel");
    world
        .set_material_balance("circuit_board", 2)
        .expect("seed circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 40)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 40);
    let spec = factory_spec("factory.recipe.identity-guard", 1, 1);
    prepare_module_test_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("build complete");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 1)
        .expect("seed identity-guard recipe input");

    activate_pure_module(
        &mut world,
        "m4.recipe.identity-guard",
        b"identity-guard-recipe-module",
    );
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.identity-guard".to_string(),
        recipe_id: "recipe.identity-guard".to_string(),
        module_id: "m4.recipe.identity-guard".to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 1)],
                vec![MaterialStack::new("gear", 1)],
                Vec::new(),
                10,
                1,
            ))
            .expect("serialize identity-guard recipe plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("start module recipe");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending module recipe")
        .clone();

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before tampered blocker");
    let event = DomainEvent::FactoryProductionBlocked {
        action_id: pending.job_id,
        requester_agent_id: "forged-requester".to_string(),
        factory_id: "forged-factory".to_string(),
        recipe_id: "forged-recipe".to_string(),
        blocker_kind: "product_validation".to_string(),
        blocker_detail: "forged product validation disposition".to_string(),
    };
    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "tampered product-validation blocker must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after tampered blocker"),
        before,
        "tampered blocker must not mutate serialized world state"
    );
    assert_eq!(
        replay.pending_recipe_jobs.get(&pending.job_id),
        Some(&pending),
        "tampered blocker must retain the pending recipe commitment"
    );
}
