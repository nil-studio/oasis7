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
