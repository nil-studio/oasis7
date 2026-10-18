use super::*;
use crate::simulator::llm_agent::prompt_assembly::PromptSectionKind;

fn seed_known_factory<C: LlmCompletionClient>(
    behavior: &mut LlmAgentBehavior<C>,
    factory_id: &str,
    factory_kind: &str,
    location_id: &str,
) {
    behavior.on_action_result(&ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: location_id.to_string(),
            factory_id: factory_id.to_string(),
            factory_kind: factory_kind.to_string(),
        },
        action_id: 9_100,
        success: true,
        event: WorldEvent {
            id: 9_101,
            time: 9_102,
            kind: WorldEventKind::FactoryBuilt {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: location_id.to_string(),
                factory_id: factory_id.to_string(),
                factory_kind: factory_kind.to_string(),
                electricity_cost: 8,
                hardware_cost: 4,
            },
            runtime_event: None,
        },
    });
}

#[test]
fn llm_agent_repair_round_can_recover_invalid_output() {
    let client = SequenceMockClient::new(vec![
        "not-json".to_string(),
        r#"{"decision":"wait_ticks","ticks":2}"#.to_string(),
    ]);
    let mut config = base_config();
    config.max_repair_rounds = 1;
    config.max_decision_steps = 4;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::WaitTicks(2));

    let trace = behavior.take_decision_trace().expect("trace exists");
    let diagnostics = trace.llm_diagnostics.expect("diagnostics");
    assert_eq!(diagnostics.retry_count, 1);
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "repair")
    );
}

#[test]
fn llm_agent_long_run_stress_keeps_pipeline_stable() {
    const TICKS: usize = 240;
    let calls = Arc::new(AtomicUsize::new(0));
    let client = StressMockClient::new(Arc::clone(&calls));

    let mut config = base_config();
    config.max_decision_steps = 6;
    config.max_module_calls = 2;
    config.max_repair_rounds = 1;
    config.prompt_max_history_items = 2;
    config.prompt_profile = LlmPromptProfile::Compact;
    config.execute_until_auto_reenter_ticks = 0;

    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    for tick in 0..TICKS {
        let time = 10_000 + tick as u64;
        behavior
            .memory
            .record_note(time, format!("stress-note-{tick}-{}", "x".repeat(180)));
        let observation = make_dense_observation(time, 8);

        let decision = behavior.decide(&observation);
        assert!(matches!(
            decision,
            AgentDecision::Act(Action::MoveAgent { .. })
                | AgentDecision::Wait
                | AgentDecision::WaitTicks(_)
        ));

        let trace = behavior.take_decision_trace().expect("trace exists");
        assert!(trace.llm_error.is_none());
        if let Some(parse_error) = trace.parse_error.as_deref() {
            assert!(
                parse_error.contains("deprecated in dialogue mode")
                    || parse_error.contains("no terminal decision")
                    || parse_error.contains("no actionable")
                    || parse_error.contains("replan guard requires"),
                "unexpected parse_error: {parse_error}"
            );
        }
        assert!(trace.llm_effect_intents.len() <= 1);
        assert_eq!(
            trace.llm_effect_receipts.len(),
            trace.llm_effect_intents.len()
        );
        assert!(
            trace
                .llm_step_trace
                .iter()
                .any(|step| step.step_type == "dialogue_turn" || step.step_type == "repair")
        );
        assert!(!trace.llm_prompt_section_trace.is_empty());
        let input_len = trace.llm_input.unwrap_or_default().len();
        assert!(input_len < 120_000, "llm_input too large: {input_len}");
        assert!(
            trace
                .llm_diagnostics
                .as_ref()
                .map(|diagnostics| diagnostics.retry_count)
                .unwrap_or_default()
                <= 1
        );
    }

    let total_calls = calls.load(Ordering::SeqCst);
    assert!(total_calls >= TICKS * 2);
    assert!(total_calls <= TICKS * 4);
}

#[test]
fn llm_agent_limits_module_call_rounds() {
    let client = SequenceMockClient::new(vec![
        "{\"type\":\"module_call\",\"module\":\"agent.modules.list\",\"args\":{}}".to_string(),
        "{\"type\":\"module_call\",\"module\":\"agent.modules.list\",\"args\":{}}".to_string(),
    ]);

    let mut config = base_config();
    config.max_module_calls = 1;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert_eq!(trace.llm_effect_intents.len(), 1);
    assert_eq!(trace.llm_effect_receipts.len(), 1);
}

