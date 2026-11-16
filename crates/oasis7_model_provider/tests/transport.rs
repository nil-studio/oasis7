use oasis7_agent_api::{ModelProvider, ModelProviderRequest, ModelProviderTurn};
use oasis7_model_provider::{CompletionToolProfile, LlmCompletionRequest};
use serde_json::{Value, json};
use std::{io::Write, net::TcpListener, sync::mpsc, thread, time::Duration};

mod support;
use support::{client, completion, read_request};
fn server(bodies: Vec<String>) -> (String, mpsc::Receiver<Value>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (send, receive) = mpsc::channel();
    let handle = thread::spawn(move || {
        for body in bodies {
            let (mut stream, _) = listener.accept().unwrap();
            send.send(read_request(&mut stream)).unwrap();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    (format!("http://{address}/v1"), receive, handle)
}
fn request() -> ModelProviderRequest {
    serde_json::from_value(json!({
        "request_digest":format!("blake3:{}", "a".repeat(64)),"model":"test-model",
        "system_prompt":"complete full authorized goal", "user_prompt":"prepared factory context",
        "tools":[{"name":"quote_factory","description":"Read a quote","parameters":{"type":"object","properties":{"factory":{"type":"string"}},"required":["factory"],"additionalProperties":false}}],
        "budget":{"max_latency_ms":2000,"max_repair_attempts":0,"max_model_calls":1,"max_tool_calls":1}
    })).unwrap()
}
#[test]
fn caller_tools_reach_production_wire_and_return_exact_typed_calls() {
    let (endpoint, received, handle) = server(vec![completion("quote_factory")]);
    let mut client = client(&endpoint);
    let request = request();
    let response = ModelProvider::complete_checked(&mut client, &request).unwrap();
    assert_eq!(response.request_digest, request.request_digest);
    assert_eq!(
        response.turns,
        vec![ModelProviderTurn::ToolCall {
            name: "quote_factory".into(),
            arguments: json!({"factory":"f1"})
        }]
    );
    assert_eq!(response.usage.total_tokens, Some(5));
    let body = received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(body["tools"][0]["name"], "quote_factory");
    assert_eq!(body["tools"][0]["parameters"], request.tools[0].parameters);
    assert_eq!(body["tools"].as_array().unwrap().len(), 1);
    assert_eq!(body["parallel_tool_calls"], false);
    handle.join().unwrap();
}
#[test]
fn caller_names_are_not_reinterpreted_as_legacy_decisions() {
    let (endpoint, _received, handle) = server(vec![completion("agent_submit_decision")]);
    let client = client(&endpoint);
    let mut request = request();
    request.tools[0].name = "agent_submit_decision".into();
    let result = client
        .complete_with_tools(
            &LlmCompletionRequest {
                model: request.model,
                system_prompt: request.system_prompt,
                user_prompt: request.user_prompt,
                debug_mode: false,
                max_model_calls: Some(1),
            },
            &request.tools,
        )
        .unwrap();
    assert!(
        matches!(&result.turns[0], oasis7_model_provider::LlmCompletionTurn::ModuleCall {module,..} if module == "agent_submit_decision")
    );
    handle.join().unwrap();
}
#[test]
fn budgeted_transport_does_not_hide_a_concurrency_retry() {
    let error = "data: {\"error\":{\"type\":\"rate_limit_error\",\"message\":\"Concurrency limit exceeded\"}}\n\n";
    let (endpoint, received, handle) = server(vec![error.into()]);
    assert!(ModelProvider::complete_checked(&mut client(&endpoint), &request()).is_err());
    received.recv_timeout(Duration::from_secs(3)).unwrap();
    handle.join().unwrap();
}
#[test]
fn legacy_profile_preserves_catalog_and_one_unbudgeted_retry() {
    let error = "data: {\"error\":{\"type\":\"rate_limit_error\",\"message\":\"Concurrency limit exceeded\"}}\n\n";
    let (endpoint, received, handle) =
        server(vec![error.into(), completion("agent_submit_decision")]);
    let client = client(&endpoint);
    let result = client
        .complete_with_profile(
            &LlmCompletionRequest {
                model: "test-model".into(),
                system_prompt: "legacy".into(),
                user_prompt: "wait".into(),
                debug_mode: false,
                max_model_calls: None,
            },
            CompletionToolProfile::LegacySimulator,
        )
        .unwrap();
    assert!(matches!(
        result.turns[0],
        oasis7_model_provider::LlmCompletionTurn::Decision { .. }
    ));
    let first = received.recv_timeout(Duration::from_secs(3)).unwrap();
    let second = received.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(first, second);
    assert_eq!(first["tools"].as_array().unwrap().len(), 10);
    handle.join().unwrap();
}
