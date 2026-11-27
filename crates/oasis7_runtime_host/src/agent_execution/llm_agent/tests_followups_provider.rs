use oasis7_runtime::{
    DomainEvent as RuntimeDomainEvent, MaterialLedgerId, MaterialStack,
    WorldEvent as RuntimeWorldEvent, WorldEventBody as RuntimeWorldEventBody,
};

#[path = "tests_recipe_replay_window.rs"]
mod tests_recipe_replay_window;

fn establish_completed_recipe_for_test<C: LlmCompletionClient>(
    behavior: &mut LlmAgentBehavior<C>,
    recipe_id: &str,
) {
    behavior.recipe_coverage.mark_completed(recipe_id);
}

fn runtime_recipe_completed_event_with_identity(
    requester_agent_id: &str,
    recipe_id: &str,
    event_id: u64,
    job_id: u64,
) -> WorldEvent {
    let runtime_event = RuntimeWorldEvent {
        id: event_id,
        time: event_id.saturating_add(1),
        caused_by: None,
        body: RuntimeWorldEventBody::Domain(RuntimeDomainEvent::RecipeCompleted {
            committed_recipe_origin: None,
            job_id,
            requester_agent_id: requester_agent_id.to_string(),
            factory_id: "factory.alpha".to_string(),
            recipe_id: recipe_id.to_string(),
            accepted_batches: 1,
            produce: vec![MaterialStack::new("control_chip", 1)],
            byproducts: Vec::new(),
            output_ledger: MaterialLedgerId::world(),
            bottleneck_tags: Vec::new(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        }),
    };

    WorldEvent {
        id: runtime_event.id,
        time: runtime_event.time,
        kind: WorldEventKind::RuntimeEvent {
            kind: "runtime.economy.recipe_completed".to_string(),
            domain_kind: Some(format!("recipe={recipe_id}")),
        },
        runtime_event: Some(runtime_event),
    }
}

fn runtime_recipe_completed_event(requester_agent_id: &str, recipe_id: &str) -> WorldEvent {
    runtime_recipe_completed_event_with_identity(requester_agent_id, recipe_id, 12_200, 12_199)
}

#[test]
fn llm_agent_segments_move_agent_when_target_distance_exceeds_limit() {
    let client = MockClient {
        output: Some(r#"{"decision":"move_agent","to":"loc-factory"}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.visible_locations = vec![
        ObservedLocation {
            location_id: "loc-home".to_string(),
            name: "home".to_string(),
            pos: GeoPos {
                x_cm: 0,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 0,
        },
        ObservedLocation {
            location_id: "loc-relay".to_string(),
            name: "relay".to_string(),
            pos: GeoPos {
                x_cm: 900_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 900_000,
        },
        ObservedLocation {
            location_id: "loc-factory".to_string(),
            name: "factory".to_string(),
            pos: GeoPos {
                x_cm: 2_500_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 2_500_000,
        },
    ];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-relay".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("segmented by distance guardrail")));
}

#[test]
fn llm_agent_uses_relay_fallback_after_move_distance_exceeded_history() {
    let client = SequenceMockClient::new(vec![
        r#"{"decision":"move_agent","to":"loc-far"}"#.to_string(),
        r#"{"decision":"move_agent","to":"loc-far"}"#.to_string(),
    ]);
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.visible_locations = vec![
        ObservedLocation {
            location_id: "loc-home".to_string(),
            name: "home".to_string(),
            pos: GeoPos {
                x_cm: 0,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 0,
        },
        ObservedLocation {
            location_id: "loc-relay".to_string(),
            name: "relay".to_string(),
            pos: GeoPos {
                x_cm: 900_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 900_000,
        },
    ];

    let first = behavior.decide(&observation);
    assert_eq!(
        first,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-far".to_string(),
        })
    );
    let _ = behavior.take_decision_trace();

    behavior.on_action_result(&ActionResult {
        action: Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-far".to_string(),
        },
        action_id: 611,
        success: false,
        event: WorldEvent {
            id: 711,
            time: 150,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::MoveDistanceExceeded {
                    distance_cm: 1_800_000,
                    max_distance_cm: 1_000_000,
                },
            },
            runtime_event: None,
        },
    });

    observation.time = 151;
    let second = behavior.decide(&observation);
    assert_eq!(
        second,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-relay".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("fallback relay after move_distance_exceeded")));
}

#[test]
fn llm_agent_hard_switches_schedule_recipe_to_next_uncovered_recipe() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.alpha",
        "factory.smelter.mk1",
        "loc-home",
    );

    establish_completed_recipe_for_test(
        &mut behavior,
        "recipe.assembler.control_chip",
    );

    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.smelter.iron_ingot".to_string(),
            batches: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(!trace.llm_step_trace.is_empty());
}

#[test]
fn llm_agent_counts_pure_simulator_recipe_event_as_completed_coverage() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let event = WorldEvent {
        id: 12_001,
        time: 12_002,
        kind: WorldEventKind::RecipeScheduled {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
            electricity_cost: 6,
            hardware_cost: 2,
            data_output: 1,
            finished_product_id: "product.component.control_chip".to_string(),
            finished_product_units: 1,
        },
        runtime_event: None,
    };

    behavior.on_event(&event);

    assert!(
        behavior
            .recipe_coverage
            .is_completed("recipe.assembler.control_chip"),
        "the pure simulator event carries an already-applied atomic recipe transformation"
    );
}

#[test]
fn llm_agent_counts_pure_simulator_recipe_result_as_completed_coverage() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let event = WorldEvent {
        id: 12_101,
        time: 12_102,
        kind: WorldEventKind::RecipeScheduled {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
            electricity_cost: 6,
            hardware_cost: 2,
            data_output: 1,
            finished_product_id: "product.component.control_chip".to_string(),
            finished_product_units: 1,
        },
        runtime_event: None,
    };
    let action = Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        factory_id: "factory.alpha".to_string(),
        recipe_id: "recipe.assembler.control_chip".to_string(),
        batches: 1,
    };

    let result = ActionResult {
        action,
        action_id: 12_100,
        success: true,
        event,
    };
    behavior.on_action_result(&result);

    assert!(
        behavior
            .recipe_coverage
            .is_completed("recipe.assembler.control_chip"),
        "LlmAgentBehavior must count the WorldKernel-shaped atomic simulator result as authoritative completion feedback"
    );
}

