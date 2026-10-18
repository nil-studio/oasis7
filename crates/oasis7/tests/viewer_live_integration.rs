#![cfg(all(feature = "viewer_live_integration", feature = "test_tier_full"))]

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use oasis7::viewer::{
    PromptControlApplyRequest, PromptControlCommand, VIEWER_PROTOCOL_VERSION, ViewerControl,
    ViewerLiveServer, ViewerLiveServerConfig, ViewerRequest, ViewerResponse, ViewerStream,
};
use oasis7_simulator::WorldScenario;

#[test]
fn live_server_accepts_client_and_emits_snapshot_and_event() {
    let port = find_free_port();
    let addr = format!("127.0.0.1:{port}");

    let mut server = ViewerLiveServer::new(
        ViewerLiveServerConfig::new(WorldScenario::TwinRegionBootstrap)
            .with_bind_addr(addr.clone()),
    )
    .expect("server");

    let handle = thread::spawn(move || server.run_once().expect("run once"));

    let stream = connect_with_retry(&addr, Duration::from_secs(1));
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("timeout");
    let reader_stream = stream.try_clone().expect("clone");
    let mut writer = BufWriter::new(stream);

    send_request(
        &mut writer,
        &ViewerRequest::Hello {
            client: "test".to_string(),
            version: VIEWER_PROTOCOL_VERSION,
        },
    );
    send_request(
        &mut writer,
        &ViewerRequest::Subscribe {
            streams: vec![
                ViewerStream::Snapshot,
                ViewerStream::Events,
                ViewerStream::Metrics,
            ],
            event_kinds: Vec::new(),
        },
    );
    send_request(&mut writer, &ViewerRequest::RequestSnapshot);
    send_request(
        &mut writer,
        &ViewerRequest::PromptControl {
            command: Box::new(PromptControlCommand::Apply {
                request: PromptControlApplyRequest {
                    agent_id: "agent-0".to_string(),
                    player_id: "integration-player".to_string(),
                    request_id: None,
                    session_epoch: None,
                    binding_epoch: None,
                    expected_authority_epoch: None,
                    public_key: None,
                    auth: None,
                    strong_auth_grant: None,
                    expected_version: Some(0),
                    updated_by: Some("integration-test".to_string()),
                    system_prompt_override: Some(Some("system".to_string())),
                    short_term_goal_override: None,
                    long_term_goal_override: None,
                },
            }),
        },
    );
    send_request(
        &mut writer,
        &ViewerRequest::Control {
            mode: ViewerControl::Step { count: 1 },
            request_id: None,
        },
    );

    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();
    let mut saw_hello = false;
    let mut saw_snapshot = false;
    let mut saw_event = false;
    let mut saw_prompt_error = false;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(2) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(response) = serde_json::from_str::<ViewerResponse>(trimmed) {
                    match response {
                        ViewerResponse::HelloAck { .. } => saw_hello = true,
                        ViewerResponse::Snapshot { .. } => saw_snapshot = true,
                        ViewerResponse::Event { .. } => saw_event = true,
                        ViewerResponse::PromptControlError { .. } => saw_prompt_error = true,
                        _ => {}
                    }
                }
            }
            Err(_) => break,
        }

        if saw_hello && saw_snapshot && saw_event && saw_prompt_error {
            break;
        }
    }

    assert!(saw_hello);
    assert!(saw_snapshot);
    assert!(saw_event);
    assert!(saw_prompt_error);

    drop(reader);
    drop(writer);
    handle.join().expect("server exit");
}

fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("free port")
}

fn connect_with_retry(addr: &str, timeout: Duration) -> TcpStream {
    let start = Instant::now();
    loop {
        if let Ok(stream) = TcpStream::connect(addr) {
            return stream;
        }
        if start.elapsed() > timeout {
            panic!("connect timeout");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn send_request(writer: &mut BufWriter<TcpStream>, request: &ViewerRequest) {
    serde_json::to_writer(&mut *writer, request).expect("write request");
    writer.write_all(b"\n").expect("newline");
    writer.flush().expect("flush");
}
