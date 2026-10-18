use super::recovery_ledger::{
    RECOVERY_METRIC_SCHEMA_VERSION, RecoveryActionEvidence, RecoveryErrorEvidence, RecoveryEvent,
    RecoveryLedger, RecoveryLineage, assess_recovery_events, metric_summary,
};
use super::*;
use oasis7::simulator::{
    ContinuousAgentResponseContextV1, DecisionProvider, DecisionResponse, FeedbackEnvelopeV1,
    cognition_response_digest, golden_decision_provider_fixtures,
};
use oasis7_simulator::Action;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[path = "target_context_legacy_tests.rs"]
mod target_context_legacy_tests;

#[path = "provider_info_tests.rs"]
mod provider_info_tests;

const ORIGIN_DIGEST: &str =
    "blake3:0000000000000000000000000000000000000000000000000000000000000000";
const SECOND_ORIGIN_DIGEST: &str =
    "blake3:1111111111111111111111111111111111111111111111111111111111111111";
const RECOVERY_DIGEST: &str =
    "blake3:2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn parse_options_accepts_provider_loopback_http() {
    let options = parse_options(
        [
            "--provider",
            "provider_loopback_http",
            "--scenario-id",
            "P0-002",
            "--benchmark-run-id",
            "run-1",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--out-dir",
            ".tmp/parity",
        ]
        .into_iter(),
    )
    .expect("parse options");
    assert_eq!(options.provider, BenchProviderKind::ProviderLoopbackHttp);
    assert_eq!(options.scenario_id, "P0-002");
    assert_eq!(options.benchmark_run_id, "run-1");
    assert_eq!(
        options.provider_base_url.as_deref(),
        Some("http://127.0.0.1:5841")
    );
    assert_eq!(
        options.agent_provider_profile,
        DEFAULT_PROVIDER_AGENT_PROFILE
    );
}

#[test]
fn parse_options_rejects_provider_loopback_http_without_base_url() {
    let err = parse_options(
        [
            "--provider",
            "provider_loopback_http",
            "--benchmark-run-id",
            "run-1",
        ]
        .into_iter(),
    )
    .expect_err("missing base url should fail");
    assert!(err.contains("--agent-provider-url"));
}

#[test]
fn parse_options_accepts_custom_provider_agent_profile() {
    let options = parse_options(
        [
            "--provider",
            "provider_loopback_http",
            "--benchmark-run-id",
            "run-2",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--agent-provider-profile",
            "oasis7_p1_memory_loop",
        ]
        .into_iter(),
    )
    .expect("parse custom profile");
    assert_eq!(options.agent_provider_profile, "oasis7_p1_memory_loop");
}

#[test]
fn parse_options_defaults_use_real_provider_timeout_budget() {
    let options =
        parse_options(["--benchmark-run-id", "run-defaults"].into_iter()).expect("parse defaults");
    assert_eq!(options.timeout_ms, 15_000);
    assert_eq!(options.agent_provider_connect_timeout_ms, 15_000);
}

#[test]
fn builtin_parity_short_term_goal_matches_memory_summary() {
    assert_eq!(
        builtin_parity_short_term_goal("P0-001").as_deref(),
        parity_memory_summary("P0-001")
    );
    assert_eq!(builtin_parity_short_term_goal("unknown"), None);
}

fn sample_patrol_observation() -> Observation {
    Observation {
        time: 7,
        agent_id: "agent-1".to_string(),
        pos: oasis7::geometry::GeoPos {
            x_cm: 0,
            y_cm: 0,
            z_cm: 0,
        },
        self_resources: Default::default(),
        visibility_range_cm: 1_000,
        visible_agents: Vec::new(),
        visible_locations: vec![
            oasis7_simulator::ObservedLocation {
                location_id: "loc-1".to_string(),
                name: "base".to_string(),
                pos: oasis7::geometry::GeoPos {
                    x_cm: 0,
                    y_cm: 0,
                    z_cm: 0,
                },
                profile: Default::default(),
                distance_cm: 0,
            },
            oasis7_simulator::ObservedLocation {
                location_id: "loc-2".to_string(),
                name: "neighbor".to_string(),
                pos: oasis7::geometry::GeoPos {
                    x_cm: 100,
                    y_cm: 0,
                    z_cm: 0,
                },
                profile: Default::default(),
                distance_cm: 100,
            },
        ],
        module_lifecycle: Default::default(),
        module_market: Default::default(),
        power_market: Default::default(),
        social_state: Default::default(),
    }
}

