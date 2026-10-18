#[test]
fn schedule_recipe_accepts_smelter_recipe_on_smelter_factory() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 0;
    config.economy.recipe_hardware_cost_per_batch = 0;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    kernel.submit_action(Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        factory_id: "factory.smelter.alpha".to_string(),
        recipe_id: "recipe.smelter.iron_ingot".to_string(),
        batches: 1,
    });
    let event = kernel.step().expect("schedule smelter recipe");
    match event.kind {
        WorldEventKind::RecipeScheduled {
            recipe_id,
            finished_product_id,
            ..
        } => {
            assert_eq!(recipe_id, "recipe.smelter.iron_ingot");
            assert_eq!(finished_product_id, "iron_ingot");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn schedule_recipe_accepts_scale_out_recipe_on_smelter_factory() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 6;
    config.economy.recipe_hardware_cost_per_batch = 2;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        ResourceKind::Electricity,
        32,
    );
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        ResourceKind::Data,
        16,
    );

    kernel.submit_action(Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-smelter".to_string(),
        },
        factory_id: "factory.smelter.alpha".to_string(),
        recipe_id: "recipe.smelter.alloy_plate".to_string(),
        batches: 1,
    });
    let event = kernel.step().expect("schedule alloy plate recipe");
    match event.kind {
        WorldEventKind::RecipeScheduled {
            recipe_id,
            finished_product_id,
            ..
        } => {
            assert_eq!(recipe_id, "recipe.smelter.alloy_plate");
            assert_eq!(finished_product_id, "alloy_plate");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

fn set_agent_power(kernel: &mut WorldKernel, agent_id: &str, capacity: i64, level: i64) {
    let config = kernel.config().clone();
    let mut snapshot = kernel.kernel_snapshot();
    let agent = snapshot
        .model
        .agents
        .get_mut(agent_id)
        .expect("agent exists in snapshot");
    let mut power = AgentPowerStatus::new(capacity, level);
    power.update_state(&config.power);
    agent.power = power;
    let journal = kernel.kernel_journal_snapshot();
    *kernel = WorldKernel::from_kernel_snapshot(snapshot, journal)
        .expect("rebuild kernel from power-configured snapshot");
}

#[test]
fn schedule_recipe_quote_keeps_healthy_battery_runway_independent_from_electricity_ledger() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 6;
    config.economy.recipe_hardware_cost_per_batch = 2;
    config.economy.recipe_data_output_per_batch = 1;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 32);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 16);
    set_agent_power(&mut kernel, "agent-smelter", 100, 100);

    let journal_len_before_quote = kernel.journal().len();
    let resources_before_quote = kernel
        .model()
        .agents
        .get("agent-smelter")
        .expect("agent exists")
        .resources
        .clone();
    let quote = kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            2,
        )
        .expect("schedule quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-smelter")
            .expect("agent exists")
            .resources,
        resources_before_quote
    );
    assert_eq!(quote.factory_id, "factory.smelter.alpha");
    assert_eq!(quote.recipe_id, "recipe.smelter.alloy_plate");
    assert_eq!(quote.batches, 2);
    assert_eq!(quote.base_duration_ticks, 1);
    assert_eq!(quote.electricity_cost, 18);
    assert_eq!(quote.hardware_cost, 8);
    assert_eq!(quote.data_output, 4);
    assert_eq!(quote.finished_product_id, "alloy_plate");
    assert_eq!(quote.finished_product_units, 4);
    assert_eq!(quote.local_shortage_delay_ticks, 0);
    assert_eq!(quote.shortage_reason, "none");
    assert_eq!(quote.electricity_after, 14);
    assert_eq!(quote.runway_before_ticks, 100);
    assert_eq!(quote.runway_after_ticks, 100);
    assert_eq!(quote.downtime_threshold_ppm, 50_000);
    assert_eq!(quote.continue_production_risk, "low");
    assert_eq!(quote.maintenance_pressure_delta, "unchanged");
    assert_eq!(quote.recommended_pre_step, "schedule_now");
    assert_eq!(quote.recommended_maintenance_action, "continue_production");
}

