#![cfg(not(target_arch = "wasm32"))]

use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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
fn provider_loopback_http_client_round_trips_info_health_decision_and_feedback() {
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let fixture = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("golden fixture");
    let mut expected_request = fixture.request.clone();
    expected_request.agent_profile = Some("oasis7_p0_low_freq_npc".to_string());
    let expected_feedback = FeedbackEnvelope {
        action_id: 7,
        success: true,
        reject_reason: None,
        emitted_events: vec![],
        world_delta_summary: Some("agent moved to loc-2".to_string()),
    };
    let expected_response = DecisionResponse {
        decision: fixture.expected_decision,
        module_command: None,
        provider_error: None,
        diagnostics: ProviderDiagnostics {
            provider_id: Some("provider_local_bridge".to_string()),
            provider_version: Some("0.1.0".to_string()),
            latency_ms: Some(41),
            retry_count: 0,
        },
        trace_payload: ProviderTraceEnvelope {
            provider_id: Some("provider_local_bridge".to_string()),
            input_summary: Some("fixture=golden.move.visible_location.v1".to_string()),
            output_summary: Some("decision=move_agent(to=loc-2)".to_string()),
            latency_ms: Some(41),
            transcript: vec![ProviderTranscriptEntry {
                role: "assistant".to_string(),
                content: "move to loc-2".to_string(),
            }],
            tool_trace: vec!["selected visible location loc-2".to_string()],
            token_usage: Some(ProviderTokenUsage {
                prompt_tokens: Some(8),
                completion_tokens: Some(5),
                total_tokens: Some(13),
            }),
            cost_cents: Some(1),
            upstream_trace: None,
            schema_repair_count: 0,
        },
        memory_write_intents: vec![MemoryWriteIntent {
            scope: "short_term".to_string(),
            summary: "agent prefers loc-2 when visible".to_string(),
            tags: vec!["movement".to_string()],
        }],
    };
    let expected_request_for_server = expected_request.clone();
    let expected_feedback_for_server = expected_feedback.clone();
    let response_clone = expected_response.clone();
    let feedback_ack = ProviderFeedbackAck {
        ok: true,
        error_code: None,
        error: None,
    };
    let base_url = spawn_mock_http_server(4, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/v1/provider/info") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "provider_id": "provider_local_bridge",
                        "name": "Provider Local Bridge",
                        "version": "0.1.0",
                        "protocol_version": "world-simulator-provider-loopback-http-v1",
                        "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
                        "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
                        "capabilities": ["decision", "feedback"],
                        "supported_action_sets": ["phase1_low_frequency"]
                    })
                    .to_string(),
                },
                ("GET", "/v1/provider/health") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "ok": true,
                        "status": "ready",
                        "uptime_ms": 1234,
                        "last_error": null,
                        "queue_depth": 0
                    })
                    .to_string(),
                },
                ("POST", "/v1/world-simulator/decision") => {
                    let decoded: DecisionRequest = serde_json::from_slice(request.body.as_slice())
                        .expect("decode decision request");
                    assert_eq!(decoded, expected_request_for_server);
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&response_clone)
                            .expect("encode decision response"),
                    }
                }
                ("POST", "/v1/world-simulator/feedback") => {
                    let decoded: FeedbackEnvelope = serde_json::from_slice(request.body.as_slice())
                        .expect("decode feedback request");
                    assert_eq!(decoded, expected_feedback_for_server);
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&feedback_ack).expect("encode feedback ack"),
                    }
                }
                _ => MockHttpResponse {
                    status_code: 404,
                    body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
                },
            }
        }
    });

    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), Some("secret-token"), 200)
        .expect("build client");
    let info = client.provider_info().expect("info");
    assert_eq!(info.provider_id, "provider_local_bridge");
    assert_eq!(info.version.as_deref(), Some("0.1.0"));
    assert_eq!(
        info.supported_action_sets,
        vec!["phase1_low_frequency".to_string()]
    );

    let health = client.provider_health().expect("health");
    assert!(health.ok);
    assert_eq!(health.status.as_deref(), Some("ready"));
    assert_eq!(health.queue_depth, Some(0));

    let decision = client
        .request_decision(&expected_request)
        .expect("decision");
    assert_eq!(decision, expected_response);

    let ack = client
        .submit_feedback(&expected_feedback)
        .expect("feedback ack");
    assert!(ack.ok);

    let recorded = recorded.lock().expect("recorded lock");
    assert_eq!(recorded.len(), 4);
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/provider/info");
    assert_eq!(
        recorded[0].headers.get("authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
    assert_eq!(recorded[1].path, "/v1/provider/health");
    assert_eq!(recorded[2].path, "/v1/world-simulator/decision");
    assert_eq!(recorded[3].path, "/v1/world-simulator/feedback");
}

#[test]
fn provider_loopback_http_client_rejects_non_loopback_base_url() {
    let err = ProviderLoopbackHttpClient::new("http://192.168.0.5:5841", None, 200)
        .expect_err("non-loopback should fail");
    assert!(err.to_string().contains("loopback"));
}

#[test]
fn provider_loopback_http_client_bypasses_proxy_env_for_loopback_transport() {
    let _guard = proxy_env_lock().lock().expect("proxy env lock");
    let _proxy_env = ProxyEnvGuard::new();
    set_bad_proxy_env();

    let base_url = spawn_mock_http_server(1, |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::json!({
            "provider_id": "provider_local_bridge",
            "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
            "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
            "capabilities": ["decision", "feedback"],
            "supported_action_sets": ["phase1_low_frequency"]
        })
        .to_string(),
    });
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 200)
        .expect("build loopback client");
    let info = client
        .provider_info()
        .expect("loopback client should ignore proxy env");
    assert_eq!(info.provider_id, "provider_local_bridge");
}

