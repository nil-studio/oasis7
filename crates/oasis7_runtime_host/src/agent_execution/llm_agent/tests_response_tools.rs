#[test]
fn responses_tools_preserve_synchronous_wire_contract() {
    for tool in responses_tools_with_debug_mode(true) {
        let value = serde_json::to_value(tool).expect("serialize response tool");
        assert_eq!(value["type"], "function");
        for field in ["async", "output_schema", "allowed_callers"] {
            assert!(value.get(field).is_none(), "unexpected optional field {field}");
        }
    }
}

#[test]
fn responses_tools_register_expected_function_names() {
    let tools = responses_tools();
    assert_eq!(tools.len(), 10);

    let names = tools
        .into_iter()
        .filter_map(|tool| match tool {
            Tool::Function(function_tool) => Some(function_tool.name),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            OPENAI_TOOL_AGENT_MODULES_LIST.to_string(),
            OPENAI_TOOL_ENVIRONMENT_CURRENT_OBSERVATION.to_string(),
            OPENAI_TOOL_MEMORY_SHORT_TERM_RECENT.to_string(),
            OPENAI_TOOL_MEMORY_LONG_TERM_SEARCH.to_string(),
            OPENAI_TOOL_WORLD_RULES_GUIDE.to_string(),
            OPENAI_TOOL_MODULE_LIFECYCLE_STATUS.to_string(),
            OPENAI_TOOL_POWER_ORDER_BOOK_STATUS.to_string(),
            OPENAI_TOOL_MODULE_MARKET_STATUS.to_string(),
            OPENAI_TOOL_SOCIAL_STATE_STATUS.to_string(),
            OPENAI_TOOL_AGENT_SUBMIT_DECISION.to_string(),
        ]
    );
}

