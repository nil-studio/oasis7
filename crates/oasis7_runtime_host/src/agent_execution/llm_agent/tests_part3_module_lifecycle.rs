use super::*;
use oasis7_agent_api::world_action::ModuleInstallTarget;

#[test]
fn decision_tool_schema_includes_module_lifecycle_actions_and_fields() {
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
        .expect("decision enum")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();

    assert!(decision_enum.contains(&"compile_module_artifact_from_source"));
    assert!(decision_enum.contains(&"deploy_module_artifact"));
    assert!(decision_enum.contains(&"install_module_from_artifact"));
    assert!(decision_enum.contains(&"install_module_to_target_from_artifact"));
    assert!(decision_enum.contains(&"list_module_artifact_for_sale"));
    assert!(decision_enum.contains(&"buy_module_artifact"));
    assert!(decision_enum.contains(&"delist_module_artifact"));
    assert!(decision_enum.contains(&"destroy_module_artifact"));
    assert!(decision_enum.contains(&"place_module_artifact_bid"));
    assert!(decision_enum.contains(&"cancel_module_artifact_bid"));

    let properties = parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .expect("decision properties");
    assert!(properties.contains_key("manifest_path"));
    assert!(properties.contains_key("source_files"));
    assert!(properties.contains_key("wasm_hash"));
    assert!(properties.contains_key("wasm_bytes_hex"));
    assert!(properties.contains_key("module_version"));
    assert!(properties.contains_key("activate"));
    assert!(properties.contains_key("install_target_type"));
    assert!(properties.contains_key("install_target_location_id"));
    assert!(properties.contains_key("price_kind"));
    assert!(properties.contains_key("price_amount"));
    assert!(properties.contains_key("bid_order_id"));
    assert!(properties.contains_key("bidder"));
}

#[test]
fn llm_parse_compile_module_artifact_from_source_action() {
    let turns = completion_turns_from_output(
        r#"{"decision":"compile_module_artifact_from_source","publisher":"self","module_id":"m.llm.compile","manifest_path":"Cargo.toml","source_files":{"Cargo.toml":"cargo-content","src/lib.rs":"lib-content"}}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Decision {
            decision:
                AgentDecision::Act(Action::CompileModuleArtifactFromSource {
                    publisher_agent_id,
                    module_id,
                    manifest_path,
                    source_files,
                }),
            ..
        } => {
            assert_eq!(publisher_agent_id, "agent-1");
            assert_eq!(module_id, "m.llm.compile");
            assert_eq!(manifest_path, "Cargo.toml");
            assert!(source_files.contains_key("Cargo.toml"));
            assert!(source_files.contains_key("src/lib.rs"));
            assert_eq!(
                source_files
                    .get("src/lib.rs")
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                    .as_deref(),
                Some("lib-content")
            );
        }
        other => panic!("unexpected parsed turn: {other:?}"),
    }
}

#[test]
fn llm_parse_deploy_module_artifact_rejects_invalid_hex_bytes() {
    let turns = completion_turns_from_output(
        r#"{"decision":"deploy_module_artifact","publisher":"self","wasm_hash":"abc","wasm_bytes_hex":"not-hex"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Invalid(message) => {
            assert!(
                message.contains("wasm_bytes_hex") && message.contains("valid hex"),
                "unexpected parse error: {message}"
            );
        }
        other => panic!("expected invalid decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_install_module_from_artifact_defaults_version_and_activate() {
    let turns = completion_turns_from_output(
        r#"{"decision":"install_module_from_artifact","installer":"self","module_id":"m.llm.install","wasm_hash":"abcd"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Decision {
            decision:
                AgentDecision::Act(Action::InstallModuleFromArtifact {
                    installer_agent_id,
                    module_id,
                    module_version,
                    wasm_hash,
                    activate,
                }),
            ..
        } => {
            assert_eq!(installer_agent_id, "agent-1");
            assert_eq!(module_id, "m.llm.install");
            assert_eq!(module_version, "0.1.0");
            assert_eq!(wasm_hash, "abcd");
            assert!(*activate);
        }
        other => panic!("unexpected parsed turn: {other:?}"),
    }
}

