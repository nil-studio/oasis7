#![cfg(not(target_arch = "wasm32"))]

use super::*;
use oasis7_runtime::{
    CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext, WorldError,
};
use oasis7_wasm_abi::{AgentCommandResponse, CapabilityCatalogSnapshot, CapabilitySubject};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct RecordedHttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug, Clone)]
struct MockHttpResponse {
    status_code: u16,
    body: String,
}

#[test]
fn provider_loopback_adapter_decides_and_pushes_feedback_via_local_http() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture");
    let mut request = fixture.request.clone();
    request.agent_profile = Some("oasis7_p0_low_freq_npc".to_string());
    let feedback = FeedbackEnvelope {
        action_id: 9,
        success: true,
        reject_reason: None,
        emitted_events: vec![],
        world_delta_summary: Some("move completed".to_string()),
    };
    let response = DecisionResponse {
        decision: fixture.expected_decision,
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics {
            provider_id: Some("provider_local_bridge".to_string()),
            provider_version: Some("0.1.0".to_string()),
            latency_ms: Some(37),
            retry_count: 0,
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("provider_local_bridge".to_string()),
            output_summary: Some("decision=move_agent(to=loc-2)".to_string()),
            latency_ms: Some(37),
            ..ProviderTraceEnvelope::default()
        },
        memory_write_intents: vec![],
    };
    let expected_response = response.clone();
    let request_for_server = request.clone();
    let feedback_for_server = feedback.clone();
    let base_url = spawn_mock_http_server(2, move |incoming| {
        match (incoming.method.as_str(), incoming.path.as_str()) {
            ("POST", "/v1/world-simulator/decision") => {
                let decoded: DecisionRequest = serde_json::from_slice(incoming.body.as_slice())
                    .expect("decode decision request");
                assert_eq!(decoded, request_for_server);
                assert_eq!(
                    incoming.headers.get("authorization").map(String::as_str),
                    Some("Bearer secret-token")
                );
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::to_string(&expected_response).expect("encode response"),
                }
            }
            ("POST", "/v1/world-simulator/feedback") => {
                let decoded: FeedbackEnvelope = serde_json::from_slice(incoming.body.as_slice())
                    .expect("decode feedback request");
                assert_eq!(decoded, feedback_for_server);
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                }
            }
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });

    let mut adapter = ProviderLoopbackAdapter::new(base_url.as_str(), Some("secret-token"), 200)
        .expect("adapter");
    assert_eq!(adapter.provider_id(), "provider_loopback_http");

    let decided = adapter.decide(&request).expect("decision response");
    assert_eq!(decided, response);
    adapter.push_feedback(&feedback).expect("feedback ack");
}

#[test]
fn provider_loopback_adapter_rejects_action_ref_outside_phase1_whitelist() {
    let request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;
    let disallowed_response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "build_factory".to_string(),
            action: Action::BuildFactory {
                owner: ResourceOwner::Agent {
                    agent_id: "agent-1".to_string(),
                },
                location_id: "loc-1".to_string(),
                factory_id: "fac-1".to_string(),
                factory_kind: "basic".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(1, move |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::to_string(&disallowed_response).expect("encode response"),
    });

    let mut adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let err = adapter.decide(&request).expect_err("should reject action");
    assert_eq!(err.code, "action_ref_not_in_catalog");
}

