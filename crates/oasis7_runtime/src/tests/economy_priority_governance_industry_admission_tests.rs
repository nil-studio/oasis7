#[test]
fn product_validation_failure_clears_prior_stable_line_candidate() {
    let factory_id = "factory.identity.product-validation-reset";
    let recipe_id = "recipe.identity.product-validation-reset";
    let action_id = 42;
    let mut state = stable_line_minimal_state(factory_id);
    let factory = state
        .factories
        .get_mut(factory_id)
        .expect("product-validation reset factory");
    factory.production.status = crate::FactoryProductionStatus::Running;
    factory.production.active_jobs = 1;
    factory.production.current_job_id = Some(action_id);
    factory.production.current_recipe_id = Some(recipe_id.to_string());
    factory.production.last_completed_recipe_id = Some(recipe_id.to_string());
    factory.production.same_recipe_repeat_count = 2;
    factory.production.last_completed_canonical_snapshot = Some(
        FactoryProductionSnapshot {
            recipe_id: recipe_id.to_string(),
            ..FactoryProductionSnapshot::default()
        },
    );
    state.pending_recipe_jobs.insert(
        action_id,
        crate::RecipeJobState {
            committed_recipe_origin: None,
            job_id: action_id,
            requester_agent_id: "stable-line-test-agent".to_string(),
            factory_id: factory_id.to_string(),
            recipe_id: recipe_id.to_string(),
            accepted_batches: 1,
            consume: vec![MaterialStack::new("input", 1)],
            produce: vec![MaterialStack::new("product", 1)],
            byproducts: Vec::new(),
            power_required: 0,
            power_owner_agent_id: Some("stable-line-test-agent".to_string()),
            duration_ticks: 1,
            consume_ledger: MaterialLedgerId::site("stable-line-test-site"),
            output_ledger: MaterialLedgerId::site("stable-line-test-site"),
            bottleneck_tags: Vec::new(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
            ready_at: 1,
        },
    );

    state
        .apply_domain_event(
            &DomainEvent::FactoryProductionBlocked {
                action_id,
                requester_agent_id: "stable-line-test-agent".to_string(),
                factory_id: factory_id.to_string(),
                recipe_id: recipe_id.to_string(),
                blocker_kind: "product_validation".to_string(),
                blocker_detail: "product validation rejected".to_string(),
            },
            2,
        )
        .expect("product-validation blocker");

    let production = &state
        .factories
        .get(factory_id)
        .expect("product-validation reset factory after blocker")
        .production;
    assert_eq!(
        production.status,
        crate::FactoryProductionStatus::Blocked
    );
    assert_eq!(production.active_jobs, 0);
    assert_eq!(production.last_completed_recipe_id, None);
    assert_eq!(production.same_recipe_repeat_count, 0);
    assert_eq!(production.last_completed_canonical_snapshot, None);
}

#[test]
fn duplicate_recipe_material_stacks_are_rejected_atomically() {
    let factory_id = "factory.identity.duplicate-recipe-input";
    let mut world = stable_identity_fixture(factory_id);
    let input_ledger = MaterialLedgerId::site("site-1");
    world
        .set_ledger_material_balance(input_ledger.clone(), "iron_ingot", 3)
        .expect("seed aggregate recipe input");
    let journal_start = world.journal().events.len();

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.duplicate-recipe-input".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![
                MaterialStack::new("iron_ingot", 2),
                MaterialStack::new("iron_ingot", 2),
            ],
            vec![MaterialStack::new("unprofiled_product", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("duplicate recipe material stacks should become a rejection");

    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(world.ledger_material_balance(&input_ledger, "iron_ingot"), 3);
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::InsufficientMaterial {
                    material_kind,
                    requested,
                    available,
                },
                ..
            }) if material_kind == "iron_ingot" && *requested == 4 && *available == 3
        )
    }));
}
