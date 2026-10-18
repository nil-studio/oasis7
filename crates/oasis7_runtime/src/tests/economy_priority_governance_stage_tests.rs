#[test]
fn industry_stage_progresses_from_bootstrap_to_scale_out_and_governance() {
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
    let spec = factory_spec("factory.stage", 1, 1);
    prepare_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 30)
        .expect("seed local recipe material");
    seed_builder_electricity(&mut world, 100);
    world.set_resource_balance(ResourceKind::Electricity, 100);

    authorize_policy_update(&mut world, "builder-a", "proposal.policy.disable-tax");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("disable tax policy");

    let recipe_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    for _ in 0..3 {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stage".to_string(),
            recipe_id: "recipe.stage.stable_line".to_string(),
            plan: recipe_plan.clone(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        world.step().expect("start recipe");
        world.step().expect("complete recipe");
    }

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 3);
    assert_eq!(
        world.state().industry_progress.completed_material_transits,
        0
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-a"), "copper_wire", 60)
        .expect("seed transit material");
    for _ in 0..3 {
        world.submit_action(Action::TransferMaterial {
            requester_agent_id: "builder-a".to_string(),
            from_ledger: MaterialLedgerId::site("site-a"),
            to_ledger: MaterialLedgerId::site("site-b"),
            kind: "copper_wire".to_string(),
            amount: 10,
            distance_km: 100,
            priority: None,
            route_id: None,
            route_ids: Vec::new(),
            auto_reroute: false,
        });
        world.step().expect("start transit");
        world.step().expect("complete transit");
    }
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
    assert_eq!(
        world.state().industry_progress.completed_material_transits,
        3
    );

    authorize_policy_update(&mut world, "builder-a", "proposal.policy.enable-tax");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 500,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("enable tax policy");

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Governance
    );
}

#[test]
fn mixed_recipe_completions_on_one_factory_do_not_unlock_scale_out() {
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
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.stage.mixed");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.stage.mixed", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed iron recipe material");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "copper_wire", 2)
        .expect("seed copper recipe material");
    seed_builder_electricity(&mut world, 100);
    world.set_resource_balance(ResourceKind::Electricity, 100);

    let mixed_recipes = [
        (
            "recipe.stage.mixed.gear",
            RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 2)],
                vec![MaterialStack::new("gear", 1)],
                Vec::new(),
                1,
                1,
            ),
        ),
        (
            "recipe.stage.mixed.module",
            RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("iron_ingot", 2)],
                vec![MaterialStack::new("module", 1)],
                Vec::new(),
                1,
                1,
            ),
        ),
        (
            "recipe.stage.mixed.cable",
            RecipeExecutionPlan::accepted(
                1,
                vec![MaterialStack::new("copper_wire", 2)],
                vec![MaterialStack::new("cable", 1)],
                Vec::new(),
                1,
                1,
            ),
        ),
    ];

    for (recipe_id, plan) in mixed_recipes {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stage.mixed".to_string(),
            recipe_id: recipe_id.to_string(),
            plan,
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        world.step().expect("start mixed recipe");
        world.step().expect("complete mixed recipe");
    }

    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 3);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap,
        "mixed recipes on one factory are not one stable canonical production line"
    );
}

#[test]
fn factory_blocker_resets_stable_line_and_requires_three_fresh_completions() {
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
    let spec = factory_spec("factory.stage.blocker", 1, 1);
    prepare_factory_build(&mut world, "builder-a", "site-1", &spec);
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    authorize_policy_update(
        &mut world,
        "builder-a",
        "proposal.policy.disable-tax.blocker",
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
    world.step().expect("disable tax policy");

    let recipe_id = "recipe.stage.stable_line";
    let recipe_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 12)
        .expect("seed pre- and post-block recipe material");
    seed_builder_electricity(&mut world, 100);
    world.set_resource_balance(ResourceKind::Electricity, 100);

    for _ in 0..3 {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stage.blocker".to_string(),
            recipe_id: recipe_id.to_string(),
            plan: recipe_plan.clone(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        world.step().expect("start pre-block recipe");
        world.step().expect("complete pre-block recipe");
    }
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
    assert_eq!(
        world
            .state()
            .factories
            .get("factory.stage.blocker")
            .expect("factory after stable line")
            .production
            .same_recipe_repeat_count,
        3
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 0)
        .expect("remove input to trigger blocker");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.stage.blocker".to_string(),
        recipe_id: recipe_id.to_string(),
        plan: recipe_plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("block stable line");

    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
            factory_id,
            recipe_id: blocked_recipe_id,
            ..
        })) if factory_id == "factory.stage.blocker" && blocked_recipe_id == recipe_id
    ));
    let blocked_factory = world
        .state()
        .factories
        .get("factory.stage.blocker")
        .expect("factory after blocker");
    assert_eq!(blocked_factory.production.same_recipe_repeat_count, 0);
    assert!(
        blocked_factory
            .production
            .last_completed_recipe_id
            .is_none()
    );
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 3);

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed post-block recipe material");
    for completion_index in 0..3 {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stage.blocker".to_string(),
            recipe_id: recipe_id.to_string(),
            plan: recipe_plan.clone(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        world.step().expect("resume or start post-block recipe");
        world.step().expect("complete post-block recipe");

        let factory = world
            .state()
            .factories
            .get("factory.stage.blocker")
            .expect("factory after post-block completion");
        assert_eq!(
            factory.production.same_recipe_repeat_count,
            completion_index + 1
        );
        assert_eq!(
            world.state().industry_progress.stage,
            if completion_index == 2 {
                IndustryStage::ScaleOut
            } else {
                IndustryStage::Bootstrap
            }
        );
    }
}

