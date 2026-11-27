#[test]
fn build_responses_request_payload_includes_tools_and_required_choice() {
    let request = LlmCompletionRequest {
        model: "gpt-test".to_string(),
        system_prompt: "system".to_string(),
        user_prompt: "user".to_string(),
        debug_mode: false,
        max_model_calls: None,
    };

    let payload = build_responses_request_payload(&request).expect("payload");
    let payload_json = serde_json::to_value(payload).expect("payload json");

    assert_eq!(
        payload_json.get("instructions").and_then(|v| v.as_str()),
        Some("system")
    );
    assert_eq!(
        payload_json
            .get("input")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("type"))
            .and_then(|v| v.as_str()),
        Some("message")
    );
    assert_eq!(
        payload_json
            .get("input")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("role"))
            .and_then(|v| v.as_str()),
        Some("user")
    );
    assert_eq!(
        payload_json
            .get("input")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("content"))
            .and_then(|v| v.as_array())
            .and_then(|content| content.first())
            .and_then(|part| part.get("type"))
            .and_then(|v| v.as_str()),
        Some("input_text")
    );
    assert_eq!(
        payload_json
            .get("input")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("content"))
            .and_then(|v| v.as_array())
            .and_then(|content| content.first())
            .and_then(|part| part.get("text"))
            .and_then(|v| v.as_str()),
        Some("user")
    );

    let tool_choice = payload_json
        .get("tool_choice")
        .expect("tool choice exists")
        .as_str()
        .expect("tool choice string");
    assert_eq!(tool_choice, "required");
    assert_eq!(
        payload_json
            .get("parallel_tool_calls")
            .and_then(|v| v.as_bool()),
        Some(false)
    );

    let tools = payload_json
        .get("tools")
        .and_then(|v| v.as_array())
        .expect("tools array");
    assert_eq!(tools.len(), responses_tools().len());

    let function_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        function_names,
        vec![
            OPENAI_TOOL_AGENT_MODULES_LIST,
            OPENAI_TOOL_ENVIRONMENT_CURRENT_OBSERVATION,
            OPENAI_TOOL_MEMORY_SHORT_TERM_RECENT,
            OPENAI_TOOL_MEMORY_LONG_TERM_SEARCH,
            OPENAI_TOOL_WORLD_RULES_GUIDE,
            OPENAI_TOOL_MODULE_LIFECYCLE_STATUS,
            OPENAI_TOOL_POWER_ORDER_BOOK_STATUS,
            OPENAI_TOOL_MODULE_MARKET_STATUS,
            OPENAI_TOOL_SOCIAL_STATE_STATUS,
            OPENAI_TOOL_AGENT_SUBMIT_DECISION,
        ]
    );
}

#[test]
fn completion_result_from_sdk_stream_events_uses_completed_response() {
    let completed = serde_json::from_value::<async_openai::types::responses::ResponseStreamEvent>(
        serde_json::json!({
            "type": "response.completed",
            "sequence_number": 3,
            "response": {
                "id": "resp_stream_1",
                "object": "response",
                "created_at": 1,
                "completed_at": 2,
                "model": "gpt-test",
                "output": [{
                    "type": "function_call",
                    "call_id": "call_decision",
                    "name": OPENAI_TOOL_AGENT_SUBMIT_DECISION,
                    "arguments": "{\"decision\":\"wait_ticks\",\"ticks\":2}"
                }],
                "status": "completed",
                "parallel_tool_calls": false
            }
        }),
    )
    .expect("stream completed event");

    let result = completion_result_from_sdk_stream_events(vec![completed]).expect("completion");

    assert_eq!(result.model.as_deref(), Some("gpt-test"));
    assert_eq!(result.output, "{\"decision\":\"wait_ticks\",\"ticks\":2}");
    assert_eq!(
        result.turns,
        vec![LlmCompletionTurn::Decision {
            payload: serde_json::json!({
                "decision": "wait_ticks",
                "ticks": 2,
            }),
        }]
    );
}