#[test]
fn llm_agent_keeps_recipe_coverage_idempotent_across_completion_feedback() {
    let recipe_id = "recipe.assembler.control_chip";
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let scheduled_event = WorldEvent {
        id: 12_101,
        time: 12_102,
        kind: WorldEventKind::RecipeScheduled {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: recipe_id.to_string(),
            batches: 1,
            electricity_cost: 6,
            hardware_cost: 2,
            data_output: 1,
            finished_product_id: "product.component.control_chip".to_string(),
            finished_product_units: 1,
        },
        runtime_event: None,
    };
    let action = Action::ScheduleRecipe {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        factory_id: "factory.alpha".to_string(),
        recipe_id: recipe_id.to_string(),
        batches: 1,
    };

    behavior.on_action_result(&ActionResult {
        action,
        action_id: 12_100,
        success: true,
        event: scheduled_event,
    });
    assert!(behavior.recipe_coverage.is_completed(recipe_id));

    // A completion for another requester is not feedback for this Agent.
    behavior.on_event(&runtime_recipe_completed_event("agent-2", recipe_id));
    assert!(behavior.recipe_coverage.is_completed(recipe_id));

    let completion = runtime_recipe_completed_event("agent-1", recipe_id);
    behavior.on_event(&completion);
    behavior.on_event(&completion);

    assert!(behavior.recipe_coverage.is_completed(recipe_id));
    assert_eq!(
        behavior.recipe_coverage.completed.len(),
        1,
        "replaying one authoritative receipt must not duplicate coverage state"
    );
}

#[test]
fn llm_agent_keeps_distinct_recipe_completion_jobs_in_feedback_memory() {
    let recipe_id = "recipe.assembler.control_chip";
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());

    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 12_220, 12_219,
    ));
    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 12_222, 12_221,
    ));
    // A replayed receipt may be wrapped in a fresh journal event, but its
    // durable job identity must still suppress duplicate feedback.
    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 12_224, 12_221,
    ));

    assert!(behavior.recipe_coverage.is_completed(recipe_id));
    assert_eq!(
        behavior.memory.short_term.len(),
        2,
        "two distinct completion jobs must remain observable even when they use one recipe"
    );
}

#[test]
fn llm_agent_bounds_recipe_completion_replay_window_and_preserves_recent_replays() {
    const REPLAY_WINDOW: usize = 64;
    let recipe_id = "recipe.assembler.control_chip";
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());

    for offset in 0..=REPLAY_WINDOW {
        let job_id = 13_000 + offset as u64;
        behavior.on_event(&runtime_recipe_completed_event_with_identity(
            "agent-1",
            recipe_id,
            job_id + 1_000,
            job_id,
        ));
    }

    assert_eq!(
        behavior.recipe_coverage.completion_receipt_ids.len(),
        REPLAY_WINDOW,
        "completion receipt dedupe must stay bounded after long-running production"
    );
    let memory_before_replays = behavior.memory.short_term.len();

    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 14_100, 13_064,
    ));
    assert_eq!(
        behavior.memory.short_term.len(),
        memory_before_replays,
        "recent receipt replay must not add feedback memory"
    );

    behavior.on_event(&runtime_recipe_completed_event_with_identity(
        "agent-1", recipe_id, 14_101, 13_000,
    ));
    assert_eq!(
        behavior.memory.short_term.len(),
        memory_before_replays + 1,
        "a receipt outside the bounded window may be observed again"
    );
}

