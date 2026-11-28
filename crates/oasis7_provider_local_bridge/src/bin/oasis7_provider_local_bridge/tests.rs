use super::*;
use crate::agent_decision::parse_model_decision;
use oasis7_agent_api::{
    ActionCatalogEntry, DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION,
    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION, ObservationEnvelope, ProviderExecutionMode,
    ProviderInteractionTarget, ProviderMissionContext, ProviderNavigationNode,
    ProviderNearbyEntity, ProviderObservation, ProviderRecentEvent, ProviderSelfState,
    RuntimeBindingV1,
};
use oasis7_simulator::Action;
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Mutex, MutexGuard, OnceLock};
#[path = "tests_agent_chat.rs"]
mod tests_agent_chat;
#[path = "tests_continuous_identity.rs"]
mod tests_continuous_identity;
#[path = "tests_options.rs"]
mod tests_options;
#[path = "tests_query_parsing.rs"]
mod tests_query_parsing;

#[derive(Debug, Clone)]
struct FakeInvoker {
    response: Result<AgentInvocationOutput, String>,
}

impl AgentInvoker for FakeInvoker {
    fn invoke(&self, _invocation: AgentInvocation) -> Result<AgentInvocationOutput, String> {
        self.response.clone()
    }
}

fn bridge_env_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn timeout_env_guard() -> MutexGuard<'static, ()> {
    bridge_env_guard()
}

#[test]
fn decision_identity_uses_the_shared_v1_domain_hash() {
    let digest = oasis7_agent_api::h_v1("oasis7.cognition.request.v1", b"bridge-fixture");
    assert_eq!(
        digest,
        oasis7_agent_api::h_v1("oasis7.cognition.request.v1", b"bridge-fixture")
    );
    assert_ne!(
        digest,
        oasis7_agent_api::h_v1("oasis7.cognition.provider-invocation.v1", b"bridge-fixture")
    );
}