#[test]
fn builtin_parity_guardrail_reroutes_passive_patrol_decision_to_move() {
    let observation = sample_patrol_observation();
    let (decision, note) =
        apply_builtin_parity_guardrail("P0-001", "agent-1", &observation, AgentDecision::Wait);
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        })
    );
    assert!(
        note.unwrap_or_default()
            .contains("builtin_parity_guardrail")
    );
}

#[test]
fn builtin_parity_guardrail_reroutes_non_move_patrol_decision_to_move() {
    let observation = sample_patrol_observation();
    let (decision, note) = apply_builtin_parity_guardrail(
        "P0-001",
        "agent-1",
        &observation,
        AgentDecision::Act(Action::HarvestRadiation {
            agent_id: "agent-1".to_string(),
            max_amount: 3,
        }),
    );
    assert_eq!(
        decision,
        AgentDecision::Act(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-2".to_string(),
        })
    );
    assert!(note.unwrap_or_default().contains("act:other"));
}

#[test]
fn builtin_parity_guardrail_keeps_valid_move_agent_decision() {
    let observation = sample_patrol_observation();
    let decision = AgentDecision::Act(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    let (rewritten, note) =
        apply_builtin_parity_guardrail("P0-001", "agent-1", &observation, decision.clone());
    assert_eq!(rewritten, decision);
    assert_eq!(note, None);
}

#[test]
fn parity_target_context_is_production_valid() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden parity fixture");
    let observation = sample_patrol_observation();
    let (turn_context, request_context) = build_target_context(
        fixture.request,
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-test",
        0,
    );

    assert_eq!(turn_context.request_digest, request_context.request_digest);
    request_context
        .validate_production_lane()
        .expect("parity target request must satisfy the production lane");
    turn_context
        .validate_for_agent("agent-1")
        .expect("parity target turn context must retain actor identity");
}

#[test]
fn parity_target_context_starts_each_logical_turn_at_transport_attempt_one() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden parity fixture");
    let observation = sample_patrol_observation();
    let (_, request_context) = build_target_context(
        fixture.request,
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-transport-attempt-test",
        7,
    );

    assert_eq!(request_context.retry_seq, 1);
    assert_eq!(
        request_context.transport_attempt, 1,
        "a fresh logical turn must not inherit the previous turn's transport attempt"
    );
}

#[test]
fn blocked_or_failed_summary_returns_nonzero_exit_code() {
    assert_eq!(exit_code_for_status("passed"), 0);
    assert_ne!(exit_code_for_status("blocked"), 0);
    assert_ne!(exit_code_for_status("failed"), 0);
}

#[test]
fn simulator_smoke_never_claims_runtime_certification() {
    assert_eq!(LOCAL_EXECUTION_AUTHORITY, "simulator_world_kernel");
    assert_eq!(RUNTIME_CERTIFICATION_STATUS, "not_certified");
    assert!(RUNTIME_CERTIFICATION_REASON.contains("local simulator smoke"));
}

