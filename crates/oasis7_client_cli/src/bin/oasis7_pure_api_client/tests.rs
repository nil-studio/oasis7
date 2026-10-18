use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

fn governance_snapshot_response() -> Value {
    let mut snapshot = oasis7::simulator::WorldKernel::new().snapshot();
    let mut runtime = oasis7::runtime::World::new().snapshot();
    runtime.governance_finality_epoch_snapshots.insert(
        0,
        oasis7::runtime::GovernanceFinalityEpochSnapshot::default(),
    );
    snapshot.runtime_snapshot = Some(runtime);
    serde_json::to_value(ViewerResponse::Snapshot { snapshot }).expect("snapshot wire JSON")
}

#[test]
fn snapshot_response_preserves_numeric_governance_epoch_keys() {
    let raw = governance_snapshot_response();
    let before = raw.clone();
    // This is a legitimate serialized wire snapshot; the tagged enum path loses
    // the JSON object-key semantics before reaching its typed inner snapshot.
    assert!(serde_json::from_value::<ViewerResponse>(raw.clone()).is_err());
    assert!(
        serde_json::from_str::<ViewerResponse>(&serde_json::to_string(&raw).expect("wire JSON"))
            .is_err()
    );
    let ViewerResponse::Snapshot { snapshot } = decode_viewer_response(&raw).expect("decode")
    else {
        panic!("expected snapshot");
    };
    assert!(
        snapshot
            .runtime_snapshot
            .expect("runtime")
            .governance_finality_epoch_snapshots
            .contains_key(&0)
    );
    assert_eq!(raw, before);
}

#[test]
fn snapshot_response_rejects_invalid_envelopes_and_numeric_values() {
    for raw in [
        serde_json::json!({"type":"snapshot"}),
        serde_json::json!({"type":"snapshot","snapshot":null}),
        serde_json::json!({"type":"snapshot","snapshot":{}}),
        serde_json::json!({"type":"unknown"}),
    ] {
        assert!(decode_viewer_response(&raw).is_err());
    }
    for value in [
        serde_json::json!("0"),
        serde_json::json!(true),
        serde_json::json!(0.0),
        serde_json::json!(-1),
    ] {
        let mut raw = governance_snapshot_response();
        raw["snapshot"]["runtime_snapshot"]["governance_finality_epoch_snapshots"]["0"]["epoch_id"] =
            value;
        assert!(decode_viewer_response(&raw).is_err());
    }
    let mut raw = governance_snapshot_response();
    let epochs = raw["snapshot"]["runtime_snapshot"]["governance_finality_epoch_snapshots"]
        .as_object_mut()
        .expect("epoch map");
    let epoch = epochs.remove("0").expect("epoch");
    epochs.insert("invalid-u64".into(), epoch);
    assert!(decode_viewer_response(&raw).is_err());
}

fn fixed_private_key_hex(seed: u8) -> String {
    hex::encode([seed; 32])
}

#[test]
fn session_registration_signs_binding_without_force_or_secret_output() {
    let private = fixed_private_key_hex(77);
    let request = build_signed_session_register_request(
        "player-local",
        &private,
        None,
        Some("agent-0".to_string()),
        None,
    )
    .expect("signed registration");
    assert!(!request.force_rebind);
    let proof = request.auth.as_ref().expect("proof");
    oasis7::viewer::verify_session_register_auth_proof(&request, proof).expect("valid binding");
    let mut changed = request.clone();
    changed.requested_agent_id = Some("other-agent".to_string());
    assert!(oasis7::viewer::verify_session_register_auth_proof(&changed, proof).is_err());
    let serialized = serde_json::to_string(&request).expect("serialize");
    assert!(!serialized.contains(&private));
}

#[test]
fn session_registration_hosted_identity_still_requires_grant() {
    let request = build_signed_session_register_request(
        "hosted-player-no-grant",
        &fixed_private_key_hex(78),
        None,
        None,
        None,
    )
    .expect("signed registration");
    assert!(
        oasis7::viewer::verify_session_register_auth_proof(
            &request,
            request.auth.as_ref().expect("proof"),
        )
        .is_err()
    );
}

#[test]
fn session_registration_cli_rejects_force_rebind() {
    let mut args = ArgCursor {
        args: vec!["register-session".to_string(), "--force-rebind".to_string()],
        pos: 0,
    };
    assert!(parse_cli(&mut args).is_err());
}

