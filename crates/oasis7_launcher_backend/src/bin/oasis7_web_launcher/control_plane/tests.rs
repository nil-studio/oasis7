use super::{
    ChainFeedbackSubmitRequest, ChainTransferSubmitRequest, LauncherConfig, ServiceState,
    parse_chain_feedback_request, parse_chain_transfer_request, start_chain_process,
    submit_chain_feedback_remote, submit_chain_transfer, submit_chain_transfer_remote,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];

    loop {
        let read = stream.read(&mut buffer).expect("read request");
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        let Some(boundary) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header =
            std::str::from_utf8(&bytes[..boundary]).expect("request header should be UTF-8");
        let content_length = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if bytes.len() >= boundary + 4 + content_length {
            break;
        }
    }

    bytes
}

fn write_http_json_response(stream: &mut std::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .expect("write response should succeed");
}

#[test]
fn parse_chain_transfer_request_rejects_invalid_json() {
    let err = parse_chain_transfer_request(br#"{"from_account_id":"player:alice"}"#)
        .expect_err("invalid payload should fail");
    assert!(err.contains("parse chain transfer request JSON failed"));
}

#[test]
fn parse_chain_feedback_request_rejects_invalid_json() {
    let err = parse_chain_feedback_request(br#"{"category":"bug","title":"x"}"#)
        .expect_err("invalid payload should fail");
    assert!(err.contains("parse chain feedback request JSON failed"));
}

#[test]
fn submit_chain_transfer_remote_posts_expected_payload_and_reads_success() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let bind = listener.local_addr().expect("local addr");

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let request_bytes = read_http_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request_bytes);
        assert!(request_text.starts_with("POST /v1/chain/transfer/submit HTTP/1.1"));
        assert!(request_text.contains("\"from_account_id\":\"player:alice\""));
        assert!(request_text.contains("\"to_account_id\":\"player:bob\""));
        assert!(request_text.contains("\"amount\":7"));
        assert!(request_text.contains("\"nonce\":2"));
        assert!(request_text.contains(
            "\"public_key\":\"1111111111111111111111111111111111111111111111111111111111111111\""
        ));
        assert!(request_text.contains("\"signature\":\"octransferauth:v1:"));
        write_http_json_response(
            &mut stream,
            "200 OK",
            r#"{"ok":true,"action_id":11,"submitted_at_unix_ms":1700000000}"#,
        );
    });

    let request = ChainTransferSubmitRequest {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 7,
        nonce: 2,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
        public_key: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        signature: concat!(
            "octransferauth:v1:",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "2222222222222222222222222222222222222222222222222222222222222222"
        )
        .to_string(),
    };
    let response =
        submit_chain_transfer_remote(format!("127.0.0.1:{}", bind.port()).as_str(), &request)
            .expect("submit should succeed");
    assert!(response.ok);
    assert_eq!(response.action_id, Some(11));
    server.join().expect("server thread should finish");
}

#[test]
fn submit_chain_transfer_remote_returns_rejected_payload_for_http_400() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let bind = listener.local_addr().expect("local addr");

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let _ = read_http_request(&mut stream);
        write_http_json_response(
            &mut stream,
            "400 Bad Request",
            r#"{"ok":false,"error_code":"invalid_request","error":"bad payload"}"#,
        );
    });

    let request = ChainTransferSubmitRequest {
        from_account_id: "player:alice".to_string(),
        to_account_id: "player:bob".to_string(),
        amount: 7,
        nonce: 2,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
        public_key: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        signature: concat!(
            "octransferauth:v1:",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "2222222222222222222222222222222222222222222222222222222222222222"
        )
        .to_string(),
    };
    let response =
        submit_chain_transfer_remote(format!("127.0.0.1:{}", bind.port()).as_str(), &request)
            .expect("proxy should return rejected payload");
    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("invalid_request"));
    assert_eq!(response.error.as_deref(), Some("bad payload"));
    server.join().expect("server thread should finish");
}