#[test]
fn parity_target_context_semantic_retry_increments_and_binds_origin() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden parity fixture");
    let observation = sample_patrol_observation();
    let (_, first_request) = build_target_context(
        fixture.request.clone(),
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-retry-test",
        0,
    );
    let origin = RecoveryLineage {
        agent_id: first_request.agent_subject.clone(),
        agent_session_id: first_request.agent_session_id.clone(),
        recovery_chain_id: "parity-recovery-chain:retry-test".to_string(),
        agent_turn_id: first_request.agent_turn_id.clone(),
        decision_request_id: first_request.decision_request_id.clone(),
        request_digest: first_request.request_digest.to_string(),
    };
    let (retry_turn, retry_request) = target_context::build_target_context_for_retry(
        fixture.request,
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-retry-test",
        1,
        2,
        &origin,
    );

    assert_eq!(retry_request.retry_seq, 2);
    assert_ne!(retry_request.agent_turn_id, origin.agent_turn_id);
    assert_ne!(
        retry_request.decision_request_id,
        origin.decision_request_id
    );
    assert_ne!(
        retry_request.continuation_digest,
        first_request.continuation_digest
    );
    assert_ne!(retry_request.request_digest, first_request.request_digest);
    let continuation = retry_turn
        .continuation
        .as_ref()
        .expect("semantic retry must carry a continuation proposal");
    assert_eq!(continuation.origin_turn_id, origin.agent_turn_id);
    assert_eq!(continuation.origin_request_digest, origin.request_digest);
    retry_request
        .validate_production_lane()
        .expect("semantic retry request must remain production-valid");
    retry_turn
        .validate_for_agent("agent-1")
        .expect("semantic retry turn must remain agent-valid");
}

#[test]
fn parity_target_route_round_trip_uses_outer_context_endpoints() {
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden parity fixture");
    let observation = sample_patrol_observation();
    let (turn_context, request_context) = build_target_context(
        fixture.request,
        &observation,
        fixture.fixture_id.as_str(),
        "parity-session-route-test",
        0,
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind route test listener");
    let bind = listener.local_addr().expect("route test listener address");
    let paths = Arc::new(Mutex::new(Vec::<String>::new()));
    let paths_for_server = Arc::clone(&paths);
    let response_context = request_context.clone();
    let serve = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept route request");
            let mut request = [0_u8; 8 * 1024];
            let bytes = stream.read(&mut request).expect("read route request");
            let request_text = String::from_utf8_lossy(&request[..bytes]);
            let path = request_text
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or_default()
                .to_string();
            paths_for_server
                .lock()
                .expect("route test path lock")
                .push(path.clone());
            let body = if path == "/v1/world-simulator/decision-context" {
                let base_response = DecisionResponse::wait("parity-route-test-provider");
                serde_json::to_string(&ContinuousAgentResponseContextV1 {
                    base_decision_response: base_response.clone(),
                    context_discriminator: response_context.context_discriminator.clone(),
                    context_version: response_context.context_version,
                    agent_session_id: response_context.agent_session_id.clone(),
                    agent_turn_id: response_context.agent_turn_id.clone(),
                    decision_request_id: response_context.decision_request_id.clone(),
                    retry_seq: response_context.retry_seq,
                    transport_attempt: response_context.transport_attempt,
                    request_digest: response_context.request_digest.clone(),
                    response_digest: cognition_response_digest(&base_response),
                })
                .expect("encode route test response")
            } else {
                r#"{"ok":true}"#.to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write route test response");
        }
    });

    let mut adapter = ProviderLoopbackAdapter::new(&format!("http://{bind}"), None, 5_000)
        .expect("create loopback adapter");
    let response = adapter
        .decide_with_continuous_request_context(
            &request_context.base_decision_request,
            &turn_context,
            &request_context,
        )
        .expect("target decision-context request");
    assert_eq!(response.request_digest, request_context.request_digest);
    adapter
        // This is a route-only smoke probe. A local simulator run has no
        // Runtime receipt, so it must use a non-committed disposition here.
        .push_continuous_feedback(&FeedbackEnvelopeV1 {
            feedback_id: "parity-route-test-feedback".to_string(),
            feedback_seq: 1,
            agent_subject: "agent-1".to_string(),
            agent_session_id: request_context.agent_session_id.clone(),
            agent_turn_id: request_context.agent_turn_id.clone(),
            decision_request_id: request_context.decision_request_id.clone(),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "failed".to_string(),
            request_digest: request_context.request_digest.clone(),
            reject_reason: Some("local_simulator_smoke_not_runtime_certified".to_string()),
            provenance: "runtime_authoritative".to_string(),
        })
        .expect("target feedback-context request");
    serve.join().expect("route test server should finish");

    assert_eq!(
        *paths.lock().expect("route test path lock"),
        vec![
            "/v1/world-simulator/decision-context".to_string(),
            "/v1/world-simulator/feedback-context".to_string(),
        ]
    );
}

