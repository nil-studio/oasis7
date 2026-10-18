//! Native C0 request budget and retry accounting tests.

use super::agent_cognition_identity::{
    production_observation, production_request_fixture, production_turn_context, request_fixture,
    request_from_value,
};
use super::restore_runner_feedback_history;
use crate::simulator::{
    AgentBehavior, AgentCognitionStore, AsyncAgentRunner, LlmAgentBehavior,
    OpenAiChatCompletionClient,
};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn cognition_budget_fields_are_wire_data_bound_to_request_identity() {
    let mut limited = request_fixture(0, 60_000);
    limited["budget_contract"] = json!({
        "max_latency_ms": 60_000,
        "max_repair_attempts": 2,
        "max_model_calls": 2,
        "max_tool_calls": 1
    });
    let limited_context = request_from_value(limited.clone());
    let encoded = serde_json::to_value(&limited_context).expect("encode budget context");
    assert_eq!(encoded["budget_contract"]["max_model_calls"], json!(2));
    assert_eq!(encoded["budget_contract"]["max_tool_calls"], json!(1));

    for field in ["max_model_calls", "max_tool_calls"] {
        let mut changed = limited.clone();
        changed["budget_contract"][field] = json!(3);
        let changed_context = request_from_value(changed);
        assert_ne!(
            limited_context.request_digest, changed_context.request_digest,
            "changing {field} must change request identity"
        );
        assert_ne!(
            limited_context.provider_invocation_key(),
            changed_context.provider_invocation_key(),
            "changing {field} must change provider invocation identity"
        );

        let mut store = AgentCognitionStore::default();
        store
            .begin_request(limited_context.clone())
            .expect("first budget-bound request accepted");
        let error = store
            .begin_request(changed_context)
            .expect_err("changing budget on the same request key must fail closed");
        assert_eq!(error.code(), "request_identity_collision");
    }
}

#[test]
fn cognition_budget_wire_rejects_missing_model_or_tool_limits() {
    for field in ["max_model_calls", "max_tool_calls"] {
        let mut fixture = request_fixture(0, 60_000);
        fixture["budget_contract"]
            .as_object_mut()
            .expect("budget contract object")
            .remove(field);
        let error =
            serde_json::from_value::<crate::simulator::ContinuousAgentRequestContextV1>(fixture)
                .expect_err("missing budget limit must be rejected at wire decode");
        assert!(
            error.to_string().contains(field),
            "wire decode error must identify missing field {field}: {error}"
        );
    }
}

#[test]
fn cognition_budget_wire_rejects_unknown_authority_fields() {
    for (field, value) in [
        ("unlimited", json!(true)),
        ("max_calls", json!(100)),
        ("future_reservation", json!({"credits": 100})),
    ] {
        let mut fixture = request_fixture(0, 60_000);
        fixture["budget_contract"][field] = value;
        let wire_error =
            serde_json::from_value::<crate::simulator::ContinuousAgentRequestContextV1>(
                fixture.clone(),
            )
            .expect_err("V1 must not discard an unknown budget authority field");
        assert!(wire_error.to_string().contains(field));
        let context_error =
            crate::simulator::ContinuousAgentRequestContextV1::validate_value(&fixture)
                .expect_err("outer validation must reject the unknown nested budget field");
        assert_eq!(context_error.code(), "unknown_context_field");
    }
}