#[test]
fn agency_control_cli_accepts_one_exact_tagged_json_request() {
    let mut args = ArgCursor {
        args: vec![
            "agency-control".to_string(),
            "--request-json".to_string(),
            r#"{"type":"agency_control_request","request_id":"r1"}"#.to_string(),
        ],
        pos: 0,
    };
    let config = parse_cli(&mut args).expect("parse agency-control command");
    assert!(matches!(config.command, Command::AgencyControl { .. }));
    assert!(!args.has_more());
}

#[test]
fn agency_control_pure_client_sends_and_reads_matching_json_line() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test socket");
    let address = listener.local_addr().expect("socket address");
    let client_stream = TcpStream::connect(address).expect("connect test client");
    let (server_stream, _) = listener.accept().expect("accept test client");
    let mut conn = ViewerConnection {
        reader: BufReader::new(client_stream.try_clone().expect("clone client stream")),
        writer: BufWriter::new(client_stream),
        hello_ack: Value::Null,
        pending_line: String::new(),
        endpoint: "fixture".into(),
    };
    let server = thread::spawn(move || {
        let server_reader = server_stream.try_clone().expect("clone server stream");
        let mut reader = BufReader::new(server_reader);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("read request line");
        let request: Value = serde_json::from_str(request_line.trim()).expect("decode request");
        assert_eq!(request["type"], "agency_control_request");
        assert_eq!(request["request_id"], "r1");
        let mut writer = server_stream;
        serde_json::to_writer(
            &mut writer,
            &json!({"type":"agency_control_response","request_id":"r1","status":"ok"}),
        )
        .expect("write response");
        writer.write_all(b"\n").expect("write response newline");
        writer.flush().expect("flush response");
    });
    let request = json!({"type":"agency_control_request","request_id":"r1"});
    oasis7_pure_api_client_support::send_raw_json_line(&mut conn, &request)
        .expect("send agency command");
    let response =
        oasis7_pure_api_client_support::read_raw_json_line(&mut conn, Duration::from_secs(1))
            .expect("read agency response");
    assert_eq!(response["type"], "agency_control_response");
    assert_eq!(response["request_id"], "r1");
    server.join().expect("server thread");
}

#[test]
fn derive_public_key_hex_matches_signing_key() {
    let private_key_hex = fixed_private_key_hex(7);
    let derived = derive_public_key_hex(private_key_hex.as_str()).expect("derive public key");
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    assert_eq!(derived, hex::encode(signing_key.verifying_key().to_bytes()));
}

#[test]
fn build_signed_agent_chat_request_attaches_auth_and_intent_seq() {
    let private_key_hex = fixed_private_key_hex(8);
    let request = build_signed_agent_chat_request(
        "agent-0",
        "player-1",
        "hello",
        private_key_hex.as_str(),
        None,
        Some(42),
        Some(9),
    )
    .expect("signed chat request");
    assert_eq!(request.player_id.as_deref(), Some("player-1"));
    assert_eq!(request.intent_tick, Some(42));
    assert_eq!(request.intent_seq, Some(9));
    assert!(request.auth.is_some());
    assert!(request.public_key.is_some());
}

#[test]
fn build_signed_prompt_apply_request_supports_clear_and_set() {
    let private_key_hex = fixed_private_key_hex(9);
    let request = build_signed_prompt_apply_request(
        "agent-0",
        "player-1",
        private_key_hex.as_str(),
        None,
        Some(3),
        Some("tester".to_string()),
        Some(Some("system".to_string())),
        Some(None),
        None,
        false,
    )
    .expect("signed prompt apply request");
    assert_eq!(request.expected_version, Some(3));
    assert_eq!(request.updated_by.as_deref(), Some("tester"));
    assert_eq!(
        request.system_prompt_override,
        Some(Some("system".to_string()))
    );
    assert_eq!(request.short_term_goal_override, Some(None));
    assert!(request.auth.is_some());
}

#[test]
fn build_signed_gameplay_action_request_attaches_auth() {
    let private_key_hex = fixed_private_key_hex(10);
    let request = build_signed_gameplay_action_request(
        "build_factory_smelter_mk1",
        "runtime-agent-0",
        None,
        "player-1",
        private_key_hex.as_str(),
        None,
    )
    .expect("signed gameplay action request");
    assert_eq!(request.action_id, "build_factory_smelter_mk1");
    assert_eq!(request.target_agent_id, "runtime-agent-0");
    assert_eq!(request.player_id, "player-1");
    assert!(request.public_key.is_some());
    assert!(request.auth.is_some());
}

