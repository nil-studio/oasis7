use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

const CANARY: &str = "PRIVATE_PROVIDER_CANARY_5103";
const INFO: &str = r#"{"provider_id":"mock","protocol_version":"v1","chain_resource_manifest_schema_version":"oasis7.world_resource_manifest.v1","chain_resource_delta_schema_version":"oasis7.world_resource_delta.v1","capabilities":["decision","feedback"],"supported_action_sets":["phase1_low_frequency"]}"#;

fn run_probe(replies: Vec<(u16, String)>, timeout_ms: u64, delay: bool) -> ClientLauncherApp {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for (status, body) in replies {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "missing probe request"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock accept: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            stream.read(&mut request).unwrap();
            if delay {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let mut app = ClientLauncherApp::default();
    app.config.agent_provider_url = format!("http://{address}/{CANARY}/");
    app.config.agent_provider_transport = "loopback_http".into();
    app.config.agent_provider_auth_token = CANARY.into();
    app.config.agent_provider_connect_timeout_ms = timeout_ms.to_string();
    app.check_local_provider();
    server.join().unwrap();
    app
}

fn assert_safe_surfaces(app: &ClientLauncherApp, expected: &str) {
    let detail = app
        .provider_check_status
        .detail()
        .expect("diagnostic detail");
    let logs = app.logs.iter().cloned().collect::<Vec<_>>().join("\n");
    assert!(
        !detail.contains(CANARY),
        "UI leaked provider content: {detail}"
    );
    assert!(
        !logs.contains(CANARY),
        "log leaked provider content: {logs}"
    );
    assert!(detail.contains(expected), "expected {expected}: {detail}");
    assert!(
        logs.contains(expected),
        "expected log diagnostic {expected}: {logs}"
    );
    assert!(detail.len() < 256, "error detail must be bounded");
}

#[test]
fn native_provider_probe_errors_do_not_echo_body_or_decoder_tokens_to_ui_and_log() {
    for route in ["info", "health"] {
        let invalid_typed_json = if route == "health" {
            serde_json::json!({"ok":CANARY}).to_string()
        } else {
            serde_json::json!({"provider_id":"mock", "capabilities":CANARY}).to_string()
        };
        for (status, body, expected) in [
            (401, format!("token={CANARY}"), "HTTP 401"),
            (503, format!("upstream secret {CANARY}"), "HTTP 503"),
            (200, invalid_typed_json, "decode failed"),
            (200, format!("{{{CANARY}"), "decode failed"),
        ] {
            let mut replies = if route == "health" {
                vec![(200, INFO.into())]
            } else {
                vec![]
            };
            replies.push((status, body));
            let app = run_probe(replies, 500, false);
            assert_safe_surfaces(&app, expected);
            if status == 401 {
                assert!(matches!(
                    app.provider_check_status,
                    ProviderCheckStatus::Unauthorized(_)
                ));
            } else {
                assert!(matches!(
                    app.provider_check_status,
                    ProviderCheckStatus::Unreachable(_)
                ));
            }
        }
    }
}

#[test]
fn native_provider_probe_network_errors_do_not_echo_private_urls() {
    let app = run_probe(vec![(200, INFO.into())], 100, true);
    assert_safe_surfaces(&app, "timed out");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let mut app = ClientLauncherApp::default();
    app.config.agent_provider_url = format!("http://{address}/{CANARY}/");
    app.config.agent_provider_transport = "loopback_http".into();
    app.check_local_provider();
    assert_safe_surfaces(&app, "connection failed");
    app.config.agent_provider_auth_token = format!("{CANARY}\ninvalid-header");
    app.check_local_provider();
    assert_safe_surfaces(&app, "configuration failed");
}

#[test]
fn native_provider_probe_health_error_text_is_not_a_ui_or_log_diagnostic() {
    for (ok, status, error) in [
        (false, "ready", Some(CANARY)),
        (false, CANARY, None),
        (true, CANARY, None),
    ] {
        let health = serde_json::json!({"ok":ok,"status":status,"last_error":error});
        let app = run_probe(
            vec![(200, INFO.into()), (200, health.to_string())],
            500,
            false,
        );
        assert!(matches!(
            app.provider_check_status,
            ProviderCheckStatus::Degraded(_)
        ));
        let detail = app.provider_check_status.detail().unwrap();
        assert!(!detail.contains(CANARY), "{detail}");
        assert!(app.logs.iter().all(|line| !line.contains(CANARY)));
    }
}

#[test]
fn native_provider_probe_invalid_configuration_does_not_echo_values() {
    let mut app = ClientLauncherApp::default();
    app.config.agent_provider_url = format!("http://127.0.0.1:{CANARY}/");
    app.config.agent_provider_transport = "loopback_http".into();
    app.check_local_provider();
    assert_safe_surfaces(&app, "configuration");
    app.config.agent_provider_url = "http://127.0.0.1:1/".into();
    app.config.agent_provider_connect_timeout_ms = CANARY.into();
    app.check_local_provider();
    assert_safe_surfaces(&app, "timeout");
}

#[test]
fn native_provider_probe_unsupported_schema_error_does_not_echo_remote_value() {
    for (field, category) in [
        (
            "chain_resource_manifest_schema_version",
            "world_resource_manifest_schema",
        ),
        (
            "chain_resource_delta_schema_version",
            "world_resource_delta_schema",
        ),
    ] {
        for value in [Some(CANARY), Some(""), None] {
            let mut info: serde_json::Value = serde_json::from_str(INFO).unwrap();
            if let Some(value) = value {
                info[field] = serde_json::json!(value);
            } else {
                info.as_object_mut().unwrap().remove(field);
            }
            let app = run_probe(
                vec![
                    (200, info.to_string()),
                    (200, r#"{"ok":true,"status":"ready"}"#.into()),
                ],
                500,
                false,
            );
            assert!(matches!(
                app.provider_check_status,
                ProviderCheckStatus::Incompatible(_)
            ));
            let detail = app.provider_check_status.detail().unwrap();
            let diagnostic = if value == Some(CANARY) {
                format!("{category}=unsupported")
            } else {
                format!("missing_{category}")
            };
            assert!(!detail.contains(CANARY), "{detail}");
            assert!(detail.contains(&diagnostic), "{detail}");
            let logs = app.logs.iter().cloned().collect::<Vec<_>>().join("\n");
            assert!(!logs.contains(CANARY), "{logs}");
            assert!(logs.contains(&diagnostic), "{logs}");
        }
    }
}

// Raw HTTP fixtures exercise the production reader and app status/log projection.
fn run_wire_probe(
    replies: Vec<(String, u64)>,
    timeout_ms: u64,
) -> (ClientLauncherApp, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        for (reply, pause_ms) in replies {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "missing request");
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let len = stream.read(&mut request).unwrap();
            paths.push(
                String::from_utf8_lossy(&request[..len])
                    .lines()
                    .next()
                    .unwrap()
                    .into(),
            );
            let _ = stream.write_all(reply.as_bytes());
            std::thread::sleep(std::time::Duration::from_millis(pause_ms));
        }
        paths
    });
    let mut app = ClientLauncherApp::default();
    app.config.agent_provider_url = format!("http://{address}/{CANARY}/");
    app.config.agent_provider_transport = "loopback_http".into();
    app.config.agent_provider_connect_timeout_ms = timeout_ms.to_string();
    app.check_local_provider();
    (app, server.join().unwrap())
}