#[test]
fn continuous_feedback_partitions_interleaved_agents_and_sessions() {
    let state = ProviderState::new(CliOptions {
        mode: ProviderMode::Mock,
        ..CliOptions::default()
    })
    .expect("build mock provider state");
    let feedback = |subject: &str, session: &str, seq: u64| FeedbackEnvelopeV1 {
        feedback_id: format!("feedback-{subject}-{session}-{seq}"),
        feedback_seq: seq,
        agent_subject: subject.to_string(),
        agent_session_id: session.to_string(),
        agent_turn_id: format!("turn-{subject}-{seq}"),
        decision_request_id: format!("request-{subject}-{seq}"),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "pending".to_string(),
        request_digest: Digest32::from(
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        reject_reason: None,
        provenance: "runtime_authoritative".to_string(),
    };
    let agent_a_first = feedback("agent-a", "session-a", 1);
    seed_accepted_feedback_lineage(&state, &agent_a_first);
    state
        .record_continuous_feedback(agent_a_first)
        .expect("agent A feedback");
    let agent_b_first = feedback("agent-b", "session-b", 1);
    seed_accepted_feedback_lineage(&state, &agent_b_first);
    state
        .record_continuous_feedback(agent_b_first)
        .expect("agent B feedback");
    let agent_a_second = feedback("agent-a", "session-a", 2);
    seed_accepted_feedback_lineage(&state, &agent_a_second);
    state
        .record_continuous_feedback(agent_a_second)
        .expect("agent A second feedback");
    let mut id_collision = feedback("agent-a", "session-a", 3);
    seed_accepted_feedback_lineage(&state, &id_collision);
    id_collision.feedback_id = "feedback-agent-a-session-a-1".to_string();
    assert_eq!(
        state
            .record_continuous_feedback(id_collision)
            .expect_err("feedback identity cannot move to another sequence"),
        "feedback_id_conflict"
    );

    let partitions = state.recent_feedback.lock().expect("feedback lock");
    assert_eq!(
        partitions[&("agent-a".to_string(), "session-a".to_string())]
            .recent
            .len(),
        2
    );
    assert_eq!(
        partitions[&("agent-b".to_string(), "session-b".to_string())]
            .recent
            .len(),
        1
    );
}

fn seed_accepted_feedback_lineage(state: &ProviderState, feedback: &FeedbackEnvelopeV1) {
    state
        .accepted_requests
        .lock()
        .expect("accepted request lock")
        .insert(
            feedback.decision_request_id.clone(),
            AcceptedRequestIdentity {
                agent_subject: feedback.agent_subject.clone(),
                agent_session_id: feedback.agent_session_id.clone(),
                agent_turn_id: feedback.agent_turn_id.clone(),
                decision_request_id: feedback.decision_request_id.clone(),
                request_digest: feedback.request_digest.clone(),
                response_digest: Digest32::from(
                    "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                ),
                accepted_order: 0,
            },
        );
}

#[test]
fn parse_model_decision_accepts_code_fence_and_maps_move_agent() {
    let request = sample_request();
    let raw = "```json\n{\"decision\":\"act\",\"action_ref\":\"move_agent\",\"args\":{\"to\":\"loc-2\"}}\n```";
    let (decision, repairs) =
        parse_model_decision("agent-1", &request, raw).expect("parse decision");
    assert_eq!(repairs, 1);
    assert_eq!(
        decision,
        ProviderDecision::Act {
            action_ref: "move_agent".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        }
    );
}

#[test]
fn parse_model_decision_accepts_typed_module_command_without_core_action_mapping() {
    let request = sample_request();
    let raw = r#"{
        "decision":"module_command",
        "module_command":{
            "module_id":"m.weather",
            "module_version":"0.1.0",
            "namespace":"weather",
            "name":"observe",
            "schema_version":1,
            "schema_hash":"0000000000000000000000000000000000000000000000000000000000000000",
            "payload":[1,2,3]
        }
    }"#;
    let (decision, repairs) =
        parse_model_decision("agent-1", &request, raw).expect("typed module command");
    assert_eq!(repairs, 0);
    let encoded = serde_json::to_value(decision).expect("encode typed module command");
    assert_eq!(encoded["decision"], "module_command");
    assert!(encoded["module_command"].is_object());
    assert!(encoded.get("action").is_none());
}

#[test]
fn handle_decision_returns_wait_without_provider_error_on_invalid_json() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = FakeInvoker {
        response: Ok(AgentInvocationOutput {
            prompt: "prompt".to_string(),
            text: "not-json".to_string(),
            provider_version: Some("provider/model".to_string()),
            duration_ms: Some(42),
            prompt_tokens: Some(11),
            completion_tokens: Some(7),
            total_tokens: Some(18),
            route_note: None,
            upstream_trace: None,
        }),
    };
    let response = state.handle_decision(sample_request(), None, &invoker);
    assert_eq!(response.decision, ProviderDecision::Wait);
    assert!(response.provider_error.is_none());
    assert_eq!(response.trace_payload.schema_repair_count, 1);
    assert!(
        response
            .trace_payload
            .output_summary
            .as_deref()
            .unwrap_or_default()
            .contains("invalid_model_output")
    );
}

#[test]
fn handle_decision_surfaces_gateway_failure_as_provider_error() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = FakeInvoker {
        response: Err("gateway down".to_string()),
    };
    let response = state.handle_decision(sample_request(), None, &invoker);
    assert_eq!(response.decision, ProviderDecision::Wait);
    assert_eq!(
        response
            .provider_error
            .as_ref()
            .expect("provider_error")
            .code,
        "provider_gateway_unreachable"
    );
}

