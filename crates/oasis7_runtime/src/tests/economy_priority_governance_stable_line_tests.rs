use crate::WorldError;
use crate::agent_state::AgentState;
use crate::state::FactoryProductionSnapshot;
use crate::{
    AgentCell, FactoryProductionState, FactoryState, MaterialLedgerId, WorldState,
};

fn stable_line_minimal_state(factory_id: &str) -> WorldState {
    let agent_id = "stable-line-test-agent";
    let site_id = "stable-line-test-site";
    let mut state = WorldState::default();
    state.agents.insert(
        agent_id.to_string(),
        AgentCell::new(AgentState::new(agent_id, pos(0, 0)), 0),
    );
    state.factories.insert(
        factory_id.to_string(),
        FactoryState {
            factory_id: factory_id.to_string(),
            site_id: site_id.to_string(),
            builder_agent_id: agent_id.to_string(),
            spec: factory_spec(factory_id, 1, 1),
            input_ledger: MaterialLedgerId::site(site_id),
            output_ledger: MaterialLedgerId::site(site_id),
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    state
}

fn stable_identity_fixture(factory_id: &str) -> World {
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
    let spec = factory_spec(factory_id, 1, 1);
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
        &format!("proposal.policy.disable-tax.{factory_id}"),
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
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 100)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 100);
    world
}

fn complete_identity_recipe(
    world: &mut World,
    factory_id: &str,
    recipe_id: &str,
    plan: RecipeExecutionPlan,
) {
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start identity recipe");
    world.step().expect("complete identity recipe");
}

#[test]
fn stable_line_input_ledger_does_not_fallback_to_world_ledger() {
    let factory_id = "factory.identity.input";
    let recipe_id = "recipe.identity.input";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed local input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("factory after local input")
            .production
            .same_recipe_repeat_count,
        2
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 0)
        .expect("clear local input for site-bound recipe");
    world
        .set_material_balance("iron_ingot", 2)
        .expect("seed world stock that must not satisfy site-bound recipe");
    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan.clone(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("reject recipe without local input");
    assert!(!world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted { .. })
        )
    }), "direct recipe scheduling must not fall back to world stock");
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::InsufficientMaterial { material_kind, .. },
                ..
            }) if material_kind == "iron_ingot"
        )
    }));

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    let factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after rejected world fallback");
    assert_eq!(
        factory.production.same_recipe_repeat_count,
        0,
        "rejected input must reset the stable-line candidate to W=0"
    );
    assert!(
        factory
            .production
            .last_completed_canonical_snapshot
            .is_none(),
        "rejected input must clear the stable-line canonical snapshot"
    );
}

#[test]
fn stable_line_power_requirement_change_starts_fresh_candidate() {
    let factory_id = "factory.identity.power";
    let recipe_id = "recipe.identity.power";
    let mut world = stable_identity_fixture(factory_id);
    let plan_power_one = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    let plan_power_two = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        2,
        1,
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed power identity input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan_power_one.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan_power_one);

    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: plan_power_two,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start higher-power recipe");
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                power_required: 2,
                ..
            })
        )
    }));
    world.step().expect("complete higher-power recipe");

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("factory after power change")
            .production
            .same_recipe_repeat_count,
        1,
        "changing the effective power prerequisite starts a fresh candidate"
    );
}

#[test]
fn stable_line_logistics_snapshot_change_starts_fresh_candidate() {
    let factory_id = "factory.identity.logistics";
    let recipe_id = "recipe.identity.logistics";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: recipe_id.to_string(),
            bottleneck_tags: vec!["Iron_Ingot".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("insert initial logistics profile");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed logistics identity input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());

    world
        .upsert_recipe_profile(RecipeProfileV1 {
            recipe_id: recipe_id.to_string(),
            bottleneck_tags: vec!["Copper_Wire".to_string()],
            stage_gate: "bootstrap".to_string(),
            preferred_factory_tags: vec!["assembly".to_string()],
        })
        .expect("update logistics profile");
    let journal_start = world.journal().events.len();
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan);
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                bottleneck_tags,
                ..
            }) if bottleneck_tags == &["copper_wire".to_string()]
        )
    }));

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("factory after logistics change")
            .production
            .same_recipe_repeat_count,
        1,
        "changing the normalized logistics snapshot starts a fresh candidate"
    );
}