fn cognition_request_context_with_budget(
    max_model_calls: u32,
    max_tool_calls: u32,
) -> crate::simulator::ContinuousAgentRequestContextV1 {
    let request = serde_json::json!({
        "observation": {
            "agent_id": "agent-1",
            "world_time": 7,
            "observation": {
                "self_state": {"location_ref": "loc-1", "pose_hint": "origin"},
                "mission_context": {"goal_summary": "test"}
            },
            "action_catalog": [],
            "timeout_budget_ms": 1_000
        },
        "timeout_budget_ms": 1_000
    });
    serde_json::from_value(serde_json::json!({
        "base_decision_request": request,
        "context_discriminator": "oasis7.continuous-agent-context",
        "context_version": 1,
        "protocol_version": "continuous-agent-v1",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.1",
        "decision_request_id": "request.agent-1.1",
        "retry_seq": 1,
        "transport_attempt": 1,
        "agent_subject": "agent-1",
        "runtime_binding": {
            "world_id": "world-1",
            "branch_id": "main",
            "finality_epoch": 1,
            "finality_block_hash": format!("blake3:{}", "a".repeat(64)),
            "finality_status": "verified",
            "base_tick": 7,
            "base_world_hash": format!("blake3:{}", "b".repeat(64)),
            "reorg_epoch": 1,
            "runtime_manifest_hash": format!("blake3:{}", "c".repeat(64))
        },
        "observation_digest": format!("blake3:{}", "d".repeat(64)),
        "capability_catalog_digest": format!("blake3:{}", "e".repeat(64)),
        "capability_invocation_context_digest": format!("blake3:{}", "f".repeat(64)),
        "memory_snapshot_digest": format!("blake3:{}", "1".repeat(64)),
        "goal_snapshot_digest": format!("blake3:{}", "2".repeat(64)),
        "continuation_digest": format!("blake3:{}", "3".repeat(64)),
        "adapter_protocol_version": "loopback-http-v1",
        "budget_contract": {
            "max_latency_ms": 60_000,
            "max_repair_attempts": 1,
            "max_model_calls": max_model_calls,
            "max_tool_calls": max_tool_calls
        },
        "request_digest": format!("blake3:{}", "4".repeat(64))
    }))
    .expect("decode cognition request budget context")
}

#[test]
fn cognition_budget_zero_model_calls_denies_before_provider_and_effects() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![r#"{"decision":"move_agent","to":"loc-2"}"#.to_string()],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let context = cognition_request_context_with_budget(0, 1);
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&context));

    assert_eq!(behavior.decide(&make_observation()), AgentDecision::Wait);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let trace = behavior.take_decision_trace().expect("budget trace exists");
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace.llm_effect_receipts.is_empty());
    assert!(behavior.take_memory_write_intents().is_empty());
}

#[test]
fn cognition_budget_zero_tool_calls_denies_module_before_execution() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"type":"module_call","module":"agent.modules.list","args":{}}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let context = cognition_request_context_with_budget(2, 0);
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&context));

    assert_eq!(behavior.decide(&make_observation()), AgentDecision::Wait);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let trace = behavior.take_decision_trace().expect("budget trace exists");
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace.llm_effect_receipts.is_empty());
    assert!(behavior.take_memory_write_intents().is_empty());
}

#[test]
fn cognition_budget_counts_repair_calls_before_reentering_provider() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            "not-json".to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let context = cognition_request_context_with_budget(1, 1);
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&context));

    assert_eq!(behavior.decide(&make_observation()), AgentDecision::Wait);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let trace = behavior.take_decision_trace().expect("budget trace exists");
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace.llm_effect_receipts.is_empty());
}