#[test]
fn provider_loopback_adapter_maps_provider_error_envelope_to_decision_provider_error() {
    let request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;
    let provider_error_response = DecisionResponse {
        decision: ProviderDecision::Wait,
        module_command: None,
        provider_error: Some(ProviderErrorEnvelope {
            code: "provider_timeout".to_string(),
            message: "request exceeded 200ms budget".to_string(),
            retryable: true,
        }),
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope {
            upstream_trace: Some(serde_json::json!({
                "stage": "decision_invocation",
                "diagnostics": {
                    "status_code": 200,
                    "data_event_count": 2,
                },
            })),
            ..ProviderTraceEnvelope::default()
        },
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(1, move |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::to_string(&provider_error_response).expect("encode response"),
    });

    let mut adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let err = adapter
        .decide(&request)
        .expect_err("provider error should surface");
    assert_eq!(err.code, "provider_timeout");
    assert!(err.retryable);
    assert_eq!(
        err.upstream_trace
            .as_ref()
            .and_then(|trace| trace.get("diagnostics"))
            .and_then(|diagnostics| diagnostics.get("data_event_count"))
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
}

#[test]
fn provider_loopback_adapter_rejects_wait_ticks_when_request_catalog_omits_it() {
    let mut request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;
    request
        .observation
        .action_catalog
        .retain(|entry| entry.action_ref != "wait_ticks");
    let wait_ticks_response = DecisionResponse {
        decision: ProviderDecision::WaitTicks { ticks: 2 },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(1, move |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::to_string(&wait_ticks_response).expect("encode response"),
    });

    let mut adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let err = adapter
        .decide(&request)
        .expect_err("wait_ticks outside action_catalog should be rejected");
    assert_eq!(err.code, "action_ref_not_in_catalog");
    assert!(err.message.contains("wait_ticks"));
}

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

fn provider_phase1_action_catalog() -> Vec<ActionCatalogEntry> {
    vec![
        ActionCatalogEntry::new("wait", "Skip this tick"),
        ActionCatalogEntry::new("wait_ticks", "Pause for fixed ticks"),
        ActionCatalogEntry::new("move_agent", "Move the acting agent to a visible location"),
        ActionCatalogEntry::new("speak_to_nearby", "Emit a lightweight nearby speech event"),
        ActionCatalogEntry::new("inspect_target", "Inspect a nearby world target"),
        ActionCatalogEntry::new(
            "simple_interact",
            "Perform a lightweight interaction with a target",
        ),
    ]
}

#[test]
fn provider_backed_behavior_executes_provider_loopback_adapter_move_and_records_feedback() {
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
            provider_id: Some("provider_local_bridge".to_string()),
            provider_version: Some("0.1.0".to_string()),
            latency_ms: Some(33),
            retry_count: 0,
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("provider_local_bridge".to_string()),
            output_summary: Some("decision=move_agent(to=loc-2)".to_string()),
            latency_ms: Some(33),
            ..ProviderTraceEnvelope::default()
        },
        memory_write_intents: vec![],
    };
    let response_for_server = response.clone();
    let base_url = spawn_mock_http_server(2, move |incoming| {
        match (incoming.method.as_str(), incoming.path.as_str()) {
            ("POST", "/v1/world-simulator/decision") => {
                let decoded: DecisionRequest = serde_json::from_slice(incoming.body.as_slice())
                    .expect("decode decision request");
                assert_eq!(
                    decoded.agent_profile.as_deref(),
                    Some("oasis7_p0_low_freq_npc")
                );
                assert_eq!(
                    decoded.observation.mode,
                    ProviderExecutionMode::HeadlessAgent
                );
                assert_eq!(
                    decoded.observation.observation_schema_version,
                    DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION
                );
                assert_eq!(
                    decoded.observation.action_schema_version,
                    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
                );
                assert_eq!(
                    decoded.observation.environment_class.as_deref(),
                    Some("adapter_test")
                );
                assert_eq!(decoded.fixture_id.as_deref(), Some("fixture.adapter.move"));
                assert_eq!(decoded.replay_id.as_deref(), Some("replay.adapter.move"));
                MockHttpResponse {
                    status_code: 200,
                    body: serde_json::to_string(&response_for_server)
                        .expect("encode decision response"),
                }
            }
            ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                status_code: 200,
                body: serde_json::json!({"ok": true}).to_string(),
            },
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });

    let adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let behavior =
        ProviderBackedAgentBehavior::new("agent-1", adapter, provider_phase1_action_catalog())
            .legacy_compatibility()
            .with_provider_config_ref("provider://loopback-http")
            .with_agent_profile("oasis7_p0_low_freq_npc")
            .with_environment_class("adapter_test")
            .with_fixture_id("fixture.adapter.move")
            .with_replay_id("replay.adapter.move")
            .with_memory_summary("goal=move");
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("runner tick");
    assert!(matches!(
        tick.decision,
        AgentDecision::Act(Action::MoveAgent { .. })
    ));
    assert!(tick.is_success());
    let trace = tick.decision_trace.expect("decision trace");
    assert_eq!(
        trace.llm_diagnostics.and_then(|value| value.latency_ms),
        Some(33)
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
}