#[test]
fn llm_parse_install_module_to_target_from_artifact_action() {
    let turns = completion_turns_from_output(
        r#"{"decision":"install_module_to_target_from_artifact","installer":"self","module_id":"m.llm.install.target","module_version":"0.2.0","wasm_hash":"hash-target","activate":false,"install_target_type":"location_infrastructure","install_target_location_id":"loc-hub"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Decision {
            decision:
                AgentDecision::Act(Action::InstallModuleToTargetFromArtifact {
                    installer_agent_id,
                    module_id,
                    module_version,
                    wasm_hash,
                    activate,
                    install_target,
                }),
            ..
        } => {
            assert_eq!(installer_agent_id, "agent-1");
            assert_eq!(module_id, "m.llm.install.target");
            assert_eq!(module_version, "0.2.0");
            assert_eq!(wasm_hash, "hash-target");
            assert!(!*activate);
            assert_eq!(
                install_target,
                &ModuleInstallTarget::LocationInfrastructure {
                    location_id: "loc-hub".to_string(),
                }
            );
        }
        other => panic!("unexpected parsed turn: {other:?}"),
    }
}

#[test]
fn llm_parse_install_module_to_target_from_artifact_rejects_missing_location_id() {
    let turns = completion_turns_from_output(
        r#"{"decision":"install_module_to_target_from_artifact","installer":"self","module_id":"m.llm.install.target","wasm_hash":"hash-target","install_target_type":"location_infrastructure"}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Invalid(message) => {
            assert!(
                message.contains("install_target_location_id"),
                "unexpected parse error: {message}"
            );
        }
        other => panic!("expected invalid decision, got {other:?}"),
    }
}

#[test]
fn llm_parse_list_module_artifact_for_sale_action() {
    let turns = completion_turns_from_output(
        r#"{"decision":"list_module_artifact_for_sale","seller":"self","wasm_hash":"hash-1","price_kind":"data","price_amount":3}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Decision {
            decision:
                AgentDecision::Act(Action::ListModuleArtifactForSale {
                    seller_agent_id,
                    wasm_hash,
                    price_kind,
                    price_amount,
                }),
            ..
        } => {
            assert_eq!(seller_agent_id, "agent-1");
            assert_eq!(wasm_hash, "hash-1");
            assert_eq!(*price_kind, ResourceKind::Data);
            assert_eq!(*price_amount, 3);
        }
        other => panic!("unexpected parsed turn: {other:?}"),
    }
}

#[test]
fn llm_parse_cancel_module_artifact_bid_rejects_non_agent_bidder() {
    let turns = completion_turns_from_output(
        r#"{"decision":"cancel_module_artifact_bid","bidder":"location:loc-a","wasm_hash":"hash-1","bid_order_id":7}"#,
    );
    let parsed = super::decision_flow::parse_llm_turn_payloads(turns.as_slice(), "agent-1");

    match parsed.first().expect("parsed turn") {
        ParsedLlmTurn::Invalid(message) => {
            assert!(
                message.contains("self or agent:<id>"),
                "unexpected parse error: {message}"
            );
        }
        other => panic!("expected invalid decision, got {other:?}"),
    }
}