#[test]
fn parse_options_accepts_provider_player_parity_execution_mode() {
    let options = parse_options(
        [
            "--provider",
            "provider_loopback_http",
            "--benchmark-run-id",
            "run-3",
            "--agent-provider-url",
            "http://127.0.0.1:5841",
            "--execution-mode",
            "player_parity",
        ]
        .into_iter(),
    )
    .expect("parse parity execution mode");
    assert_eq!(options.execution_mode, ProviderExecutionMode::PlayerParity);
}

#[test]
fn parse_options_rejects_builtin_player_parity_execution_mode() {
    let err = parse_options(
        [
            "--provider",
            "builtin",
            "--benchmark-run-id",
            "run-4",
            "--execution-mode",
            "player_parity",
        ]
        .into_iter(),
    )
    .expect_err("builtin parity mode should fail");
    assert!(err.contains("provider_loopback_http"));
}

#[test]
fn scenario_goal_completed_uses_p0_rules() {
    let mut action_kind_counts = BTreeMap::new();
    action_kind_counts.insert("move_agent".to_string(), 3);
    assert!(scenario_goal_completed(
        "P0-001",
        &action_kind_counts,
        &BTreeMap::new(),
        0,
        &metric_summary(0, 0),
    ));

    action_kind_counts.clear();
    action_kind_counts.insert("simple_interact".to_string(), 1);
    assert!(scenario_goal_completed(
        "P0-004",
        &action_kind_counts,
        &BTreeMap::new(),
        0,
        &metric_summary(0, 0),
    ));
    assert!(!scenario_goal_completed(
        "P0-004",
        &action_kind_counts,
        &BTreeMap::new(),
        1,
        &metric_summary(0, 0),
    ));
}

fn recovery_lineage(
    agent_id: &str,
    session_id: &str,
    chain_id: &str,
    turn_id: &str,
    request_id: &str,
    request_digest: &str,
) -> RecoveryLineage {
    RecoveryLineage {
        agent_id: agent_id.to_string(),
        agent_session_id: session_id.to_string(),
        recovery_chain_id: chain_id.to_string(),
        agent_turn_id: turn_id.to_string(),
        decision_request_id: request_id.to_string(),
        request_digest: request_digest.to_string(),
    }
}

fn recovery_error(lineage: RecoveryLineage) -> RecoveryErrorEvidence {
    RecoveryErrorEvidence {
        error_code: "timeout".to_string(),
        lineage,
    }
}

fn recovery_action(
    origin: RecoveryLineage,
    recovery_turn: &str,
    recovery_request: &str,
    recovery_digest: &str,
    action_id: u64,
) -> RecoveryActionEvidence {
    let lineage = recovery_lineage(
        &origin.agent_id,
        &origin.agent_session_id,
        &origin.recovery_chain_id,
        recovery_turn,
        recovery_request,
        recovery_digest,
    );
    RecoveryActionEvidence {
        lineage,
        origin,
        action_id,
        authority_ref: format!("fixture-action://action-{action_id}"),
        retry_seq: 2,
    }
}

fn recovery_ledger_with_error_and_resolution(resolution_count: usize) -> RecoveryLedger {
    let mut ledger = RecoveryLedger::new("sample-recovery");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin.clone()));
    for index in 0..resolution_count {
        let _ = ledger.record_action_committed(recovery_action(
            origin.clone(),
            &format!("turn-{}", index + 2),
            &format!("request-{}", index + 2),
            RECOVERY_DIGEST,
            index as u64 + 1,
        ));
    }
    ledger
}