#[test]
fn cognition_budget_does_not_charge_execute_until_fast_path() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"decision":"execute_until","action":{"decision":"harvest_radiation","max_amount":9},"until":{"event":"new_visible_agent|new_visible_location"},"max_ticks":3}"#.to_string(),
            r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
        ],
        Arc::clone(&calls),
    );
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);
    let context = cognition_request_context_with_budget(1, 1);
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&context));

    let mut observation = make_observation();
    observation.time = 20;
    assert!(matches!(
        behavior.decide(&observation),
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    observation.time = 21;
    assert!(matches!(
        behavior.decide(&observation),
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "continuation fast path did not invoke the model"
    );
}

#[test]
fn llm_agent_system_prompt_contains_configured_goals() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let system_prompt = behavior.system_prompt();
    assert!(system_prompt.contains("short-goal"));
    assert!(system_prompt.contains("long-goal"));
    assert!(system_prompt.contains("agent_submit_decision"));
}

#[test]
fn llm_agent_runtime_prompt_overrides_take_effect() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    behavior.apply_prompt_overrides(
        Some("runtime-system".to_string()),
        Some("runtime-short".to_string()),
        Some("runtime-long".to_string()),
    );

    let system_prompt = behavior.system_prompt();
    assert!(system_prompt.contains("runtime-system"));
    assert!(system_prompt.contains("runtime-short"));
    assert!(system_prompt.contains("runtime-long"));
    assert!(!system_prompt.contains("short-goal"));
    assert!(!system_prompt.contains("long-goal"));
}

#[test]
fn llm_agent_long_term_memory_can_export_and_restore() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    behavior
        .memory
        .long_term
        .store_with_tags("mined node alpha", 10, vec!["mining".to_string()]);
    behavior.memory.long_term.store_with_tags(
        "factory beta stalled",
        12,
        vec!["factory".to_string()],
    );

    let exported = behavior.export_long_term_memory_entries();
    assert_eq!(exported.len(), 2);

    let mut restored = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    restored.restore_long_term_memory_entries(&exported);
    let restored_exported = restored.export_long_term_memory_entries();
    assert_eq!(restored_exported.len(), 2);
    assert!(
        restored_exported
            .iter()
            .any(|entry| entry.content.contains("mined node alpha"))
    );
    assert!(
        restored_exported
            .iter()
            .any(|entry| entry.content.contains("factory beta stalled"))
    );
}

#[test]
fn llm_agent_user_prompt_omits_step_context_metadata() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let prompt = behavior.user_prompt(&make_observation(), &[], 2, 5);
    assert!(!prompt.contains("step_index"));
    assert!(!prompt.contains("max_steps"));
    assert!(!prompt.contains("module_calls_used"));
    assert!(!prompt.contains("module_calls_max"));
    assert!(prompt.contains("harvest_radiation"));
    assert!(prompt.contains("max_amount"));
    assert!(prompt.contains(format!("不超过 {}", DEFAULT_LLM_HARVEST_MAX_AMOUNT_CAP).as_str()));
}

#[test]
fn llm_agent_user_prompt_contains_failure_recovery_policy() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("[Failure Recovery Policy]"));
    assert!(prompt.contains("insufficient_resource.data -> mine_compound"));
    assert!(prompt.contains("insufficient_resource.electricity -> harvest_radiation"));
    assert!(prompt.contains("factory_not_found -> build_factory"));
    assert!(prompt.contains("location_not_found -> 仅使用 observation.visible_locations"));
    assert!(prompt.contains("rule_denied -> 检查 recipe_id 与 factory_kind 兼容关系"));
}

#[test]
fn llm_agent_user_prompt_includes_last_action_summary_after_feedback() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let action_result = ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-1".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 9,
        success: false,
        event: WorldEvent {
            id: 9,
            time: 11,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Agent {
                        agent_id: "agent-1".to_string(),
                    },
                    kind: ResourceKind::Data,
                    requested: 10,
                    available: 0,
                },
            },
            runtime_event: None,
        },
    };

    behavior.on_action_result(&action_result);
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("\"last_action\""));
    assert!(prompt.contains("\"kind\":\"build_factory\""));
    assert!(prompt.contains("\"success\":false"));
    assert!(prompt.contains("\"reject_reason\":\"insufficient_resource.data\""));
}

