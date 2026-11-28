#[test]
fn llm_agent_auto_reentry_can_be_disabled() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"harvest_radiation","max_amount":9}"#.to_string(),
            r#"{"decision":"harvest_radiation","max_amount":9}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );

    let mut config = base_config();
    config.execute_until_auto_reenter_ticks = 0;
    config.force_replan_after_same_action = 6;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let mut observation = make_observation();
    observation.time = 29;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 9, .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 9,
        },
        action_id: 303,
        success: true,
        event: WorldEvent {
            id: 403,
            time: 29,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 9,
                available: 90,
            },
            runtime_event: None,
        },
    });

    observation.time = 30;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 9, .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 9,
        },
        action_id: 304,
        success: true,
        event: WorldEvent {
            id: 404,
            time: 30,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 8,
                available: 82,
            },
            runtime_event: None,
        },
    });

    observation.time = 31;
    let third = behavior.decide(&observation);
    assert!(matches!(
        third,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn llm_agent_execute_until_stops_on_insufficient_electricity_reject_reason() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":6},"until":{"event":"insufficient_electricity"},"max_ticks":4}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 30;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 6, .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 6,
        },
        action_id: 101,
        success: false,
        event: WorldEvent {
            id: 201,
            time: 30,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    kind: ResourceKind::Electricity,
                    requested: 8,
                    available: 1,
                },
            },
            runtime_event: None,
        },
    });

    observation.time = 31;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_execute_until_stops_on_action_rejected_for_mine_compound() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000},"until":{"event":"action_rejected"},"max_ticks":4}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-alt"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 32;
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
            location_id: "loc-alt".to_string(),
            name: "alt".to_string(),
            pos: GeoPos {
                x_cm: 600_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 600_000,
        },
    ];
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::MineCompound { .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 3_000,
        },
        action_id: 132,
        success: false,
        event: WorldEvent {
            id: 232,
            time: 32,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Location {
                        location_id: "loc-home".to_string(),
                    },
                    kind: ResourceKind::Data,
                    requested: 3_000,
                    available: 0,
                },
            },
            runtime_event: None,
        },
    });

    observation.time = 33;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_rebuilds_execute_until_until_when_mine_is_guardrail_rewritten_to_move() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000},"until":{"event":"action_rejected"},"max_ticks":6}"#.to_string(),
            r#"{"decision":"wait"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    behavior.on_action_result(&ActionResult {
        action: Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 3_000,
        },
        action_id: 333,
        success: false,
        event: WorldEvent {
            id: 433,
            time: 500,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Location {
                        location_id: "loc-home".to_string(),
                    },
                    kind: ResourceKind::Data,
                    requested: 3_000,
                    available: 0,
                },
            },
            runtime_event: None,
        },
    });

    let mut observation = make_observation();
    observation.time = 501;
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
            location_id: "loc-alt".to_string(),
            name: "alt".to_string(),
            pos: GeoPos {
                x_cm: 600_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 600_000,
        },
    ];

    let first = behavior.decide(&observation);
    assert_eq!(
        first,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-alt".to_string(),
        })
    );
    behavior.take_decision_trace().expect("first trace");

    behavior.on_action_result(&ActionResult {
        action: Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-alt".to_string(),
        },
        action_id: 334,
        success: true,
        event: WorldEvent {
            id: 434,
            time: 501,
            kind: WorldEventKind::AgentMoved {
                agent_id: "agent-1".to_string(),
                from: "loc-home".to_string(),
                to: "loc-alt".to_string(),
                distance_cm: 600_000,
                electricity_cost: 0,
            },
            runtime_event: None,
        },
    });

    observation.time = 502;
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
            distance_cm: 600_000,
        },
        ObservedLocation {
            location_id: "loc-alt".to_string(),
            name: "alt".to_string(),
            pos: GeoPos {
                x_cm: 600_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 0,
        },
    ];

    let second = behavior.decide(&observation);
    assert_eq!(second, AgentDecision::Wait);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_execute_until_stops_on_thermal_overload_reject_reason() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":7},"until":{"event":"thermal_overload"},"max_ticks":4}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 40;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 7, .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 7,
        },
        action_id: 102,
        success: false,
        event: WorldEvent {
            id: 202,
            time: 40,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::ThermalOverload {
                    heat: 130,
                    capacity: 100,
                },
            },
            runtime_event: None,
        },
    });

    observation.time = 41;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_execute_until_stops_on_harvest_yield_threshold() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":9},"until":{"event":"harvest_yield_below","value_lte":2},"max_ticks":4}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 50;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 9, .. })
    ));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 9,
        },
        action_id: 103,
        success: true,
        event: WorldEvent {
            id: 203,
            time: 50,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 2,
                available: 8,
            },
            runtime_event: None,
        },
    });

    observation.time = 51;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_clamps_harvest_max_amount_to_configured_cap() {
    let client = MockClient {
        output: Some(r#"{"decision":"harvest_radiation","max_amount":1000000}"#.to_string()),
        err: None,
    };
    let mut config = base_config();
    config.harvest_max_amount_cap = 42;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 42,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("max_amount clamped")));
}