#[test]
fn recovery_ledger_happy_path_records_ordered_host_resolution() {
    let assessment = recovery_ledger_with_error_and_resolution(1).assess();
    assert_eq!(assessment.trace_validity.as_str(), "valid");
    assert_eq!(assessment.metric.numerator, 1);
    assert_eq!(assessment.metric.denominator, 1);
    assert_eq!(assessment.metric.value, Some(1.0));
    assert_eq!(assessment.metric.gate_status, "evaluable");
    assert_eq!(assessment.recovery_events.len(), 2);
    assert_eq!(assessment.recovery_events[0].event_seq, 1);
    assert_eq!(assessment.recovery_events[1].event_seq, 2);
    assert_eq!(
        assessment.recovery_events[0].request_digest.as_deref(),
        Some(ORIGIN_DIGEST)
    );
    assert_eq!(
        assessment.recovery_events[1].authority.as_deref(),
        Some("runtime_or_fixture_host")
    );
    assert_eq!(
        assessment.recovery_events[1].origin_turn_id.as_deref(),
        Some("turn-1")
    );
    assert_eq!(
        assessment.recovery_events[1]
            .origin_request_digest
            .as_deref(),
        Some(ORIGIN_DIGEST)
    );
    assert_eq!(
        assessment.recovery_events[1].sample_id.as_deref(),
        Some("sample-recovery")
    );
    assert_eq!(
        assessment.recovery_events[1].decision_request_id.as_deref(),
        Some("request-2")
    );
    assert_eq!(
        assessment.recovery_events[1].request_digest.as_deref(),
        Some(RECOVERY_DIGEST)
    );
    assert_eq!(assessment.recovery_events[1].retry_seq, Some(2));
}

#[test]
fn recovery_ledger_rejects_resolution_before_semantic_retry() {
    let mut ledger = RecoveryLedger::new("sample-retry");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin.clone()));
    let mut evidence = recovery_action(origin, "turn-2", "request-2", RECOVERY_DIGEST, 1);
    evidence.retry_seq = 1;
    assert!(ledger.record_action_committed(evidence).is_none());
    assert_eq!(ledger.assess().recovery_events.len(), 1);

    let mut events = recovery_ledger_with_error_and_resolution(1)
        .assess()
        .recovery_events;
    events[1].retry_seq = Some(1);
    let assessment = assess_recovery_events(events);
    assert_eq!(assessment.trace_validity.as_str(), "blocked");
    assert_eq!(assessment.metric.value, None);
    assert!(assessment.errors.iter().any(|error| {
        error.contains("recovery_resolved") && error.contains("retry_seq must be at least 2")
    }));
}

#[test]
fn recovery_ledger_rejects_resolved_event_without_sample_identity() {
    let error = RecoveryEvent {
        event_kind: "recoverable_error".to_string(),
        event_seq: 1,
        error_id: "error-1".to_string(),
        error_code: Some("timeout".to_string()),
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-1".to_string(),
        decision_request_id: Some("request-1".to_string()),
        request_digest: Some(ORIGIN_DIGEST.to_string()),
        retry_seq: Some(1),
        origin_turn_id: None,
        origin_request_digest: None,
        authority: None,
        runtime_outcome: None,
        authority_ref: None,
    };
    let resolved = RecoveryEvent {
        event_kind: "recovery_resolved".to_string(),
        event_seq: 2,
        error_id: "error-1".to_string(),
        error_code: None,
        sample_id: None,
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-2".to_string(),
        decision_request_id: Some("request-2".to_string()),
        request_digest: Some(RECOVERY_DIGEST.to_string()),
        retry_seq: Some(2),
        origin_turn_id: Some("turn-1".to_string()),
        origin_request_digest: Some(ORIGIN_DIGEST.to_string()),
        authority: Some("runtime_or_fixture_host".to_string()),
        runtime_outcome: Some("action_committed".to_string()),
        authority_ref: Some("fixture-action://action-1".to_string()),
    };
    let assessment = assess_recovery_events(vec![error, resolved]);
    assert_eq!(assessment.trace_validity.as_str(), "blocked");
    assert!(
        assessment
            .errors
            .iter()
            .any(|error| { error.contains("recovery_resolved") && error.contains("sample_id") })
    );
}