#[test]
fn terminal_agent_chat_waits_past_ack_for_reply_or_error() {
    let ack = ViewerResponse::AgentChatAck {
        ack: oasis7::viewer::AgentChatAck {
            auth_nonce: None,
            agent_id: "agent-0".to_string(),
            accepted_at_tick: 7,
            message_len: 5,
            player_id: Some("player-1".to_string()),
            intent_tick: Some(7),
            intent_seq: Some(9),
            idempotent_replay: false,
            intent_id: None,
            accepted_event_seq: None,
            status: None,
            receipt_ref: None,
            replaced_by: None,
        },
    };
    assert!(!terminal_agent_chat(&ack));

    let spoke = ViewerResponse::Event {
        event: oasis7::simulator::WorldEvent {
            id: 1,
            time: 8,
            kind: oasis7_simulator::WorldEventKind::AgentSpoke {
                agent_id: "agent-0".to_string(),
                location_id: "loc-1".to_string(),
                message: "reply".to_string(),
                target_agent_id: Some("agent-0".to_string()),
            },
            runtime_event: None,
        },
    };
    assert!(terminal_agent_chat(&spoke));

    let error = ViewerResponse::AgentChatError {
        error: oasis7::viewer::AgentChatError {
            code: "provider_unreachable".to_string(),
            message: "provider failed".to_string(),
            agent_id: Some("agent-0".to_string()),
        },
    };
    assert!(terminal_agent_chat(&error));
}

#[test]
fn collect_until_reports_timeout_when_peer_stays_open() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let (request_seen_tx, request_seen_rx) = mpsc::channel();
    let (release_server_tx, release_server_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept client");
        let reader_stream = stream.try_clone().expect("clone reader stream");
        let writer_stream = stream.try_clone().expect("clone writer stream");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = writer_stream;
        let mut line = String::new();
        reader.read_line(&mut line).expect("read hello");
        let hello = ViewerResponse::HelloAck {
            server: "oasis7".to_string(),
            version: VIEWER_PROTOCOL_VERSION,
            min_version: 1,
            max_version: VIEWER_PROTOCOL_VERSION,
            capabilities: Vec::new(),
            world_id: "test-world".to_string(),
            control_profile: oasis7::viewer::ViewerControlProfile::Live,
            authority_epoch: None,
        };
        writeln!(
            writer,
            "{}",
            serde_json::to_string(&hello).expect("serialize hello")
        )
        .expect("write hello");
        line.clear();
        reader.read_line(&mut line).expect("read request");
        request_seen_tx.send(()).expect("signal request observed");
        release_server_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("wait for timeout assertion to finish");
    });

    let mut conn = ViewerConnection::connect(
        addr.to_string().as_str(),
        "timeout-test-client",
        Duration::from_millis(50),
    )
    .expect("connect viewer");
    conn.send(&ViewerRequest::RequestSnapshot)
        .expect("request snapshot");
    request_seen_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("server observed request");
    let err = conn
        .collect_until(
            Duration::from_millis(50),
            terminal_snapshot,
            "waiting for snapshot response",
        )
        .expect_err("collect_until should time out");
    assert!(err.contains("timeout after"), "unexpected error: {err}");
    release_server_tx
        .send(())
        .expect("release server after timeout assertion");
    server.join().expect("server join");
}

#[test]
fn pure_api_negotiates_canonical_chat_with_shipped_viewer() {
    use oasis7::viewer::{ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig};
    use oasis7::world_service::{WorldIdentity, client::WorldServiceClientConfig};
    use std::sync::{Arc, Mutex};
    let private = fixed_private_key_hex(73);
    let mut config = ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
    config.world_id = "canonical-cli-fixture".into();
    config.world_service = Some(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: derive_public_key_hex(&private).unwrap(),
        expected_world: WorldIdentity {
            world_id: config.world_id.clone(),
            genesis_digest: "fixture".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: private,
        timeout: Duration::from_millis(50),
        max_response_bytes: 1024 * 1024,
    });
    let server = Arc::new(Mutex::new(ViewerRuntimeLiveServer::new(config).unwrap()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let serving = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        ViewerRuntimeLiveServer::test_serve_shared_stream(server, socket)
    });
    let conn = ViewerConnection::connect(
        &addr.to_string(),
        "pure-api-canonical-qa",
        Duration::from_secs(2),
    )
    .unwrap();
    let capabilities = conn.hello_ack["capabilities"].as_array().unwrap().clone();
    conn.writer
        .get_ref()
        .shutdown(std::net::Shutdown::Both)
        .unwrap();
    assert!(serving.join().unwrap().is_ok());
    assert!(
        capabilities.iter().any(|v| v == "canonical_agent_chat_v1"),
        "pure API must negotiate the actual canonical chat contract: {capabilities:?}"
    );
}