#[test]
fn stable_line_requires_canonical_snapshot_before_scale_out() {
    let mut state = stable_line_minimal_state("factory.identity.missing-snapshot");
    let factory = state
        .factories
        .get_mut("factory.identity.missing-snapshot")
        .expect("factory after identity fixture");
    factory.production.same_recipe_repeat_count = 3;
    factory.production.last_completed_recipe_id =
        Some("recipe.identity.missing-snapshot".to_string());
    factory.production.last_completed_canonical_snapshot = None;

    state
        .apply_domain_event(
            &DomainEvent::GameplayPolicyUpdated {
                operator_agent_id: "stable-line-test-agent".to_string(),
                electricity_tax_bps: 0,
                data_tax_bps: 0,
                power_trade_fee_bps: 0,
                max_open_contracts_per_agent: 16,
                blocked_agents: Vec::new(),
                forbidden_location_ids: Vec::new(),
            },
            100,
        )
        .expect("refresh stage through public domain-event reducer");

    assert_eq!(
        state.industry_progress.stage,
        IndustryStage::Bootstrap,
        "a repeat count without a canonical snapshot must not unlock scale-out"
    );
}

#[test]
fn stable_line_requires_snapshot_recipe_to_match_last_completed_recipe() {
    let mut state = stable_line_minimal_state("factory.identity.recipe-mismatch");
    let factory = state
        .factories
        .get_mut("factory.identity.recipe-mismatch")
        .expect("factory after identity fixture");
    factory.production.same_recipe_repeat_count = 3;
    factory.production.last_completed_recipe_id =
        Some("recipe.identity.current".to_string());
    factory.production.last_completed_canonical_snapshot = Some(
        FactoryProductionSnapshot {
            recipe_id: "recipe.identity.stale".to_string(),
            ..FactoryProductionSnapshot::default()
        },
    );

    state
        .apply_domain_event(
            &DomainEvent::GameplayPolicyUpdated {
                operator_agent_id: "stable-line-test-agent".to_string(),
                electricity_tax_bps: 0,
                data_tax_bps: 0,
                power_trade_fee_bps: 0,
                max_open_contracts_per_agent: 16,
                blocked_agents: Vec::new(),
                forbidden_location_ids: Vec::new(),
            },
            100,
        )
        .expect("refresh stage through public domain-event reducer");

    assert_eq!(
        state.industry_progress.stage,
        IndustryStage::Bootstrap,
        "a snapshot for another recipe must not unlock scale-out"
    );
}

#[test]
fn planned_pause_resets_idle_candidate_without_erasing_history() {
    let factory_id = "factory.identity.pause";
    let recipe_id = "recipe.identity.pause";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed pause test input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());

    let completed_jobs_before = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory before planned pause")
        .production
        .completed_jobs;
    let output_before = world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "gear");
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );

    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        reason: "planned maintenance window".to_string(),
    });
    world.step().expect("pause idle factory");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(
            DomainEvent::FactoryProductionPaused { .. }
        ))
    ));

    let paused_factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after planned pause");
    assert_eq!(paused_factory.production.same_recipe_repeat_count, 0);
    assert!(
        paused_factory
            .production
            .last_completed_canonical_snapshot
            .is_none()
    );
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
    assert_eq!(
        paused_factory.production.completed_jobs,
        completed_jobs_before
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "gear"),
        output_before,
        "planned pause preserves committed outputs"
    );

    complete_identity_recipe(&mut world, factory_id, recipe_id, plan);
    let resumed_factory = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after planned pause resume");
    assert_eq!(resumed_factory.production.same_recipe_repeat_count, 1);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
}