#[test]
fn mock_mode_returns_deterministic_move_without_invoking_provider_cli() {
    let options = parse_options(["--mode", "mock"].into_iter()).expect("parse mock mode");
    assert_eq!(options.mode, ProviderMode::Mock);
    let state = ProviderState::new(options).expect("provider state");
    let invoker = FakeInvoker {
        response: Err("mock mode must not invoke provider cli".to_string()),
    };
    let response = state.handle_decision(sample_request(), None, &invoker);
    assert_eq!(
        response.decision,
        ProviderDecision::Act {
            action_ref: "move_agent".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        }
    );
    assert!(response.provider_error.is_none());
    assert_eq!(
        response.diagnostics.provider_id.as_deref(),
        Some("provider_local_mock")
    );
    assert_eq!(
        response.trace_payload.provider_id.as_deref(),
        Some("provider_local_mock")
    );
    assert_eq!(response.trace_payload.token_usage, None);
    assert_eq!(response.trace_payload.cost_cents, None);
    assert!(
        response
            .trace_payload
            .tool_trace
            .iter()
            .any(|entry| entry.contains("deterministic_mock"))
    );
}

#[test]
fn mock_mode_health_is_ready_without_gateway_probe() {
    let options = parse_options(["--mode", "mock"].into_iter()).expect("parse mock mode");
    let state = ProviderState::new(options).expect("provider state");
    let info = state.provider_info();
    assert_eq!(info.provider_id, "provider_local_mock");
    assert!(info.capabilities.iter().any(|value| value == "mock"));
    assert!(info.capabilities.iter().any(|value| value == "agent_chat"));
    assert!(info.capabilities.iter().any(|value| value == "no_billing"));
    let health = state.provider_health();
    assert!(health.ok);
    assert_eq!(health.status.as_deref(), Some("ready"));
    assert_eq!(health.last_error, None);
}

#[test]
fn build_decision_prompt_embeds_preferred_visible_move_hint() {
    let prompt = build_decision_prompt(&sample_request(), &[]);
    assert!(prompt.contains("Preferred next visible non-current location: loc-2"));
    assert!(
        prompt.contains("Do not output wait if move_agent to that preferred location is legal")
    );
}

#[test]
fn timeout_seconds_from_budget_uses_local_bridge_floor() {
    let _guard = timeout_env_guard();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::remove_var("OASIS7_PROVIDER_LOCAL_BRIDGE_MIN_TIMEOUT_MS");
    }
    assert_eq!(timeout_seconds_from_budget(3_000), 15);
    assert_eq!(timeout_seconds_from_budget(60_000), 60);
}

#[test]
fn timeout_seconds_from_budget_allows_env_override_floor() {
    let _guard = timeout_env_guard();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::set_var("OASIS7_PROVIDER_LOCAL_BRIDGE_MIN_TIMEOUT_MS", "7000");
    }
    assert_eq!(timeout_seconds_from_budget(3_000), 7);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        std::env::remove_var("OASIS7_PROVIDER_LOCAL_BRIDGE_MIN_TIMEOUT_MS");
    }
}

#[test]
fn build_session_key_uses_provider_config_ref_to_avoid_cross_talk() {
    let mut request = sample_request();
    request.provider_config_ref = Some("provider://loopback-http/parity/run-a/agent-0".to_string());
    let session_a = build_session_key(&request, "main");
    request.provider_config_ref = Some("provider://loopback-http/parity/run-b/agent-0".to_string());
    let session_b = build_session_key(&request, "main");
    assert_ne!(session_a, session_b);
    assert!(session_a.contains("run-a"));
    assert!(session_b.contains("run-b"));
}

#[test]
fn parse_provider_agent_command_output_accepts_local_shape() {
    let parsed = agent_output_from_json(
        "prompt".to_string(),
        r#"{"payloads":[{"text":"{\"decision\":\"wait\"}"}],"meta":{"durationMs":2484,"agentMeta":{"provider":"custom-right-codes","model":"gpt-5.4","promptTokens":9885,"usage":{"output":9,"total":9894}}}}"#,
        None,
    )
    .expect("parse local output");
    assert_eq!(parsed.text, "{\"decision\":\"wait\"}");
    assert_eq!(parsed.duration_ms, Some(2484));
    assert_eq!(
        parsed.provider_version.as_deref(),
        Some("custom-right-codes/gpt-5.4")
    );
    assert_eq!(parsed.prompt_tokens, Some(9885));
    assert_eq!(parsed.completion_tokens, Some(9));
    assert_eq!(parsed.total_tokens, Some(9894));
}

