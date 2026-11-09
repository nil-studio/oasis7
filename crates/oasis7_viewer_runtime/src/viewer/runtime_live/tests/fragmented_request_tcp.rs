//! Actual TCP control framing; provider setup is isolated in a child process.
use super::*;

#[test]
fn fragmented_pause_survives_an_observed_read_timeout_and_preserves_next_frame() {
    const CHILD: &str = "OASIS7_FRAGMENTED_PAUSE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let name = concat!(
            module_path!(),
            "::fragmented_pause_survives_an_observed_read_timeout_and_preserves_next_frame"
        );
        let name = name.split_once("::").unwrap().1;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated fragmented Pause failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
        return;
    }
    let _env = runtime_provider_env_lock().lock().unwrap();
    clear_runtime_provider_env();
    // SAFETY: isolated child, with the canonical provider environment lock.
    unsafe {
        for (key, value) in [
            (VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed"),
            (VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed"),
            (VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock"),
            (VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1"),
            (VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http"),
            (VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9"),
            (VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc"),
            (VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity"),
        ] {
            oasis7::env_mut::set_var(key, value);
        }
    }
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_hosted_public_join_mode(true)
            .with_auto_play_on_connect(false)
            .with_test_cognition_runtime_binding(
                "framing",
                0,
                Some(crate::simulator::h_v1("framing", &0).to_string()),
                "verified",
                0,
            ),
    )
    .unwrap();
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let sink = TraceSink::for_test();
    let mut client = StreamClient::start(shared.clone(), Some(sink.connection()), true, None);
    let send = |client: &mut StreamClient, value: Value| {
        serde_json::to_writer(&mut client.writer, &value).unwrap();
        client.writer.write_all(b"\n").unwrap();
        client.writer.flush().unwrap();
    };
    let hello = serde_json::json!({"type":"hello_v2","client":"framing-regression","version":2,"capabilities":[]});
    send(
        &mut client,
        serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":7801}),
    );
    send(&mut client, hello.clone());
    assert!(
        (0..16).any(|_| matches!(client.read_response(), ViewerResponse::HelloAck { .. })),
        "following Hello must be decoded"
    );
    assert!(
        !shared.lock().unwrap().auto_play_paused,
        "genuine Play must apply first"
    );
    let pause = serde_json::to_vec(
        &serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":7802}),
    )
    .unwrap();
    let split = 13;
    client.writer.write_all(&pause[..split]).unwrap();
    client.writer.flush().unwrap();
    assert!(
        wait_for_event(&sink, TEST_SOCKET_TIMEOUT, |fields| {
            fields.get("event").is_some_and(|v| v == "read_timeout")
                && fields
                    .get("buffered_bytes")
                    .is_some_and(|v| v == &split.to_string())
        })
        .is_some(),
        "server must actually consume the prefix and observe a timeout"
    );
    client.writer.write_all(&pause[split..]).unwrap();
    client.writer.write_all(b"\n").unwrap();
    client.writer.flush().unwrap();
    send(&mut client, hello);
    assert!(
        (0..16).any(|_| matches!(client.read_response(), ViewerResponse::HelloAck { .. })),
        "following Hello must be decoded"
    );
    assert!(
        shared.lock().unwrap().auto_play_paused,
        "fragmented genuine Pause must apply before the following Hello"
    );
    client.close();
    assert!(
        client.finish(),
        "EOF must finish the actual shared-stream worker"
    );
}

#[test]
fn exclusive_stream_requires_lf_before_dispatch_even_at_eof() {
    use std::io::Read;
    for terminated in [false, true] {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_auto_play_on_connect(false),
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            server.serve_stream(stream)
        });
        let mut client = std::net::TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.write_all(
            br#"{"type":"hello_v2","client":"exclusive-framing","version":2,"capabilities":[]}"#,
        ).unwrap();
        if terminated {
            client.write_all(b"\n").unwrap();
        }
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut responses = String::new();
        client.read_to_string(&mut responses).unwrap();
        assert!(worker.join().unwrap().is_ok());
        if terminated {
            assert!(
                responses.lines().any(|line| {
                    serde_json::from_str::<Value>(line)
                        .is_ok_and(|value| value["type"] == "hello_ack")
                }),
                "complete exclusive Hello must be dispatched"
            );
        } else {
            assert!(
                responses.is_empty(),
                "EOF must not execute an unterminated Hello"
            );
        }
    }
}