#[test]
fn native_budget_diagnostics_capture_success_usage_before_next_request_bind() {
    let first_request = request_from_value(production_request_fixture(1, 60_000));
    let mut second_fixture = production_request_fixture(2, 60_000);
    second_fixture["retry_seq"] = json!(1);
    let second_request = request_from_value(second_fixture);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut behavior = LlmAgentBehavior::new(
        "agent-1",
        retry_budget_llm_config(),
        RetryCountingLlmClient {
            calls: Arc::clone(&calls),
        },
    );

    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&first_request));
    assert_eq!(
        behavior.decide(&production_observation("agent-1", 42)),
        crate::simulator::AgentDecision::Wait
    );
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&second_request));
    let first_trace = behavior
        .take_decision_trace()
        .expect("successful request trace exists");

    assert_eq!(
        behavior.decide(&production_observation("agent-1", 43)),
        crate::simulator::AgentDecision::Wait
    );
    let second_trace = behavior
        .take_decision_trace()
        .expect("next request trace exists");

    let first_diagnostics = first_trace.llm_diagnostics.expect("first diagnostics");
    assert_eq!(first_diagnostics.max_model_calls, Some(4));
    assert_eq!(first_diagnostics.model_calls_used, Some(1));
    assert_eq!(first_diagnostics.max_tool_calls, Some(3));
    assert_eq!(first_diagnostics.tool_calls_used, Some(0));

    let second_diagnostics = second_trace.llm_diagnostics.expect("second diagnostics");
    assert_eq!(second_diagnostics.max_model_calls, Some(4));
    assert_eq!(second_diagnostics.model_calls_used, Some(1));
    assert_eq!(second_diagnostics.max_tool_calls, Some(3));
    assert_eq!(second_diagnostics.tool_calls_used, Some(0));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn native_budget_diagnostics_capture_one_category_exhaustion_before_next_request_bind() {
    let mut first_fixture = production_request_fixture(1, 60_000);
    first_fixture["budget_contract"]["max_model_calls"] = json!(1);
    first_fixture["budget_contract"]["max_tool_calls"] = json!(3);
    let first_request = request_from_value(first_fixture);
    let mut second_fixture = production_request_fixture(2, 60_000);
    second_fixture["retry_seq"] = json!(1);
    let second_request = request_from_value(second_fixture);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut behavior = LlmAgentBehavior::new(
        "agent-1",
        retry_budget_llm_config(),
        SequenceBudgetLlmClient {
            outputs: Arc::new(vec![
                r#"{"type":"module_call","module":"agent.modules.list","args":{}}"#.to_string(),
                r#"{"decision":"wait"}"#.to_string(),
            ]),
            calls: Arc::clone(&calls),
        },
    );

    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&first_request));
    assert_eq!(
        behavior.decide(&production_observation("agent-1", 42)),
        crate::simulator::AgentDecision::Wait
    );
    AgentBehavior::set_continuous_request_context(&mut behavior, Some(&second_request));
    let first_trace = behavior
        .take_decision_trace()
        .expect("exhausted request trace exists");
    let first_diagnostics = first_trace.llm_diagnostics.expect("exhaustion diagnostics");
    assert_eq!(first_diagnostics.max_model_calls, Some(1));
    assert_eq!(first_diagnostics.model_calls_used, Some(1));
    assert_eq!(first_diagnostics.max_tool_calls, Some(3));
    assert_eq!(first_diagnostics.tool_calls_used, Some(1));
    assert!(
        first_trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );

    assert_eq!(
        behavior.decide(&production_observation("agent-1", 43)),
        crate::simulator::AgentDecision::Wait
    );
    let second_trace = behavior
        .take_decision_trace()
        .expect("next request trace exists");
    let second_diagnostics = second_trace.llm_diagnostics.expect("next diagnostics");
    assert_eq!(second_diagnostics.max_model_calls, Some(4));
    assert_eq!(second_diagnostics.model_calls_used, Some(1));
    assert_eq!(second_diagnostics.max_tool_calls, Some(3));
    assert_eq!(second_diagnostics.tool_calls_used, Some(0));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[derive(Clone)]
struct RetryCountingLlmClient {
    calls: Arc<AtomicUsize>,
}

impl crate::simulator::llm_agent::LlmCompletionClient for RetryCountingLlmClient {
    fn complete(
        &self,
        request: &crate::simulator::llm_agent::LlmCompletionRequest,
    ) -> Result<crate::simulator::llm_agent::LlmCompletionResult, crate::simulator::LlmClientError>
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(crate::simulator::llm_agent::LlmCompletionResult {
            turns: vec![crate::simulator::llm_agent::LlmCompletionTurn::Decision {
                payload: json!({"decision": "wait"}),
            }],
            output: "{\"decision\":\"wait\"}".to_string(),
            model: Some(request.model.clone()),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            total_tokens: Some(2),
        })
    }
}

