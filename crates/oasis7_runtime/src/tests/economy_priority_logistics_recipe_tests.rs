fn recipe_route_fixture(factory_id: &str) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register recipe route builder");
    world
        .set_material_balance("steel_plate", 20)
        .expect("seed factory steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed factory circuits");
    let spec = factory_spec(factory_id, 1, 1);
    prepare_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start recipe route factory");
    world.step().expect("finish recipe route factory");
    authorize_policy_update(
        &mut world,
        "builder-a",
        &format!("proposal.route.recipe.{factory_id}"),
    );
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("disable recipe route tax");
    seed_builder_electricity(&mut world, 100);
    world.set_resource_balance(ResourceKind::Electricity, 100);
    world
}

fn register_recipe_route(
    world: &mut World,
    from_ledger: &str,
    to_ledger: &str,
    kind: &str,
) -> String {
    let journal_before = world.journal().events.len();
    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: "builder-a".to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: kind.to_string(),
        distance_km: 100,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 0,
    });
    world.step().expect("register recipe route");
    world.journal().events[journal_before..]
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered {
                route_id: Some(route_id),
                from_ledger: event_from,
                to_ledger: event_to,
                kind: event_kind,
                distance_km,
                priority,
                ..
            }) if event_from == &MaterialLedgerId::site(from_ledger)
                && event_to == &MaterialLedgerId::site(to_ledger)
                && event_kind == kind
                && *distance_km == 100
                && *priority == MaterialTransitPriority::Standard =>
            {
                Some(route_id.clone())
            }
            _ => None,
        })
        .expect("recipe route registration event in registration step")
}

fn complete_recipe_route_transfer(
    world: &mut World,
    route_id: &str,
    from_ledger: &str,
    to_ledger: &str,
    kind: &str,
    amount: i64,
) {
    world
        .set_ledger_material_balance(MaterialLedgerId::site(from_ledger), kind, amount)
        .expect("seed recipe route source");
    world.submit_action(Action::TransferMaterial {
        requester_agent_id: "builder-a".to_string(),
        from_ledger: MaterialLedgerId::site(from_ledger),
        to_ledger: MaterialLedgerId::site(to_ledger),
        kind: kind.to_string(),
        amount,
        distance_km: 100,
        priority: Some(MaterialTransitPriority::Standard),
        route_id: Some(route_id.to_string()),
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().expect("start recipe route transit");
    world.step().expect("complete recipe route transit");
    assert!(
        world
            .state()
            .completed_logistics_route_ids
            .contains(route_id),
        "completed route should be available for explicit recipe binding"
    );
}

fn assert_recipe_route_rejected(
    world: &mut World,
    factory_id: &str,
    recipe_id: &str,
    plan: &RecipeExecutionPlan,
    logistics_route_ids: Vec<String>,
) {
    let pending_before = world.state().pending_recipe_jobs.len();
    let completed_before = world.state().industry_progress.completed_recipe_jobs;
    let journal_before = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids,
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("reject recipe route binding");
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
    assert_eq!(world.state().pending_recipe_jobs.len(), pending_before);
    assert_eq!(
        world.state().industry_progress.completed_recipe_jobs,
        completed_before
    );
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
}

fn complete_bound_recipe(
    world: &mut World,
    factory_id: &str,
    recipe_id: &str,
    plan: &RecipeExecutionPlan,
    logistics_route_ids: Vec<String>,
) {
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids,
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start explicit route recipe");
    world.step().expect("complete explicit route recipe");
}

#[test]
fn completed_route_binds_recipe_start_job_completion_and_world_state() {
    let factory_id = "factory.recipe.route.binding";
    let recipe_id = "recipe.recipe.route.binding";
    let mut world = recipe_route_fixture(factory_id);
    let route_id = register_recipe_route(&mut world, "source-a", "site-1", "iron_ingot");
    complete_recipe_route_transfer(&mut world, &route_id, "source-a", "site-1", "iron_ingot", 2);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );

    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: vec![route_id.clone()],
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start route-bound recipe");
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                logistics_route_ids,
                ..
            }) if logistics_route_ids == &vec![route_id.clone()]
        )
    }));
    assert_eq!(
        world
            .state()
            .pending_recipe_jobs
            .values()
            .next()
            .map(|job| job.logistics_route_ids.clone()),
        Some(vec![route_id.clone()])
    );

    world.step().expect("complete route-bound recipe");
    assert!(world.journal().events.iter().rev().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                logistics_route_ids,
                ..
            }) if logistics_route_ids == &vec![route_id.clone()]
        )
    }));
    assert!(
        world
            .state()
            .completed_logistics_route_ids
            .contains(&route_id)
    );
}