#[test]
fn completion_result_from_sdk_stream_events_uses_output_item_done_when_completed_output_is_empty() {
    let output_item_done = serde_json::from_value::<
        async_openai::types::responses::ResponseStreamEvent,
    >(serde_json::json!({
        "type": "response.output_item.done",
        "sequence_number": 14,
        "output_index": 0,
        "item": {
            "type": "function_call",
            "call_id": "call_decision",
            "name": OPENAI_TOOL_AGENT_SUBMIT_DECISION,
            "arguments": "{\"decision\":\"wait_ticks\",\"ticks\":2}",
            "status": "completed"
        }
    }))
    .expect("output item done event");
    let completed = serde_json::from_value::<async_openai::types::responses::ResponseStreamEvent>(
        serde_json::json!({
            "type": "response.completed",
            "sequence_number": 15,
            "response": {
                "id": "resp_stream_2",
                "object": "response",
                "created_at": 1,
                "completed_at": 2,
                "model": "gpt-test",
                "output": [],
                "status": "completed",
                "parallel_tool_calls": false
            }
        }),
    )
    .expect("stream completed event");

    let result = completion_result_from_sdk_stream_events(vec![output_item_done, completed])
        .expect("completion from output item");

    assert_eq!(result.model.as_deref(), Some("gpt-test"));
    assert_eq!(result.output, "{\"decision\":\"wait_ticks\",\"ticks\":2}");
    assert_eq!(
        result.turns,
        vec![LlmCompletionTurn::Decision {
            payload: serde_json::json!({
                "decision": "wait_ticks",
                "ticks": 2,
            }),
        }]
    );
}

#[test]
fn decision_tool_schema_includes_market_and_social_actions() {
    let decision_tool = responses_tools()
        .into_iter()
        .find_map(|tool| match tool {
            Tool::Function(function_tool)
                if function_tool.name == OPENAI_TOOL_AGENT_SUBMIT_DECISION =>
            {
                Some(function_tool)
            }
            _ => None,
        })
        .expect("decision tool exists");

    let parameters = decision_tool.parameters.expect("decision tool parameters");
    let decision_enum = parameters
        .get("properties")
        .and_then(|value| value.get("decision"))
        .and_then(|value| value.get("enum"))
        .and_then(|value| value.as_array())
        .expect("decision enum");
    let decision_enum = decision_enum
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();

    assert!(decision_enum.contains(&"buy_power"));
    assert!(decision_enum.contains(&"sell_power"));
    assert!(decision_enum.contains(&"place_power_order"));
    assert!(decision_enum.contains(&"cancel_power_order"));
    assert!(decision_enum.contains(&"publish_social_fact"));
    assert!(decision_enum.contains(&"challenge_social_fact"));
    assert!(decision_enum.contains(&"adjudicate_social_fact"));
    assert!(decision_enum.contains(&"revoke_social_fact"));
    assert!(decision_enum.contains(&"declare_social_edge"));
    assert!(decision_enum.contains(&"form_alliance"));
    assert!(decision_enum.contains(&"join_alliance"));
    assert!(decision_enum.contains(&"leave_alliance"));
    assert!(decision_enum.contains(&"dissolve_alliance"));
    assert!(decision_enum.contains(&"declare_war"));
    assert!(decision_enum.contains(&"open_governance_proposal"));
    assert!(decision_enum.contains(&"cast_governance_vote"));
    assert!(decision_enum.contains(&"resolve_crisis"));
    assert!(decision_enum.contains(&"grant_meta_progress"));
    assert!(decision_enum.contains(&"open_economic_contract"));
    assert!(decision_enum.contains(&"accept_economic_contract"));
    assert!(decision_enum.contains(&"settle_economic_contract"));

    let properties = parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("properties");
    assert!(properties.contains_key("price_per_pu"));
    assert!(properties.contains_key("side"));
    assert!(properties.contains_key("confidence_ppm"));
    assert!(properties.contains_key("adjudication"));
    assert!(properties.contains_key("backing_fact_ids"));
    assert!(properties.contains_key("alliance_id"));
    assert!(properties.contains_key("members"));
    assert!(properties.contains_key("member_agent_id"));
    assert!(properties.contains_key("war_id"));
    assert!(properties.contains_key("proposal_key"));
    assert!(properties.contains_key("voting_window_ticks"));
    assert!(properties.contains_key("pass_threshold_bps"));
    assert!(properties.contains_key("crisis_id"));
    assert!(properties.contains_key("target_agent_id"));
    assert!(properties.contains_key("points"));
    assert!(properties.contains_key("contract_id"));
    assert!(properties.contains_key("settlement_kind"));
    assert!(properties.contains_key("settlement_amount"));
    assert!(properties.contains_key("reputation_stake"));
    assert!(properties.contains_key("expires_at"));
    assert!(properties.contains_key("accepter_agent_id"));
}