#[test]
fn schedule_recipe_quote_keeps_critical_battery_risk_elevated_despite_healthy_ledger() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 6;
    config.economy.recipe_hardware_cost_per_batch = 2;
    config.economy.recipe_data_output_per_batch = 1;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 32);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 16);
    set_agent_power(&mut kernel, "agent-smelter", 100, 5);

    let quote = kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            2,
        )
        .expect("schedule quote");

    assert_eq!(quote.electricity_after, 14);
    assert_eq!(quote.runway_before_ticks, 5);
    assert_eq!(quote.runway_after_ticks, 5);
    assert_eq!(quote.continue_production_risk, "elevated");
    assert_eq!(
        quote.recommended_pre_step,
        "restore_power_before_scheduling"
    );
    assert_eq!(quote.recommended_maintenance_action, "restore_power");
}

#[test]
fn schedule_recipe_quote_keeps_critical_risk_with_unbounded_runway_when_idle_cost_is_zero() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 6;
    config.economy.recipe_hardware_cost_per_batch = 2;
    config.economy.recipe_data_output_per_batch = 1;
    config.power.idle_cost_per_tick = 0;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 32);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 16);
    set_agent_power(&mut kernel, "agent-smelter", 100, 5);

    let quote = kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            2,
        )
        .expect("schedule quote without dividing by zero");

    assert_eq!(quote.runway_before_ticks, i64::MAX);
    assert_eq!(quote.runway_after_ticks, i64::MAX);
    assert_eq!(quote.continue_production_risk, "elevated");
    assert_eq!(
        quote.recommended_pre_step,
        "restore_power_before_scheduling"
    );
    assert_eq!(quote.recommended_maintenance_action, "restore_power");
}

#[test]
fn schedule_recipe_quote_reports_zero_shutdown_runway_when_idle_cost_is_zero_without_mutation() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 6;
    config.economy.recipe_hardware_cost_per_batch = 2;
    config.economy.recipe_data_output_per_batch = 1;
    config.power.idle_cost_per_tick = 0;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 32);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 16);
    set_agent_power(&mut kernel, "agent-smelter", 100, 0);

    let journal_len_before_quote = kernel.journal().len();
    let agent_before_quote = kernel
        .model()
        .agents
        .get("agent-smelter")
        .expect("agent exists")
        .clone();
    let quote = kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            2,
        )
        .expect("shutdown schedule quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-smelter")
            .expect("agent exists"),
        &agent_before_quote
    );
    assert_eq!(quote.electricity_after, 14);
    assert_eq!(quote.runway_before_ticks, 0);
    assert_eq!(quote.runway_after_ticks, 0);
    assert_eq!(quote.continue_production_risk, "elevated");
    assert_eq!(
        quote.recommended_pre_step,
        "restore_power_before_scheduling"
    );
    assert_eq!(quote.recommended_maintenance_action, "restore_power");
}

#[test]
fn schedule_recipe_quote_rejects_output_overflow_like_execution() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 0;
    config.economy.recipe_hardware_cost_per_batch = 0;
    config.economy.recipe_data_output_per_batch = 1;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 8);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, i64::MAX - 1);

    let journal_len_before_quote = kernel.journal().len();
    let reason = kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            1,
        )
        .expect_err("output overflow should reject");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(reason, RejectReason::InvalidAmount { amount: 2 });
}

#[test]
fn schedule_recipe_output_overflow_rejection_is_atomic_without_a_success_sink() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 1;
    config.economy.recipe_hardware_cost_per_batch = 0;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 7);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, i64::MAX - 1);

    let resources_before_submit = kernel
        .model()
        .agents
        .get("agent-smelter")
        .expect("agent exists")
        .resources
        .clone();
    let journal_len_before_submit = kernel.journal().len();
    kernel.submit_action(Action::ScheduleRecipe {
        owner,
        factory_id: "factory.smelter.alpha".to_string(),
        recipe_id: "recipe.smelter.alloy_plate".to_string(),
        batches: 1,
    });

    let event = kernel.step().expect("overflowing schedule submission");
    assert!(matches!(
        event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::InvalidAmount { amount: 2 }
        }
    ));
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-smelter")
            .expect("agent exists")
            .resources,
        resources_before_submit,
        "a late output rejection must not debit either input resource"
    );
    assert_eq!(kernel.journal().len(), journal_len_before_submit + 1);
    assert!(matches!(
        kernel
            .journal()
            .last()
            .expect("rejection journal entry")
            .kind,
        WorldEventKind::ActionRejected { .. }
    ));
    assert!(
        !kernel.journal()[journal_len_before_submit..]
            .iter()
            .any(|event| matches!(event.kind, WorldEventKind::RecipeScheduled { .. })),
        "a rejected submission must not emit a success receipt/sink"
    );
}