#[test]
fn llm_agent_module_lifecycle_status_module_reads_observation_snapshot() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    // Seed behavior cache with stale records and verify query output still follows observation.
    let stale_action = Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: "stale-hash".to_string(),
        wasm_bytes: vec![0x00, 0x61, 0x73, 0x6d],
        module_id_hint: Some("m.llm.lifecycle".to_string()),
    };
    behavior.on_action_result(&ActionResult {
        action: stale_action.clone(),
        action_id: 1,
        success: true,
        event: WorldEvent {
            id: 1,
            time: 10,
            kind: WorldEventKind::ModuleArtifactDeployed {
                publisher_agent_id: "agent-1".to_string(),
                wasm_hash: "stale-hash".to_string(),
                wasm_bytes: vec![0x00, 0x61, 0x73, 0x6d],
                bytes_len: 4,
                module_id_hint: Some("m.llm.lifecycle".to_string()),
            },
            runtime_event: None,
        },
    });
    behavior.on_action_result(&ActionResult {
        action: Action::InstallModuleFromArtifact {
            installer_agent_id: "agent-1".to_string(),
            module_id: "m.llm.lifecycle".to_string(),
            module_version: "0.1.0".to_string(),
            wasm_hash: "stale-hash".to_string(),
            activate: true,
        },
        action_id: 2,
        success: true,
        event: WorldEvent {
            id: 2,
            time: 11,
            kind: WorldEventKind::ModuleInstalled {
                installer_agent_id: "agent-1".to_string(),
                module_id: "m.llm.lifecycle".to_string(),
                module_version: "0.1.0".to_string(),
                wasm_hash: "stale-hash".to_string(),
                active: true,
                install_target: ModuleInstallTarget::SelfAgent,
            },
            runtime_event: None,
        },
    });

    let mut observation = make_observation();
    observation.module_lifecycle = oasis7_simulator::ObservedModuleLifecycleState {
        artifacts: vec![
            oasis7_simulator::ObservedModuleArtifactRecord {
                wasm_hash: "hash-live-a".to_string(),
                publisher_agent_id: "agent-1".to_string(),
                module_id_hint: Some("m.llm.lifecycle".to_string()),
                bytes_len: 128,
                deployed_at_tick: 42,
            },
            oasis7_simulator::ObservedModuleArtifactRecord {
                wasm_hash: "hash-other".to_string(),
                publisher_agent_id: "agent-2".to_string(),
                module_id_hint: Some("m.other".to_string()),
                bytes_len: 64,
                deployed_at_tick: 43,
            },
            oasis7_simulator::ObservedModuleArtifactRecord {
                wasm_hash: "hash-live-b".to_string(),
                publisher_agent_id: "agent-3".to_string(),
                module_id_hint: Some("m.llm.lifecycle".to_string()),
                bytes_len: 256,
                deployed_at_tick: 44,
            },
        ],
        installed_modules: vec![
            oasis7_simulator::InstalledModuleState {
                module_id: "m.llm.lifecycle".to_string(),
                module_version: "0.2.0".to_string(),
                wasm_hash: "hash-live-a".to_string(),
                installer_agent_id: "agent-1".to_string(),
                install_target: ModuleInstallTarget::SelfAgent,
                active: true,
                installed_at_tick: 43,
            },
            oasis7_simulator::InstalledModuleState {
                module_id: "m.other".to_string(),
                module_version: "0.1.0".to_string(),
                wasm_hash: "hash-other".to_string(),
                installer_agent_id: "agent-2".to_string(),
                install_target: ModuleInstallTarget::SelfAgent,
                active: true,
                installed_at_tick: 44,
            },
            oasis7_simulator::InstalledModuleState {
                module_id: "m.llm.lifecycle".to_string(),
                module_version: "0.3.0".to_string(),
                wasm_hash: "hash-live-b".to_string(),
                installer_agent_id: "agent-3".to_string(),
                install_target: ModuleInstallTarget::SelfAgent,
                active: false,
                installed_at_tick: 45,
            },
        ],
    };
    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "module.lifecycle.status".to_string(),
            args: serde_json::json!({
                "module_id": "m.llm.lifecycle",
                "limit_artifacts": 1,
                "limit_installed": 1
            }),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let status = result
        .get("result")
        .expect("module lifecycle status result");
    assert_eq!(
        status
            .get("artifacts_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    let artifacts = status
        .get("artifacts")
        .and_then(|value| value.as_array())
        .expect("artifacts array");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(
        artifacts[0]
            .get("wasm_hash")
            .and_then(|value| value.as_str()),
        Some("hash-live-a")
    );

    assert_eq!(
        status
            .get("installed_modules_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    let installed = status
        .get("installed_modules")
        .and_then(|value| value.as_array())
        .expect("installed modules array");
    assert_eq!(installed.len(), 1);
    assert_eq!(
        installed[0]
            .get("module_id")
            .and_then(|value| value.as_str()),
        Some("m.llm.lifecycle")
    );
    assert_eq!(
        installed[0]
            .get("install_target")
            .and_then(|value| value.get("type"))
            .and_then(|value| value.as_str()),
        Some("self_agent")
    );
    assert_eq!(
        installed[0]
            .get("wasm_hash")
            .and_then(|value| value.as_str()),
        Some("hash-live-a")
    );
}

#[test]
fn llm_agent_prompt_mentions_module_lifecycle_decisions() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);

    assert!(prompt.contains("compile_module_artifact_from_source"));
    assert!(prompt.contains("deploy_module_artifact"));
    assert!(prompt.contains("install_module_from_artifact"));
    assert!(prompt.contains("install_module_to_target_from_artifact"));
    assert!(prompt.contains("list_module_artifact_for_sale"));
    assert!(prompt.contains("buy_module_artifact"));
    assert!(prompt.contains("place_module_artifact_bid"));
    assert!(prompt.contains("cancel_module_artifact_bid"));
    assert!(prompt.contains("module.lifecycle.status"));
}

#[test]
fn llm_agent_oasis7_rules_guide_module_returns_stage_playbook() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let observation = make_observation();

    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "world.rules.guide".to_string(),
            args: serde_json::json!({ "topic": "industry" }),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let guide = result
        .get("result")
        .and_then(|value| value.get("guide"))
        .expect("world rules guide result");
    assert_eq!(
        result
            .get("result")
            .and_then(|value| value.get("topic"))
            .and_then(|value| value.as_str()),
        Some("industry")
    );
    assert_eq!(
        guide
            .get("goal")
            .and_then(|value| value.as_str())
            .unwrap_or_default(),
        "形成工业闭环（采矿 -> 精炼 -> 建厂 -> 排产）"
    );
}