#[test]
fn build_responses_request_payload_includes_debug_tool_when_enabled() {
    let request = LlmCompletionRequest {
        model: "gpt-test".to_string(),
        system_prompt: "system".to_string(),
        user_prompt: "user".to_string(),
        debug_mode: true,
        max_model_calls: None,
    };

    let payload = build_responses_request_payload(&request).expect("payload");
    let payload_json = serde_json::to_value(payload).expect("payload json");
    let tools = payload_json
        .get("tools")
        .and_then(|v| v.as_array())
        .expect("tools array");

    assert_eq!(tools.len(), responses_tools_with_debug_mode(true).len());
    let function_names = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(|v| v.as_str()))
        .collect::<Vec<_>>();
    for base_name in [
        OPENAI_TOOL_AGENT_MODULES_LIST,
        OPENAI_TOOL_ENVIRONMENT_CURRENT_OBSERVATION,
        OPENAI_TOOL_MEMORY_SHORT_TERM_RECENT,
        OPENAI_TOOL_MEMORY_LONG_TERM_SEARCH,
        OPENAI_TOOL_WORLD_RULES_GUIDE,
        OPENAI_TOOL_MODULE_LIFECYCLE_STATUS,
        OPENAI_TOOL_POWER_ORDER_BOOK_STATUS,
        OPENAI_TOOL_MODULE_MARKET_STATUS,
        OPENAI_TOOL_SOCIAL_STATE_STATUS,
        OPENAI_TOOL_AGENT_SUBMIT_DECISION,
    ] {
        assert!(function_names.contains(&base_name));
    }
    assert!(function_names.contains(&OPENAI_TOOL_AGENT_DEBUG_GRANT_RESOURCE));
}

#[test]
fn llm_agent_parse_move_action() {
    let client = MockClient {
        output: Some("{\"decision\":\"move_agent\",\"to\":\"loc-2\"}".to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_transfer_resource_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"transfer_resource\",\"from_owner\":\"location:loc-1\",\"to_owner\":\"self\",\"kind\":\"electricity\",\"amount\":7}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::TransferResource {
            from: ResourceOwner::Location {
                location_id: "loc-1".to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            kind: ResourceKind::Electricity,
            amount: 7,
        })
    );
}

#[test]
fn llm_agent_parse_mine_compound_action_defaults_to_self_owner() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"mine_compound\",\"location_id\":\"frag-1\",\"compound_mass_g\":1200}"
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
            compound_mass_g: 1_200,
        })
    );
}

#[test]
fn llm_agent_parse_refine_compound_action_defaults_to_self_owner() {
    let client = MockClient {
        output: Some("{\"decision\":\"refine_compound\",\"compound_mass_g\":80}".to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::RefineCompound {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            compound_mass_g: 80,
        })
    );
}