#[test]
fn submit_chain_transfer_requires_strong_auth_for_hosted_public_join() {
    let mut state = ServiceState::new(
        "launcher".to_string(),
        "chain".to_string(),
        LauncherConfig {
            deployment_mode: "hosted_public_join".to_string(),
            chain_enabled: true,
            ..LauncherConfig::default()
        },
    );
    let request = ChainTransferSubmitRequest {
        from_account_id: "oc:pk:alice".to_string(),
        to_account_id: "protocol:treasury".to_string(),
        amount: 7,
        nonce: 2,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
        public_key: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        signature: concat!(
            "octransferauth:v1:",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "2222222222222222222222222222222222222222222222222222222222222222"
        )
        .to_string(),
    };

    let response = submit_chain_transfer(&mut state, &request);

    assert!(!response.ok);
    assert_eq!(response.error_code.as_deref(), Some("strong_auth_required"));
    assert!(
        response.error.as_deref().is_some_and(
            |message| message.contains("hosted public join blocks main token transfer")
        )
    );
    assert!(
        state
            .logs
            .iter()
            .any(|line| line.contains("strong_auth/private plane"))
    );
}

#[test]
fn start_chain_process_rejects_hosted_public_join_local_runtime() {
    let mut state = ServiceState::new(
        "launcher".to_string(),
        "chain".to_string(),
        LauncherConfig::default(),
    );
    let err = start_chain_process(
        &mut state,
        LauncherConfig {
            deployment_mode: "hosted_public_join".to_string(),
            chain_enabled: true,
            ..LauncherConfig::default()
        },
    )
    .expect_err("hosted public join should block local chain runtime");

    assert!(err.contains("guest/player session lane"));
    assert!(state.logs.iter().any(|line| {
        line.contains("chain runtime start rejected") && line.contains("operator-managed")
    }));
}

#[test]
fn start_chain_process_rejects_invalid_deployment_mode_as_most_restrictive() {
    let mut state = ServiceState::new(
        "launcher".to_string(),
        "chain".to_string(),
        LauncherConfig::default(),
    );
    let err = start_chain_process(
        &mut state,
        LauncherConfig {
            deployment_mode: "hosted-public-join-typo".to_string(),
            chain_enabled: true,
            ..LauncherConfig::default()
        },
    )
    .expect_err("invalid deployment mode should still block local chain runtime");

    assert!(err.contains("deployment_mode=hosted-public-join-typo is invalid"));
    assert!(
        state.logs.iter().any(|line| {
            line.contains("chain runtime start rejected") && line.contains("invalid")
        })
    );
}

#[test]
fn submit_chain_feedback_remote_posts_expected_payload_and_reads_success() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let bind = listener.local_addr().expect("local addr");

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let request_bytes = read_http_request(&mut stream);
        let request_text = String::from_utf8_lossy(&request_bytes);
        assert!(request_text.starts_with("POST /v1/chain/feedback/submit HTTP/1.1"));
        assert!(request_text.contains("\"category\":\"bug\""));
        assert!(request_text.contains("\"title\":\"web feedback\""));
        assert!(request_text.contains("\"description\":\"looks good\""));
        write_http_json_response(
            &mut stream,
            "200 OK",
            r#"{"ok":true,"feedback_id":"fb-1","event_id":"evt-1"}"#,
        );
    });

    let request = ChainFeedbackSubmitRequest {
        category: "bug".to_string(),
        title: "web feedback".to_string(),
        description: "looks good".to_string(),
        platform: "client_launcher_web".to_string(),
        game_version: "unknown".to_string(),
    };
    let response =
        submit_chain_feedback_remote(format!("127.0.0.1:{}", bind.port()).as_str(), &request)
            .expect("submit should succeed");
    assert!(response.ok);
    assert_eq!(response.feedback_id.as_deref(), Some("fb-1"));
    assert_eq!(response.event_id.as_deref(), Some("evt-1"));
    server.join().expect("server thread should finish");
}

#[test]
fn submit_chain_feedback_remote_returns_rejected_payload_for_http_400() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let bind = listener.local_addr().expect("local addr");

    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let _ = read_http_request(&mut stream);
        write_http_json_response(
            &mut stream,
            "400 Bad Request",
            r#"{"ok":false,"error":"invalid category"}"#,
        );
    });

    let request = ChainFeedbackSubmitRequest {
        category: "bug".to_string(),
        title: "web feedback".to_string(),
        description: "looks good".to_string(),
        platform: "client_launcher_web".to_string(),
        game_version: "unknown".to_string(),
    };
    let response =
        submit_chain_feedback_remote(format!("127.0.0.1:{}", bind.port()).as_str(), &request)
            .expect("proxy should return rejected payload");
    assert!(!response.ok);
    assert_eq!(response.error.as_deref(), Some("invalid category"));
    server.join().expect("server thread should finish");
}