#[test]
fn llm_agent_current_observation_module_exposes_build_ready_context() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let mut observation = make_observation();
    observation.visible_locations = vec![ObservedLocation {
        location_id: "loc-build".to_string(),
        name: "build-site".to_string(),
        pos: GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        profile: Default::default(),
        distance_cm: 0,
    }];

    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "environment.current_observation".to_string(),
            args: serde_json::json!({}),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let module_result = result.get("result").expect("current observation result");
    assert_eq!(
        module_result
            .get("current_location_id")
            .and_then(|value| value.as_str()),
        Some("loc-build")
    );
    assert_eq!(
        module_result
            .get("can_build_factory_smelter_mk1_now")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        module_result
            .get("missing_build_prerequisites")
            .and_then(|value| value.as_array())
            .map(|items| items.len()),
        Some(0)
    );
    assert_eq!(
        module_result
            .get("recommended_build_factory_action")
            .and_then(|value| value.get("decision"))
            .and_then(|value| value.as_str()),
        Some("build_factory")
    );
    assert_eq!(
        module_result
            .get("recommended_build_factory_action")
            .and_then(|value| value.get("location_id"))
            .and_then(|value| value.as_str()),
        Some("loc-build")
    );
    assert_eq!(
        module_result
            .get("factory_build_costs_default")
            .and_then(|value| value.get("electricity"))
            .and_then(|value| value.as_i64()),
        Some(10)
    );
}

