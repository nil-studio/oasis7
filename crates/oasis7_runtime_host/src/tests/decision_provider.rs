use super::*;

fn setup_kernel_with_provider_agent(agent_id: &str) -> WorldKernel {
    let mut kernel = WorldKernel::with_config(WorldConfig {
        move_cost_per_km_electricity: 0,
        ..Default::default()
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "neighbor".to_string(),
        pos: pos(100, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();
    kernel
}

fn provider_action_catalog() -> Vec<ActionCatalogEntry> {
    vec![
        ActionCatalogEntry::new("wait", "Skip this tick"),
        ActionCatalogEntry::new("move_agent", "Move the acting agent to a visible location"),
    ]
}

#[test]
fn golden_decision_provider_fixture_round_trips() {
    let fixtures = golden_decision_provider_fixtures();
    assert_eq!(fixtures.len(), 1);
    assert_eq!(fixtures[0].fixture_id, "golden.move.visible_location.v1");
    let encoded = serde_json::to_string_pretty(&fixtures[0]).expect("encode fixture");
    let decoded: GoldenDecisionFixture = serde_json::from_str(encoded.as_str()).expect("decode");
    assert_eq!(decoded, fixtures[0]);
    assert!(
        decoded
            .request
            .observation
            .action_catalog
            .iter()
            .any(|entry| entry.action_ref == "move_agent")
    );
    assert_eq!(
        decoded.request.observation.mode,
        ProviderExecutionMode::HeadlessAgent
    );
    assert_eq!(
        decoded.request.observation.observation_schema_version,
        DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION
    );
    assert_eq!(
        decoded.request.observation.action_schema_version,
        DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
    );
    assert_eq!(
        decoded.request.observation.environment_class.as_deref(),
        Some("golden_fixture")
    );
    assert_eq!(
        decoded.request.fixture_id.as_deref(),
        Some("golden.move.visible_location.v1")
    );
}

#[test]
fn provider_backed_agent_behavior_executes_mocked_move_and_records_feedback() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "move_agent".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics {
            provider_id: Some("mock-provider-local-bridge".to_string()),
            provider_version: Some("v0".to_string()),
            latency_ms: Some(42),
            retry_count: 0,
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("mock-provider-local-bridge".to_string()),
            input_summary: Some("fixture=golden.move.visible_location.v1".to_string()),
            output_summary: Some("decision=move_agent(to=loc-2)".to_string()),
            latency_ms: Some(42),
            transcript: vec![ProviderTranscriptEntry {
                role: "agent".to_string(),
                content: "move to loc-2".to_string(),
            }],
            tool_trace: vec!["selected visible location loc-2".to_string()],
            token_usage: Some(ProviderTokenUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
            }),
            cost_cents: Some(1),
            upstream_trace: None,
            schema_repair_count: 0,
        },
        memory_write_intents: vec![],
    };
    let provider = MockDecisionProvider::with_scripted_responses(
        "mock-provider-local-bridge",
        vec![Ok(response)],
    );
    let shared_state = provider.shared_state();
    let behavior = ProviderBackedAgentBehavior::new("agent-1", provider, provider_action_catalog())
        .legacy_compatibility()
        .with_provider_config_ref("mock://provider-loopback-http")
        .with_agent_profile("oasis7_p0_low_freq_npc")
        .with_execution_mode(ProviderExecutionMode::PlayerParity)
        .with_environment_class("unit_test")
        .with_fallback_reason("parity_probe")
        .with_fixture_id("fixture.agent-1")
        .with_replay_id("replay.agent-1")
        .with_memory_summary("goal=move");

    let mut runner: AgentRunner<ProviderBackedAgentBehavior<MockDecisionProvider>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("provider-backed tick");
    assert!(matches!(
        tick.decision,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert!(tick.is_success());
    let trace = tick.decision_trace.expect("trace emitted");
    assert_eq!(trace.llm_error, None);
    assert_eq!(
        trace.llm_diagnostics.and_then(|value| value.latency_ms),
        Some(42)
    );
    assert!(
        trace
            .llm_output
            .as_deref()
            .unwrap_or_default()
            .contains("move_agent")
    );

    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.location_id, "loc-2");

    let snapshot = shared_state.lock().expect("mock state lock").clone();
    assert_eq!(snapshot.recorded_requests.len(), 1);
    assert_eq!(snapshot.recorded_feedback.len(), 1);
    assert!(snapshot.recorded_feedback[0].success);
    assert_eq!(
        snapshot.recorded_requests[0].provider_config_ref.as_deref(),
        Some("mock://provider-loopback-http")
    );
    assert_eq!(
        snapshot.recorded_requests[0].agent_profile.as_deref(),
        Some("oasis7_p0_low_freq_npc")
    );
    assert!(
        snapshot.recorded_requests[0]
            .observation
            .action_catalog
            .iter()
            .any(|entry| entry.action_ref == "move_agent")
    );
    assert_eq!(
        snapshot.recorded_requests[0].observation.mode,
        ProviderExecutionMode::PlayerParity
    );
    assert_eq!(
        snapshot.recorded_requests[0]
            .observation
            .observation_schema_version,
        DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION
    );
    assert_eq!(
        snapshot.recorded_requests[0]
            .observation
            .action_schema_version,
        DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
    );
    assert_eq!(
        snapshot.recorded_requests[0]
            .observation
            .environment_class
            .as_deref(),
        Some("unit_test")
    );
    assert_eq!(
        snapshot.recorded_requests[0]
            .observation
            .fallback_reason
            .as_deref(),
        Some("parity_probe")
    );
    assert_eq!(
        snapshot.recorded_requests[0].fixture_id.as_deref(),
        Some("fixture.agent-1")
    );
    assert_eq!(
        snapshot.recorded_requests[0].replay_id.as_deref(),
        Some("replay.agent-1")
    );
}

#[test]
fn provider_backed_agent_behavior_downgrades_provider_error_to_wait() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let provider = MockDecisionProvider::with_scripted_responses(
        "mock-provider-local-bridge",
        vec![Err(DecisionProviderError::new(
            "provider_timeout",
            "request exceeded 3000ms budget",
            true,
        ))],
    );
    let behavior = ProviderBackedAgentBehavior::new("agent-1", provider, provider_action_catalog())
        .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<MockDecisionProvider>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("provider tick");
    assert!(matches!(tick.decision, AgentDecision::Wait));
    assert!(tick.action_result.is_none());
    let trace = tick.decision_trace.expect("error trace emitted");
    assert!(
        trace
            .llm_error
            .as_deref()
            .unwrap_or_default()
            .contains("provider_timeout")
    );
}