#[test]
fn schedule_recipe_revalidates_after_a_competing_submission_without_a_second_success() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    config.economy.recipe_electricity_cost_per_batch = 2;
    config.economy.recipe_hardware_cost_per_batch = 1;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-smelter".to_string(),
        name: "smelter-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-smelter".to_string(),
        location_id: "loc-smelter".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-smelter".to_string(),
    };
    kernel.submit_action(Action::BuildFactory {
        owner: owner.clone(),
        location_id: "loc-smelter".to_string(),
        factory_id: "factory.smelter.alpha".to_string(),
        factory_kind: "factory.smelter.mk1".to_string(),
    });
    kernel.step().expect("build smelter factory");
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 5);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 2);
    kernel
        .quote_schedule_recipe(
            &owner,
            "factory.smelter.alpha",
            "recipe.smelter.alloy_plate",
            1,
        )
        .expect("quote is valid before competition");

    let action = || Action::ScheduleRecipe {
        owner: owner.clone(),
        factory_id: "factory.smelter.alpha".to_string(),
        recipe_id: "recipe.smelter.alloy_plate".to_string(),
        batches: 1,
    };
    kernel.submit_action(action());
    let competing_event = kernel.step().expect("competing submission");
    assert!(matches!(
        competing_event.kind,
        WorldEventKind::RecipeScheduled { .. }
    ));
    let resources_after_competition = kernel
        .model()
        .agents
        .get("agent-smelter")
        .expect("agent exists")
        .resources
        .clone();
    let journal_len_before_stale_submit = kernel.journal().len();

    kernel.submit_action(action());
    let stale_event = kernel.step().expect("stale quoted submission");
    assert!(matches!(
        stale_event.kind,
        WorldEventKind::ActionRejected {
            reason: RejectReason::InsufficientResource {
                kind: ResourceKind::Electricity,
                requested: 5,
                available: 0,
                ..
            }
        }
    ));
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-smelter")
            .expect("agent exists")
            .resources,
        resources_after_competition,
        "the stale submission must not create a hidden debt after revalidation"
    );
    assert_eq!(kernel.journal().len(), journal_len_before_stale_submit + 1);
    assert!(matches!(
        kernel
            .journal()
            .last()
            .expect("rejection journal entry")
            .kind,
        WorldEventKind::ActionRejected { .. }
    ));
}

#[test]
fn refine_compound_quote_previews_net_value_without_mutating_state() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_hardware_cost = 5;
    config.economy.refine_electricity_cost_per_kg = 3;
    config.economy.refine_hardware_yield_ppm = 2_000;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-refiner".to_string(),
    };
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 50);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 2_500);

    let journal_len_before_quote = kernel.journal().len();
    let resources_before_quote = kernel
        .model()
        .agents
        .get("agent-refiner")
        .expect("agent exists")
        .resources
        .clone();
    let quote = kernel
        .quote_refine_compound(&owner, 2_500)
        .expect("refine quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-refiner")
            .expect("agent exists")
            .resources,
        resources_before_quote
    );
    assert_eq!(quote.owner, owner);
    assert_eq!(quote.compound_mass_g, 2_500);
    assert_eq!(quote.electricity_cost, 9);
    assert_eq!(quote.hardware_output, 5);
    assert_eq!(quote.electricity_after, 41);
    assert_eq!(quote.hardware_shortfall_before, 5);
    assert_eq!(quote.hardware_shortfall_after, 0);
    assert_eq!(
        quote.first_goal_relevance,
        "enables_factory_build_hardware_goal"
    );
    assert_eq!(quote.recommended_refine_amount, 2_500);
    assert_eq!(quote.refine_value_class, "enough_to_advance");
}