#[test]
fn provider_loopback_http_client_bypasses_proxy_env_for_trimmed_loopback_transport() {
    let _guard = proxy_env_lock().lock().expect("proxy env lock");
    let _proxy_env = ProxyEnvGuard::new();
    set_bad_proxy_env();

    let base_url = spawn_mock_http_server(1, |_| MockHttpResponse {
        status_code: 200,
        body: serde_json::json!({
            "provider_id": "provider_local_bridge",
            "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
            "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
            "capabilities": ["decision", "feedback"],
            "supported_action_sets": ["phase1_low_frequency"]
        })
        .to_string(),
    });
    let client = ProviderLoopbackHttpClient::new_with_transport(
        base_url.as_str(),
        None,
        200,
        " loopback_http ",
    )
    .expect("build trimmed loopback client");
    let info = client
        .provider_info()
        .expect("trimmed loopback client should ignore proxy env");
    assert_eq!(info.provider_id, "provider_local_bridge");
}

#[test]
fn provider_http_client_accepts_remote_https_base_url() {
    let client = ProviderLoopbackHttpClient::new_with_transport(
        "https://provider.example",
        Some("secret-token"),
        200,
        "remote_https",
    )
    .expect("remote https client should build");
    let _ = client;
}

#[test]
fn provider_http_client_rejects_remote_http_for_remote_https_transport() {
    let err = ProviderLoopbackHttpClient::new_with_transport(
        "http://provider.example",
        None,
        200,
        "remote_https",
    )
    .expect_err("plain http remote transport should fail");
    assert!(err.to_string().contains("https"));
}

#[test]
fn provider_http_client_rejects_loopback_remote_https_host() {
    let err = ProviderLoopbackHttpClient::new_with_transport(
        "https://127.0.0.1:5841",
        None,
        200,
        "remote_https",
    )
    .expect_err("loopback remote https host should fail");
    assert!(err.to_string().contains("public remote https host"));
}