#[test]
fn provider_backed_behavior_executes_provider_loopback_adapter_speak_action() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "speak_to_nearby".to_string(),
            action: Action::SpeakToNearby {
                agent_id: "agent-1".to_string(),
                message: "hello nearby".to_string(),
                target_agent_id: None,
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(2, move |incoming| {
        match (incoming.method.as_str(), incoming.path.as_str()) {
            ("POST", "/v1/world-simulator/decision") => MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&response).expect("encode response"),
            },
            ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                status_code: 200,
                body: serde_json::json!({"ok": true}).to_string(),
            },
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });

    let adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let behavior =
        ProviderBackedAgentBehavior::new("agent-1", adapter, provider_phase1_action_catalog())
            .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("runner tick");
    assert!(tick.is_success());
    let action_result = tick.action_result.expect("action result");
    assert!(matches!(
        action_result.event.kind,
        WorldEventKind::AgentSpoke { .. }
    ));
}

#[test]
fn provider_backed_behavior_executes_provider_loopback_adapter_inspect_action() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "inspect_target".to_string(),
            action: Action::InspectTarget {
                agent_id: "agent-1".to_string(),
                target_kind: "location".to_string(),
                target_id: "loc-1".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(2, move |incoming| {
        match (incoming.method.as_str(), incoming.path.as_str()) {
            ("POST", "/v1/world-simulator/decision") => MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&response).expect("encode response"),
            },
            ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                status_code: 200,
                body: serde_json::json!({"ok": true}).to_string(),
            },
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });

    let adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let behavior =
        ProviderBackedAgentBehavior::new("agent-1", adapter, provider_phase1_action_catalog())
            .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("runner tick");
    assert!(tick.is_success());
    let action_result = tick.action_result.expect("action result");
    assert!(matches!(
        action_result.event.kind,
        WorldEventKind::TargetInspected { .. }
    ));
}

#[test]
fn provider_backed_behavior_executes_provider_loopback_adapter_simple_interact_action() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "simple_interact".to_string(),
            action: Action::SimpleInteract {
                agent_id: "agent-1".to_string(),
                target_kind: "location".to_string(),
                target_id: "loc-1".to_string(),
                interaction: "press_console".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(2, move |incoming| {
        match (incoming.method.as_str(), incoming.path.as_str()) {
            ("POST", "/v1/world-simulator/decision") => MockHttpResponse {
                status_code: 200,
                body: serde_json::to_string(&response).expect("encode response"),
            },
            ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                status_code: 200,
                body: serde_json::json!({"ok": true}).to_string(),
            },
            _ => MockHttpResponse {
                status_code: 404,
                body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
            },
        }
    });

    let adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let behavior =
        ProviderBackedAgentBehavior::new("agent-1", adapter, provider_phase1_action_catalog())
            .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("runner tick");
    assert!(tick.is_success());
    let action_result = tick.action_result.expect("action result");
    assert!(matches!(
        action_result.event.kind,
        WorldEventKind::SimpleInteractionPerformed { .. }
    ));
}

#[test]
fn provider_backed_behavior_downgrades_provider_loopback_adapter_unsupported_semantics_to_wait() {
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let response = DecisionResponse {
        decision: ProviderDecision::Act {
            action_ref: "simple_interact".to_string(),
            action: Action::MoveAgent {
                agent_id: "agent-1".to_string(),
                to: "loc-2".to_string(),
            },
        },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: vec![],
    };
    let base_url = spawn_mock_http_server(1, move |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::to_string(&response).expect("encode response"),
    });

    let adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let behavior =
        ProviderBackedAgentBehavior::new("agent-1", adapter, provider_phase1_action_catalog())
            .legacy_compatibility();
    let mut runner: AgentRunner<ProviderBackedAgentBehavior<ProviderLoopbackAdapter>> =
        AgentRunner::new();
    runner.register(behavior);

    let tick = runner.tick(&mut kernel).expect("runner tick");
    assert!(matches!(tick.decision, AgentDecision::Wait));
    assert!(tick.action_result.is_none());
    let trace = tick.decision_trace.expect("error trace emitted");
    assert!(
        trace
            .llm_error
            .as_deref()
            .unwrap_or_default()
            .contains("action_ref_mismatch")
    );
}

fn module_command_catalog_entry_json() -> serde_json::Value {
    serde_json::json!({
        "module_id": "m.weather",
        "module_version": "0.1.0",
        "namespace": "weather",
        "name": "observe",
        "schema_version": 1,
        "schema_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "max_payload_bytes": 128
    })
}

fn request_with_module_command_catalog() -> DecisionRequest {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture");
    let mut wire = serde_json::to_value(fixture.request).expect("encode request fixture");
    wire["observation"]["module_command_catalog"] =
        serde_json::Value::Array(vec![module_command_catalog_entry_json()]);
    serde_json::from_value(wire).expect("decode request with module command catalog")
}