#[test]
fn llm_agent_user_prompt_preserves_facility_already_exists_reject_reason() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let action_result = ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc-1".to_string(),
            factory_id: "factory.alpha".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 10,
        success: false,
        event: WorldEvent {
            id: 10,
            time: 12,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::FacilityAlreadyExists {
                    facility_id: "factory.alpha".to_string(),
                },
            },
            runtime_event: None,
        },
    };

    behavior.on_action_result(&action_result);
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("\"last_action\""));
    assert!(prompt.contains("\"kind\":\"build_factory\""));
    assert!(prompt.contains("\"success\":false"));
    assert!(prompt.contains("\"reject_reason\":\"facility_already_exists\""));
    assert!(!prompt.contains("\"reject_reason\":\"other\""));
}

#[test]
fn llm_agent_user_prompt_preserves_rule_denied_reject_reason() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let action_result = ActionResult {
        action: Action::ScheduleRecipe {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            factory_id: "factory.power.radiation.mk1".to_string(),
            recipe_id: "recipe.assembler.control_chip".to_string(),
            batches: 1,
        },
        action_id: 11,
        success: false,
        event: WorldEvent {
            id: 11,
            time: 13,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["factory/recipe mismatch".to_string()],
                },
            },
            runtime_event: None,
        },
    };

    behavior.on_action_result(&action_result);
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("\"reject_reason\":\"rule_denied\""));
    assert!(!prompt.contains("\"reject_reason\":\"other\""));
}

#[test]
fn llm_agent_user_prompt_preserves_location_not_found_reject_reason() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let action_result = ActionResult {
        action: Action::BuildFactory {
            owner: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            location_id: "loc.missing".to_string(),
            factory_id: "factory.assembler.mk1".to_string(),
            factory_kind: "factory.assembler.mk1".to_string(),
        },
        action_id: 12,
        success: false,
        event: WorldEvent {
            id: 12,
            time: 14,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::LocationNotFound {
                    location_id: "loc.missing".to_string(),
                },
            },
            runtime_event: None,
        },
    };

    behavior.on_action_result(&action_result);
    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);
    assert!(prompt.contains("\"reject_reason\":\"location_not_found\""));
    assert!(!prompt.contains("\"reject_reason\":\"other\""));
}

#[test]
fn llm_agent_user_prompt_contains_memory_digest_section() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    behavior
        .memory
        .record_note(7, "recent-memory-note-for-prompt");

    let prompt_output = behavior.assemble_prompt_output(&make_observation(), &[], 0, 4);
    let memory_trace = prompt_output
        .section_trace
        .iter()
        .find(|trace| trace.kind == PromptSectionKind::Memory)
        .expect("memory section trace");
    assert!(memory_trace.estimated_tokens > 0);

    if memory_trace.included {
        assert!(prompt_output.user_prompt.contains("[Memory Digest]"));
        assert!(
            prompt_output
                .user_prompt
                .contains("recent-memory-note-for-prompt")
        );
    } else {
        assert!(!prompt_output.user_prompt.contains("[Memory Digest]"));
    }
}

#[test]
fn llm_agent_user_prompt_respects_history_item_cap() {
    let mut config = base_config();
    config.prompt_max_history_items = 2;
    let behavior = LlmAgentBehavior::new("agent-1", config, MockClient::default());

    let history = vec![
        ModuleCallExchange {
            module: "mod-a".to_string(),
            args: serde_json::json!({}),
            result: serde_json::json!({"ok": true}),
        },
        ModuleCallExchange {
            module: "mod-b".to_string(),
            args: serde_json::json!({}),
            result: serde_json::json!({"ok": true}),
        },
        ModuleCallExchange {
            module: "mod-c".to_string(),
            args: serde_json::json!({}),
            result: serde_json::json!({"ok": true}),
        },
    ];

    let history_start = history
        .len()
        .saturating_sub(behavior.config.prompt_max_history_items);
    let history_slice = &history[history_start..];
    let history_json =
        LlmAgentBehavior::<MockClient>::module_history_json_for_prompt(history_slice);
    assert!(!history_json.contains("mod-a"));
    assert!(history_json.contains("mod-b"));
    assert!(history_json.contains("mod-c"));

    let _ = behavior.user_prompt(&make_observation(), &history, 0, 4);
}