#[test]
fn industry_stage_downgrades_when_last_completed_factory_is_recycled() {
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
    prepare_factory_build_id(&mut world, "builder-a", "site-1", "factory.stage");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.stage", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("factory ready");

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 30)
        .expect("seed local recipe material");
    seed_builder_electricity(&mut world, 100);
    world.set_resource_balance(ResourceKind::Electricity, 100);

    authorize_policy_update(&mut world, "builder-a", "proposal.policy.disable-tax");
    world.submit_action(Action::UpdateGameplayPolicy {
        operator_agent_id: "builder-a".to_string(),
        electricity_tax_bps: 0,
        data_tax_bps: 0,
        power_trade_fee_bps: 0,
        max_open_contracts_per_agent: 16,
        blocked_agents: Vec::new(),
        forbidden_location_ids: Vec::new(),
    });
    world.step().expect("disable tax policy");

    let recipe_plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    let recipe_id = "recipe.stage.recycle";
    for _ in 0..3 {
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stage".to_string(),
            recipe_id: recipe_id.to_string(),
            plan: recipe_plan.clone(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        world.step().expect("start recipe");
        world.step().expect("complete recipe");
    }

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.stage".to_string(),
    });
    world.step().expect("recycle completed factory");

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert!(!world.has_factory("factory.stage"));
}

fn assert_rejected_note_contains(world: &World, action_id: u64, expected: &str) {
    let reason = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id: rejected_action_id,
                reason,
            }) if *rejected_action_id == action_id => Some(format!("{reason:?}")),
            _ => None,
        })
        .expect("action rejected event");
    assert!(
        reason.contains(expected),
        "expected `{expected}` in rejection reason: {reason}"
    );
}

#[test]
fn govern_profile_requires_existing_approved_or_applied_manifest_proposal() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");

    let missing_action_id = world.submit_action(Action::GovernMaterialProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id: 9_999,
        profile: MaterialProfileV1 {
            kind: "copper_wire".to_string(),
            tier: 2,
            category: "intermediate".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::High,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Urgent,
        },
    });
    world.step().expect("missing proposal rejection");
    assert_rejected_note_contains(&world, missing_action_id, "governance proposal not found");

    let mut manifest = world.manifest().clone();
    manifest.version = manifest.version.saturating_add(1);
    let proposed_only_id = world
        .propose_manifest_update(manifest, "operator-a".to_string())
        .expect("propose manifest update");
    let proposed_action_id = world.submit_action(Action::GovernProductProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id: proposed_only_id,
        profile: ProductProfileV1 {
            product_id: "governed_product".to_string(),
            role_tag: "scale".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        },
    });
    world.step().expect("proposed-only rejection");
    assert_rejected_note_contains(
        &world,
        proposed_action_id,
        "governance proposal must be approved or applied",
    );
}