fn module_command_response_json(command: serde_json::Value) -> String {
    serde_json::json!({
        "decision": {"decision": "wait"},
        "module_command": command,
        "diagnostics": {},
        "trace_payload": {},
        "memory_write_intents": []
    })
    .to_string()
}

fn assert_module_command_response_rejected(command: serde_json::Value, expected_code: &str) {
    let request = request_with_module_command_catalog();
    let body = module_command_response_json(command);
    let base_url = spawn_mock_http_server(1, move |_| MockHttpResponse {
        status_code: 200,
        body: body.clone(),
    });
    let mut adapter = ProviderLoopbackAdapter::new(base_url.as_str(), None, 200).expect("adapter");
    let error = adapter
        .decide(&request)
        .expect_err("module command response must fail closed");
    assert_eq!(error.code, expected_code, "error={error:?}");
}

#[test]
fn decision_request_round_trip_preserves_module_command_catalog() {
    let request = request_with_module_command_catalog();
    let wire = serde_json::to_value(&request).expect("encode request");
    let catalog = wire
        .get("observation")
        .and_then(|observation| observation.get("module_command_catalog"))
        .and_then(serde_json::Value::as_array)
        .expect("provider request must carry module_command_catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0]["module_id"], "m.weather");
    assert_eq!(catalog[0]["module_version"], "0.1.0");
    assert_eq!(catalog[0]["namespace"], "weather");
    assert_eq!(catalog[0]["name"], "observe");
    assert_eq!(catalog[0]["schema_version"], 1);
    assert_eq!(catalog[0]["max_payload_bytes"], 128);
}

#[test]
fn provider_module_command_response_rejects_module_id_mismatch() {
    let mut command = module_command_catalog_entry_json();
    command["module_id"] = serde_json::json!("m.other");
    command["payload"] = serde_json::json!([1, 2, 3]);
    assert_module_command_response_rejected(command, "module_command_identity_mismatch");
}

#[test]
fn provider_module_command_response_rejects_version_namespace_name_mismatch() {
    for (field, value) in [
        ("module_version", serde_json::json!("0.2.0")),
        ("namespace", serde_json::json!("forecast")),
        ("name", serde_json::json!("write")),
        ("schema_version", serde_json::json!(2)),
    ] {
        let mut command = module_command_catalog_entry_json();
        command[field] = value;
        command["payload"] = serde_json::json!([1, 2, 3]);
        assert_module_command_response_rejected(command, "module_command_identity_mismatch");
    }
}

#[test]
fn provider_module_command_response_rejects_schema_hash_mismatch() {
    let mut command = module_command_catalog_entry_json();
    command["schema_hash"] =
        serde_json::json!("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
    command["payload"] = serde_json::json!([1, 2, 3]);
    assert_module_command_response_rejected(command, "module_command_schema_hash_mismatch");
}

#[test]
fn provider_module_command_response_rejects_payload_above_declared_bound() {
    let mut command = module_command_catalog_entry_json();
    command["payload"] = serde_json::json!(vec![0_u8; 129]);
    assert_module_command_response_rejected(command, "module_command_payload_too_large");
}

#[test]
fn provider_module_command_response_rejects_reserved_core_namespace() {
    let mut command = module_command_catalog_entry_json();
    command["namespace"] = serde_json::json!("core");
    command["payload"] = serde_json::json!([1, 2, 3]);
    assert_module_command_response_rejected(command, "module_command_reserved_namespace");
}

#[test]
fn provider_module_command_response_without_executor_is_explicitly_unroutable() {
    let mut command = module_command_catalog_entry_json();
    command["payload"] = serde_json::json!([1, 2, 3]);
    assert_module_command_response_rejected(command, "module_command_unroutable");
}

#[derive(Debug, Default)]
struct RecordingTrustedModuleExecutor {
    responses: Vec<AgentCommandResponse>,
}

impl TrustedModuleCommandExecutor for RecordingTrustedModuleExecutor {
    fn execute_trusted_module_command(
        &mut self,
        response: AgentCommandResponse,
    ) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
        self.responses.push(response);
        Ok(serde_json::from_value(serde_json::json!({
            "receipt_id": "receipt.provider-bridge.1",
            "subject": {"kind": "agent", "agent_id": "agent-1"},
            "audience": {"world_id": "world-1", "branch_id": "main"},
            "scope_hash": "scope-hash",
            "decision": "accepted",
            "world_head_before": 1,
            "branch_id": "main",
            "finality_epoch": 1,
            "finality_status": "finalized",
            "state_hash_before": "state-before",
            "canonical_request_hash": "request-hash",
            "canonical_result_hash": "result-hash"
        }))
        .expect("recording executor receipt"))
    }
}

fn typed_provider_bridge_fixture() -> (
    DecisionRequest,
    ProviderCapabilityContext,
    AgentCommandResponse,
) {
    let hash = "0000000000000000000000000000000000000000000000000000000000000000";
    let subject = serde_json::from_value::<CapabilitySubject>(serde_json::json!({
        "kind": "agent",
        "agent_id": "agent-1",
        "owner_binding": "owner-1",
        "generation": 1
    }))
    .expect("fixture subject");
    let catalog: CapabilityCatalogSnapshot = serde_json::from_value(serde_json::json!({
        "snapshot_id": "catalog.provider-bridge.1",
        "world_id": "world-1",
        "world_head": 1,
        "branch_id": "main",
        "finality_epoch": 1,
        "logical_tick": 7,
        "module_registry_hash": "registry-hash",
        "policy_hash": "policy-hash",
        "revocation_epoch": 1,
        "subject": subject,
        "presenter": {
            "presenter_id": "provider-1",
            "presenter_kind": "provider"
        },
        "audience": {
            "world_id": "world-1",
            "branch_id": "main",
            "finality_epoch": 1,
            "target_kind": "module_instance",
            "target_id": "instance-1"
        },
        "entries": [{
            "module_id": "m.weather",
            "module_version": "0.1.0",
            "namespace": "weather",
            "command": "observe",
            "schema_version": 1,
            "schema_hash": hash,
            "max_payload_bytes": 128
        }],
        "valid_until_tick": 8
    }))
    .expect("fixture catalog");
    let entry = catalog.entries[0].clone();
    let response: AgentCommandResponse = serde_json::from_value(serde_json::json!({
        "response_nonce": "nonce.provider-bridge.1",
        "subject": catalog.subject,
        "presenter": catalog.presenter,
        "audience": catalog.audience,
        "catalog_snapshot_id": catalog.snapshot_id,
        "selected_entry": entry,
        "envelope": {
            "namespace": "weather",
            "name": "observe",
            "schema_version": 1,
            "schema_hash": hash,
            "payload": [1, 2, 3]
        },
        "provider_id": "provider-1",
        "trace_id": "trace.provider-bridge.1"
    }))
    .expect("fixture response");
    let invocation = CapabilityInvocationContext {
        grant_id: "grant.provider-bridge.1".to_string(),
        subject: response.subject.clone(),
        presenter: response.presenter.clone(),
        audience: response.audience.clone(),
        catalog_snapshot_id: response.catalog_snapshot_id.clone(),
        module_id: response.selected_entry.module_id.clone(),
        module_version: response.selected_entry.module_version.clone(),
        response_nonce: response.response_nonce.clone(),
    };
    let mut request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;
    request.capability_catalog = Some(catalog.clone());
    request.capability_invocation_context = Some(invocation.clone());
    (
        request,
        ProviderCapabilityContext {
            catalog,
            invocation,
        },
        response,
    )
}

fn typed_decision(response: AgentCommandResponse) -> DecisionResponse {
    DecisionResponse {
        decision: ProviderDecision::ModuleCommandResponse { response },
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics::default(),
        trace_payload: ProviderTraceEnvelope::default(),
        memory_write_intents: Vec::new(),
    }
}

#[test]
fn provider_loopback_typed_response_reaches_executor_exactly_once_without_core_action() {
    let (request, _context, response) = typed_provider_bridge_fixture();
    let expected_response = response.clone();
    let adapter = ProviderLoopbackAdapter::new("http://127.0.0.1:1", None, 200).expect("adapter");
    let mut executor = RecordingTrustedModuleExecutor::default();

    let receipt = adapter
        .execute_trusted_module_command(&request, typed_decision(response), &mut executor)
        .expect("typed response should route");

    assert_eq!(executor.responses, vec![expected_response]);
    assert_eq!(receipt.decision, "accepted");
    assert_eq!(executor.responses[0].envelope.payload, vec![1, 2, 3]);
    assert!(matches!(
        typed_decision(executor.responses[0].clone()).decision,
        ProviderDecision::ModuleCommandResponse { .. }
    ));
}

#[test]
fn provider_loopback_typed_response_rejects_missing_forged_and_stale_context_before_executor() {
    let (request, _context, response) = typed_provider_bridge_fixture();
    let adapter = ProviderLoopbackAdapter::new("http://127.0.0.1:1", None, 200).expect("adapter");

    let mut missing_request = request.clone();
    missing_request.capability_catalog = None;
    missing_request.capability_invocation_context = None;
    let mut missing_executor = RecordingTrustedModuleExecutor::default();
    let missing_error = adapter
        .execute_trusted_module_command(
            &missing_request,
            typed_decision(response.clone()),
            &mut missing_executor,
        )
        .expect_err("missing context must fail closed");
    assert_eq!(missing_error.code, "module_command_catalog_unavailable");
    assert!(missing_executor.responses.is_empty());

    let mut forged_response = response.clone();
    forged_response.subject = serde_json::from_value(serde_json::json!({
        "kind": "agent",
        "agent_id": "agent-forged",
        "owner_binding": "owner-1",
        "generation": 1
    }))
    .expect("forged subject");
    let mut forged_executor = RecordingTrustedModuleExecutor::default();
    let forged_error = adapter
        .execute_trusted_module_command(
            &request,
            typed_decision(forged_response),
            &mut forged_executor,
        )
        .expect_err("forged response must fail closed");
    assert_eq!(forged_error.code, "module_command_context_mismatch");
    assert!(forged_executor.responses.is_empty());

    let mut stale_request = request;
    stale_request
        .capability_invocation_context
        .as_mut()
        .expect("invocation context")
        .response_nonce = "nonce.stale".to_string();
    let mut stale_executor = RecordingTrustedModuleExecutor::default();
    let stale_error = adapter
        .execute_trusted_module_command(
            &stale_request,
            typed_decision(response),
            &mut stale_executor,
        )
        .expect_err("stale response must fail closed");
    assert_eq!(stale_error.code, "module_command_context_mismatch");
    assert!(stale_executor.responses.is_empty());
}

#[test]
fn provider_backed_behavior_returns_typed_module_command_decision_without_core_action() {
    let (_request, context, response) = typed_provider_bridge_fixture();
    let provider = MockDecisionProvider::with_scripted_responses(
        "provider-typed",
        vec![Ok(typed_decision(response.clone()))],
    );
    let mut behavior =
        ProviderBackedAgentBehavior::new("agent-1", provider, provider_phase1_action_catalog())
            .legacy_compatibility()
            .with_capability_context(context);
    let mut kernel = setup_kernel_with_provider_agent("agent-1");
    let observation = kernel.observe("agent-1").expect("observation");

    let decision = behavior.decide(&observation);
    assert_eq!(
        decision,
        AgentDecision::ModuleCommand {
            response: response.clone()
        }
    );
    assert!(decision.action().is_none());
}

fn spawn_mock_http_server<F>(expected_connections: usize, handler: F) -> String
where
    F: Fn(RecordedHttpRequest) -> MockHttpResponse + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock http server");
    let bind = listener.local_addr().expect("listener addr");
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for _ in 0..expected_connections {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            let request = read_http_request(&mut stream);
            let response = handler(request);
            write_json_response(&mut stream, response.status_code, response.body.as_str());
        }
    });
    format!("http://{}", bind)
}

fn read_http_request(stream: &mut TcpStream) -> RecordedHttpRequest {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let bytes = stream.read(&mut chunk).expect("read request bytes");
        if bytes == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes]);
        if header_end.is_none() {
            header_end = find_header_terminator(buffer.as_slice());
            if let Some(boundary) = header_end {
                let header = std::str::from_utf8(&buffer[..boundary]).expect("utf8 header");
                content_length = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
            }
        }
        if let Some(boundary) = header_end
            && buffer.len() >= boundary + 4 + content_length
        {
            break;
        }
    }

    let boundary = header_end.expect("header boundary");
    let header = std::str::from_utf8(&buffer[..boundary]).expect("utf8 header");
    let mut lines = header.lines();
    let request_line = lines.next().expect("request line");
    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts.next().expect("method").to_string();
    let path = request_line_parts.next().expect("path").to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let body = buffer[(boundary + 4)..(boundary + 4 + content_length)].to_vec();

    RecordedHttpRequest {
        method,
        path,
        headers,
        body,
    }
}

fn find_header_terminator(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_json_response(stream: &mut TcpStream, status_code: u16, body: &str) {
    let status_text = match status_code {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .expect("write mock response");
}