#[test]
fn recovery_ledger_timeout_only_is_unresolved() {
    let assessment = recovery_ledger_with_error_and_resolution(0).assess();
    assert_eq!(assessment.metric.numerator, 0);
    assert_eq!(assessment.metric.denominator, 1);
    assert_eq!(assessment.metric.value, Some(0.0));
    assert_eq!(assessment.metric.gate_status, "evaluable");
}

#[test]
fn p0_005_goal_flag_cannot_replace_recovery_resolution() {
    let mut actions = BTreeMap::new();
    actions.insert("wait".to_string(), 1);
    let assessment = recovery_ledger_with_error_and_resolution(0).assess();
    let mut errors = BTreeMap::new();
    errors.insert("timeout".to_string(), 1);
    assert!(!scenario_goal_completed(
        "P0-005",
        &actions,
        &errors,
        0,
        &assessment.metric,
    ));
}

#[test]
fn recovery_ledger_partial_resolution_counts_occurrences() {
    let mut ledger = RecoveryLedger::new("sample-partial");
    let origin_one = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    let origin_two = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-2",
        "turn-2",
        "request-2",
        SECOND_ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin_one.clone()));
    ledger.record_recoverable_error(recovery_error(origin_two));
    let _ = ledger.record_action_committed(recovery_action(
        origin_one,
        "turn-3",
        "request-3",
        RECOVERY_DIGEST,
        1,
    ));
    let assessment = ledger.assess();
    assert_eq!(assessment.metric.numerator, 1);
    assert_eq!(assessment.metric.denominator, 2);
    assert_eq!(assessment.metric.value, Some(0.5));
}

#[test]
fn recovery_ledger_unrelated_success_does_not_resolve_pending_error() {
    let mut ledger = RecoveryLedger::new("sample-unrelated");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin.clone()));
    let unrelated_origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-unrelated",
        "request-unrelated",
        SECOND_ORIGIN_DIGEST,
    );
    assert!(
        ledger
            .record_action_committed(recovery_action(
                unrelated_origin,
                "turn-2",
                "request-2",
                RECOVERY_DIGEST,
                1,
            ))
            .is_none()
    );
    let assessment = ledger.assess();
    assert_eq!(assessment.metric.numerator, 0);
    assert_eq!(assessment.metric.denominator, 1);
}

#[test]
fn recovery_ledger_wrong_chain_does_not_resolve_pending_error() {
    let mut ledger = RecoveryLedger::new("sample-wrong-chain");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin));
    let wrong_chain = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-2",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    assert!(
        ledger
            .record_action_committed(recovery_action(
                wrong_chain,
                "turn-2",
                "request-2",
                RECOVERY_DIGEST,
                1,
            ))
            .is_none()
    );
    assert_eq!(ledger.assess().metric.numerator, 0);
}

#[test]
fn recovery_ledger_cross_agent_success_does_not_resolve_pending_error() {
    let mut ledger = RecoveryLedger::new("sample-cross-agent");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin));
    let other_agent = recovery_lineage(
        "agent-2",
        "session-2",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    assert!(
        ledger
            .record_action_committed(recovery_action(
                other_agent,
                "turn-2",
                "request-2",
                RECOVERY_DIGEST,
                1,
            ))
            .is_none()
    );
    assert_eq!(ledger.assess().metric.numerator, 0);
}

#[test]
fn recovery_ledger_missing_chain_evidence_stays_unresolved() {
    let mut ledger = RecoveryLedger::new("sample-missing-chain");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin));
    let missing_chain = recovery_lineage(
        "agent-1",
        "session-1",
        "",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    assert!(
        ledger
            .record_action_committed(recovery_action(
                missing_chain.clone(),
                "turn-2",
                "request-2",
                RECOVERY_DIGEST,
                1,
            ))
            .is_none()
    );
    assert!(missing_chain.recovery_chain_id.is_empty());
    assert_eq!(ledger.assess().metric.numerator, 0);
}