#[test]
fn refine_compound_quote_rejects_zero_output_like_execution() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-refiner".to_string(),
    };
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 10);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 999);

    let journal_len_before_quote = kernel.journal().len();
    let resources_before_quote = kernel
        .model()
        .agents
        .get("agent-refiner")
        .expect("agent exists")
        .resources
        .clone();
    let reason = kernel
        .quote_refine_compound(&owner, 999)
        .expect_err("zero hardware output should reject");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-refiner")
            .expect("agent exists")
            .resources,
        resources_before_quote
    );
    assert_eq!(reason, RejectReason::InvalidAmount { amount: 999 });
}

#[test]
fn refine_compound_quote_marks_partial_progress_when_output_does_not_cover_goal() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_hardware_cost = 5;
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-refiner".to_string(),
    };
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 10);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 2_500);

    let quote = kernel
        .quote_refine_compound(&owner, 2_500)
        .expect("partial refine quote");

    assert_eq!(quote.hardware_output, 2);
    assert_eq!(quote.hardware_shortfall_before, 5);
    assert_eq!(quote.hardware_shortfall_after, 3);
    assert_eq!(
        quote.first_goal_relevance,
        "reduces_factory_build_hardware_shortfall"
    );
    assert_eq!(quote.recommended_refine_amount, 2_500);
    assert_eq!(quote.refine_value_class, "partial_progress");
}

#[test]
fn refine_compound_quote_uses_existing_goal_progress_for_recommendation() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_hardware_cost = 5;
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-refiner".to_string(),
    };
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 10);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 6_002);

    let quote = kernel
        .quote_refine_compound(&owner, 6_000)
        .expect("existing progress refine quote");

    assert_eq!(quote.hardware_output, 6);
    assert_eq!(quote.hardware_shortfall_before, 3);
    assert_eq!(quote.hardware_shortfall_after, 0);
    assert_eq!(quote.recommended_refine_amount, 3_000);
    assert_eq!(quote.refine_value_class, "enough_to_advance");
}

#[test]
fn refine_compound_quote_marks_poor_tradeoff_when_goal_already_met() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_hardware_cost = 5;
    config.economy.refine_electricity_cost_per_kg = 1;
    config.economy.refine_hardware_yield_ppm = 1_000;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-refine".to_string(),
        name: "refine-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-refiner".to_string(),
        location_id: "loc-refine".to_string(),
    });
    kernel.step_until_empty();

    let owner = ResourceOwner::Agent {
        agent_id: "agent-refiner".to_string(),
    };
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Electricity, 10);
    seed_owner_resource(&mut kernel, owner.clone(), ResourceKind::Data, 6_005);

    let quote = kernel
        .quote_refine_compound(&owner, 1_000)
        .expect("poor tradeoff refine quote");

    assert_eq!(quote.hardware_output, 1);
    assert_eq!(quote.hardware_shortfall_before, 0);
    assert_eq!(quote.hardware_shortfall_after, 0);
    assert_eq!(
        quote.first_goal_relevance,
        "does_not_reduce_factory_build_hardware_shortfall"
    );
    assert_eq!(quote.recommended_refine_amount, 0);
    assert_eq!(quote.refine_value_class, "poor_power_tradeoff");
}