#[test]
fn llm_agent_power_order_book_status_module_reads_snapshot_with_limit() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let mut observation = make_observation();
    observation.power_market = oasis7_simulator::ObservedPowerMarketState {
        next_order_id: 9,
        open_orders: vec![
            oasis7_simulator::PowerOrderState {
                order_id: 3,
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                side: PowerOrderSide::Buy,
                remaining_amount: 10,
                limit_price_per_pu: 2,
                created_at: 30,
            },
            oasis7_simulator::PowerOrderState {
                order_id: 4,
                owner: ResourceOwner::Agent {
                    agent_id: "agent-2".to_string(),
                },
                side: PowerOrderSide::Sell,
                remaining_amount: 8,
                limit_price_per_pu: 3,
                created_at: 31,
            },
        ],
    };

    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "power.order_book.status".to_string(),
            args: serde_json::json!({ "limit_orders": 1 }),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let status = result
        .get("result")
        .expect("power order book module result");
    assert_eq!(
        status.get("next_order_id").and_then(|value| value.as_u64()),
        Some(9)
    );
    assert_eq!(
        status
            .get("open_orders_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    let open_orders = status
        .get("open_orders")
        .and_then(|value| value.as_array())
        .expect("open orders array");
    assert_eq!(open_orders.len(), 1);
    assert_eq!(
        open_orders[0]
            .get("order_id")
            .and_then(|value| value.as_u64()),
        Some(3)
    );
}

#[test]
fn llm_agent_module_market_status_module_filters_wasm_hash() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let mut observation = make_observation();
    observation.module_market = oasis7_simulator::ObservedModuleMarketState {
        listings: vec![
            oasis7_simulator::ModuleArtifactListingState {
                order_id: 11,
                wasm_hash: "hash-a".to_string(),
                seller_agent_id: "agent-1".to_string(),
                price_kind: ResourceKind::Data,
                price_amount: 7,
                listed_at_tick: 15,
            },
            oasis7_simulator::ModuleArtifactListingState {
                order_id: 12,
                wasm_hash: "hash-b".to_string(),
                seller_agent_id: "agent-2".to_string(),
                price_kind: ResourceKind::Electricity,
                price_amount: 5,
                listed_at_tick: 16,
            },
            oasis7_simulator::ModuleArtifactListingState {
                order_id: 15,
                wasm_hash: "hash-b".to_string(),
                seller_agent_id: "agent-5".to_string(),
                price_kind: ResourceKind::Data,
                price_amount: 11,
                listed_at_tick: 19,
            },
        ],
        bids: vec![
            oasis7_simulator::ModuleArtifactBidState {
                order_id: 13,
                wasm_hash: "hash-a".to_string(),
                bidder_agent_id: "agent-3".to_string(),
                price_kind: ResourceKind::Data,
                price_amount: 4,
                placed_at_tick: 17,
            },
            oasis7_simulator::ModuleArtifactBidState {
                order_id: 14,
                wasm_hash: "hash-b".to_string(),
                bidder_agent_id: "agent-4".to_string(),
                price_kind: ResourceKind::Electricity,
                price_amount: 9,
                placed_at_tick: 18,
            },
            oasis7_simulator::ModuleArtifactBidState {
                order_id: 16,
                wasm_hash: "hash-b".to_string(),
                bidder_agent_id: "agent-6".to_string(),
                price_kind: ResourceKind::Data,
                price_amount: 12,
                placed_at_tick: 20,
            },
        ],
    };

    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "module.market.status".to_string(),
            args: serde_json::json!({
                "wasm_hash": "hash-b",
                "limit_listings": 1,
                "limit_bids": 1
            }),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let status = result.get("result").expect("module market status result");
    assert_eq!(
        status
            .get("listings_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        status.get("bids_total").and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        status
            .get("listings")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        status
            .get("listings")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("wasm_hash"))
            .and_then(|value| value.as_str()),
        Some("hash-b")
    );
    assert_eq!(
        status
            .get("listings")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("order_id"))
            .and_then(|value| value.as_u64()),
        Some(12)
    );
    assert_eq!(
        status
            .get("bids")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        status
            .get("bids")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("wasm_hash"))
            .and_then(|value| value.as_str()),
        Some("hash-b")
    );
    assert_eq!(
        status
            .get("bids")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("order_id"))
            .and_then(|value| value.as_u64()),
        Some(14)
    );
}