#[test]
fn recovery_ledger_duplicate_resolution_is_counted_once() {
    let mut ledger = RecoveryLedger::new("sample-duplicate");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin.clone()));
    let evidence = recovery_action(origin, "turn-2", "request-2", RECOVERY_DIGEST, 1);
    assert!(ledger.record_action_committed(evidence.clone()).is_some());
    assert!(ledger.record_action_committed(evidence).is_none());
    let assessment = ledger.assess();
    assert_eq!(assessment.metric.numerator, 1);
    assert_eq!(assessment.metric.denominator, 1);
    assert_eq!(assessment.recovery_events.len(), 2);
}

#[test]
fn recovery_ledger_action_authority_must_bind_to_action_id() {
    let mut ledger = RecoveryLedger::new("sample-authority-binding");
    let origin = recovery_lineage(
        "agent-1",
        "session-1",
        "chain-1",
        "turn-1",
        "request-1",
        ORIGIN_DIGEST,
    );
    ledger.record_recoverable_error(recovery_error(origin.clone()));
    let mut mismatched = recovery_action(origin.clone(), "turn-2", "request-2", RECOVERY_DIGEST, 7);
    mismatched.authority_ref = "fixture-action://action-8".to_string();
    assert!(ledger.record_action_committed(mismatched).is_none());
    assert_eq!(ledger.assess().metric.numerator, 0);
    assert!(
        ledger
            .record_action_committed(recovery_action(
                origin,
                "turn-2",
                "request-2",
                RECOVERY_DIGEST,
                7,
            ))
            .is_some()
    );
    assert_eq!(ledger.assess().metric.numerator, 1);
}

#[test]
fn recovery_ledger_zero_case_is_not_evaluable() {
    let assessment = RecoveryLedger::new("sample-zero").assess();
    assert_eq!(assessment.metric.numerator, 0);
    assert_eq!(assessment.metric.denominator, 0);
    assert_eq!(assessment.metric.value, None);
    assert_eq!(
        assessment.metric.zero_case.as_deref(),
        Some("not_applicable")
    );
    assert_eq!(assessment.metric.gate_status, "not_evaluable");
}

#[test]
fn recovery_ledger_malformed_or_out_of_order_is_blocked() {
    let malformed = RecoveryEvent {
        event_kind: "recovery_resolved".to_string(),
        event_seq: 1,
        error_id: "error-1".to_string(),
        error_code: None,
        sample_id: None,
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-1".to_string(),
        decision_request_id: None,
        request_digest: None,
        retry_seq: None,
        origin_turn_id: Some("turn-0".to_string()),
        origin_request_digest: Some("blake3:origin".to_string()),
        authority: Some("runtime_or_fixture_host".to_string()),
        runtime_outcome: Some("action_committed".to_string()),
        authority_ref: None,
    };
    let error = RecoveryEvent {
        event_kind: "recoverable_error".to_string(),
        event_seq: 2,
        error_id: "error-1".to_string(),
        error_code: Some("timeout".to_string()),
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-0".to_string(),
        decision_request_id: Some("request-1".to_string()),
        request_digest: Some(ORIGIN_DIGEST.to_string()),
        retry_seq: Some(1),
        origin_turn_id: None,
        origin_request_digest: None,
        authority: None,
        runtime_outcome: None,
        authority_ref: None,
    };
    let assessment = assess_recovery_events(vec![malformed, error]);
    assert_eq!(assessment.trace_validity.as_str(), "blocked");
    assert_eq!(assessment.metric.gate_status, "blocked");
    assert_eq!(assessment.metric.value, None);
    assert_eq!(assessment.metric.denominator, 1);
    assert_eq!(
        RECOVERY_METRIC_SCHEMA_VERSION,
        "recoverable_error_resolution_rate.v1"
    );
}