#[test]
fn llm_agent_keeps_hard_switch_within_current_factory_kind() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.alpha",
        "factory.assembler.mk1",
        "loc-home",
    );

    establish_completed_recipe_for_test(
        &mut behavior,
        "recipe.assembler.control_chip",
    );

    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.gear".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_hands_off_coverage_to_known_assembler_after_last_smelter_recipe() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.smelter.1","recipe_id":"recipe.smelter.polymer_resin","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.smelter.1",
        "factory.smelter.mk1",
        "loc-home",
    );
    seed_known_factory(
        &mut behavior,
        "factory.assembler.1",
        "factory.assembler.mk1",
        "loc-home",
    );

    for recipe_id in [
        "recipe.smelter.iron_ingot",
        "recipe.smelter.copper_wire",
        "recipe.smelter.polymer_resin",
        "recipe.smelter.alloy_plate",
    ] {
        establish_completed_recipe_for_test(&mut behavior, recipe_id);
    }

    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.assembler.1".to_string(),
            recipe_id: "recipe.assembler.gear".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_moves_before_cross_factory_coverage_handoff() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.smelter.1","recipe_id":"recipe.smelter.polymer_resin","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.smelter.1",
        "factory.smelter.mk1",
        "loc-smelter",
    );
    seed_known_factory(
        &mut behavior,
        "factory.assembler.1",
        "factory.assembler.mk1",
        "loc-assembler",
    );

    for recipe_id in [
        "recipe.smelter.iron_ingot",
        "recipe.smelter.copper_wire",
        "recipe.smelter.polymer_resin",
    ] {
        establish_completed_recipe_for_test(&mut behavior, recipe_id);
    }

    let mut observation = make_observation();
    observation.visible_locations = vec![
        ObservedLocation {
            location_id: "loc-smelter".to_string(),
            name: "smelter".to_string(),
            pos: GeoPos {
                x_cm: 0,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 0,
        },
        ObservedLocation {
            location_id: "loc-assembler".to_string(),
            name: "assembler".to_string(),
            pos: GeoPos {
                x_cm: 500,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 500,
        },
    ];
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.smelter.1".to_string(),
            recipe_id: "recipe.smelter.alloy_plate".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_hard_switches_with_factory_kind_inferred_from_recipe_hint() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    behavior.remember_factory_location_hint("factory.alpha", "loc-home", None);

    establish_completed_recipe_for_test(
        &mut behavior,
        "recipe.assembler.control_chip",
    );

    let mut observation = make_observation();
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.gear".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_builds_missing_factory_before_cross_stage_coverage_handoff() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.smelter.1","recipe_id":"recipe.smelter.polymer_resin","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.smelter.1",
        "factory.smelter.mk1",
        "loc-home",
    );

    for recipe_id in [
        "recipe.smelter.iron_ingot",
        "recipe.smelter.copper_wire",
        "recipe.smelter.polymer_resin",
    ] {
        establish_completed_recipe_for_test(&mut behavior, recipe_id);
    }

    let mut observation = make_observation();
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.smelter.1".to_string(),
            recipe_id: "recipe.smelter.alloy_plate".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_preserves_hard_switch_on_noncanonical_same_kind_factory() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.assembler.legacy","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.assembler.legacy",
        "factory.assembler.mk1",
        "loc-home",
    );
    seed_known_factory(
        &mut behavior,
        "factory.assembler.current",
        "factory.assembler.mk1",
        "loc-home",
    );

    establish_completed_recipe_for_test(
        &mut behavior,
        "recipe.assembler.control_chip",
    );

    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 24)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 100)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.assembler.legacy".to_string(),
            recipe_id: "recipe.assembler.gear".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_rewrites_wait_ticks_to_sustained_schedule_after_full_recipe_coverage() {
    let client = MockClient {
        output: Some(r#"{"decision":"wait_ticks","ticks":3}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    behavior.on_action_result(&ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 900,
        success: true,
        event: WorldEvent {
            id: 901,
            time: 180,
            kind: WorldEventKind::FactoryBuilt {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-home".to_string(),
                factory_id: "factory.alpha".to_string(),
                factory_kind: "factory.assembler.mk1".to_string(),
                electricity_cost: 10,
                hardware_cost: 5,
            },
            runtime_event: None,
        },
    });

    let coverage_events = [
        (
            "recipe.smelter.iron_ingot",
            6_i64,
            2_i64,
            "product.material.iron_ingot",
        ),
        (
            "recipe.smelter.copper_wire",
            6_i64,
            2_i64,
            "product.material.copper_wire",
        ),
        (
            "recipe.smelter.polymer_resin",
            6_i64,
            2_i64,
            "product.material.polymer_resin",
        ),
        (
            "recipe.smelter.alloy_plate",
            9_i64,
            4_i64,
            "product.material.alloy_plate",
        ),
        (
            "recipe.assembler.gear",
            4_i64,
            2_i64,
            "product.component.gear",
        ),
        (
            "recipe.assembler.control_chip",
            6_i64,
            4_i64,
            "product.component.control_chip",
        ),
        (
            "recipe.assembler.motor_mk1",
            7_i64,
            4_i64,
            "product.component.motor_mk1",
        ),
        (
            "recipe.assembler.logistics_drone",
            24_i64,
            8_i64,
            "product.component.logistics_drone",
        ),
        (
            "recipe.assembler.sensor_pack",
            8_i64,
            4_i64,
            "product.component.sensor_pack",
        ),
        (
            "recipe.assembler.module_rack",
            10_i64,
            6_i64,
            "product.finished.module_rack",
        ),
        (
            "recipe.assembler.factory_core",
            14_i64,
            8_i64,
            "product.infrastructure.factory_core",
        ),
    ];

    for (recipe_id, _, _, _) in coverage_events {
        establish_completed_recipe_for_test(&mut behavior, recipe_id);
    }

    let mut observation = make_observation();
    observation.time = 190;
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];
    observation
        .self_resources
        .add(ResourceKind::Data, 16)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 90)
        .expect("add test electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.gear".to_string(),
            batches: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("wait_ticks(3) rewritten to sustained production")));
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("decision_rewrite={")));
}