#[derive(Clone)]
struct SequenceBudgetLlmClient {
    outputs: Arc<Vec<String>>,
    calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct ErrorBudgetLlmClient;

impl crate::simulator::llm_agent::LlmCompletionClient for ErrorBudgetLlmClient {
    fn complete(
        &self,
        _request: &crate::simulator::llm_agent::LlmCompletionRequest,
    ) -> Result<crate::simulator::llm_agent::LlmCompletionResult, crate::simulator::LlmClientError>
    {
        Err(crate::simulator::LlmClientError::Http {
            message: "provider unavailable".to_string(),
        })
    }
}

impl crate::simulator::llm_agent::LlmCompletionClient for SequenceBudgetLlmClient {
    fn complete(
        &self,
        request: &crate::simulator::llm_agent::LlmCompletionRequest,
    ) -> Result<crate::simulator::llm_agent::LlmCompletionResult, crate::simulator::LlmClientError>
    {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let output = self
            .outputs
            .get(index)
            .cloned()
            .unwrap_or_else(|| r#"{"decision":"wait"}"#.to_string());
        let payload = serde_json::from_str(&output).expect("valid scripted budget payload");
        Ok(crate::simulator::llm_agent::LlmCompletionResult {
            turns: vec![crate::simulator::llm_agent::LlmCompletionTurn::Decision { payload }],
            output,
            model: Some(request.model.clone()),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            total_tokens: Some(2),
        })
    }
}

fn retry_budget_llm_config() -> crate::simulator::LlmAgentConfig {
    crate::simulator::LlmAgentConfig {
        model: "gpt-budget-test".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        api_key: "test-key".to_string(),
        timeout_ms: 1_000,
        system_prompt: "test".to_string(),
        short_term_goal: "test".to_string(),
        long_term_goal: "test".to_string(),
        max_module_calls: 3,
        max_decision_steps: 4,
        max_repair_rounds: 1,
        prompt_max_history_items: 4,
        prompt_profile: crate::simulator::llm_agent::LlmPromptProfile::Balanced,
        force_replan_after_same_action: 4,
        harvest_max_amount_cap: 100,
        execute_until_auto_reenter_ticks: 4,
        llm_debug_mode: false,
    }
}

#[test]
fn transport_retry_keeps_request_model_budget_consumed() {
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    fixture["budget_contract"]["max_model_calls"] = json!(1);
    fixture["budget_contract"]["max_tool_calls"] = json!(1);
    let request_context = request_from_value(fixture);
    let turn_context = production_turn_context(&request_context);
    let calls = Arc::new(AtomicUsize::new(0));
    let behavior = crate::simulator::LlmAgentBehavior::new(
        "agent-1",
        retry_budget_llm_config(),
        RetryCountingLlmClient {
            calls: Arc::clone(&calls),
        },
    );
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner.register(behavior).expect("register builtin actor");
    let world = crate::runtime_host::world::new();
    restore_runner_feedback_history(&mut runner, &world);
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context.clone(),
            request_context.clone(),
        )
        .expect("open production builtin turn");
    let first = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll production builtin turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        first.decision,
        Some(crate::simulator::AgentDecision::Wait),
        "first outcome: {first:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    runner
        .retry_awaiting_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 43),
            turn_context,
            request_context,
        )
        .expect("transport retry accepted");
    let retry = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll retried builtin turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        retry.lifecycle,
        crate::simulator::AsyncTurnLifecycle::Completed
    );
    assert_eq!(retry.feedback, crate::simulator::AsyncTurnFeedback::Wait);
    assert_eq!(
        retry.world_effect,
        crate::simulator::AsyncWorldEffect::NoEffect
    );
    assert_eq!(retry.decision, Some(crate::simulator::AgentDecision::Wait));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        retry
            .decision_trace
            .and_then(|trace| trace.llm_error)
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
}

// The listener polls accept, but request framing must be read in blocking mode:
// Darwin inherits O_NONBLOCK, and responding before draining a large request
// can reset the connection instead of delivering the intended SDK error event.
fn read_budget_provider_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_nonblocking(false)
        .expect("set blocking request stream");
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("set budget fixture read timeout");
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        assert!(header.len() < 16 * 1024, "budget fixture header too large");
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .expect("read budget fixture header");
        header.push(byte[0]);
    }
    let header = std::str::from_utf8(&header).expect("budget fixture header utf8");
    let lengths: Vec<usize> = header
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().expect("budget fixture content length"))
        })
        .collect();
    assert_eq!(lengths.len(), 1, "one budget fixture content length");
    assert!(lengths[0] <= 1024 * 1024, "budget fixture body too large");
    let mut body = vec![0; lengths[0]];
    stream
        .read_exact(&mut body)
        .expect("read complete budget fixture body");
    body
}

#[test]
fn budget_provider_fixture_drains_fragmented_large_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    // Reproduce the inherited Darwin mode on every platform.
    stream.set_nonblocking(true).unwrap();
    let body = vec![b'x'; 16 * 1024];
    let expected = body.clone();
    let sender = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        write!(
            client,
            "POST /v1/responses HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        client.write_all(&body[..4096]).unwrap();
        thread::sleep(Duration::from_millis(20));
        client.write_all(&body[4096..]).unwrap();
    });
    let received = read_budget_provider_request(&mut stream);
    assert_eq!(received.len(), expected.len(), "complete request body");
    assert_eq!(received, expected);
    sender.join().unwrap();
}

