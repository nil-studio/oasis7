#[test]
fn llm_agent_user_prompt_includes_recipe_coverage_summary() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    behavior.on_action_result(&ActionResult {
        action: Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.alpha".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
        },
        action_id: 521,
        success: true,
        event: WorldEvent {
            id: 621,
            time: 121,
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
        },
    });

    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("\"recipe_coverage\""));
    assert!(prompt.contains("\"recipe.smelter.iron_ingot\"") || prompt.contains("...(truncated)"));
    assert!(
        prompt.contains("\"recipe.assembler.control_chip\"") || prompt.contains("...(truncated)")
    );
    assert!(prompt.contains("\"recipe.assembler.motor_mk1\"") || prompt.contains("...(truncated)"));
}

#[test]
fn llm_agent_prioritizes_mine_alternative_with_lower_failure_streak() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    for (action_id, event_id, time) in [(2_100, 2_101, 100_u64), (2_102, 2_103, 102_u64)] {
        behavior.on_action_result(&ActionResult {
            action: Action::MineCompound {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-bad".to_string(),
                compound_mass_g: 3_000,
            },
            action_id,
            success: false,
            event: WorldEvent {
                id: event_id,
                time,
                kind: WorldEventKind::ActionRejected {
                    reason: RejectReason::InsufficientResource {
                        owner: ResourceOwner::Location {
                            location_id: "loc-bad".to_string(),
                        },
                        kind: ResourceKind::Data,
                        requested: 3_000,
                        available: 0,
                    },
                },
                runtime_event: None,
            },
        });
    }

    behavior.on_action_result(&ActionResult {
        action: Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 3_000,
        },
        action_id: 2_104,
        success: false,
        event: WorldEvent {
            id: 2_105,
            time: 110,
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
    observation.time = 112;
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
            location_id: "loc-bad".to_string(),
            name: "bad".to_string(),
            pos: GeoPos {
                x_cm: 200_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 200_000,
        },
        ObservedLocation {
            location_id: "loc-good".to_string(),
            name: "good".to_string(),
            pos: GeoPos {
                x_cm: 500_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 500_000,
        },
    ];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-good".to_string(),
        })
    );
}

#[test]
fn llm_agent_clamps_schedule_recipe_batches_by_available_hardware() {
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
            recipe_id: "recipe.assembler.logistics_drone".to_string(),
            batches: 4,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("batches clamped")));
}

#[test]
fn llm_agent_reroutes_schedule_recipe_to_harvest_when_electricity_is_insufficient() {
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
    observation
        .self_resources
        .add(ResourceKind::Data, 8)
        .expect("add test hardware");
    observation
        .self_resources
        .remove(ResourceKind::Electricity, 25)
        .expect("trim electricity");

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
        .contains("schedule_recipe electricity precheck rerouted")));
}

#[test]
fn llm_agent_reroutes_move_agent_to_harvest_when_electricity_is_insufficient() {
    let client = MockClient {
        output: Some(r#"{"decision":"move_agent","to":"loc-2"}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let mut observation = make_observation();
    observation
        .self_resources
        .remove(ResourceKind::Electricity, 30)
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
        .contains("move_agent electricity precheck rerouted")));
}

#[test]
fn llm_agent_rewrites_execute_until_wait_action_to_actionable_harvest() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"execute_until","action":{"decision":"wait"},"until":{"event":"new_visible_agent"},"max_ticks":4}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let mut observation = make_observation();

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 1,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.parse_error.is_none());
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("decision_rewrite={")));

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 1,
        },
        action_id: 301,
        success: true,
        event: WorldEvent {
            id: 401,
            time: 101,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-1".to_string(),
                amount: 1,
                available: 12,
            },
            runtime_event: None,
        },
    });
    observation.time = 102;
    let prompt = behavior.user_prompt(&observation, &[], 0, 4);
    assert!(prompt.contains("\"decision_rewrite\":"));
    assert!(prompt.contains("\"from\":\"wait\""));
    assert!(prompt.contains("\"to\":\"harvest_radiation\"") || prompt.contains("...(truncated)"));
}