#[test]
fn llm_agent_compacts_large_module_result_payload_for_prompt_history() {
    let giant_payload = format!("payload-{}", "x".repeat(6000));
    let compact = LlmAgentBehavior::<MockClient>::module_result_for_prompt(&serde_json::json!({
        "ok": true,
        "module": "memory.short_term.recent",
        "result": [giant_payload.clone()],
    }));

    let compact_json = serde_json::to_string(&compact).expect("serialize compact result");
    assert!(compact_json.contains("\"truncated\":true"));
    assert!(compact_json.contains("\"original_chars\":"));
    assert!(!compact_json.contains(giant_payload.as_str()));
}

#[test]
fn llm_agent_compacts_dense_observation_for_prompt_context() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let observation = make_dense_observation(42, 40);

    let observation_json = behavior.observation_json_for_prompt(&observation);
    assert!(observation_json.contains("\"visible_agents_total\":41"));
    assert!(observation_json.contains("\"visible_agents_omitted\":"));
    assert!(observation_json.contains("\"visible_locations_total\":41"));
    assert!(observation_json.contains("\"visible_locations_omitted\":"));
    assert!(observation_json.contains("\"self_resources\""));
    assert!(observation_json.contains("\"electricity\":30"));
    assert!(!observation_json.contains("agent-extra-39"));
    assert!(!observation_json.contains("loc-extra-39"));
}

#[test]
fn llm_agent_compacts_large_module_args_payload_for_prompt_history() {
    let giant_query = format!("query-{}", "x".repeat(4_000));
    let history = vec![ModuleCallExchange {
        module: "memory.long_term.search".to_string(),
        args: serde_json::json!({"query": giant_query.clone()}),
        result: serde_json::json!({"ok": true}),
    }];

    let history_json = LlmAgentBehavior::<MockClient>::module_history_json_for_prompt(&history);
    assert!(history_json.contains("\"truncated\":true"));
    assert!(!history_json.contains(giant_query.as_str()));
}

#[test]
fn llm_agent_records_failed_action_into_long_term_memory() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let result = ActionResult {
        action: Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-x".to_string(),
        },
        action_id: 11,
        success: false,
        event: WorldEvent {
            id: 3,
            time: 9,
            kind: WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: "agent-1".to_string(),
                },
            },
            runtime_event: None,
        },
    };

    behavior.on_action_result(&result);

    assert!(!behavior.memory.long_term.is_empty());
    let failed = behavior.memory.long_term.search_by_tag("failed");
    assert!(!failed.is_empty());
}

#[test]
fn llm_agent_emits_parse_error_in_trace() {
    let client = MockClient {
        output: Some("not json".to_string()),
        err: None,
    };
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);

    let trace = behavior.take_decision_trace().expect("trace should exist");
    assert!(trace.parse_error.is_some());
    assert!(
        trace
            .llm_output
            .as_deref()
            .unwrap_or_default()
            .contains("not json")
    );
    let diagnostics = trace.llm_diagnostics.as_ref().expect("diagnostics");
    assert_eq!(diagnostics.model.as_deref(), Some("gpt-test"));
    assert_eq!(diagnostics.retry_count, 1);
}