#[test]
fn resolve_viewer_static_env_override_returns_trimmed_oasis7_value() {
    let resolved = super::resolve_viewer_static_env_override(Some(" web-dist ".to_string()));
    assert_eq!(resolved.as_deref(), Some("web-dist"));
}

#[test]
fn resolve_viewer_static_env_override_rejects_blank_value() {
    let resolved = super::resolve_viewer_static_env_override(Some("   ".to_string()));
    assert!(resolved.is_none());
}

fn state_with_previous_chain_identity() -> ServiceState {
    let mut state = ServiceState::new(
        "launcher".into(),
        "missing-runtime".into(),
        LauncherConfig {
            deployment_mode: "trusted_local_only".into(),
            chain_enabled: true,
            ..LauncherConfig::default()
        },
    );
    state.chain_identity = Some(super::ChainIdentitySnapshot {
        chain_id: "old-chain".into(),
        network_id: "old-network".into(),
    });
    state.chain_replication_status = Some(super::ChainReplicationSnapshot {
        local_peer_id: "old-peer".into(),
        connected_peers: vec![],
        peer_healths: vec![],
        request_peer_scores: std::collections::BTreeMap::new(),
    });
    state
}

#[test]
fn session_stop_discards_previous_chain_evidence() {
    let mut state = state_with_previous_chain_identity();
    state.chain_runtime_status = super::ChainRuntimeStatus::Unreachable("probe failed".into());
    super::stop_chain_process(&mut state).expect("stop no-op");
    assert!(
        state.chain_replication_status.is_none(),
        "retained old replication evidence"
    );
    assert!(
        state.chain_identity.is_none(),
        "stopped session retained old authority"
    );
    assert!(matches!(
        state.chain_runtime_status,
        super::ChainRuntimeStatus::Unreachable(_)
    ));
}

#[test]
fn session_failed_start_discards_previous_chain_evidence() {
    let mut state = state_with_previous_chain_identity();
    let config = state.config.clone();
    assert!(start_chain_process(&mut state, config).is_err());
    assert!(
        state.chain_replication_status.is_none(),
        "retained old replication evidence"
    );
    assert!(
        state.chain_identity.is_none(),
        "failed new session retained old authority"
    );
}

#[test]
fn session_without_process_discards_previous_chain_evidence() {
    let mut state = state_with_previous_chain_identity();
    state.chain_runtime_status = super::ChainRuntimeStatus::StaleExecutionWorld("stale".into());
    super::update_chain_runtime_status(&mut state);
    assert!(
        state.chain_replication_status.is_none(),
        "retained old replication evidence"
    );
    assert!(
        state.chain_identity.is_none(),
        "stale session retained old authority"
    );
    assert!(matches!(
        state.chain_runtime_status,
        super::ChainRuntimeStatus::StaleExecutionWorld(_)
    ));
}

#[test]
#[cfg(unix)]
fn session_process_exit_discards_previous_chain_evidence() {
    let mut state = state_with_previous_chain_identity();
    let mut child = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("spawn child");
    child.wait().expect("wait exit");
    let (_, log_rx) = std::sync::mpsc::channel();
    state.chain_running = Some(super::RunningProcess { child, log_rx });
    super::poll_chain_process_state(&mut state);
    assert!(state.chain_identity.is_none());
    assert!(state.chain_replication_status.is_none());
    assert!(state.chain_running.is_none());
    assert!(matches!(
        state.chain_runtime_status,
        super::ChainRuntimeStatus::Unreachable(_)
    ));
}

#[test]
#[cfg(unix)]
fn session_duplicate_start_keeps_running_process_and_evidence() {
    let mut state = state_with_previous_chain_identity();
    let child = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("spawn child");
    let (_, log_rx) = std::sync::mpsc::channel();
    state.chain_running = Some(super::RunningProcess { child, log_rx });
    let mut config = state.config.clone();
    config.chain_enabled = false;
    let result = start_chain_process(&mut state, config);
    let mut running = state
        .chain_running
        .take()
        .expect("running process preserved");
    let _ = running.child.kill();
    running.child.wait().expect("reap child");
    assert_eq!(
        result.expect_err("duplicate start rejected"),
        "oasis7_chain_runtime is already running"
    );
    assert!(state.config.chain_enabled);
    assert!(state.chain_identity.is_some());
    assert!(state.chain_replication_status.is_some());
}