#[test]
fn llm_agent_clamps_execute_until_harvest_action_to_configured_cap() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":1000},"until":{"event":"new_visible_agent"},"max_ticks":4}"#.to_string(),
        ),
        err: None,
    };
    let mut config = base_config();
    config.harvest_max_amount_cap = 25;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 25,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("max_amount clamped")));
}

#[test]
fn llm_agent_clamps_execute_until_harvest_max_ticks_to_short_cap() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":8},"until":{"event":"new_visible_agent"},"max_ticks":8}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 70;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 8, .. })
    ));
    let first_trace = behavior.take_decision_trace().expect("first trace");
    assert!(first_trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("max_ticks=3")));
    assert!(first_trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("max_ticks clamped")));

    for offset in 0..4_u64 {
        behavior.on_action_result(&ActionResult {
            action: Action::HarvestRadiation {
                agent_id: "agent-1".to_string(),
                max_amount: 8,
            },
            action_id: 200 + offset,
            success: true,
            event: WorldEvent {
                id: 300 + offset,
                time: 70 + offset,
                kind: WorldEventKind::RadiationHarvested {
                    agent_id: "agent-1".to_string(),
                    location_id: "loc-2".to_string(),
                    amount: 6,
                    available: 80,
                },
                runtime_event: None,
            },
        });

        observation.time = 71 + offset;
        let decision = behavior.decide(&observation);
        if offset < 3 {
            assert!(matches!(
                decision,
                AgentDecision::Act(Action::HarvestRadiation { .. })
            ));
        } else {
            assert!(matches!(
                decision,
                AgentDecision::Act(Action::MoveAgent { .. })
            ));
        }
        let _ = behavior.take_decision_trace();
    }

    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_execute_until_stops_on_harvest_available_threshold() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":9},"until":{"event":"harvest_available_below","value_lte":1},"max_ticks":4}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 60;
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
        action_id: 104,
        success: true,
        event: WorldEvent {
            id: 204,
            time: 60,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 5,
                available: 1,
            },
            runtime_event: None,
        },
    });

    observation.time = 61;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_prompt_contains_execute_until_and_exploration_guidance() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let system_prompt = behavior.system_prompt();
    let user_prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);

    assert!(system_prompt.contains("anti_stagnation"));
    assert!(system_prompt.contains("exploration_bias"));
    assert!(system_prompt.contains("execute_until"));
    assert!(user_prompt.contains("execute_until"));
    assert!(user_prompt.contains("transfer_resource"));
    assert!(user_prompt.contains("buy_power"));
    assert!(user_prompt.contains("sell_power"));
    assert!(user_prompt.contains("place_power_order"));
    assert!(user_prompt.contains("cancel_power_order"));
    assert!(user_prompt.contains("refine_compound"));
    assert!(user_prompt.contains("build_factory"));
    assert!(user_prompt.contains("schedule_recipe"));
    assert!(user_prompt.contains("publish_social_fact"));
    assert!(user_prompt.contains("adjudicate_social_fact"));
    assert!(user_prompt.contains("declare_social_edge"));
    assert!(user_prompt.contains("observation.recipe_coverage.missing"));
    assert!(user_prompt.contains("move_agent.to 不能是当前所在位置"));
}

#[test]
fn llm_agent_collapses_multiple_tool_calls_in_single_turn() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"wait"}

---

{"decision":"wait"}"#
                .to_string(),
        ),
        err: None,
    };
    let mut config = base_config();
    config.max_repair_rounds = 0;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.parse_error.is_none());
    assert!(trace
        .llm_output
        .as_deref()
        .is_some_and(|output| output.contains("---")));
    let notes = trace
        .llm_chat_messages
        .iter()
        .filter(|msg| matches!(msg.role, LlmChatRole::System))
        .map(|msg| msg.content.as_str())
        .collect::<Vec<_>>();
    assert!(notes
        .iter()
        .any(|note| note.contains("multi-turn output collapsed by guardrail")));
}

