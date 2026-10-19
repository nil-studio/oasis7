use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_dispatch_http_restart_fence_issues_one_request() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    fn read_http_request(stream: &TcpStream) -> (String, Vec<u8>) {
        let mut reader = BufReader::new(stream.try_clone().expect("clone provider stream"));
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("read provider request line");
        let path = request_line
            .split_whitespace()
            .nth(1)
            .expect("provider request path")
            .to_string();
        let mut content_length = 0_usize;
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("read provider request header");
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                content_length = value.trim().parse().expect("provider content length");
            }
        }
        let mut body = vec![0_u8; content_length];
        reader
            .read_exact(&mut body)
            .expect("read provider request body");
        (path, body)
    }

    fn write_sse_response(stream: &mut TcpStream, event: &str) {
        let body = format!("data: {event}\n\n");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write provider response");
        stream.flush().expect("flush provider response");
    }

    fn write_json_response(stream: &mut TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write provider JSON response");
        stream.flush().expect("flush provider JSON response");
    }

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider HTTP fixture");
    listener
        .set_nonblocking(true)
        .expect("make provider HTTP fixture nonblocking");
    let base_url = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("provider HTTP fixture address")
    );
    let request_count = Arc::new(AtomicUsize::new(0));
    let stop_server = Arc::new(AtomicBool::new(false));
    let server_count = Arc::clone(&request_count);
    let server_stop = Arc::clone(&stop_server);
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Accepted sockets inherit the listener's nonblocking
                    // mode on some platforms. Switch the connection back to
                    // blocking before reading headers so concurrent tests do
                    // not race the provider client and turn a normal
                    // WouldBlock into a poisoned test thread.
                    stream
                        .set_nonblocking(false)
                        .expect("make provider connection blocking");
                    let (path, _body) = read_http_request(&stream);
                    match path.as_str() {
                        "/v1/responses" => {
                            server_count.fetch_add(1, Ordering::AcqRel);
                            let event = serde_json::json!({
                                "type": "response.completed",
                                "sequence_number": 1,
                                "response": {
                                    "id": "resp_dispatch_marker",
                                    "object": "response",
                                    "created_at": 1,
                                    "completed_at": 2,
                                    "model": "gpt-dispatch-marker",
                                    "output": [{
                                        "type": "function_call",
                                        "call_id": "call_decision",
                                        "name": "agent_submit_decision",
                                        "arguments": "{\"decision\":\"wait\"}"
                                    }],
                                    "status": "completed",
                                    "parallel_tool_calls": false
                                }
                            });
                            write_sse_response(&mut stream, event.to_string().as_str());
                        }
                        // Other runtime-live tests can legitimately flush a
                        // feedback outbox while this fixture owns the shared
                        // provider environment. Serve that route so an
                        // unrelated request cannot panic this test's server
                        // thread and poison its environment lock.
                        "/v1/world-simulator/feedback-context" => {
                            write_json_response(&mut stream, r#"{"ok":true}"#);
                        }
                        _ => {
                            write!(
                                stream,
                                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                            .expect("write provider route response");
                            stream.flush().expect("flush provider route response");
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("provider HTTP fixture accept failed: {error}"),
            }
        }
    });

    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url.as_str());
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let world_id = "http-dispatch-marker-world";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            world_id,
            "http-dispatch-marker-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");

    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-http-marker-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));

    let make_runner = || {
        let config = crate::simulator::LlmAgentConfig {
            model: "gpt-dispatch-marker".to_string(),
            base_url: format!("{base_url}/v1"),
            api_key: "test-key".to_string(),
            timeout_ms: 1_000,
            system_prompt: "test".to_string(),
            short_term_goal: "test".to_string(),
            long_term_goal: "test".to_string(),
            max_module_calls: 3,
            max_decision_steps: 4,
            max_repair_rounds: 1,
            prompt_max_history_items: 4,
            prompt_profile: crate::simulator::LlmPromptProfile::Balanced,
            force_replan_after_same_action: 4,
            harvest_max_amount_cap: 100,
            execute_until_auto_reenter_ticks: 4,
            llm_debug_mode: false,
        };
        let client = crate::simulator::OpenAiChatCompletionClient::from_config(&config)
            .expect("native OpenAI HTTP client");
        let behavior = crate::simulator::LlmAgentBehavior::new("agent-0", config, client);
        let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
        runner
            .register(behavior)
            .expect("register native OpenAI actor");
        RuntimeDecisionRunner::Builtin(runner)
    };

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(lineage_path.clone());
    first.runner = Some(make_runner());
    first.provider_agent_ids.insert("agent-0".to_string());
    first
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync provider HTTP shadow kernel");
    let mut kernel = first
        .shadow_kernel
        .take()
        .expect("provider HTTP shadow kernel");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        first
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("native provider runner"),
    )
    .expect("restore Runtime feedback history before first HTTP dispatch");
    assert!(
        first
            .next_async_provider_decision(&mut world, &mut kernel, world_id)
            .is_none(),
        "first provider HTTP request should remain in flight"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while request_count.load(Ordering::Acquire) < 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        request_count.load(Ordering::Acquire),
        1,
        "the native provider actor must issue one actual HTTP request"
    );

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(lineage_path.clone());
    restarted
        .restore_provider_lineage(&world)
        .expect("restore HTTP dispatch marker");
    restarted.runner = Some(make_runner());
    restarted
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync restarted provider HTTP shadow kernel");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        restarted
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("restarted native provider runner"),
    )
    .expect("restore Runtime feedback history before restarted HTTP dispatch");
    let mut restarted_kernel = restarted
        .shadow_kernel
        .take()
        .expect("restarted provider HTTP shadow kernel");
    assert!(
        restarted
            .next_async_provider_decision(&mut world, &mut restarted_kernel, world_id)
            .is_none(),
        "restart fence must not return a duplicate provider decision"
    );
    thread::sleep(Duration::from_millis(50));
    assert_eq!(
        request_count.load(Ordering::Acquire),
        1,
        "restart fence must issue no additional HTTP request"
    );
    assert!(restarted.provider_transport_exhausted.contains("agent-0"));
    assert!(!restarted.provider_retry_contexts.contains_key("agent-0"));
    assert_eq!(
        restarted
            .provider_contexts
            .get("agent-0")
            .map(|context| context.request_context.agent_turn_id.as_str()),
        Some("runtime-test-session:agent-0-turn-1"),
        "restart fence retains the original logical identity"
    );

    stop_server.store(true, Ordering::Release);
    server.join().expect("join provider HTTP fixture");
    let _ = std::fs::remove_file(lineage_path);
}