#[test]
fn incomplete_unknown_or_incompatible_recipe_routes_reject_without_progress() {
    let factory_id = "factory.recipe.route.reject";
    let recipe_id = "recipe.recipe.route.reject";
    let mut world = recipe_route_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );

    let uncompleted =
        register_recipe_route(&mut world, "source-uncompleted", "site-1", "iron_ingot");
    assert_recipe_route_rejected(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec!["route-unknown".to_string()],
    );
    assert_recipe_route_rejected(&mut world, factory_id, recipe_id, &plan, vec![uncompleted]);

    let wrong_destination =
        register_recipe_route(&mut world, "source-destination", "site-wrong", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &wrong_destination,
        "source-destination",
        "site-wrong",
        "iron_ingot",
        2,
    );
    assert_recipe_route_rejected(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![wrong_destination],
    );

    let wrong_material =
        register_recipe_route(&mut world, "source-material", "site-1", "copper_wire");
    complete_recipe_route_transfer(
        &mut world,
        &wrong_material,
        "source-material",
        "site-1",
        "copper_wire",
        2,
    );
    assert_recipe_route_rejected(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![wrong_material],
    );

    let route_a = register_recipe_route(&mut world, "source-a", "site-1", "iron_ingot");
    complete_recipe_route_transfer(&mut world, &route_a, "source-a", "site-1", "iron_ingot", 2);
    let route_b = register_recipe_route(&mut world, "source-b", "site-1", "iron_ingot");
    complete_recipe_route_transfer(&mut world, &route_b, "source-b", "site-1", "iron_ingot", 2);
    assert_recipe_route_rejected(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_a.clone(), route_a.clone()],
    );
    assert_recipe_route_rejected(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_b, route_a],
    );
}

#[test]
fn changing_completed_route_resets_stable_line_identity_but_retries_same_route() {
    let factory_id = "factory.recipe.route.identity";
    let recipe_id = "recipe.recipe.route.identity";
    let mut world = recipe_route_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    let route_a = register_recipe_route(&mut world, "source-identity-a", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_a,
        "source-identity-a",
        "site-1",
        "iron_ingot",
        4,
    );
    let route_b = register_recipe_route(&mut world, "source-identity-b", "site-1", "iron_ingot");
    complete_recipe_route_transfer(
        &mut world,
        &route_b,
        "source-identity-b",
        "site-1",
        "iron_ingot",
        6,
    );

    complete_bound_recipe(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_a.clone()],
    );
    let path_a = world
        .state()
        .factories
        .get(factory_id)
        .and_then(|factory| {
            factory
                .production
                .last_completed_canonical_snapshot
                .as_ref()
        })
        .map(|snapshot| snapshot.logistics_path_ids.clone())
        .expect("route A stable-line path identity");
    assert!(!path_a.is_empty());
    complete_bound_recipe(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_a.clone()],
    );
    let factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after route A");
    assert_eq!(factory.production.same_recipe_repeat_count, 2);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );

    complete_bound_recipe(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_b.clone()],
    );
    let factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after route B reset");
    assert_eq!(factory.production.same_recipe_repeat_count, 1);
    let path_b = factory
        .production
        .last_completed_canonical_snapshot
        .as_ref()
        .expect("route B stable-line path identity")
        .logistics_path_ids
        .clone();
    assert!(!path_b.is_empty());
    assert_ne!(path_a, path_b, "effective path change must reset identity");
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );

    complete_bound_recipe(
        &mut world,
        factory_id,
        recipe_id,
        &plan,
        vec![route_b.clone()],
    );
    complete_bound_recipe(&mut world, factory_id, recipe_id, &plan, vec![route_b]);
    let factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after route B retries");
    assert_eq!(factory.production.same_recipe_repeat_count, 3);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
}

#[test]
fn schedule_recipe_rejects_when_profile_stage_gate_exceeds_current_stage() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.stage_gate");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.stage_gate", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.profile.governance".to_string(),
            bottleneck_tags: vec!["gear".to_string()],
            stage_gate: "governance".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("insert recipe profile");

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 1)],
        vec![MaterialStack::new("module_rack", 1)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.stage_gate".to_string(),
        recipe_id: "recipe.profile.governance".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("schedule blocked by stage gate");

    let message = latest_action_rejected_message(&world);
    assert!(
        message.contains("stage gate denied"),
        "expected stage gate reject, got {message}"
    );

    let (blocker_kind, blocker_detail) = latest_factory_production_block(&world);
    assert_eq!(blocker_kind, "governance_gate");
    assert!(blocker_detail.contains("stage gate denied"));
}