#[test]
fn llm_agent_collapses_multi_segment_output_to_last_terminal_decision() {
    let client = MockClient {
        output: Some(
            r#"{"type":"module_call","module":"environment.current_observation","args":{}}

---

{"type":"module_call","module":"environment.current_observation","args":{}}

---

{"decision":"harvest_radiation","max_amount":5}"#
                .to_string(),
        ),
        err: None,
    };
    let mut config = base_config();
    config.max_repair_rounds = 0;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 5,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.parse_error.is_none());
    assert_eq!(
        trace.llm_diagnostics.expect("diagnostics").retry_count,
        0,
        "collapsed multi-turn output should not consume repair rounds"
    );
}

#[test]
fn llm_agent_normalizes_mine_compound_unknown_location_to_inferred_current_location() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-001","compound_mass_g":1000}"#
                .to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-2".to_string(),
            compound_mass_g: 1000,
        })
    );
}

#[test]
fn llm_parse_turn_responses_extracts_multiple_json_blocks() {
    let turns = completion_turns_from_output(
        r#"{"type":"module_call","module":"agent.modules.list","args":{}}

---

{"type":"decision_draft","decision":{"decision":"wait"},"need_verify":false}

---

{"decision":"wait"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    assert_eq!(parsed.len(), 3);
    assert!(matches!(
        parsed[0],
        super::decision_flow::ParsedLlmTurn::ModuleCall { .. }
    ));
    assert!(matches!(
        parsed[1],
        super::decision_flow::ParsedLlmTurn::DecisionDraft { .. }
    ));
    assert!(matches!(
        parsed[2],
        super::decision_flow::ParsedLlmTurn::Decision {
            decision: AgentDecision::Wait,
            ..
        }
    ));
}

#[test]
fn llm_parse_schedule_recipe_non_positive_batches_normalizes_to_one() {
    let turns = completion_turns_from_output(
        r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":0}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Decision { decision, .. } => {
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
        }
        other => panic!("expected decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_cancel_power_order_defaults_owner_to_self() {
    let turns = completion_turns_from_output(r#"{"decision":"cancel_power_order","order_id":12}"#);
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Decision { decision, .. } => {
            assert_eq!(
                decision,
                AgentDecision::Act(Action::CancelPowerOrder {
                    owner: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    order_id: 12,
                })
            );
        }
        other => panic!("expected decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_declare_social_edge_action() {
    let turns = completion_turns_from_output(
        r#"{"decision":"declare_social_edge","declarer":"self","schema_id":"social.relation.v1","relation_kind":"trusted_peer","from":"self","to":"agent:agent-2","weight_bps":4200,"backing_fact_ids":[3,4],"ttl_ticks":8}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Decision { decision, .. } => {
            assert_eq!(
                decision,
                AgentDecision::Act(Action::DeclareSocialEdge {
                    declarer: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    schema_id: "social.relation.v1".to_string(),
                    relation_kind: "trusted_peer".to_string(),
                    from: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    to: ResourceOwner::Agent {
                        agent_id: "agent-2".to_string(),
                    },
                    weight_bps: 4_200,
                    backing_fact_ids: vec![3, 4],
                    ttl_ticks: Some(8),
                })
            );
        }
        other => panic!("expected decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_publish_social_fact_rejects_invalid_confidence_ppm() {
    let turns = completion_turns_from_output(
        r#"{"decision":"publish_social_fact","actor":"self","schema_id":"social.reputation.v1","subject":"agent:agent-2","claim":"claim","confidence_ppm":1000001,"evidence_event_ids":[1]}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Invalid(err) => {
            assert!(err.contains("confidence_ppm out of range"));
        }
        other => panic!("expected invalid, got {other:?}"),
    }
}

#[test]
fn llm_parse_execute_until_accepts_event_any_of() {
    let turns = completion_turns_from_output(
        r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":3},"until":{"event_any_of":["new_visible_agent","new_visible_location"]},"max_ticks":5}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::ExecuteUntil { directive, .. } => {
            assert_eq!(directive.until_conditions.len(), 2);
            assert_eq!(
                directive.until_conditions[0],
                super::decision_flow::ExecuteUntilCondition {
                    kind: super::decision_flow::ExecuteUntilEventKind::NewVisibleAgent,
                    value_lte: None,
                }
            );
            assert_eq!(
                directive.until_conditions[1],
                super::decision_flow::ExecuteUntilCondition {
                    kind: super::decision_flow::ExecuteUntilEventKind::NewVisibleLocation,
                    value_lte: None,
                }
            );
        }
        other => panic!("expected execute_until, got {other:?}"),
    }
}

#[test]
fn llm_parse_execute_until_rewrites_wait_action_to_minimal_harvest() {
    let turns = completion_turns_from_output(
        r#"{"decision":"execute_until","action":{"decision":"wait"},"until":{"event":"new_visible_agent"},"max_ticks":5}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::ExecuteUntil {
            directive,
            rewrite_receipt,
            ..
        } => {
            assert!(matches!(
                directive.action,
                Action::HarvestRadiation { max_amount: 1, .. }
            ));
            let rewrite_receipt = rewrite_receipt.expect("rewrite receipt");
            assert_eq!(rewrite_receipt.from, "wait");
            assert_eq!(rewrite_receipt.to, "harvest_radiation");
            assert!(rewrite_receipt.reason.contains("non-actionable"));
        }
        other => panic!("expected execute_until, got {other:?}"),
    }
}

#[test]
fn llm_parse_execute_until_accepts_threshold_event_with_value_lte() {
    let turns = completion_turns_from_output(
        r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":3},"until":{"event":"harvest_yield_below","value_lte":2},"max_ticks":5}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::ExecuteUntil { directive, .. } => {
            assert_eq!(directive.until_conditions.len(), 1);
            assert_eq!(
                directive.until_conditions[0],
                super::decision_flow::ExecuteUntilCondition {
                    kind: super::decision_flow::ExecuteUntilEventKind::HarvestYieldBelow,
                    value_lte: Some(2),
                }
            );
        }
        other => panic!("expected execute_until, got {other:?}"),
    }
}

#[test]
fn llm_parse_execute_until_rejects_threshold_event_without_value_lte() {
    let turns = completion_turns_from_output(
        r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":3},"until":{"event":"harvest_available_below"},"max_ticks":5}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Invalid(err) => {
            assert!(err.contains("requires until.value_lte"));
        }
        other => panic!("expected invalid execute_until, got {other:?}"),
    }
}

#[test]
fn llm_parse_decision_draft_accepts_shorthand_decision_payload() {
    let turns = completion_turns_from_output(
        r#"{"type":"decision_draft","decision":"harvest_radiation","max_amount":7,"need_verify":false}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::DecisionDraft { draft, .. } => {
            assert!(matches!(
                draft.decision,
                AgentDecision::Act(Action::HarvestRadiation { max_amount: 7, .. })
            ));
            assert!(!draft.need_verify);
        }
        other => panic!("expected decision_draft, got {other:?}"),
    }
}