#[test]
fn provider_loopback_http_client_surfaces_http_401_on_decision() {
    let base_url = spawn_mock_http_server(1, |_| MockHttpResponse {
        status_code: 401,
        body: "unauthorized".to_string(),
    });
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), Some("bad-token"), 200)
        .expect("build client");
    let request = golden_decision_provider_fixtures()
        .into_iter()
        .next()
        .expect("fixture")
        .request;

    let err = client
        .request_decision(&request)
        .expect_err("401 should surface");
    assert!(matches!(
        err,
        ProviderLoopbackHttpError::Unauthorized { .. }
    ));
    assert!(err.to_string().contains("unauthorized"));
}

#[test]
fn provider_http_failure_retryability_is_deterministic() {
    assert!(
        ProviderLoopbackHttpError::RequestFailed {
            path: "/v1/world-simulator/agent-chat".to_string(),
            detail: "connection reset".to_string(),
        }
        .retryable()
    );
    assert!(
        ProviderLoopbackHttpError::UnexpectedStatus {
            path: "/v1/world-simulator/agent-chat".to_string(),
            status_code: 429,
            body: "rate limited".to_string(),
        }
        .retryable()
    );
    assert!(
        ProviderLoopbackHttpError::UnexpectedStatus {
            path: "/v1/world-simulator/agent-chat".to_string(),
            status_code: 503,
            body: "unavailable".to_string(),
        }
        .retryable()
    );
    assert!(
        !ProviderLoopbackHttpError::Unauthorized {
            path: "/v1/world-simulator/agent-chat".to_string(),
            detail: "bad token".to_string(),
        }
        .retryable()
    );
    assert!(
        !ProviderLoopbackHttpError::UnexpectedStatus {
            path: "/v1/world-simulator/agent-chat".to_string(),
            status_code: 400,
            body: "bad request".to_string(),
        }
        .retryable()
    );
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

#[test]
fn captured_provider_call_persists_exact_request_before_send_and_raw_response_before_decode() {
    let request = captured_provider_request();
    let expected_response = captured_provider_response(&request);
    let response_bytes = serde_json::to_vec(&expected_response).expect("encode response bytes");
    let response_body = String::from_utf8(response_bytes.clone()).expect("response is UTF-8");
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let recorded_server = Arc::clone(&recorded);
    let response_body_server = response_body.clone();
    let base_url = spawn_mock_http_server(1, move |http_request| {
        recorded_server.lock().unwrap().push(http_request);
        MockHttpResponse {
            status_code: 200,
            body: response_body_server.clone(),
        }
    });
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("build provider client");

    let (response, refs, _response_context_lease) = client
        .request_decision_with_context_and_material_store(&request, store.as_ref())
        .expect("captured request/response round trip");

    assert_eq!(response, expected_response);
    let captured = store.snapshot();
    let expected_request_bytes = serde_json::to_vec(&request).expect("encode request bytes");
    assert_eq!(
        captured.request_body.as_deref(),
        Some(expected_request_bytes.as_slice())
    );
    assert_eq!(
        captured.response_body.as_deref(),
        Some(response_bytes.as_slice())
    );
    assert_eq!(
        captured.events,
        [
            "persist_request",
            "mark_dispatched",
            "persist_response",
            "finish_response"
        ]
    );
    assert!(captured.committed.is_none());
    assert_eq!(refs.owner, *store.owner_binding());
    assert_eq!(
        refs.operation_id,
        request.provider_invocation_key().to_string()
    );
    assert_eq!(refs.transport_attempt, request.transport_attempt);

    let recorded = recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1, "one attempt must issue one HTTP request");
    assert_eq!(recorded[0].path, "/v1/world-simulator/decision-context");
    assert_eq!(recorded[0].body, expected_request_bytes);
}

#[test]
fn captured_request_persistence_failure_makes_zero_http_calls() {
    let request = captured_provider_request();
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    store
        .fail_request_persistence
        .store(true, Ordering::Release);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind no-send listener");
    listener
        .set_nonblocking(true)
        .expect("enable nonblocking accept");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("build provider client");

    let error = client
        .request_decision_with_context_and_material_store(&request, store.as_ref())
        .expect_err("request persistence failure must block dispatch");
    assert!(error.to_string().contains("request persistence failed"));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "HTTP must not be reached when the original request is not durable"
    );
    assert_eq!(store.snapshot().events, ["persist_request_failed"]);
}