#[test]
fn planned_pause_while_active_job_is_rejected_without_clearing_candidate_or_history() {
    let factory_id = "factory.identity.pause.busy";
    let recipe_id = "recipe.identity.pause.busy";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed pause busy test input");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());

    let before = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory before active pause")
        .production
        .clone();
    let output_before = world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "gear");

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
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start active recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);

    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        reason: "planned maintenance window".to_string(),
    });
    world.step().expect("reject pause while active");
    match &world
        .journal()
        .events
        .last()
        .expect("busy pause rejection")
        .body
    {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason:
                RejectReason::FactoryBusy {
                    factory_id: rejected,
                    ..
                },
            ..
        }) => assert_eq!(rejected, factory_id),
        other => panic!("expected FactoryBusy rejection, got {other:?}"),
    }

    let after = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory after rejected active pause")
        .production
        .clone();
    assert_eq!(
        after.same_recipe_repeat_count,
        before.same_recipe_repeat_count
    );
    assert_eq!(
        after.last_completed_canonical_snapshot,
        before.last_completed_canonical_snapshot
    );
    assert_eq!(after.completed_jobs, before.completed_jobs);
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "gear"),
        output_before
    );
}

#[test]
fn planned_pause_action_and_event_json_roundtrip() {
    let action = Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.identity.serde".to_string(),
        reason: "planned maintenance window".to_string(),
    };
    let action_json = serde_json::to_value(&action).expect("serialize planned pause action");
    let decoded_action: Action =
        serde_json::from_value(action_json).expect("deserialize planned pause action");
    assert_eq!(decoded_action, action);

    let event = DomainEvent::FactoryProductionPaused {
        action_id: 42,
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.identity.serde".to_string(),
        reason: "planned maintenance window".to_string(),
    };
    let event_json = serde_json::to_value(&event).expect("serialize planned pause event");
    let decoded_event: DomainEvent =
        serde_json::from_value(event_json).expect("deserialize planned pause event");
    assert_eq!(decoded_event, event);
}

#[test]
fn planned_pause_on_blocked_factory_is_rejected_without_clearing_blocker_or_history() {
    let factory_id = "factory.identity.pause.blocked";
    let recipe_id = "recipe.identity.pause.blocked";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed blocked pause history");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 0)
        .expect("remove blocked pause input");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("block factory production");
    let before = world
        .state()
        .factories
        .get(factory_id)
        .expect("blocked factory before pause")
        .production
        .clone();
    assert_eq!(
        before.status,
        crate::FactoryProductionStatus::Blocked
    );
    assert!(before.current_blocker_kind.is_some());
    assert!(before.current_blocker_detail.is_some());

    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        reason: "planned maintenance window".to_string(),
    });
    world.step().expect("reject pause on blocked factory");
    match &world
        .journal()
        .events
        .last()
        .expect("blocked pause rejection")
        .body
    {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { .. },
            ..
        }) => {}
        other => panic!("expected RuleDenied rejection, got {other:?}"),
    }

    let after = world
        .state()
        .factories
        .get(factory_id)
        .expect("blocked factory after rejected pause")
        .production
        .clone();
    assert_eq!(after, before);
}

#[test]
fn planned_pause_on_paused_factory_is_rejected_without_clearing_candidate_or_history() {
    let factory_id = "factory.identity.pause.paused";
    let recipe_id = "recipe.identity.pause.paused";
    let mut world = stable_identity_fixture(factory_id);
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("iron_ingot", 2)],
        vec![MaterialStack::new("gear", 1)],
        Vec::new(),
        1,
        1,
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed paused pause history");
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan.clone());
    complete_identity_recipe(&mut world, factory_id, recipe_id, plan);
    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        reason: "planned maintenance window".to_string(),
    });
    world.step().expect("pause factory");
    let before = world
        .state()
        .factories
        .get(factory_id)
        .expect("paused factory before repeated pause")
        .production
        .clone();
    assert_eq!(
        before.status,
        crate::FactoryProductionStatus::Paused
    );
    assert_eq!(before.completed_jobs, 2);

    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        reason: "repeated maintenance window".to_string(),
    });
    world.step().expect("reject pause on paused factory");
    match &world
        .journal()
        .events
        .last()
        .expect("paused pause rejection")
        .body
    {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { .. },
            ..
        }) => {}
        other => panic!("expected RuleDenied rejection, got {other:?}"),
    }

    let after = world
        .state()
        .factories
        .get(factory_id)
        .expect("paused factory after rejected pause")
        .production
        .clone();
    assert_eq!(after, before);
}