#[test]
fn llm_agent_parse_debug_grant_resource_action_when_debug_mode_enabled() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"debug_grant_resource\",\"kind\":\"data\",\"amount\":9}".to_string(),
        ),
        err: None,
    };
    let mut config = base_config();
    config.llm_debug_mode = true;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::DebugGrantResource {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            kind: ResourceKind::Data,
            amount: 9,
        })
    );
}

#[test]
fn llm_agent_rejects_debug_grant_resource_action_when_debug_mode_disabled() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"debug_grant_resource\",\"kind\":\"data\",\"amount\":9}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("debug_grant_resource is disabled"));
}

#[test]
fn llm_agent_rejects_transfer_resource_with_invalid_kind() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"transfer_resource\",\"from_owner\":\"self\",\"to_owner\":\"location:loc-1\",\"kind\":\"invalid_kind\",\"amount\":3}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("invalid kind"));
}

#[test]
fn llm_agent_parse_build_factory_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"build_factory\",\"owner\":\"self\",\"location_id\":\"loc-1\",\"factory_id\":\"factory.alpha\",\"factory_kind\":\"factory.assembler.mk1\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-2".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        })
    );
}

#[test]
fn llm_agent_infers_build_factory_kind_from_supported_factory_id() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"build_factory\",\"owner\":\"self\",\"location_id\":\"loc-1\",\"factory_id\":\"factory.smelter.mk1\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-2".to_string(),
            factory_id: "factory.smelter.mk1".to_string(),
            factory_kind: "factory.smelter.mk1".to_string(),
        })
    );
}

#[test]
fn llm_agent_rejects_build_factory_without_factory_kind() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"build_factory\",\"owner\":\"self\",\"location_id\":\"loc-1\",\"factory_id\":\"factory.alpha\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("build_factory missing `factory_kind`"));
}

#[test]
fn llm_agent_parse_schedule_recipe_action_with_default_batches() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"schedule_recipe\",\"owner\":\"self\",\"factory_id\":\"factory.alpha\",\"recipe_id\":\"recipe.assembler.control_chip\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let mut observation = make_observation();
    observation
        .self_resources
        .add(ResourceKind::Data, 2)
        .expect("add test data");
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
}

#[test]
fn llm_agent_parse_buy_power_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"buy_power\",\"buyer\":\"self\",\"seller\":\"agent:agent-2\",\"amount\":7,\"price_per_pu\":0}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::BuyPower {
            buyer: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            seller: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            amount: 7,
            price_per_pu: 0,
        })
    );
}

#[test]
fn llm_agent_parse_place_power_order_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"place_power_order\",\"owner\":\"self\",\"side\":\"sell\",\"amount\":9,\"limit_price_per_pu\":3}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::PlacePowerOrder {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            side: PowerOrderSide::Sell,
            amount: 9,
            limit_price_per_pu: 3,
        })
    );
}

#[test]
fn llm_agent_parse_publish_social_fact_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"publish_social_fact\",\"actor\":\"self\",\"schema_id\":\"social.reputation.v1\",\"subject\":\"agent:agent-2\",\"claim\":\"agent-2 completed delivery\",\"confidence_ppm\":800000,\"evidence_event_ids\":[7,8],\"ttl_ticks\":12,\"stake\":{\"kind\":\"data\",\"amount\":2}}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::PublishSocialFact {
            actor: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            schema_id: "social.reputation.v1".to_string(),
            subject: ResourceOwner::Agent {
                agent_id: "agent-2".to_string(),
            },
            object: None,
            claim: "agent-2 completed delivery".to_string(),
            confidence_ppm: 800_000,
            evidence_event_ids: vec![7, 8],
            ttl_ticks: Some(12),
            stake: Some(SocialStake {
                kind: ResourceKind::Data,
                amount: 2,
            }),
        })
    );
}