#[test]
fn llm_agent_force_replan_after_repeated_actions() {
    let client = SequenceMockClient::new(vec![
        "{\"decision\":\"harvest_radiation\",\"max_amount\":5}".to_string(),
        "{\"decision\":\"harvest_radiation\",\"max_amount\":5}".to_string(),
        "{\"decision\":\"harvest_radiation\",\"max_amount\":5}".to_string(),
        "{\"type\":\"module_call\",\"module\":\"agent.modules.list\",\"args\":{}}".to_string(),
        "{\"decision\":\"move_agent\",\"to\":\"loc-2\"}".to_string(),
    ]);

    let mut config = base_config();
    config.max_decision_steps = 4;
    config.max_repair_rounds = 1;
    config.force_replan_after_same_action = 2;

    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let mut observation = make_observation();
    observation.time = 10;
    let decision_1 = behavior.decide(&observation);
    assert!(matches!(
        decision_1,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 11;
    let decision_2 = behavior.decide(&observation);
    assert!(matches!(
        decision_2,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 12;
    let decision_3 = behavior.decide(&observation);
    assert!(matches!(
        decision_3,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    let llm_input = trace.llm_input.unwrap_or_default();
    assert!(llm_input.contains("[Anti-Repetition Guard]"));
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.output_summary.contains("replan guard requires"))
    );
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.output_summary.contains("module_call"))
    );
}

#[test]
fn llm_agent_force_replan_allows_switch_to_new_terminal_action_without_module_call() {
    let client = SequenceMockClient::new(vec![
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
    ]);

    let mut config = base_config();
    config.max_decision_steps = 4;
    config.max_repair_rounds = 1;
    config.force_replan_after_same_action = 2;
    config.execute_until_auto_reenter_ticks = 0;

    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let mut observation = make_observation();
    observation.time = 40;
    let decision_1 = behavior.decide(&observation);
    assert!(matches!(
        decision_1,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 41;
    let decision_2 = behavior.decide(&observation);
    assert!(matches!(
        decision_2,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 42;
    let decision_3 = behavior.decide(&observation);
    assert!(matches!(
        decision_3,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace.parse_error.is_none());
    assert!(trace.llm_effect_intents.is_empty());
    assert!(
        !trace
            .llm_step_trace
            .iter()
            .any(|step| step.output_summary.contains("replan guard requires"))
    );
}

#[test]
fn llm_agent_force_replan_breaks_repeated_harvest_loop_with_repair() {
    let client = SequenceMockClient::new(vec![
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"refine_compound","compound_mass_g":1000}"#.to_string(),
    ]);

    let mut config = base_config();
    config.max_decision_steps = 4;
    config.max_repair_rounds = 1;
    config.force_replan_after_same_action = 2;
    config.execute_until_auto_reenter_ticks = 0;

    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);
    let mut observation = make_observation();

    observation.time = 70;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 5, .. })
    ));
    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 5,
        },
        action_id: 701,
        success: true,
        event: WorldEvent {
            id: 801,
            time: 70,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 5,
                available: 90,
            },
            runtime_event: None,
        },
    });

    observation.time = 71;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 5, .. })
    ));
    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 5,
        },
        action_id: 702,
        success: true,
        event: WorldEvent {
            id: 802,
            time: 71,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 4,
                available: 86,
            },
            runtime_event: None,
        },
    });

    observation.time = 72;
    let third = behavior.decide(&observation);
    assert!(matches!(
        third,
        AgentDecision::Act(Action::RefineCompound {
            compound_mass_g: 1000,
            ..
        })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace.parse_error.is_none());
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.output_summary.contains("replan guard requires"))
    );
}

#[test]
fn llm_agent_mock_sequence_recovers_and_completes_factory_recipe_chain() {
    let world_config = oasis7_simulator::WorldConfig::default();
    let world_init = oasis7_simulator::WorldInitConfig::from_scenario(
        oasis7_simulator::WorldScenario::LlmBootstrap,
        &world_config,
    );
    let (mut kernel, _) =
        crate::simulator::initialize_kernel(world_config, world_init).expect("init kernel");
    let start_location_id = kernel
        .model()
        .agents
        .get("agent-0")
        .expect("agent exists")
        .location_id
        .clone();

    let client = SequenceMockClient::new(vec![
        format!(
            r#"{{"decision":"build_factory","owner":"self","location_id":"{}","factory_id":"factory.alpha","factory_kind":"factory.assembler.mk1"}}"#,
            start_location_id
        ),
        r#"{"decision":"refine_compound","owner":"self","compound_mass_g":7000}"#.to_string(),
        r#"{"decision":"schedule_recipe","owner":"self","factory_id":"factory.alpha","recipe_id":"recipe.assembler.control_chip","batches":1}"#.to_string(),
    ]);

    let mut config = base_config();
    config.max_decision_steps = 1;
    config.max_repair_rounds = 0;
    config.execute_until_auto_reenter_ticks = 0;
    config.force_replan_after_same_action = 0;

    let behavior = LlmAgentBehavior::new("agent-0", config, client);
    let mut runner: crate::simulator::AgentRunner<LlmAgentBehavior<SequenceMockClient>> =
        crate::simulator::AgentRunner::new();
    runner.register(behavior);

    let tick1 = runner.tick(&mut kernel).expect("tick1");
    let action1 = tick1.action_result.expect("tick1 action");
    assert!(action1.success);
    assert!(matches!(
        action1.event.kind,
        WorldEventKind::FactoryBuilt { .. }
    ));
    let mut seeded_snapshot = kernel.snapshot();
    seeded_snapshot
        .model
        .agents
        .get_mut("agent-0")
        .expect("agent exists")
        .resources
        .add(ResourceKind::Data, 7_000)
        .expect("seed compound for refine");
    seeded_snapshot
        .model
        .agents
        .get_mut("agent-0")
        .expect("agent exists")
        .resources
        .add(ResourceKind::Electricity, 20)
        .expect("seed electricity for schedule");
    let seeded_journal = kernel.journal_snapshot();
    kernel = crate::simulator::WorldKernel::from_snapshot(seeded_snapshot, seeded_journal)
        .expect("restore seeded kernel");

    let tick2 = runner.tick(&mut kernel).expect("tick2");
    let action2 = tick2.action_result.expect("tick2 action");
    assert!(action2.success);
    assert!(matches!(
        action2.event.kind,
        WorldEventKind::CompoundRefined {
            hardware_output: 7,
            ..
        }
    ));

    let tick3 = runner.tick(&mut kernel).expect("tick3");
    let action3 = tick3.action_result.expect("tick3 action");
    assert!(action3.success);
    assert!(matches!(
        action3.event.kind,
        WorldEventKind::RecipeScheduled { .. }
    ));

    let factory = kernel
        .model()
        .factories
        .get("factory.alpha")
        .expect("factory exists");
    assert_eq!(factory.kind, "factory.assembler.mk1");

    let agent = kernel.model().agents.get("agent-0").expect("agent exists");
    assert_eq!(agent.resources.get(ResourceKind::Data), 9);
}