fn wire_response(body: &str, framing: &str) -> String {
    match framing {
        "chunked" => format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        ),
        "close" => format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{body}"),
        _ => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    }
}

#[test]
fn native_provider_probe_stream_budget_rejects_oversize_on_both_endpoints() {
    for route in ["info", "health"] {
        for framing in ["length", "close", "chunked"] {
            for valid in [true, false] {
                let base = if route == "info" {
                    INFO
                } else {
                    r#"{"ok":true}"#
                };
                let mut body = base.to_string();
                body.extend(std::iter::repeat_n(' ', 65_537 - body.len()));
                if !valid {
                    body.replace_range(0..CANARY.len(), CANARY);
                }
                let mut replies = Vec::new();
                if route == "health" {
                    replies.push((wire_response(INFO, "length"), 0));
                }
                replies.push((wire_response(&body, framing), 0));
                let (app, paths) = run_wire_probe(replies, 1000);
                assert_safe_surfaces(&app, "response_too_large");
                assert_eq!(paths.len(), if route == "health" { 2 } else { 1 });
                assert!(paths[0].contains("/v1/provider/info"));
                if route == "health" {
                    assert!(paths[1].contains("/v1/provider/health"));
                }
            }
        }
    }
}

#[test]
fn native_provider_probe_stream_budget_accepts_exact_limit_per_response() {
    for framing in ["length", "close", "chunked"] {
        let mut info = INFO.to_string();
        info.extend(std::iter::repeat_n(' ', 65_536 - info.len()));
        let mut health = r#"{"ok":true,"status":"ready"}"#.to_string();
        health.extend(std::iter::repeat_n(' ', 65_536 - health.len()));
        let (app, paths) = run_wire_probe(
            vec![
                (wire_response(&info, framing), 0),
                (wire_response(&health, framing), 0),
            ],
            1000,
        );
        assert!(matches!(
            app.provider_check_status,
            ProviderCheckStatus::Ready(_)
        ));
        assert_eq!(paths.len(), 2);
    }
}

#[test]
fn native_provider_probe_stream_budget_distinguishes_timeout_truncation_and_decode() {
    // Exact limit without EOF must still wait for termination under the existing timeout.
    let body = " ".repeat(65_536);
    let (app, _) = run_wire_probe(vec![(wire_response(&body, "close"), 350)], 100);
    assert_safe_surfaces(&app, "timed out");
    // A false large length cannot make an incomplete small response look complete.
    let reply =
        format!("HTTP/1.1 200 OK\r\nContent-Length: 60000\r\nConnection: close\r\n\r\n{CANARY}");
    let (app, _) = run_wire_probe(vec![(reply, 0)], 1000);
    assert_safe_surfaces(&app, "read failed");
    let (app, _) = run_wire_probe(vec![(wire_response(CANARY, "chunked"), 0)], 1000);
    assert_safe_surfaces(&app, "decode failed");
    // HTTP framing treats a small Content-Length as the end of this response;
    // excess wire bytes must never reach the JSON decoder or health request.
    let reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{{{}",
        "x".repeat(70_000)
    );
    let (app, paths) = run_wire_probe(vec![(reply, 0)], 1000);
    assert_safe_surfaces(&app, "decode failed");
    assert_eq!(paths.len(), 1);
}

#[test]
fn native_provider_probe_stream_budget_keeps_total_timeout_for_drip_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        );
        for _ in 0..10 {
            if stream.write_all(b"1\r\n \r\n").is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        let _ = stream.write_all(b"0\r\n\r\n");
    });
    let mut app = ClientLauncherApp::default();
    app.config.agent_provider_url = format!("http://{address}/{CANARY}/");
    app.config.agent_provider_transport = "loopback_http".into();
    app.config.agent_provider_connect_timeout_ms = "150".into();
    app.check_local_provider();
    server.join().unwrap();
    assert_safe_surfaces(&app, "timed out");
}