#[test]
fn llm_agent_parse_adjudicate_social_fact_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"adjudicate_social_fact\",\"adjudicator\":\"self\",\"fact_id\":42,\"adjudication\":\"confirm\",\"notes\":\"证据充分\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::AdjudicateSocialFact {
            adjudicator: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            fact_id: 42,
            decision: SocialAdjudicationDecision::Confirm,
            notes: "证据充分".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_form_alliance_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"form_alliance\",\"proposer_agent_id\":\"self\",\"alliance_id\":\"alliance.alpha\",\"members\":[\"self\",\"agent:agent-2\"],\"charter\":\"mutual defense\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::FormAlliance {
            proposer_agent_id: "agent-1".to_string(),
            alliance_id: "alliance.alpha".to_string(),
            members: vec!["agent-1".to_string(), "agent-2".to_string()],
            charter: "mutual defense".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_join_alliance_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"join_alliance\",\"operator_agent_id\":\"self\",\"alliance_id\":\"alliance.alpha\",\"member_agent_id\":\"agent:agent-2\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::JoinAlliance {
            operator_agent_id: "agent-1".to_string(),
            alliance_id: "alliance.alpha".to_string(),
            member_agent_id: "agent-2".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_leave_alliance_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"leave_alliance\",\"operator_agent_id\":\"self\",\"alliance_id\":\"alliance.alpha\",\"member_agent_id\":\"self\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::LeaveAlliance {
            operator_agent_id: "agent-1".to_string(),
            alliance_id: "alliance.alpha".to_string(),
            member_agent_id: "agent-1".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_dissolve_alliance_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"dissolve_alliance\",\"operator_agent_id\":\"self\",\"alliance_id\":\"alliance.alpha\",\"reason\":\"merge\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::DissolveAlliance {
            operator_agent_id: "agent-1".to_string(),
            alliance_id: "alliance.alpha".to_string(),
            reason: "merge".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_open_governance_proposal_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"open_governance_proposal\",\"proposer_agent_id\":\"self\",\"proposal_key\":\"proposal.power.tax\",\"title\":\"Power Tax\",\"description\":\"Apply power tax\",\"options\":[\"approve\",\"reject\"],\"voting_window_ticks\":36,\"quorum_weight\":2,\"pass_threshold_bps\":6500}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::OpenGovernanceProposal {
            proposer_agent_id: "agent-1".to_string(),
            proposal_key: "proposal.power.tax".to_string(),
            title: "Power Tax".to_string(),
            description: "Apply power tax".to_string(),
            options: vec!["approve".to_string(), "reject".to_string()],
            voting_window_ticks: 36,
            quorum_weight: 2,
            pass_threshold_bps: 6_500,
        })
    );
}

#[test]
fn llm_agent_parse_cast_governance_vote_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"cast_governance_vote\",\"voter_agent_id\":\"self\",\"proposal_key\":\"proposal.power.tax\",\"option\":\"approve\",\"weight\":100}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::CastGovernanceVote {
            voter_agent_id: "agent-1".to_string(),
            proposal_key: "proposal.power.tax".to_string(),
            option: "approve".to_string(),
            weight: 100,
        })
    );
}

#[test]
fn llm_agent_rejects_cast_governance_vote_weight_over_limit() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"cast_governance_vote\",\"voter_agent_id\":\"self\",\"proposal_key\":\"proposal.power.tax\",\"option\":\"approve\",\"weight\":101}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("weight must be within 1..=100"));
}

#[test]
fn llm_agent_parse_grant_meta_progress_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"grant_meta_progress\",\"operator_agent_id\":\"self\",\"target_agent_id\":\"agent:agent-2\",\"track\":\"campaign.alpha\",\"points\":12,\"achievement_id\":\"ach.first_blood\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::GrantMetaProgress {
            operator_agent_id: "agent-1".to_string(),
            target_agent_id: "agent-2".to_string(),
            track: "campaign.alpha".to_string(),
            points: 12,
            achievement_id: Some("ach.first_blood".to_string()),
        })
    );
}