#[test]
fn llm_parse_turn_response_extracts_message_to_user() {
    let turns = completion_turns_from_output(
        r#"{"decision":"wait","message_to_user":"先暂停一回合观察环境。"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::Decision {
            decision,
            message_to_user,
            ..
        } => {
            assert_eq!(decision, AgentDecision::Wait);
            assert_eq!(message_to_user.as_deref(), Some("先暂停一回合观察环境。"));
        }
        other => panic!("expected decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_turn_response_normalizes_module_alias_name() {
    let turns = completion_turns_from_output(
        r#"{"type":"module_call","module":"agent_modules_list","args":{}}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1")
        .into_iter()
        .next()
        .expect("single parsed turn");

    match parsed {
        super::decision_flow::ParsedLlmTurn::ModuleCall { request, .. } => {
            assert_eq!(request.module, "agent.modules.list");
        }
        other => panic!("expected module_call, got {other:?}"),
    }
}

#[test]
fn llm_agent_reroutes_mine_compound_from_depleted_location_to_alternative_location() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000}"#
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
            compound_mass_g: 3_000,
        },
        action_id: 613,
        success: false,
        event: WorldEvent {
            id: 713,
            time: 162,
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
                x_cm: 700_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 700_000,
        },
    ];
    observation.time = 163;

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-alt".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("rerouted to move_agent")));
}