#[test]
fn schedule_recipe_rejects_when_product_unlock_stage_exceeds_current_stage() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.unlock_stage");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.unlock_stage", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.profile.unlock_stage".to_string(),
            bottleneck_tags: Vec::new(),
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("insert recipe profile");
    world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "gear".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "governance".to_string(),
        })
        .expect("insert product profile");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed local material");
    world
        .set_material_balance("iron_ingot", 2)
        .expect("seed world material");
    seed_builder_electricity(&mut world, 10);
    world.set_resource_balance(ResourceKind::Electricity, 10);

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 1)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.unlock_stage".to_string(),
        recipe_id: "recipe.profile.unlock_stage".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("schedule blocked by product unlock stage");

    let message = latest_action_rejected_message(&world);
    assert!(
        message.contains("product unlock_stage denied"),
        "expected product unlock_stage reject, got {message}"
    );

    let (blocker_kind, blocker_detail) = latest_factory_production_block(&world);
    assert_eq!(blocker_kind, "governance_gate");
    assert!(blocker_detail.contains("product unlock_stage denied"));
}

#[test]
fn schedule_recipe_rejects_when_factory_tags_conflict_with_recipe_profile() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(
        &mut world,
        "builder-a",
        "site-1",
        "factory.preferred_tag",
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.preferred_tag", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.profile.tagged".to_string(),
            bottleneck_tags: Vec::new(),
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["smelter".to_string()],
        })
        .expect("insert recipe profile");

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 1)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.preferred_tag".to_string(),
        recipe_id: "recipe.profile.tagged".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("schedule blocked by preferred tag");

    let message = latest_action_rejected_message(&world);
    assert!(
        message.contains("preferred_factory_tags mismatch"),
        "expected preferred tag reject, got {message}"
    );

    let (blocker_kind, blocker_detail) = latest_factory_production_block(&world);
    assert_eq!(blocker_kind, "governance_gate");
    assert!(blocker_detail.contains("preferred_factory_tags mismatch"));
}

#[test]
fn schedule_recipe_uses_profile_bottleneck_tags_before_inference() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.bottleneck.profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.bottleneck.profile", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.profile.bottleneck".to_string(),
            bottleneck_tags: vec!["Copper_Wire".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("insert recipe profile");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "gear", 4)
        .expect("seed local material");
    world
        .set_material_balance("gear", 4)
        .expect("seed world material");
    seed_builder_electricity(&mut world, 20);
    world.set_resource_balance(ResourceKind::Electricity, 20);

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("gear", 2)],
        vec![MaterialStack::new("factory_frame", 1)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.bottleneck.profile".to_string(),
        recipe_id: "recipe.profile.bottleneck".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start profile bottleneck recipe");

    let bottleneck_tags = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                bottleneck_tags, ..
            }) => Some(bottleneck_tags.clone()),
            _ => None,
        })
        .expect("recipe started");
    assert_eq!(bottleneck_tags, vec!["copper_wire".to_string()]);
}

#[test]
fn due_recipe_jobs_prioritize_by_product_role_tag_before_keyword_fallback() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(
        &mut world,
        "builder-a",
        "site-1",
        "factory.role_tag",
    );
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: "factory.role_tag".to_string(),
            tier: 1,
            recipe_slots: 2,
            tags: vec!["assembly".to_string()],
        })
        .expect("set two-slot role-tag factory profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.role_tag", 1, 2),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "alpha_widget".to_string(),
            role_tag: "survival".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        })
        .expect("insert survival profile");
    world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "delta_widget".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        })
        .expect("insert scale profile");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "gear", 6)
        .expect("seed local material");
    world
        .set_material_balance("gear", 6)
        .expect("seed world material");
    seed_builder_electricity(&mut world, 20);
    world.set_resource_balance(ResourceKind::Electricity, 20);

    let scale_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("gear", 2)],
        vec![MaterialStack::new("delta_widget", 1)],
        Vec::new(),
        1,
        1,
    );
    let survival_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("gear", 2)],
        vec![MaterialStack::new("alpha_widget", 1)],
        Vec::new(),
        1,
        1,
    );

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.role_tag".to_string(),
        recipe_id: "recipe.misc.scale_a".to_string(),
        plan: scale_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.role_tag".to_string(),
        recipe_id: "recipe.misc.survival_b".to_string(),
        plan: survival_plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipes");
    assert_eq!(world.pending_recipe_jobs_len(), 2);

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
            "recipe.misc.survival_b".to_string(),
            "recipe.misc.scale_a".to_string(),
        ]
    );
}