#[test]
fn captured_dispatch_cancellation_after_marker_does_not_enter_http() {
    let request = captured_provider_request();
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    store.cancel_after_dispatch.store(true, Ordering::Release);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind no-send listener");
    listener
        .set_nonblocking(true)
        .expect("enable nonblocking accept");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("build provider client");

    let error = client
        .request_decision_with_context_and_material_store(&request, store.as_ref())
        .expect_err("cancellation after the durable marker must stop the send");
    assert!(error.to_string().contains("after dispatch marker"));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
        "HTTP must not be reached after cancellation"
    );
    assert_eq!(
        store.snapshot().events,
        ["persist_request", "mark_dispatched"]
    );
}

#[test]
fn captured_http_failure_is_nonretryable_and_retains_the_attempt() {
    let request = captured_provider_request();
    let body = b"provider unavailable".to_vec();
    let response_body = String::from_utf8(body.clone()).unwrap();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let recorded_server = Arc::clone(&recorded);
    let response_server = response_body.clone();
    let base_url = spawn_mock_http_server(1, move |http_request| {
        recorded_server.lock().unwrap().push(http_request);
        MockHttpResponse {
            status_code: 503,
            body: response_server.clone(),
        }
    });
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    let mut adapter = ProviderLoopbackAdapter::new_with_transport(
        base_url.as_str(),
        None,
        5_000,
        LOOPBACK_HTTP_PROVIDER_TRANSPORT,
    )
    .expect("build adapter")
    .with_original_material_store(store.clone());

    let error = adapter
        .decide_with_continuous_request_context(
            &request.base_decision_request,
            &super::agent_cognition_identity::production_turn_context(&request),
            &request,
        )
        .expect_err("captured 503 remains pending and fails closed");

    assert!(
        !error.retryable,
        "unknown captured attempts cannot auto-retry"
    );
    assert_eq!(error.code, "provider_attempt_retained");
    let captured = store.snapshot();
    assert_eq!(captured.response_body.as_deref(), Some(body.as_slice()));
    assert_eq!(
        captured.events,
        ["persist_request", "mark_dispatched", "persist_response"]
    );
    assert!(captured.committed.is_none());
    let recorded = recorded.lock().unwrap();
    assert_eq!(
        recorded.len(),
        1,
        "transport failure must not create attempt two"
    );
}

#[test]
fn captured_lane_does_not_follow_post_redirects_to_a_second_endpoint() {
    let request = captured_provider_request();
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    let second_listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect target");
    second_listener
        .set_nonblocking(true)
        .expect("make redirect target nonblocking");
    let second_address = second_listener
        .local_addr()
        .expect("redirect target address");
    let second_endpoint_received = Arc::new(AtomicBool::new(false));
    let second_endpoint_received_thread = Arc::clone(&second_endpoint_received);
    let second_endpoint_thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match second_listener.accept() {
                Ok((mut stream, _)) => {
                    second_endpoint_received_thread.store(true, Ordering::Release);
                    let _ = read_http_request(&mut stream);
                    let _ = stream.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    break true;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => break false,
            }
        }
    });

    let first_listener = TcpListener::bind("127.0.0.1:0").expect("bind redirect source");
    let first_address = first_listener
        .local_addr()
        .expect("redirect source address");
    let redirect_location = format!("http://{second_address}/v1/world-simulator/decision-context");
    let first_endpoint_thread = std::thread::spawn(move || {
        let (mut stream, _) = first_listener.accept().expect("accept captured request");
        let _request = read_http_request(&mut stream);
        let response = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: {redirect_location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("write redirect response");
    });

    let base_url = format!("http://{first_address}");
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("build provider client");
    let error = client
        .request_decision_with_context_and_material_store(&request, store.as_ref())
        .expect_err("captured POST must surface 307 without following it");
    assert!(matches!(
        error,
        ProviderLoopbackHttpError::UnexpectedStatus {
            status_code: 307,
            ..
        }
    ));
    first_endpoint_thread
        .join()
        .expect("redirect source finished");
    assert!(
        !second_endpoint_thread
            .join()
            .expect("redirect target finished")
    );
    assert!(!second_endpoint_received.load(Ordering::Acquire));
    assert_eq!(
        store.snapshot().events,
        ["persist_request", "mark_dispatched", "persist_response"]
    );
}