#[test]
fn llm_agent_parse_open_economic_contract_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"open_economic_contract\",\"creator_agent_id\":\"self\",\"contract_id\":\"contract.alpha\",\"counterparty_agent_id\":\"agent:agent-2\",\"settlement_kind\":\"data\",\"settlement_amount\":12,\"reputation_stake\":4,\"expires_at\":88,\"description\":\"labeling batch\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::OpenEconomicContract {
            creator_agent_id: "agent-1".to_string(),
            contract_id: "contract.alpha".to_string(),
            counterparty_agent_id: "agent-2".to_string(),
            settlement_kind: ResourceKind::Data,
            settlement_amount: 12,
            reputation_stake: 4,
            expires_at: 88,
            description: "labeling batch".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_accept_economic_contract_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"accept_economic_contract\",\"accepter_agent_id\":\"self\",\"contract_id\":\"contract.alpha\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::AcceptEconomicContract {
            accepter_agent_id: "agent-1".to_string(),
            contract_id: "contract.alpha".to_string(),
        })
    );
}

#[test]
fn llm_agent_parse_settle_economic_contract_action() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"settle_economic_contract\",\"operator_agent_id\":\"self\",\"contract_id\":\"contract.alpha\",\"success\":true,\"notes\":\"delivered\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::SettleEconomicContract {
            operator_agent_id: "agent-1".to_string(),
            contract_id: "contract.alpha".to_string(),
            success: true,
            notes: "delivered".to_string(),
        })
    );
}

#[test]
fn llm_agent_rejects_open_economic_contract_invalid_settlement_kind() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"open_economic_contract\",\"creator_agent_id\":\"self\",\"contract_id\":\"contract.alpha\",\"counterparty_agent_id\":\"agent:agent-2\",\"settlement_kind\":\"ore\",\"settlement_amount\":12,\"reputation_stake\":4,\"expires_at\":88,\"description\":\"labeling batch\"}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("invalid settlement_kind"));
}

#[test]
fn llm_agent_rejects_grant_meta_progress_zero_points() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"grant_meta_progress\",\"operator_agent_id\":\"self\",\"target_agent_id\":\"self\",\"track\":\"campaign.alpha\",\"points\":0}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("points must be non-zero"));
}

#[test]
fn llm_agent_rejects_place_power_order_with_invalid_side() {
    let client = MockClient {
        output: Some(
            "{\"decision\":\"place_power_order\",\"owner\":\"self\",\"side\":\"hold\",\"amount\":9,\"limit_price_per_pu\":3}".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace
        .parse_error
        .as_deref()
        .unwrap_or_default()
        .contains("invalid side"));
}

#[test]
fn llm_agent_parse_json_in_markdown_block() {
    let client = MockClient {
        output: Some(
            "```json\n{\"decision\":\"harvest_radiation\",\"max_amount\":5}\n```".to_string(),
        ),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());

    assert_eq!(
        decision,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 5,
        })
    );
}

#[test]
fn llm_agent_falls_back_to_wait_when_client_fails() {
    let client = MockClient {
        output: None,
        err: Some(LlmClientError::Http {
            message: "timeout".to_string(),
        }),
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);
}