#[test]
fn govern_profile_actions_emit_events_and_update_profile_state() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");
    let proposal_id = approved_manifest_proposal(&mut world, "operator-a");

    world.submit_action(Action::GovernMaterialProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: MaterialProfileV1 {
            kind: "governed_wire".to_string(),
            tier: 3,
            category: "intermediate".to_string(),
            stack_limit: 700,
            transport_loss_class: MaterialTransportLossClass::Low,
            decay_bps_per_tick: 5,
            default_priority: MaterialDefaultPriority::Standard,
        },
    });
    world.step().expect("govern material profile");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::MaterialProfileGoverned {
            operator_agent_id,
            proposal_id: event_proposal_id,
            profile,
        })) if operator_agent_id == "operator-a"
            && *event_proposal_id == proposal_id
            && profile.kind == "governed_wire"
    ));

    world.submit_action(Action::GovernProductProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: ProductProfileV1 {
            product_id: "governed_product".to_string(),
            role_tag: "survival".to_string(),
            maintenance_sink: vec![MaterialStack::new("hardware_part", 1)],
            tradable: true,
            unlock_stage: "scale_out".to_string(),
        },
    });
    world.step().expect("govern product profile");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ProductProfileGoverned {
            operator_agent_id,
            proposal_id: event_proposal_id,
            profile,
        })) if operator_agent_id == "operator-a"
            && *event_proposal_id == proposal_id
            && profile.product_id == "governed_product"
    ));

    world.submit_action(Action::GovernRecipeProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: RecipeProfileV1 {
            recipe_id: "governed_recipe".to_string(),
            bottleneck_tags: vec!["control_chip".to_string()],
            stage_gate: "governance".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        },
    });
    world.step().expect("govern recipe profile");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::RecipeProfileGoverned {
            operator_agent_id,
            proposal_id: event_proposal_id,
            profile,
        })) if operator_agent_id == "operator-a"
            && *event_proposal_id == proposal_id
            && profile.recipe_id == "governed_recipe"
    ));

    world.submit_action(Action::GovernFactoryProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: FactoryProfileV1 {
            factory_id: "governed_factory".to_string(),
            tier: 2,
            recipe_slots: 3,
            tags: vec!["assembly".to_string()],
        },
    });
    world.step().expect("govern factory profile");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::FactoryProfileGoverned {
            operator_agent_id,
            proposal_id: event_proposal_id,
            profile,
        })) if operator_agent_id == "operator-a"
            && *event_proposal_id == proposal_id
            && profile.factory_id == "governed_factory"
    ));

    assert_eq!(
        world
            .state()
            .material_profiles
            .get("governed_wire")
            .map(|profile| profile.stack_limit),
        Some(700)
    );
    assert_eq!(
        world
            .state()
            .product_profiles
            .get("governed_product")
            .map(|profile| profile.role_tag.as_str()),
        Some("survival")
    );
    assert_eq!(
        world
            .state()
            .recipe_profiles
            .get("governed_recipe")
            .map(|profile| profile.stage_gate.as_str()),
        Some("governance")
    );
    assert_eq!(
        world
            .state()
            .factory_profiles
            .get("governed_factory")
            .map(|profile| profile.recipe_slots),
        Some(3)
    );
}

#[test]
fn govern_profile_actions_reject_invalid_profile_payloads() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "operator-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register operator");
    let proposal_id = approved_manifest_proposal(&mut world, "operator-a");

    let material_action_id = world.submit_action(Action::GovernMaterialProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: MaterialProfileV1 {
            kind: "broken_material".to_string(),
            tier: 0,
            category: "intermediate".to_string(),
            stack_limit: 100,
            transport_loss_class: MaterialTransportLossClass::Medium,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        },
    });
    world.step().expect("reject invalid material profile");
    assert_rejected_note_contains(&world, material_action_id, "tier must be >= 1");

    let product_action_id = world.submit_action(Action::GovernProductProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: ProductProfileV1 {
            product_id: "broken_product".to_string(),
            role_tag: "".to_string(),
            maintenance_sink: Vec::new(),
            tradable: true,
            unlock_stage: "bootstrap".to_string(),
        },
    });
    world.step().expect("reject invalid product profile");
    assert_rejected_note_contains(&world, product_action_id, "role_tag cannot be empty");

    let recipe_action_id = world.submit_action(Action::GovernRecipeProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: RecipeProfileV1 {
            recipe_id: "".to_string(),
            bottleneck_tags: vec!["gear".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        },
    });
    world.step().expect("reject invalid recipe profile");
    assert_rejected_note_contains(&world, recipe_action_id, "recipe_id cannot be empty");

    let factory_action_id = world.submit_action(Action::GovernFactoryProfile {
        operator_agent_id: "operator-a".to_string(),
        proposal_id,
        profile: FactoryProfileV1 {
            factory_id: "broken_factory".to_string(),
            tier: 1,
            recipe_slots: 0,
            tags: vec!["assembly".to_string()],
        },
    });
    world.step().expect("reject invalid factory profile");
    assert_rejected_note_contains(&world, factory_action_id, "recipe_slots must be > 0");
}