#[test]
fn invalid_captured_response_is_persisted_before_decode_and_stays_unconsumed() {
    let request = captured_provider_request();
    let raw_response = b"{not-json".to_vec();
    let response_body = String::from_utf8(raw_response.clone()).unwrap();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let recorded_server = Arc::clone(&recorded);
    let response_server = response_body.clone();
    let base_url = spawn_mock_http_server(1, move |http_request| {
        recorded_server.lock().unwrap().push(http_request);
        MockHttpResponse {
            status_code: 200,
            body: response_server.clone(),
        }
    });
    let store = Arc::new(TestProviderMaterialStore::new(
        request.runtime_binding.world_id.clone(),
    ));
    let client = ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("build provider client");

    let error = client
        .request_decision_with_context_and_material_store(&request, store.as_ref())
        .expect_err("invalid JSON must fail after preserving raw response");

    assert!(matches!(
        error,
        ProviderLoopbackHttpError::DecodeFailed { .. }
    ));
    let captured = store.snapshot();
    assert_eq!(
        captured.response_body.as_deref(),
        Some(raw_response.as_slice())
    );
    assert_eq!(
        captured.events,
        ["persist_request", "mark_dispatched", "persist_response"]
    );
    assert!(!captured.events.contains(&"finish_response"));
    assert!(captured.committed.is_none());
    assert_eq!(recorded.lock().unwrap().len(), 1);
}

fn captured_provider_request() -> ContinuousAgentRequestContextV1 {
    let mut fixture = super::agent_cognition_identity::production_request_fixture(1, 5_000);
    fixture["retry_seq"] = serde_json::json!(1);
    super::agent_cognition_identity::request_from_value(fixture)
}