#[test]
fn provider_backed_agent_behavior_rejects_unknown_action_ref() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "unknown_action".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics {
            provider_id: Some("mock-provider-local-bridge".to_string()),
            provider_version: None,
            latency_ms: Some(10),
            retry_count: 0,
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("mock-provider-local-bridge".to_string()),
            output_summary: Some("decision=unknown_action".to_string()),
            ..ProviderTraceEnvelope::default()
        },
        memory_write_intents: vec![],
    };
    let provider = MockDecisionProvider::with_scripted_responses(
        "mock-provider-local-bridge",
        vec![Ok(response)],
    );
    let behavior = ProviderBackedAgentBehavior::new("agent-1", provider, provider_action_catalog())
        .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<MockDecisionProvider>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("provider tick");
    assert!(matches!(tick.decision, AgentDecision::Wait));
    assert!(tick.action_result.is_none());
    let trace = tick.decision_trace.expect("trace emitted");
    assert!(
        trace
            .parse_error
            .as_deref()
            .unwrap_or_default()
            .contains("unknown action_ref")
    );
}

#[test]
fn provider_backed_agent_behavior_builds_mode_differentiated_observation_payloads() {
    let mut parity_kernel = setup_kernel_with_provider_agent("agent-1");
    let parity_provider = MockDecisionProvider::with_scripted_responses(
        "mock-provider-local-bridge",
        vec![Ok(DecisionResponse::wait("mock-provider-local-bridge"))],
    );
    let parity_state = parity_provider.shared_state();
    let parity_behavior =
        ProviderBackedAgentBehavior::new("agent-1", parity_provider, provider_action_catalog())
            .legacy_compatibility()
            .with_execution_mode(ProviderExecutionMode::PlayerParity)
            .with_memory_summary("goal=patrol");
    let mut parity_runner: AgentRunner<ProviderBackedAgentBehavior<MockDecisionProvider>> =
        AgentRunner::new();
    parity_runner.register(parity_behavior);
    let _ = parity_runner.tick(&mut parity_kernel).expect("parity tick");

    let mut headless_kernel = setup_kernel_with_provider_agent("agent-1");
    let headless_provider = MockDecisionProvider::with_scripted_responses(
        "mock-provider-local-bridge",
        vec![Ok(DecisionResponse::wait("mock-provider-local-bridge"))],
    );
    let headless_state = headless_provider.shared_state();
    let headless_behavior =
        ProviderBackedAgentBehavior::new("agent-1", headless_provider, provider_action_catalog())
            .legacy_compatibility()
            .with_execution_mode(ProviderExecutionMode::HeadlessAgent)
            .with_memory_summary("goal=patrol");
    let mut headless_runner: AgentRunner<ProviderBackedAgentBehavior<MockDecisionProvider>> =
        AgentRunner::new();
    headless_runner.register(headless_behavior);
    let _ = headless_runner
        .tick(&mut headless_kernel)
        .expect("headless tick");

    let parity_request = parity_state.lock().expect("parity state").recorded_requests[0].clone();
    let headless_request = headless_state
        .lock()
        .expect("headless state")
        .recorded_requests[0]
        .clone();

    assert_eq!(
        parity_request.observation.mode,
        ProviderExecutionMode::PlayerParity
    );
    assert_eq!(
        headless_request.observation.mode,
        ProviderExecutionMode::HeadlessAgent
    );
    assert!(
        parity_request
            .observation
            .observation
            .local_navigation_graph
            .is_empty()
    );
    assert!(
        parity_request
            .observation
            .observation
            .interaction_targets
            .is_empty()
    );
    assert!(
        !headless_request
            .observation
            .observation
            .local_navigation_graph
            .is_empty()
    );
    assert!(
        !headless_request
            .observation
            .observation
            .interaction_targets
            .is_empty()
    );
    assert_ne!(
        parity_request.observation.observation.self_state.pose_hint,
        headless_request
            .observation
            .observation
            .self_state
            .pose_hint
    );

    let parity_json =
        serde_json::to_string(&parity_request).expect("encode parity observation request");
    let headless_json =
        serde_json::to_string(&headless_request).expect("encode headless observation request");
    assert!(!parity_json.contains("local_navigation_graph"));
    assert!(!parity_json.contains("interaction_targets"));
    assert!(headless_json.contains("local_navigation_graph"));
    assert!(headless_json.contains("interaction_targets"));
}

#[test]
fn decision_request_validate_contract_rejects_player_parity_headless_fields() {
    let mut request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;
    request.observation.mode = ProviderExecutionMode::PlayerParity;
    let err = request
        .validate_contract()
        .expect_err("parity mismatch should fail");
    assert_eq!(err.code, "mode_observation_mismatch");
}
