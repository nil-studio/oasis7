#[test]
fn llm_agent_collapses_mixed_multi_turn_output_to_last_terminal_decision() {
    let calls = Arc::new(AtomicUsize::new(0));
    let client = CountingSequenceMockClient::new(
        vec![
            r#"{"type":"module_call","module":"agent.modules.list","args":{}}

---

{"type":"module_call","module":"environment.current_observation","args":{}}

---

{"type":"module_call","module":"agent.modules.list","args":{}}

---

{"decision":"wait"}"#
                .to_string(),
        ],
        Arc::clone(&calls),
    );

    let mut config = base_config();
    config.max_module_calls = 2;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert_eq!(decision, AgentDecision::Wait);
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

#[test]
fn llm_agent_supports_dialogue_module_and_draft_flow() {
    let client = SequenceMockClient::new(vec![
        r#"{"type":"module_call","module":"agent.modules.list","args":{}}"#.to_string(),
        r#"{"type":"decision_draft","decision":{"decision":"move_agent","to":"loc-2"},"confidence":0.78,"need_verify":false}"#.to_string(),
    ]);
    let mut config = base_config();
    config.max_decision_steps = 6;
    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let decision = behavior.decide(&make_observation());
    assert!(matches!(
        decision,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace.parse_error.is_none());
    assert_eq!(trace.llm_effect_intents.len(), 1);
    assert_eq!(trace.llm_effect_receipts.len(), 1);
    assert!(!trace.llm_step_trace.is_empty());
    assert!(trace
        .llm_step_trace
        .iter()
        .all(|step| step.step_type == "dialogue_turn" || step.step_type == "repair"));
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("decision_draft")));
    assert!(!trace.llm_prompt_section_trace.is_empty());
}

#[test]
fn llm_agent_force_replan_plan_can_finalize_without_module_call_when_missing_is_empty() {
    let client = SequenceMockClient::new(vec![
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"decision":"harvest_radiation","max_amount":5}"#.to_string(),
        r#"{"type":"plan","missing":[],"next":"module_call"}"#.to_string(),
        r#"{"decision":"move_agent","to":"loc-2"}"#.to_string(),
    ]);

    let mut config = base_config();
    config.max_decision_steps = 5;
    config.max_repair_rounds = 1;
    config.force_replan_after_same_action = 2;
    config.execute_until_auto_reenter_ticks = 0;

    let mut behavior = LlmAgentBehavior::new("agent-1", config, client);

    let mut observation = make_observation();
    observation.time = 70;
    let first = behavior.decide(&observation);
    assert!(matches!(
        first,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 71;
    let second = behavior.decide(&observation);
    assert!(matches!(
        second,
        AgentDecision::Act(Action::HarvestRadiation { .. })
    ));

    observation.time = 72;
    let third = behavior.decide(&observation);
    assert!(matches!(
        third,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));

    let trace = behavior.take_decision_trace().expect("trace exists");
    assert!(trace.parse_error.is_none());
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace
        .llm_step_trace
        .iter()
        .any(|step| step.output_summary.contains("deprecated in dialogue mode")));
}

#[path = "tests_followups.rs"]
mod tests_followups;

#[path = "tests_part3_module_lifecycle.rs"]
mod tests_part3_module_lifecycle;