#[test]
fn schedule_recipe_rejects_incompatible_factory_kind() {
    let mut config = WorldConfig::default();
    config.economy.factory_build_electricity_cost = 0;
    config.economy.factory_build_hardware_cost = 0;
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-factory".to_string(),
        name: "factory-site".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-builder".to_string(),
        location_id: "loc-factory".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::BuildFactory {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        location_id: "loc-factory".to_string(),
        factory_id: "factory.power.alpha".to_string(),
        factory_kind: "factory.power.radiation.mk1".to_string(),
    });
    kernel.step().expect("build power factory");

    kernel.submit_action(Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-builder".to_string(),
        },
        factory_id: "factory.power.alpha".to_string(),
        recipe_id: "recipe.assembler.control_chip".to_string(),
        batches: 1,
    });
    let event = kernel.step().expect("schedule incompatible recipe");
    match event.kind {
        WorldEventKind::ActionRejected { reason } => match reason {
            RejectReason::RuleDenied { notes } => {
                assert_eq!(notes.len(), 1);
                assert!(
                    notes[0].contains("requires factory kind factory.assembler.mk1"),
                    "unexpected note: {}",
                    notes[0]
                );
                assert!(
                    notes[0].contains("factory.power.radiation.mk1"),
                    "unexpected note: {}",
                    notes[0]
                );
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
}

fn collect_basic_action_sequence(kernel: &mut WorldKernel) -> Vec<WorldEventKind> {
    let mut kinds = Vec::new();

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-seq".to_string(),
        name: "seq".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kinds.push(kernel.step().expect("register location").kind);

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-seq".to_string(),
        location_id: "loc-seq".to_string(),
    });
    kinds.push(kernel.step().expect("register agent").kind);

    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-seq".to_string(),
        location_id: "loc-seq".to_string(),
    });
    kinds.push(kernel.step().expect("reject duplicate agent").kind);

    kinds
}

#[test]
fn kernel_rule_hooks_default_path_keeps_action_behavior() {
    let mut baseline = WorldKernel::new();
    let baseline_kinds = collect_basic_action_sequence(&mut baseline);

    let mut with_noop_hooks = WorldKernel::new();
    with_noop_hooks
        .add_pre_action_rule_hook(|action_id, _, _| KernelRuleDecision::allow(action_id));
    with_noop_hooks.add_post_action_rule_hook(|_, _, _| {});
    let hook_kinds = collect_basic_action_sequence(&mut with_noop_hooks);

    assert_eq!(baseline_kinds, hook_kinds);
}

#[test]
fn kernel_rule_hooks_run_in_registration_order() {
    let mut kernel = WorldKernel::new();
    let trace = Arc::new(Mutex::new(Vec::new()));

    let trace_pre_1 = Arc::clone(&trace);
    kernel.add_pre_action_rule_hook(move |action_id, _, _| {
        trace_pre_1.lock().expect("lock trace").push("pre-1");
        KernelRuleDecision::allow(action_id)
    });

    let trace_pre_2 = Arc::clone(&trace);
    kernel.add_pre_action_rule_hook(move |action_id, _, _| {
        trace_pre_2.lock().expect("lock trace").push("pre-2");
        KernelRuleDecision::allow(action_id)
    });

    let trace_post_1 = Arc::clone(&trace);
    kernel.add_post_action_rule_hook(move |_, _, _| {
        trace_post_1.lock().expect("lock trace").push("post-1");
    });

    let trace_post_2 = Arc::clone(&trace);
    kernel.add_post_action_rule_hook(move |_, _, _| {
        trace_post_2.lock().expect("lock trace").push("post-2");
    });

    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-hook-order".to_string(),
        name: "hook-order".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step().expect("step with hooks");

    let trace = trace.lock().expect("lock trace");
    assert_eq!(*trace, vec!["pre-1", "pre-2", "post-1", "post-2"]);
}

#[test]
fn kernel_post_action_hook_receives_emitted_event() {
    let mut kernel = WorldKernel::new();
    let captured = Arc::new(Mutex::new(None::<(ActionId, Action, WorldEvent)>));
    let captured_hook = Arc::clone(&captured);

    kernel.add_post_action_rule_hook(move |action_id, action, event| {
        *captured_hook.lock().expect("lock captured") =
            Some((action_id, action.clone(), event.clone()));
    });

    let action = Action::RegisterLocation {
        location_id: "loc-hook-post".to_string(),
        name: "hook-post".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    };
    let submitted_action_id = kernel.submit_action(action.clone());
    let emitted_event = kernel.step().expect("step with post hook");

    let captured = captured.lock().expect("lock captured");
    let (hook_action_id, hook_action, hook_event) = captured.clone().expect("captured event");
    assert_eq!(hook_action_id, submitted_action_id);
    assert_eq!(hook_action, action);
    assert_eq!(hook_event, emitted_event);
}