#[test]
fn recovery_ledger_binds_resolved_origin_digest_to_original_error() {
    let error = RecoveryEvent {
        event_kind: "recoverable_error".to_string(),
        event_seq: 1,
        error_id: "error-1".to_string(),
        error_code: Some("timeout".to_string()),
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-1".to_string(),
        decision_request_id: Some("request-1".to_string()),
        request_digest: Some(ORIGIN_DIGEST.to_string()),
        retry_seq: Some(1),
        origin_turn_id: None,
        origin_request_digest: None,
        authority: None,
        runtime_outcome: None,
        authority_ref: None,
    };
    let resolved = RecoveryEvent {
        event_kind: "recovery_resolved".to_string(),
        event_seq: 2,
        error_id: "error-1".to_string(),
        error_code: None,
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-2".to_string(),
        decision_request_id: Some("request-2".to_string()),
        request_digest: Some(RECOVERY_DIGEST.to_string()),
        retry_seq: Some(2),
        origin_turn_id: Some("turn-1".to_string()),
        origin_request_digest: Some(SECOND_ORIGIN_DIGEST.to_string()),
        authority: Some("runtime_or_fixture_host".to_string()),
        runtime_outcome: Some("action_committed".to_string()),
        authority_ref: Some("fixture-action://action-1".to_string()),
    };
    let assessment = assess_recovery_events(vec![error, resolved]);
    assert_eq!(assessment.trace_validity.as_str(), "blocked");
    assert_eq!(assessment.metric.gate_status, "blocked");
    assert_eq!(assessment.metric.value, None);
    assert!(assessment.errors.iter().any(|error| {
        error.contains("origin_request_digest") && error.contains("does not match")
    }));
}

#[test]
fn recovery_ledger_rejects_malformed_origin_request_digest() {
    let error = RecoveryEvent {
        event_kind: "recoverable_error".to_string(),
        event_seq: 1,
        error_id: "error-1".to_string(),
        error_code: Some("timeout".to_string()),
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-1".to_string(),
        decision_request_id: Some("request-1".to_string()),
        request_digest: Some(ORIGIN_DIGEST.to_string()),
        retry_seq: Some(1),
        origin_turn_id: None,
        origin_request_digest: None,
        authority: None,
        runtime_outcome: None,
        authority_ref: None,
    };
    let resolved = RecoveryEvent {
        event_kind: "recovery_resolved".to_string(),
        event_seq: 2,
        error_id: "error-1".to_string(),
        error_code: None,
        sample_id: Some("sample-recovery".to_string()),
        agent_id: "agent-1".to_string(),
        agent_session_id: "session-1".to_string(),
        recovery_chain_id: "chain-1".to_string(),
        agent_turn_id: "turn-2".to_string(),
        decision_request_id: Some("request-2".to_string()),
        request_digest: Some(RECOVERY_DIGEST.to_string()),
        retry_seq: Some(2),
        origin_turn_id: Some("turn-1".to_string()),
        origin_request_digest: Some("blake3:origin".to_string()),
        authority: Some("runtime_or_fixture_host".to_string()),
        runtime_outcome: Some("action_committed".to_string()),
        authority_ref: Some("fixture-action://action-1".to_string()),
    };
    let assessment = assess_recovery_events(vec![error, resolved]);
    assert_eq!(assessment.trace_validity.as_str(), "blocked");
    assert_eq!(assessment.metric.gate_status, "blocked");
    assert!(
        assessment.errors.iter().any(|error| {
            error.contains("origin_request_digest") && error.contains("canonical")
        })
    );
}

#[test]
fn classify_trace_error_detects_timeout() {
    let trace = AgentDecisionTrace {
        agent_id: "agent-1".to_string(),
        time: 1,
        decision: AgentDecision::Wait,
        llm_input: None,
        llm_output: None,
        llm_error: Some("timeout: provider request timed out".to_string()),
        parse_error: None,
        llm_diagnostics: None,
        llm_effect_intents: Vec::new(),
        llm_effect_receipts: Vec::new(),
        llm_step_trace: Vec::new(),
        llm_prompt_section_trace: Vec::new(),
        llm_chat_messages: Vec::new(),
    };
    assert_eq!(
        classify_trace_error(Some(&trace), None).as_deref(),
        Some("timeout")
    );
}