#[test]
fn zero_output_recipe_is_rejected_without_advancing_stable_line() {
    let factory_id = "factory.identity.zero-output";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.zero-output".to_string(),
        plan: RecipeExecutionPlan::accepted(1, Vec::new(), Vec::new(), Vec::new(), 1, 1),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("reject zero-output recipe");

    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { .. },
            ..
        }))
    ));
    assert!(world.state().pending_recipe_jobs.is_empty());
    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 0);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Bootstrap
    );
}

#[test]
fn duplicate_recipe_completion_replay_does_not_duplicate_output_or_progress() {
    let factory_id = "factory.identity.duplicate-completion";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.duplicate-completion".to_string(),
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
    let snapshot = world.snapshot();
    world.step().expect("complete recipe");

    let mut journal = world.journal().clone();
    let mut duplicate = journal
        .events
        .iter()
        .rev()
        .find(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::RecipeCompleted { .. })
            )
        })
        .expect("recipe completion event")
        .clone();
    duplicate.id = journal.events.last().expect("journal event").id + 1;
    journal.append(duplicate);

    let restored = World::from_snapshot(snapshot, journal).expect("replay duplicate completion");
    assert_eq!(
        restored.ledger_material_balance(&MaterialLedgerId::site("site-1"), "gear"),
        1
    );
    assert_eq!(restored.state().industry_progress.completed_recipe_jobs, 1);
    let production = &restored
        .state()
        .factories
        .get(factory_id)
        .expect("factory")
        .production;
    assert_eq!(production.completed_jobs, 1);
    assert_eq!(production.same_recipe_repeat_count, 1);
}

#[test]
fn industrial_integrity_tampered_recipe_completion_fails_before_mutation() {
    let factory_id = "factory.identity.tampered-completion";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.tampered-completion".to_string(),
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
    world.step().expect("start recipe for tampered completion");
    let pending = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .expect("pending recipe job")
        .clone();
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before tampered completion");
    let event = DomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id: pending.job_id,
        requester_agent_id: pending.requester_agent_id,
        factory_id: pending.factory_id,
        recipe_id: "recipe.identity.attacker-controlled".to_string(),
        accepted_batches: pending.accepted_batches,
        produce: pending.produce,
        byproducts: pending.byproducts,
        output_ledger: pending.output_ledger,
        bottleneck_tags: pending.bottleneck_tags,
        logistics_route_ids: pending.logistics_route_ids,
        logistics_path_ids: pending.logistics_path_ids,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(result.is_err(), "tampered recipe completion must be rejected");
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after tampered completion") == before,
        "tampered recipe completion must not consume pending job or credit output"
    );
}

#[test]
fn industrial_integrity_duplicate_recipe_started_fails_before_second_sink() {
    let factory_id = "factory.identity.duplicate-start";
    let recipe_id = "recipe.identity.duplicate-start";
    let mut world = stable_identity_fixture(factory_id);
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed duplicate-start input");
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
        plan,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    let started = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(domain @ DomainEvent::RecipeStarted { .. }) => {
                Some(domain.clone())
            }
            _ => None,
        })
        .expect("recipe-start event");

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before duplicate start");
    let result = replay.apply_domain_event(&started, replay.time);

    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "duplicate recipe start must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after duplicate start"),
        before,
        "duplicate recipe start must not sink material/power or overwrite pending state"
    );
}