#[test]
fn local_session_id_from_session_key_hashes_invalid_chars() {
    let session_id = local_session_id_from_session_key(
        "agent:oasis7_provider_agent:subagent:world-simulator:manual:agent-1",
    );
    assert!(session_id.starts_with("ws-"));
    assert!(
        session_id
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() || ch == '-' || ch == 'w' || ch == 's')
    );
    assert_eq!(session_id.len(), 67);
}

#[test]
fn should_fallback_to_local_agent_matches_gateway_timeout() {
    assert!(should_fallback_to_local_agent(
        "provider-cli gateway call agent exited with status 1: stderr=Gateway call failed: Error: gateway timeout after 17000ms stdout="
    ));
    assert!(should_fallback_to_local_agent(
        "provider gateway call agent exited with status exit status: 1: stderr=The read operation timed out stdout="
    ));
    assert!(!should_fallback_to_local_agent("provider agent not found"));
}

#[test]
fn build_gateway_agent_params_uses_session_key_and_timeout() {
    let invocation = AgentInvocation {
        provider_cli_bin: "provider-cli".to_string(),
        agent_id: "main".to_string(),
        thinking: "off".to_string(),
        session_key: "agent:main:subagent:world-simulator:test".to_string(),
        timeout_seconds: 15,
        prompt: "{\"action\":\"wait\"}".to_string(),
        idempotency_key: "idem-1".to_string(),
        provider_invocation_key: None,
        agent_session_id: None,
        chat_request_key: None,
        route_label: None,
    };
    let params = build_gateway_agent_params(&invocation).expect("params");
    let value: Value = serde_json::from_str(params.as_str()).expect("json");
    assert_eq!(
        value.get("sessionKey").and_then(Value::as_str),
        Some("agent:main:subagent:world-simulator:test")
    );
    assert_eq!(value.get("agentId").and_then(Value::as_str), Some("main"));
    assert_eq!(
        value.get("channel").and_then(Value::as_str),
        Some("webchat")
    );
    assert_eq!(value.get("lane").and_then(Value::as_str), Some("nested"));
    assert_eq!(value.get("thinking").and_then(Value::as_str), Some("off"));
    assert_eq!(value.get("timeout").and_then(Value::as_u64), Some(15));
    assert_eq!(
        value.get("idempotencyKey").and_then(Value::as_str),
        Some("idem-1")
    );
}

#[test]
fn agent_chat_idempotency_key_distinguishes_same_tick_messages() {
    let base = ProviderAgentChatRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        message: "where are you?".to_string(),
        world_time: 7,
        location_id: Some("loc-1".to_string()),
        resources: None,
        recent_feedback: Vec::new(),
    };
    let same = agent_chat_idempotency_key("session", &base);
    assert_eq!(same, agent_chat_idempotency_key("session", &base));

    let mut different_message = base.clone();
    different_message.message = "what resources are nearby?".to_string();
    assert_ne!(
        same,
        agent_chat_idempotency_key("session", &different_message)
    );
}

#[test]
fn apply_profile_guardrails_reroutes_patrol_wait_to_move() {
    let mut request = sample_request();
    request.observation.memory_summary = Some(
        "goal=巡游移动; prefer move_agent to nearest visible non-current location".to_string(),
    );
    let (decision, note) = apply_profile_guardrails(&request, ProviderDecision::Wait);
    assert_eq!(
        decision,
        ProviderDecision::Act {
            action_ref: "move_agent".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        }
    );
    assert!(
        note.unwrap_or_default()
            .contains("profile_guardrail_reroute")
    );
}

#[test]
fn validate_profile_accepts_oasis7_and_rejects_removed_profile_alias() {
    assert_eq!(validate_profile(Some(DEFAULT_PROVIDER_AGENT_PROFILE)), None);
    assert_eq!(
        validate_profile(Some("legacy_p0_low_freq_npc")),
        Some(format!(
            "unsupported agent_profile `legacy_p0_low_freq_npc`; expected {DEFAULT_PROVIDER_AGENT_PROFILE}"
        ))
    );
}