#[test]
fn responses_tools_register_debug_grant_tool_in_debug_mode_only() {
    let normal = responses_tools_with_debug_mode(false);
    assert_eq!(normal.len(), 10);
    let normal_names = normal
        .into_iter()
        .filter_map(|tool| match tool {
            Tool::Function(function_tool) => Some(function_tool.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!normal_names.contains(&OPENAI_TOOL_AGENT_DEBUG_GRANT_RESOURCE.to_string()));

    let debug = responses_tools_with_debug_mode(true);
    assert_eq!(debug.len(), 11);
    let debug_names = debug
        .into_iter()
        .filter_map(|tool| match tool {
            Tool::Function(function_tool) => Some(function_tool.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(debug_names.contains(&OPENAI_TOOL_AGENT_DEBUG_GRANT_RESOURCE.to_string()));
}

#[test]
fn response_function_call_maps_to_typed_module_call_turn() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"limit\":5}".to_string(),
        call_id: "call_1".to_string(),
        name: OPENAI_TOOL_MEMORY_SHORT_TERM_RECENT.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "memory.short_term.recent");
            assert_eq!(args.get("limit").and_then(|v| v.as_i64()), Some(5));
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_module_lifecycle_status_tool_name() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{}".to_string(),
        call_id: "call_lifecycle".to_string(),
        name: OPENAI_TOOL_MODULE_LIFECYCLE_STATUS.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "module.lifecycle.status");
            assert_eq!(args, serde_json::json!({}));
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_world_rules_guide_tool_name() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"topic\":\"industry\"}".to_string(),
        call_id: "call_world_rules".to_string(),
        name: OPENAI_TOOL_WORLD_RULES_GUIDE.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "world.rules.guide");
            assert_eq!(
                args.get("topic").and_then(|value| value.as_str()),
                Some("industry")
            );
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_power_order_book_status_tool_name() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"limit_orders\":12}".to_string(),
        call_id: "call_order_book".to_string(),
        name: OPENAI_TOOL_POWER_ORDER_BOOK_STATUS.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "power.order_book.status");
            assert_eq!(
                args.get("limit_orders").and_then(|value| value.as_i64()),
                Some(12)
            );
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_module_market_status_tool_name() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"wasm_hash\":\"hash-1\",\"limit_listings\":6}".to_string(),
        call_id: "call_module_market".to_string(),
        name: OPENAI_TOOL_MODULE_MARKET_STATUS.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "module.market.status");
            assert_eq!(
                args.get("wasm_hash").and_then(|value| value.as_str()),
                Some("hash-1")
            );
            assert_eq!(
                args.get("limit_listings").and_then(|value| value.as_i64()),
                Some(6)
            );
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_social_state_status_tool_name() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"include_inactive\":false,\"limit_facts\":5}".to_string(),
        call_id: "call_social_state".to_string(),
        name: OPENAI_TOOL_SOCIAL_STATE_STATUS.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { module, args } => {
            assert_eq!(module, "social.state.status");
            assert_eq!(
                args.get("include_inactive")
                    .and_then(|value| value.as_bool()),
                Some(false)
            );
            assert_eq!(
                args.get("limit_facts").and_then(|value| value.as_i64()),
                Some(5)
            );
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn responses_tools_module_lifecycle_schema_declares_filter_and_limits() {
    let lifecycle = responses_tools()
        .into_iter()
        .find_map(|tool| match tool {
            Tool::Function(function_tool)
                if function_tool.name == OPENAI_TOOL_MODULE_LIFECYCLE_STATUS =>
            {
                Some(function_tool)
            }
            _ => None,
        })
        .expect("module lifecycle tool exists");
    let parameters = lifecycle.parameters.expect("module lifecycle parameters");
    let properties = parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("module lifecycle properties");
    assert!(properties.contains_key("module_id"));
    assert!(properties.contains_key("limit_artifacts"));
    assert!(properties.contains_key("limit_installed"));
}

#[test]
fn response_function_call_invalid_json_arguments_are_preserved_as_raw() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "not-json".to_string(),
        call_id: "call_2".to_string(),
        name: OPENAI_TOOL_AGENT_MODULES_LIST.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("module_call turn");
    match turn {
        LlmCompletionTurn::ModuleCall { args, .. } => {
            assert_eq!(
                args.get("_raw").and_then(|value| value.as_str()),
                Some("not-json")
            );
        }
        other => panic!("expected module_call turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_decision_tool_to_typed_decision_turn() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"decision\":\"wait_ticks\",\"ticks\":2}".to_string(),
        call_id: "call_decision".to_string(),
        name: OPENAI_TOOL_AGENT_SUBMIT_DECISION.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("decision turn");
    match turn {
        LlmCompletionTurn::Decision { payload } => {
            assert_eq!(
                payload.get("decision").and_then(|value| value.as_str()),
                Some("wait_ticks")
            );
            assert_eq!(
                payload.get("ticks").and_then(|value| value.as_i64()),
                Some(2)
            );
        }
        other => panic!("expected decision turn, got {other:?}"),
    }
}

#[test]
fn response_function_call_maps_debug_grant_tool_to_typed_decision_turn() {
    let output_item = OutputItem::FunctionCall(async_openai::types::responses::FunctionToolCall {
        arguments: "{\"owner\":\"self\",\"kind\":\"data\",\"amount\":3}".to_string(),
        call_id: "call_debug".to_string(),
        name: OPENAI_TOOL_AGENT_DEBUG_GRANT_RESOURCE.to_string(),
        id: None,
        status: None,
        namespace: None,
        caller: None,
        r#async: None,
    });

    let turn = output_item_to_completion_turn(&output_item).expect("decision turn");
    match turn {
        LlmCompletionTurn::Decision { payload } => {
            assert_eq!(
                payload.get("decision").and_then(|value| value.as_str()),
                Some("debug_grant_resource")
            );
            assert_eq!(
                payload.get("kind").and_then(|value| value.as_str()),
                Some("data")
            );
            assert_eq!(
                payload.get("amount").and_then(|value| value.as_i64()),
                Some(3)
            );
        }
        other => panic!("expected decision turn, got {other:?}"),
    }
}