#[test]
fn llm_agent_prechecks_schedule_recipe_location_and_reroutes_to_move_agent() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    behavior.on_action_result(&ActionResult {
        action: Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
        },
        action_id: 410,
        success: false,
        event: WorldEvent {
            id: 510,
            time: 90,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotAtLocation {
                    agent_id: "agent-1".to_string(),
                    location_id: "loc-factory".to_string(),
                },
            },
            runtime_event: None,
        },
    });

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
            location_id: "loc-factory".to_string(),
            name: "factory".to_string(),
            pos: GeoPos {
                x_cm: 900_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 900_000,
        },
    ];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-factory".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("factory location precheck rerouted")));
}

#[test]
fn llm_agent_normalizes_schedule_factory_id_from_kind_alias() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.assembler.mk1","recipe_id":"recipe.assembler.control_chip","batches":1}"#
                .to_string(),
        ),
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
        action_id: 531,
        success: true,
        event: WorldEvent {
            id: 631,
            time: 140,
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
        .add(ResourceKind::Data, 10)
        .expect("add test hardware");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("factory_id normalized by guardrail")));
}

#[test]
fn llm_agent_reroutes_schedule_recipe_from_incompatible_factory_to_compatible_factory() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.power.radiation.mk1","recipe_id":"recipe.assembler.control_chip","batches":1}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    for (time, factory_id, factory_kind) in [
        (180_u64, "factory.power.1", "factory.power.radiation.mk1"),
        (181_u64, "factory.assembler.1", "factory.assembler.mk1"),
    ] {
        behavior.on_action_result(&ActionResult {
            action: Action::BuildFactory {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-home".to_string(),
                factory_id: factory_id.to_string(),
                factory_kind: factory_kind.to_string(),
            },
            action_id: 2000 + time,
            success: true,
            event: WorldEvent {
                id: 3000 + time,
                time,
                kind: WorldEventKind::FactoryBuilt {
                    owner: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    location_id: "loc-home".to_string(),
                    factory_id: factory_id.to_string(),
                    factory_kind: factory_kind.to_string(),
                    electricity_cost: 8,
                    hardware_cost: 4,
                },
                runtime_event: None,
            },
        });
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
        .add(ResourceKind::Data, 10)
        .expect("add data");
    observation
        .self_resources
        .add(ResourceKind::Electricity, 20)
        .expect("add electricity");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.assembler.1".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| {
        step.output_summary
            .contains("factory kind compatibility guardrail rerouted factory_id")
            || step
                .output_summary
                .contains("schedule_recipe factory_id normalized by guardrail")
    }));
}

#[test]
fn llm_agent_reroutes_schedule_recipe_to_build_required_factory_when_kind_missing() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.assembler.mk1.0","recipe_id":"recipe.assembler.control_chip","batches":1}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    behavior.on_action_result(&ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            factory_id: "factory.power.1".to_string(),
            factory_kind: "factory.power.radiation.mk1".to_string(),
        },
        action_id: 2100,
        success: true,
        event: WorldEvent {
            id: 3100,
            time: 182,
            kind: WorldEventKind::FactoryBuilt {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-home".to_string(),
                factory_id: "factory.power.1".to_string(),
                factory_kind: "factory.power.radiation.mk1".to_string(),
                electricity_cost: 8,
                hardware_cost: 4,
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

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            factory_id: "factory.assembler.mk1".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("missing required factory kind rerouted to build_factory")));
}

#[test]
fn llm_agent_reroutes_duplicate_build_factory_to_schedule_on_known_factory() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"build_factory","owner":"self","location_id":"loc-home","factory_id":"factory.assembler.mk1","factory_kind":"factory.assembler.mk1"}"#
                .to_string(),
        ),
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
        action_id: 532,
        success: true,
        event: WorldEvent {
            id: 632,
            time: 141,
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
        .add(ResourceKind::Data, 16)
        .expect("add test hardware");

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
        .contains("build_factory dedup guardrail rerouted to schedule_recipe")));
}

#[test]
fn llm_agent_build_factory_normalizes_unknown_location_to_current_location() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"build_factory","owner":"self","location_id":"loc.unknown","factory_id":"factory.assembler.mk1","factory_kind":"factory.assembler.mk1"}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

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
        AgentDecision::Act(Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            factory_id: "factory.assembler.mk1".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("build_factory.location_id normalized by guardrail")));
}

#[test]
fn llm_agent_build_factory_normalizes_unknown_location_to_nearest_visible_when_current_unknown() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"build_factory","owner":"self","location_id":"loc.current","factory_id":"factory.assembler.mk1","factory_kind":"factory.assembler.mk1"}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.visible_locations = vec![
        ObservedLocation {
            location_id: "loc-nearest".to_string(),
            name: "near".to_string(),
            pos: GeoPos {
                x_cm: 50_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 50_000,
        },
        ObservedLocation {
            location_id: "loc-far".to_string(),
            name: "far".to_string(),
            pos: GeoPos {
                x_cm: 200_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 200_000,
        },
    ];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-nearest".to_string(),
            factory_id: "factory.assembler.mk1".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("build_factory.location_id normalized by guardrail")));
}