#[test]
fn handle_decision_rejects_unsupported_schema_version() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = FakeInvoker {
        response: Err("should not invoke".to_string()),
    };
    let mut request = sample_request();
    request.observation.observation_schema_version = "oc_dual_obs_v0".to_string();
    let response = state.handle_decision(request, None, &invoker);
    assert_eq!(response.decision, ProviderDecision::Wait);
    assert_eq!(
        response
            .provider_error
            .as_ref()
            .expect("provider_error")
            .code,
        "unsupported_schema_version"
    );
}

#[test]
fn handle_decision_rejects_unsupported_action_schema_version() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = FakeInvoker {
        response: Err("should not invoke".to_string()),
    };
    let mut request = sample_request();
    request.observation.action_schema_version = "oc_dual_act_v0".to_string();
    let response = state.handle_decision(request, None, &invoker);
    assert_eq!(response.decision, ProviderDecision::Wait);
    assert_eq!(
        response
            .provider_error
            .as_ref()
            .expect("provider_error")
            .code,
        "unsupported_schema_version"
    );
}

#[test]
fn handle_decision_rejects_player_parity_request_with_headless_fields() {
    let state = ProviderState::new(CliOptions::default()).expect("provider state");
    let invoker = FakeInvoker {
        response: Err("should not invoke".to_string()),
    };
    let mut request = sample_request();
    request.observation.mode = ProviderExecutionMode::PlayerParity;
    let response = state.handle_decision(request, None, &invoker);
    assert_eq!(response.decision, ProviderDecision::Wait);
    assert_eq!(
        response
            .provider_error
            .as_ref()
            .expect("provider_error")
            .code,
        "mode_observation_mismatch"
    );
}

#[test]
fn parse_options_accepts_auth_route_from_bearer() {
    let options = parse_options(["--auth-route-from-bearer"].into_iter()).expect("parse options");
    assert!(options.auth_route_from_bearer);
    assert!(options.auth_route_map.is_empty());
    assert!(options.auth_token.is_none());
}

#[test]
fn parse_options_defaults_to_legacy_cli_backend() {
    let options = parse_options(std::iter::empty()).expect("parse options");
    assert_eq!(options.provider_backend, ProviderBackend::LegacyCli);
}

#[test]
fn parse_options_accepts_explicit_legacy_cli_backend() {
    let options =
        parse_options(["--provider-backend", "legacy-cli"].into_iter()).expect("parse options");
    assert_eq!(options.provider_backend, ProviderBackend::LegacyCli);
}

#[test]
fn provider_cli_bin_implies_legacy_backend() {
    let options = parse_options(["--provider-cli-bin", "/tmp/provider-cli"].into_iter())
        .expect("parse options");
    assert_eq!(options.provider_backend, ProviderBackend::LegacyCli);
}

#[test]
fn error_diagnostics_json_extracts_structured_diagnostics() {
    let diagnostics = error_diagnostics_json(
        r#"upstream SSE response did not contain assistant content; diagnostics={"data_event_count":1,"chunk_samples":[{"choices_len":1}]}"#,
    );
    assert_eq!(
        diagnostics.get("data_event_count").and_then(Value::as_u64),
        Some(1)
    );
    assert!(diagnostics.get("chunk_samples").is_some());
}

#[test]
fn provider_error_upstream_trace_preserves_structured_diagnostics() {
    let trace = provider_error_upstream_trace(
        r#"upstream SSE response did not contain assistant content; diagnostics={"data_event_count":2,"chunk_samples":[{"choices_len":0}],"status_code":200}"#,
    );

    assert_eq!(
        trace
            .get("diagnostics")
            .and_then(|value| value.get("data_event_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        trace
            .get("diagnostics")
            .and_then(|value| value.get("status_code"))
            .and_then(Value::as_u64),
        Some(200)
    );
    assert_eq!(
        trace.get("diagnostics_present").and_then(Value::as_bool),
        Some(true)
    );
}

#[path = "tests_fixture.rs"]
mod fixture;
use fixture::sample_request;