#[test]
fn llm_agent_skips_depleted_location_during_cooldown_window() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000}"#
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
            compound_mass_g: 3_000,
        },
        action_id: 1_000,
        success: false,
        event: WorldEvent {
            id: 1_001,
            time: 220,
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
    observation.time = 221;
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
                x_cm: 700_000,
                y_cm: 0,
                z_cm: 0,
            },
            profile: Default::default(),
            distance_cm: 700_000,
        },
    ];

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-alt".to_string(),
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(trace.llm_step_trace.iter().any(|step| step
        .output_summary
        .contains("cooldown guardrail rerouted to move_agent")));
}

#[test]
fn llm_agent_allows_retry_depleted_location_after_cooldown_expires() {
    let client = MockClient {
        output: Some(
            r#"{"decision":"mine_compound","owner":"self","location_id":"loc-home","compound_mass_g":3000}"#
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
            compound_mass_g: 3_000,
        },
        action_id: 1_002,
        success: false,
        event: WorldEvent {
            id: 1_003,
            time: 230,
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
    observation.time = 240;
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
        AgentDecision::Act(Action::MineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-home".to_string(),
            compound_mass_g: 3_000,
        })
    );

    let trace = behavior.take_decision_trace().expect("trace");
    assert!(!trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("cooldown guardrail rerouted")));
}

#[test]
fn llm_parse_quote_micro_depot_install_reuses_serialized_install_action() {
    let install = Action::InstallMicroDepot {
        installer_agent_id: "agent-1".to_string(),
        facility_id: "depot-quote".to_string(),
        location_id: "loc-home".to_string(),
        owner_claim_id: "claim-quote".to_string(),
        regional_blocker_receipt_id: "repair-logistics:loc-home:1".to_string(),
        module_id: "regional.micro_depot".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: "quote-hash".to_string(),
        entrypoint: "evaluate_quote".to_string(),
        service_radius_cm: 250_000,
        supported_resource_kinds: vec!["data".to_string()],
    };
    let turns = completion_turns_from_output(
        serde_json::json!({
            "decision": "quote_micro_depot_install",
            "action": install,
        })
        .to_string()
        .as_str(),
    );

    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    assert!(matches!(
        parsed.first(),
        Some(super::decision_flow::ParsedLlmTurn::Decision {
            decision: AgentDecision::Query(AgentQuery::QuoteMicroDepotInstall(action)),
            ..
        }) if action == &install
    ));
}

#[test]
fn llm_parse_quote_micro_depot_install_rejects_invalid_action_payload() {
    let turns = completion_turns_from_output(
        r#"{"decision":"quote_micro_depot_install","action":{"MoveAgent":{"agent_id":"agent-1","to":"loc-home"}}}"#,
    );

    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    assert!(matches!(
        parsed.first(),
        Some(super::decision_flow::ParsedLlmTurn::Invalid(error))
            if error.contains("quote_micro_depot_install requires an InstallMicroDepot action")
    ));
}