fn captured_provider_response(
    request: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentResponseContextV1 {
    let base_decision_response = DecisionResponse::wait("captured-test-provider");
    ContinuousAgentResponseContextV1 {
        response_digest: cognition_response_digest(&base_decision_response),
        base_decision_response,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
    }
}

#[derive(Default)]
struct TestProviderMaterialState {
    events: Vec<&'static str>,
    request_body: Option<Vec<u8>>,
    response_body: Option<Vec<u8>>,
    attempt: Option<ProviderMaterialAttemptV1>,
    committed: Option<(String, u64)>,
}

struct TestProviderMaterialStore {
    owner: ProviderMaterialOwnerBindingV1,
    state: Mutex<TestProviderMaterialState>,
    memory: MemoryBudget,
    cancelled: AtomicBool,
    fail_request_persistence: AtomicBool,
    cancel_after_dispatch: AtomicBool,
}

impl TestProviderMaterialStore {
    fn new(world_id: String) -> Self {
        Self {
            owner: ProviderMaterialOwnerBindingV1 {
                world_id,
                genesis_root: [1; 32],
                owner_id: [4; 32],
            },
            state: Mutex::new(TestProviderMaterialState::default()),
            memory: MemoryBudget::new(2 * 1024 * 1024, 256 * 1024).expect("test memory pool"),
            cancelled: AtomicBool::new(false),
            fail_request_persistence: AtomicBool::new(false),
            cancel_after_dispatch: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> TestProviderMaterialStateSnapshot {
        let state = self.state.lock().unwrap();
        TestProviderMaterialStateSnapshot {
            events: state.events.clone(),
            request_body: state.request_body.clone(),
            response_body: state.response_body.clone(),
            committed: state.committed.clone(),
        }
    }
}

struct TestProviderMaterialStateSnapshot {
    events: Vec<&'static str>,
    request_body: Option<Vec<u8>>,
    response_body: Option<Vec<u8>>,
    committed: Option<(String, u64)>,
}

struct TestProviderSpoolReservation;
impl ProviderSpoolReservation for TestProviderSpoolReservation {}

impl ProviderOriginalMaterialStore for TestProviderMaterialStore {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1 {
        &self.owner
    }

    fn pending_protection_root(&self) -> Result<[u8; 32], String> {
        Ok([4; 32])
    }

    fn protected_roots_page(
        &self,
        _cursor: Option<&ProviderPendingRootsCursorV1>,
        _limit: usize,
    ) -> Result<ProviderPendingRootsPageV1, String> {
        let charge = self
            .memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let owner_charge = self
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let attempts = self
            .memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let roots = self
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let empty = oasis7_formal_protocol::incremental_state::StateRootV1::empty(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Effects,
        )
        .map_err(|error| error.to_string())?;
        Ok(ProviderPendingRootsPageV1 {
            owner: Arc::new(owner_charge.retain(self.owner.clone())),
            pending_root: [4; 32],
            object_ids: Arc::new(charge.retain(Vec::new())),
            attempts: Arc::new(attempts.retain(Vec::new())),
            state_roots: Arc::new(roots.retain(ProviderPendingStateRootsV1 {
                events: empty.clone(),
                pending: empty.clone(),
                seen: empty,
            })),
            next_cursor: None,
        })
    }

    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        if request.runtime_binding.world_id != self.owner.world_id {
            return Err("wrong owner world".into());
        }
        Ok(())
    }

    fn memory_budget(&self) -> MemoryBudget {
        self.memory.clone()
    }

    fn request_body_limit_bytes(&self) -> u64 {
        1024 * 1024
    }

    fn response_body_limit_bytes(&self) -> u64 {
        1024 * 1024
    }

    fn reserve_body_spools(
        &self,
        request_limit: u64,
        response_limit: u64,
    ) -> Result<Box<dyn ProviderSpoolReservation>, String> {
        if request_limit > 1024 * 1024 || response_limit > 1024 * 1024 {
            return Err("test spool budget exceeded".into());
        }
        Ok(Box::new(TestProviderSpoolReservation))
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String> {
        if self.fail_request_persistence.load(Ordering::Acquire) {
            self.state
                .lock()
                .unwrap()
                .events
                .push("persist_request_failed");
            return Err("injected request persistence failure".into());
        }
        if length > self.request_body_limit_bytes() {
            return Err("request limit exceeded".into());
        }
        let mut request_body = Vec::with_capacity(length as usize);
        original_body
            .take(length.saturating_add(1))
            .read_to_end(&mut request_body)
            .map_err(|error| error.to_string())?;
        if request_body.len() as u64 != length {
            return Err("request spool length mismatch".into());
        }
        let request_object_id = *blake3::hash(&request_body).as_bytes();
        if request_object_id != expected_body_id {
            return Err("request body ID differs from HTTP spool".into());
        }
        let request_context_bytes =
            serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let operation_id = request.provider_invocation_key().to_string();
        let mut attempt_hasher = blake3::Hasher::new();
        attempt_hasher.update(&self.owner.owner_id);
        attempt_hasher.update(operation_id.as_bytes());
        attempt_hasher.update(&request.transport_attempt.to_be_bytes());
        attempt_hasher.update(&request_object_id);
        let attempt_id = *attempt_hasher.finalize().as_bytes();
        let attempt = ProviderMaterialAttemptV1 {
            owner: self.owner.clone(),
            operation_id,
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: [2; 32],
            authority_snapshot: [3; 32],
            request_object_id,
            request_context_object_id: *blake3::hash(&request_context_bytes).as_bytes(),
        };
        let mut state = self.state.lock().unwrap();
        state.events.push("persist_request");
        state.request_body = Some(request_body);
        state.attempt = Some(attempt.clone());
        Ok(attempt)
    }

    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if state.attempt.as_ref() != Some(attempt) {
            return Err("dispatch attempt mismatch".into());
        }
        state.events.push("mark_dispatched");
        drop(state);
        if self.cancel_after_dispatch.load(Ordering::Acquire) {
            self.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        _status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        if length > self.response_body_limit_bytes() {
            return Err("response limit exceeded".into());
        }
        let mut response_body = Vec::with_capacity(length as usize);
        original_body
            .take(length.saturating_add(1))
            .read_to_end(&mut response_body)
            .map_err(|error| error.to_string())?;
        if response_body.len() as u64 != length {
            return Err("response spool length mismatch".into());
        }
        let response_id = *blake3::hash(&response_body).as_bytes();
        if response_id != expected_body_id {
            return Err("response body ID differs from HTTP spool".into());
        }
        let mut state = self.state.lock().unwrap();
        if state.attempt.as_ref() != Some(attempt) || !state.events.contains(&"mark_dispatched") {
            return Err("response arrived without a durable dispatch marker".into());
        }
        state.events.push("persist_response");
        state.response_body = Some(response_body.clone());
        Ok(response_id)
    }

    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        let response_context_bytes =
            serde_json::to_vec(response).map_err(|error| error.to_string())?;
        let mut state = self.state.lock().unwrap();
        if state.attempt.as_ref() != Some(attempt) || state.response_body.is_none() {
            return Err("typed response completed before raw response persistence".into());
        }
        state.events.push("finish_response");
        Ok(ProviderOriginalMaterialRefsV1 {
            owner: attempt.owner.clone(),
            operation_id: attempt.operation_id.clone(),
            transport_attempt: attempt.transport_attempt,
            attempt_id: attempt.attempt_id,
            parent_root_set: attempt.parent_root_set,
            authority_snapshot: attempt.authority_snapshot,
            request_object_id: attempt.request_object_id,
            request_context_object_id: attempt.request_context_object_id,
            response_object_id,
            response_context_object_id: *blake3::hash(&response_context_bytes).as_bytes(),
            response_status: status,
        })
    }

    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        if materials.owner != self.owner
            || materials.operation_id != request.provider_invocation_key().to_string()
            || materials.transport_attempt != request.transport_attempt
            || materials.response_status != 200
            || response.transport_attempt != materials.transport_attempt
        {
            return Err("captured material identity mismatch".into());
        }
        Ok(())
    }

    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        if materials.owner != self.owner || receipt_id.is_empty() {
            return Err("invalid committed material identity".into());
        }
        self.state.lock().unwrap().committed = Some((receipt_id.to_string(), action_id));
        Ok(())
    }

    fn mark_rejected(
        &self,
        _attempt: &ProviderMaterialAttemptV1,
        _evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        Err("test store does not support rejection evidence".into())
    }
}