#[test]
fn native_model_budget_caps_openai_internal_concurrency_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind budgeted openai server");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let bind = listener.local_addr().expect("listener addr");
    let calls = Arc::new(AtomicUsize::new(0));
    let server_calls = Arc::clone(&calls);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let Ok((mut stream, _)) = listener.accept() else {
                thread::sleep(Duration::from_millis(5));
                continue;
            };
            let attempt = server_calls.fetch_add(1, Ordering::SeqCst);
            let _request = read_budget_provider_request(&mut stream);
            let event_payload = if attempt == 0 {
                serde_json::json!({
                    "error": {
                        "type": "rate_limit_error",
                        "message": "Concurrency limit exceeded for account, please retry later"
                    }
                })
                .to_string()
            } else {
                serde_json::json!({
                    "type": "response.completed",
                    "sequence_number": 1,
                    "response": {
                        "id": "resp_budget_retry_ok",
                        "object": "response",
                        "created_at": 1,
                        "completed_at": 2,
                        "model": "gpt-budget-test",
                        "output": [{
                            "type": "function_call",
                            "call_id": "call_decision",
                            "name": "agent_submit_decision",
                            "arguments": "{\"decision\":\"wait\"}"
                        }],
                        "status": "completed",
                        "parallel_tool_calls": false
                    }
                })
                .to_string()
            };
            let body = format!("data: {event_payload}\n\n");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            if attempt >= 1 {
                break;
            }
        }
    });

    let mut config = retry_budget_llm_config();
    config.base_url = format!("http://{bind}/v1");
    let client = OpenAiChatCompletionClient::from_config(&config).expect("client");
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    fixture["budget_contract"]["max_model_calls"] = json!(1);
    let request_context = request_from_value(fixture);
    let turn_context = production_turn_context(&request_context);
    let behavior = LlmAgentBehavior::new("agent-1", config, client);
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner.register(behavior).expect("register native actor");
    let world = crate::runtime_host::world::new();
    restore_runner_feedback_history(&mut runner, &world);
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context,
            request_context,
        )
        .expect("open budgeted native turn");
    let outcome = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll budgeted native turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        thread::yield_now();
    };

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "one native model admission must allow one physical provider request"
    );
    assert!(
        outcome
            .decision_trace
            .as_ref()
            .and_then(|trace| trace.llm_error.as_ref())
            .is_some_and(|error| error.contains("responses sdk decode failed")),
        "the budgeted request must retain the first provider failure instead of consuming an unadmitted retry: {outcome:?}"
    );
    server.join().expect("join budgeted openai server");
}

#[test]
fn budget_exhaustion_preserves_prior_native_trace_evidence() {
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["budget_contract"]["max_model_calls"] = json!(1);
    fixture["budget_contract"]["max_tool_calls"] = json!(1);
    let request_context = request_from_value(fixture);
    let calls = Arc::new(AtomicUsize::new(0));
    let behavior = crate::simulator::LlmAgentBehavior::new(
        "agent-1",
        retry_budget_llm_config(),
        SequenceBudgetLlmClient {
            outputs: Arc::new(vec![
                r#"{"type":"module_call","module":"agent.modules.list","args":{}}"#.to_string(),
            ]),
            calls: Arc::clone(&calls),
        },
    );
    let mut behavior = behavior;
    crate::simulator::AgentBehavior::set_continuous_request_context(
        &mut behavior,
        Some(&request_context),
    );

    assert_eq!(
        behavior.decide(&production_observation("agent-1", 42)),
        crate::simulator::AgentDecision::Wait
    );
    let trace = behavior
        .take_decision_trace()
        .expect("budget exhaustion trace exists");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace.llm_effect_receipts.is_empty());
    assert!(
        trace
            .llm_input
            .as_deref()
            .is_some_and(|input| !input.is_empty())
    );
    assert!(
        trace
            .llm_output
            .as_deref()
            .is_some_and(|output| output.contains("agent.modules.list"))
    );
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.status == "ok" && step.output_summary.contains("module_call"))
    );
    assert!(
        trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "budget_admission" && step.status == "denied")
    );
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
}

#[test]
fn native_provider_error_remains_failed_after_budget_wait_normalization() {
    let mut fixture = production_request_fixture(1, 60_000);
    fixture["retry_seq"] = json!(1);
    fixture["budget_contract"]["max_model_calls"] = json!(1);
    fixture["budget_contract"]["max_tool_calls"] = json!(1);
    let request_context = request_from_value(fixture);
    let turn_context = production_turn_context(&request_context);
    let behavior = crate::simulator::LlmAgentBehavior::new(
        "agent-1",
        retry_budget_llm_config(),
        ErrorBudgetLlmClient,
    );
    let mut runner = AsyncAgentRunner::new(16).expect("create target actor runner");
    runner.register(behavior).expect("register builtin actor");
    let world = crate::runtime_host::world::new();
    restore_runner_feedback_history(&mut runner, &world);
    let turn_id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            production_observation("agent-1", 42),
            turn_context,
            request_context,
        )
        .expect("open production builtin turn");
    let outcome = loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll production builtin turn")
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            break outcome;
        }
        std::thread::yield_now();
    };
    assert_eq!(
        outcome.lifecycle,
        crate::simulator::AsyncTurnLifecycle::Failed
    );
    assert!(matches!(
        outcome.feedback,
        crate::simulator::AsyncTurnFeedback::ProviderError { .. }
    ));
    assert!(outcome.decision.is_none());
    assert_eq!(
        outcome.world_effect,
        crate::simulator::AsyncWorldEffect::NoEffect
    );
}