#[test]
fn llm_agent_social_state_status_module_respects_include_inactive_flag() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let mut observation = make_observation();
    observation.social_state = oasis7_simulator::ObservedSocialState {
        facts: vec![
            oasis7_simulator::SocialFactState {
                fact_id: 1,
                actor: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                schema_id: "s.alliance".to_string(),
                subject: ResourceOwner::Agent {
                    agent_id: "agent-2".to_string(),
                },
                object: None,
                claim: "allied".to_string(),
                confidence_ppm: 900_000,
                evidence_event_ids: vec![7],
                ttl_ticks: None,
                expires_at_tick: None,
                stake: None,
                challenge: None,
                lifecycle: oasis7_simulator::SocialFactLifecycleState::Active,
                created_at_tick: 20,
                updated_at_tick: 20,
            },
            oasis7_simulator::SocialFactState {
                fact_id: 2,
                actor: ResourceOwner::Agent {
                    agent_id: "agent-3".to_string(),
                },
                schema_id: "s.risk".to_string(),
                subject: ResourceOwner::Agent {
                    agent_id: "agent-4".to_string(),
                },
                object: None,
                claim: "risk".to_string(),
                confidence_ppm: 400_000,
                evidence_event_ids: vec![8],
                ttl_ticks: None,
                expires_at_tick: None,
                stake: None,
                challenge: None,
                lifecycle: oasis7_simulator::SocialFactLifecycleState::Revoked,
                created_at_tick: 21,
                updated_at_tick: 21,
            },
        ],
        edges: vec![
            oasis7_simulator::SocialEdgeState {
                edge_id: 10,
                declarer: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                schema_id: "s.network".to_string(),
                relation_kind: "ally".to_string(),
                from: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                to: ResourceOwner::Agent {
                    agent_id: "agent-2".to_string(),
                },
                weight_bps: 5_000,
                backing_fact_ids: vec![1],
                ttl_ticks: None,
                expires_at_tick: None,
                lifecycle: oasis7_simulator::SocialEdgeLifecycleState::Active,
                created_at_tick: 22,
                updated_at_tick: 22,
            },
            oasis7_simulator::SocialEdgeState {
                edge_id: 11,
                declarer: ResourceOwner::Agent {
                    agent_id: "agent-3".to_string(),
                },
                schema_id: "s.network".to_string(),
                relation_kind: "former-ally".to_string(),
                from: ResourceOwner::Agent {
                    agent_id: "agent-3".to_string(),
                },
                to: ResourceOwner::Agent {
                    agent_id: "agent-4".to_string(),
                },
                weight_bps: 2_500,
                backing_fact_ids: vec![2],
                ttl_ticks: None,
                expires_at_tick: None,
                lifecycle: oasis7_simulator::SocialEdgeLifecycleState::Expired,
                created_at_tick: 23,
                updated_at_tick: 23,
            },
        ],
    };

    let result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "social.state.status".to_string(),
            args: serde_json::json!({
                "include_inactive": false,
                "limit_facts": 8,
                "limit_edges": 8
            }),
        },
        &observation,
    );

    assert_eq!(
        result.get("ok").and_then(|value| value.as_bool()),
        Some(true)
    );
    let status = result.get("result").expect("social state result");
    assert_eq!(
        status.get("facts_total").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        status.get("edges_total").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        status
            .get("facts")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("fact_id"))
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        status
            .get("edges")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("edge_id"))
            .and_then(|value| value.as_u64()),
        Some(10)
    );

    let limited_all_result = behavior.run_prompt_module(
        &LlmModuleCallRequest {
            module: "social.state.status".to_string(),
            args: serde_json::json!({
                "include_inactive": true,
                "limit_facts": 1,
                "limit_edges": 1
            }),
        },
        &observation,
    );
    let limited_all_status = limited_all_result
        .get("result")
        .expect("limited social state result");
    assert_eq!(
        limited_all_status
            .get("facts_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        limited_all_status
            .get("facts")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        limited_all_status
            .get("edges_total")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        limited_all_status
            .get("edges")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
}