fn proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn proxy_env_keys() -> &'static [&'static str] {
    &[
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ]
}

fn save_proxy_env() -> Vec<(&'static str, Option<String>)> {
    proxy_env_keys()
        .iter()
        .map(|key| (*key, std::env::var(key).ok()))
        .collect()
}

struct ProxyEnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl ProxyEnvGuard {
    fn new() -> Self {
        Self {
            saved: save_proxy_env(),
        }
    }
}

impl Drop for ProxyEnvGuard {
    fn drop(&mut self) {
        restore_proxy_env(std::mem::take(&mut self.saved));
    }
}

fn set_bad_proxy_env() {
    for key in proxy_env_keys() {
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::remove_var(key);
        }
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var("HTTP_PROXY", "http://127.0.0.1:9");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var("HTTPS_PROXY", "http://127.0.0.1:9");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var("ALL_PROXY", "http://127.0.0.1:9");
    }
}

fn restore_proxy_env(saved: Vec<(&'static str, Option<String>)>) {
    for (key, value) in saved {
        match value {
            Some(value) => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::set_var(key, value);
                }
            }
            None => {
                // SAFETY: This test/setup code mutates process environment in a controlled scope.
                unsafe {
                    oasis7::env_mut::remove_var(key);
                }
            }
        }
    }
}