#[test]
fn schedule_recipe_applies_product_maintenance_sink_to_consume() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.maintenance_sink");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.maintenance_sink", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: "recipe.profile.maintenance_sink".to_string(),
            bottleneck_tags: vec!["iron_ingot".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("insert recipe profile");
    world
        .upsert_product_profile(ProductProfileV1 {
            product_id: "durable_part".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 2)],
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        })
        .expect("insert product profile");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed local iron");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "hardware_part", 4)
        .expect("seed local hardware");
    world
        .set_material_balance("iron_ingot", 2)
        .expect("seed world iron");
    world
        .set_material_balance("hardware_part", 4)
        .expect("seed world hardware");
    seed_builder_electricity(&mut world, 10);
    world.set_resource_balance(ResourceKind::Electricity, 10);

    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 1)],
        vec![MaterialStack::new("durable_part", 2)],
        Vec::new(),
        1,
        1,
    );
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.maintenance_sink".to_string(),
        recipe_id: "recipe.profile.maintenance_sink".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("schedule with maintenance sink");

    let consume = world
        .journal()
        .events
        .last()
        .and_then(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::RecipeStarted { consume, .. }) => {
                Some(consume.clone())
            }
            _ => None,
        })
        .expect("recipe started");
    let mut consume_map = std::collections::BTreeMap::new();
    for stack in consume {
        consume_map.insert(stack.kind, stack.amount);
    }
    assert_eq!(consume_map.get("iron_ingot"), Some(&1));
    assert_eq!(consume_map.get("hardware_part"), Some(&4));
}

#[test]
fn recipe_started_market_quote_reflects_governance_tax_change() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(
        &mut world,
        "builder-a",
        "site-1",
        "factory.quote",
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.quote", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 12)
        .expect("seed local recipe input");
    world
        .set_material_balance("iron_ingot", 100)
        .expect("seed world recipe input");
    seed_builder_electricity(&mut world, 50);
    world.set_resource_balance(ResourceKind::Electricity, 50);

    authorize_policy_update(&mut world, "builder-a", "proposal.policy.zero-tax");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("set zero tax policy");

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
        factory_id: "factory.quote".to_string(),
        recipe_id: "recipe.quote.low_tax".to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start low tax recipe");

    let low_tax_quote = match &world.journal().events.last().expect("recipe started").body {
        WorldEventBody::Domain(DomainEvent::RecipeStarted { market_quotes, .. }) => market_quotes
            .iter()
            .find(|quote| quote.kind == "iron_ingot")
            .expect("iron quote under low tax")
            .clone(),
        other => panic!("expected RecipeStarted, got {other:?}"),
    };
    assert_eq!(low_tax_quote.governance_tax_bps, 0);

    world.step().expect("complete low tax recipe");

    authorize_policy_update(&mut world, "builder-a", "proposal.policy.high-tax");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 900,
        data_tax_bps: 700,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("set high tax policy");

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.quote".to_string(),
        recipe_id: "recipe.quote.high_tax".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start high tax recipe");

    let high_tax_quote = match &world.journal().events.last().expect("recipe started").body {
        WorldEventBody::Domain(DomainEvent::RecipeStarted { market_quotes, .. }) => market_quotes
            .iter()
            .find(|quote| quote.kind == "iron_ingot")
            .expect("iron quote under high tax")
            .clone(),
        other => panic!("expected RecipeStarted, got {other:?}"),
    };
    assert_eq!(high_tax_quote.governance_tax_bps, 1_600);
    assert!(
        high_tax_quote.effective_cost_index_ppm > low_tax_quote.effective_cost_index_ppm,
        "expected effective cost to increase with governance tax: low={:?} high={:?}",
        low_tax_quote,
        high_tax_quote
    );
}

#[test]
fn recipe_started_market_quote_uses_material_profile_transport_loss() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");

    world
        .set_material_balance("steel_plate", 20)
        .expect("seed build steel");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed build circuits");
    prepare_factory_build_id(
        &mut world,
        "builder-a",
        "site-1",
        "factory.quote.profile_loss",
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.quote.profile_loss", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .upsert_material_profile(MaterialProfileV1 {
            kind: "iron_ingot".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::High,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        })
        .expect("insert iron profile");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 12)
        .expect("seed local recipe input");
    world
        .set_material_balance("iron_ingot", 100)
        .expect("seed world recipe input");
    seed_builder_electricity(&mut world, 50);
    world.set_resource_balance(ResourceKind::Electricity, 50);

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
        factory_id: "factory.quote.profile_loss".to_string(),
        recipe_id: "recipe.quote.profile_loss".to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");

    let quote = match &world.journal().events.last().expect("recipe started").body {
        WorldEventBody::Domain(DomainEvent::RecipeStarted { market_quotes, .. }) => market_quotes
            .iter()
            .find(|quote| quote.kind == "iron_ingot")
            .expect("iron quote")
            .clone(),
        other => panic!("expected RecipeStarted, got {other:?}"),
    };
    assert_eq!(quote.transit_loss_bps, 20);
}