#[test]
fn llm_agent_rewrites_wait_to_recovery_action_after_full_recipe_coverage() {
    let client = MockClient {
        output: Some(r#"{"decision":"wait"}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    behavior.on_action_result(&ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 930,
        success: true,
        event: WorldEvent {
            id: 931,
            time: 200,
            kind: WorldEventKind::FactoryBuilt {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-home".to_string(),
                factory_id: "factory.alpha".to_string(),
                factory_kind: "factory.assembler.mk1".to_string(),
                electricity_cost: 10,
                hardware_cost: 5,
            },
            runtime_event: None,
        },
    });

    let covered_recipe_ids = [
        "recipe.smelter.iron_ingot",
        "recipe.smelter.copper_wire",
        "recipe.smelter.polymer_resin",
        "recipe.smelter.alloy_plate",
        "recipe.assembler.gear",
        "recipe.assembler.control_chip",
        "recipe.assembler.motor_mk1",
        "recipe.assembler.logistics_drone",
        "recipe.assembler.sensor_pack",
        "recipe.assembler.module_rack",
        "recipe.assembler.factory_core",
    ];
    for recipe_id in covered_recipe_ids {
        establish_completed_recipe_for_test(&mut behavior, recipe_id);
    }

    let mut observation = make_observation();
    observation.time = 210;
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];
    observation
        .self_resources
        .add(ResourceKind::Data, 4)
        .expect("add hardware");
    observation
        .self_resources
        .remove(ResourceKind::Electricity, 28)
        .expect("drain electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("wait rewritten to sustained production")));
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("decision_rewrite={")));
}

#[test]
fn llm_agent_reroutes_schedule_recipe_when_hardware_cannot_cover_one_batch() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.logistics_drone","batches":5}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.alpha",
        "factory.assembler.mk1",
        "loc-home",
    );
    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 7)
        .expect("add test hardware");
    observation
        .self_resources
        .add(ResourceKind::Data, 1_000)
        .expect("add test compound");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.logistics_drone".to_string(),
            batches: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("batches clamped")));
}

#[test]
fn llm_agent_reroutes_schedule_recipe_to_mine_when_compound_missing_and_caps_mass() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.logistics_drone","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    seed_known_factory(
        &mut behavior,
        "factory.alpha",
        "factory.assembler.mk1",
        "loc-home",
    );
    let mut observation = make_observation();
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.logistics_drone".to_string(),
            batches: 1,
        })
    );
}

#[test]
fn llm_agent_clamps_mine_compound_mass_by_known_location_availability() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":4000}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    behavior.on_action_result(&ActionResult {
        action: Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 4_000,
        },
        action_id: 612,
        success: false,
        event: WorldEvent {
            id: 712,
            time: 160,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Location {
                        location_id: "loc-home".to_string(),
                    },
                    kind: ResourceKind::Data,
                    requested: 4_000,
                    available: 1_000,
                },
            },
            runtime_event: None,
        },
    });

    let mut observation = make_observation();
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];
    observation.time = 161;

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 1_000,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("clamped by known_location_compound_available")));
}