#[test]
fn legacy_recipe_started_without_power_owner_fails_closed_before_mutation() {
    let factory_id = "factory.identity.legacy-power-owner";
    let recipe_id = "recipe.identity.legacy-power-owner";
    let mut world = stable_identity_fixture(factory_id);
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed legacy payer input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 100)
        .expect("seed legacy payer electricity");
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
            2,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });

    // Build a valid current-schema RecipeStarted event, but apply it to the
    // pre-event state after removing the optional payer field as a legacy
    // snapshot/replay would have done.
    let mut replay = world.state().clone();
    world.step().expect("produce current recipe-start event");
    let started = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(domain @ DomainEvent::RecipeStarted { .. }) => {
                Some(domain.clone())
            }
            _ => None,
        })
        .expect("current recipe-start event");
    let mut legacy_json = serde_json::to_value(&started).expect("serialize recipe-start event");
    legacy_json
        .get_mut("data")
        .and_then(serde_json::Value::as_object_mut)
        .expect("recipe-start event data object")
        .remove("power_owner_agent_id");
    let legacy_started: DomainEvent =
        serde_json::from_value(legacy_json).expect("legacy payer field defaults to None");
    assert!(matches!(
        &legacy_started,
        DomainEvent::RecipeStarted {
            power_owner_agent_id: None,
            ..
        }
    ));

    let before = serde_json::to_vec(&replay).expect("serialize state before legacy replay");
    let material_before = replay
        .material_ledgers
        .clone();
    let agent_power_before = replay
        .agents
        .get("builder-a")
        .expect("builder agent")
        .state
        .resources
        .get(ResourceKind::Electricity);
    let global_power_before = replay
        .resources
        .get(&ResourceKind::Electricity)
        .copied()
        .unwrap_or_default();

    let result = replay.apply_domain_event(&legacy_started, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "legacy RecipeStarted without payer must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after legacy replay"),
        before,
        "legacy replay must not mutate pending jobs or any state"
    );
    assert_eq!(replay.material_ledgers, material_before);
    assert_eq!(
        replay
            .agents
            .get("builder-a")
            .expect("builder agent after replay")
            .state
            .resources
            .get(ResourceKind::Electricity),
        agent_power_before
    );
    assert_eq!(
        replay
            .resources
            .get(&ResourceKind::Electricity)
            .copied()
            .unwrap_or_default(),
        global_power_before
    );
}

#[test]
fn industrial_integrity_unknown_recipe_completion_fails_before_mutation() {
    let world = stable_identity_fixture("factory.identity.unknown-completion");
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before unknown completion");
    let event = DomainEvent::RecipeCompleted {
        committed_recipe_origin: None,
        job_id: 9_999,
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.identity.unknown-completion".to_string(),
        recipe_id: "recipe.identity.unknown-completion".to_string(),
        accepted_batches: 1,
        produce: vec![MaterialStack::new("gear", 1)],
        byproducts: Vec::new(),
        output_ledger: MaterialLedgerId::site("site-1"),
        bottleneck_tags: Vec::new(),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    };

    let result = replay.apply_domain_event(&event, replay.time);

    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "unknown recipe completion must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after unknown completion"),
        before,
        "unknown recipe completion must not credit output or progress"
    );
}

#[test]
fn non_owner_cannot_schedule_recipe_before_material_or_power_sink() {
    let factory_id = "factory.identity.schedule-owner";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-b".to_string(),
        pos: pos(1, 0),
    });
    world.step().expect("register non-owner");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 4)
        .expect("seed owner-guard input");
    let input_before =
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot");
    let power_before = world.resource_balance(ResourceKind::Electricity);
    let journal_start = world.journal().events.len();

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-b".to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: "recipe.identity.schedule-owner".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 2)],
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("reject non-owner schedule");

    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { .. },
                ..
            })
        )
    }));
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        input_before,
        "non-owner schedule must not sink input"
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        power_before,
        "non-owner schedule must not sink power"
    );
}

#[test]
fn non_owner_cannot_pause_idle_factory_or_clear_stable_line_candidate() {
    let factory_id = "factory.identity.pause.owner";
    let recipe_id = "recipe.identity.pause.owner";
    let mut world = stable_identity_fixture(factory_id);
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-b".to_string(),
        pos: pos(1, 0),
    });
    world.step().expect("register non-owner");
    complete_identity_recipe(
        &mut world,
        factory_id,
        recipe_id,
        RecipeExecutionPlan::accepted(
            1,
            Vec::new(),
            vec![MaterialStack::new("gear", 1)],
            Vec::new(),
            1,
            1,
        ),
    );
    let before = world
        .state()
        .factories
        .get(factory_id)
        .expect("factory before unauthorized pause")
        .production
        .clone();

    world.submit_action(Action::PauseFactoryProduction {
        requester_agent_id: "builder-b".to_string(),
        factory_id: factory_id.to_string(),
        reason: "sabotage".to_string(),
    });
    world.step().expect("reject unauthorized pause");

    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { .. },
            ..
        }))
    ));
    assert_eq!(
        world
            .state()
            .factories
            .get(factory_id)
            .expect("factory after unauthorized pause")
            .production,
        before
    );
}