#[test]
fn llm_agent_execute_until_continues_without_llm_until_event() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            "{\"decision\":\"execute_until\",\"action\":{\"decision\":\"harvest_radiation\",\"max_amount\":9},\"until\":{\"event\":\"new_visible_agent|new_visible_location\"},\"max_ticks\":3}".to_string(),
            "{\"decision\":\"move_agent\",\"to\":\"loc-2\"}".to_string(),
        ],
        Arc::clone(&calls),
    );

    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), client);

    let mut observation = make_observation();
    observation.time = 20;

    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 21;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));
    let second_trace = behavior.take_decision_trace().expect("second trace");
    assert!(second_trace.llm_input.is_none());
    assert!(
        second_trace
            .llm_output
            .unwrap_or_default()
            .contains("execute_until continue")
    );

    observation.time = 22;
    observation.visible_agents.push(ObservedAgent {
        agent_id: "agent-new".to_string(),
        location_id: "loc-new".to_string(),
        pos: GeoPos {
            x_cm: 5,
            y_cm: 1,
            z_cm: 0,
        },
        distance_cm: 5,
    });

    let third = behavior.decide(&observation);
    assert!(matches!(
        third,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn llm_agent_auto_reentry_arms_execute_until_for_repeated_actions() {
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
    config.execute_until_auto_reenter_ticks = 3;
    config.force_replan_after_same_action = 6;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let mut observation = make_observation();
    observation.time = 26;
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
        action_id: 301,
        success: true,
        event: WorldEvent {
            id: 401,
            time: 26,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 9,
                available: 90,
            },
            runtime_event: None,
        },
    });

    observation.time = 27;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 9, .. })
    ));
    let second_trace = behavior.take_decision_trace().expect("second trace");
    assert!(
        second_trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "execute_until_auto_reentry")
    );

    behavior.on_action_result(&ActionResult {
        action: Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 9,
        },
        action_id: 302,
        success: true,
        event: WorldEvent {
            id: 402,
            time: 27,
            kind: WorldEventKind::RadiationHarvested {
                agent_id: "agent-1".to_string(),
                location_id: "loc-2".to_string(),
                amount: 8,
                available: 82,
            },
            runtime_event: None,
        },
    });

    observation.time = 28;
    let third = behavior.decide(&observation);
    assert!(matches!(
        third,
        AgentDecision::Act(Action::HarvestRadiation { max_amount: 9, .. })
    ));
    let third_trace = behavior.take_decision_trace().expect("third trace");
    assert!(third_trace.llm_input.is_none());
    assert!(
        third_trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "execute_until_continue")
    );

    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