#[test]
fn llm_agent_falls_back_to_wait_when_output_invalid() {
    let client = MockClient {
        output: Some("not json".to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);
}

#[test]
fn llm_agent_emits_decision_trace_with_io() {
    let client = MockClient {
        output: Some("{\"decision\":\"move_agent\",\"to\":\"loc-2\"}".to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert!(matches!(
        decision,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace should exist");
    assert_eq!(trace.agent_id, "agent-1");
    assert!(trace
        .llm_input
        .as_deref()
        .unwrap_or_default()
        .contains("[system]"));
    assert!(trace
        .llm_output
        .as_deref()
        .unwrap_or_default()
        .contains("move_agent"));
    assert_eq!(trace.llm_error, None);
    assert_eq!(trace.parse_error, None);
    let diagnostics = trace.llm_diagnostics.as_ref().expect("diagnostics");
    assert_eq!(diagnostics.model.as_deref(), Some("gpt-test"));
    assert_eq!(diagnostics.prompt_tokens, Some(12));
    assert_eq!(diagnostics.completion_tokens, Some(4));
    assert_eq!(diagnostics.total_tokens, Some(16));
    assert_eq!(diagnostics.retry_count, 0);
    assert!(diagnostics.latency_ms.is_some());
    assert_eq!(behavior.take_decision_trace(), None);
}

#[test]
fn llm_agent_records_message_to_user_in_agent_chat_trace() {
    let client = MockClient {
        output: Some(r#"{"decision":"wait","message_to_user":"我会先观察一回合。"}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace should exist");
    assert_eq!(trace.llm_chat_messages.len(), 1);
    assert_eq!(trace.llm_chat_messages[0].role, LlmChatRole::Agent);
    assert_eq!(trace.llm_chat_messages[0].content, "我会先观察一回合。");
}

#[test]
fn llm_agent_does_not_record_raw_json_as_agent_chat_message_without_user_message() {
    let client = MockClient {
        output: Some(r#"{"decision":"wait"}"#.to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace should exist");
    assert!(trace.llm_chat_messages.is_empty());
}

#[test]
fn llm_agent_supports_module_call_then_decision() {
    let client = SequenceMockClient::new(vec![
        "{\"type\":\"module_call\",\"module\":\"agent.modules.list\",\"args\":{}}".to_string(),
        "{\"decision\":\"move_agent\",\"to\":\"loc-2\"}".to_string(),
    ]);
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert!(matches!(
        decision,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert_eq!(trace.parse_error, None);
    assert_eq!(trace.llm_effect_intents.len(), 1);
    assert_eq!(trace.llm_effect_receipts.len(), 1);
    let intent = &trace.llm_effect_intents[0];
    assert_eq!(intent.kind, "llm.prompt.module_call");
    assert_eq!(intent.cap_ref, "llm.prompt.module_access");
    assert_eq!(intent.origin, "llm_agent");
    assert_eq!(
        intent.params.get("module").and_then(|value| value.as_str()),
        Some("agent.modules.list")
    );
    let receipt = &trace.llm_effect_receipts[0];
    assert_eq!(receipt.intent_id, intent.intent_id);
    assert_eq!(receipt.status, "ok");
    assert_eq!(receipt.cost_cents, None);
    let llm_input = trace.llm_input.unwrap_or_default();
    assert!(llm_input.contains("[module_result:agent.modules.list]"));
    let llm_output = trace.llm_output.unwrap_or_default();
    assert!(llm_output.contains("module_call"));
    assert!(llm_output.contains("move_agent"));
    let diagnostics = trace.llm_diagnostics.expect("diagnostics");
    assert_eq!(diagnostics.prompt_tokens, Some(24));
    assert_eq!(diagnostics.completion_tokens, Some(8));
    assert_eq!(diagnostics.total_tokens, Some(32));
}

#[test]
fn llm_agent_collapses_multi_json_output_to_terminal_decision_without_repair() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"type":"module_call","module":"agent.modules.list","args":{}}

---

{"decision":"move_agent","to":"loc-2"}"#
                .to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        })
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace.parse_error.is_none());
    assert_eq!(trace.llm_effect_intents.len(), 0);
    assert_eq!(trace.llm_effect_receipts.len(), 0);
    assert_eq!(
        trace.llm_diagnostics.expect("diagnostics").retry_count,
        0,
        "collapsed output should not consume repair rounds"
    );
    assert!(trace.llm_chat_messages.iter().any(|msg| msg
        .content
        .contains("multi-turn output collapsed by guardrail")));
}
