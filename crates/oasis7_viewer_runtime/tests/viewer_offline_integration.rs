#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

use oasis7_viewer_runtime::{ViewerServer, ViewerServerConfig};
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use oasis7::viewer::{
    VIEWER_PROTOCOL_VERSION, ViewerControl, ViewerRequest, ViewerResponse, ViewerStream,
    generate_viewer_demo,
};
use oasis7_simulator::WorldScenario;

#[cfg(feature = "test_tier_required")]
#[test]
fn offline_server_accepts_client_and_emits_snapshot_and_event() {
    let temp_dir = make_temp_dir("viewer-offline");
    let _summary =
        generate_viewer_demo(&temp_dir, WorldScenario::TwinRegionBootstrap).expect("demo data");

    let port = find_free_port();
    let addr = format!("127.0.0.1:{port}");
    let config = ViewerServerConfig::from_dir(&temp_dir).with_bind_addr(addr.clone());
    let server = ViewerServer::load(config).expect("server load");

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
                        _ => {}
                    }
                }
            }
            Err(_) => break,
        }

        if saw_hello && saw_snapshot && saw_event {
            break;
        }
    }

    assert!(saw_hello);
    assert!(saw_snapshot);
    assert!(saw_event);

    drop(reader);
    drop(writer);
    handle.join().expect("server exit");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[cfg(feature = "test_tier_required")]
#[test]
fn offline_server_play_control_streams_events_without_timer_ticks() {
    let temp_dir = make_temp_dir("viewer-offline-play");
    let _summary =
        generate_viewer_demo(&temp_dir, WorldScenario::TwinRegionBootstrap).expect("demo data");

    let port = find_free_port();
    let addr = format!("127.0.0.1:{port}");
    let config = ViewerServerConfig::from_dir(&temp_dir).with_bind_addr(addr.clone());
    let server = ViewerServer::load(config).expect("server load");

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
            streams: vec![ViewerStream::Events],
            event_kinds: Vec::new(),
        },
    );
    send_request(
        &mut writer,
        &ViewerRequest::Control {
            mode: ViewerControl::Play,
            request_id: None,
        },
    );

    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();
    let mut event_count = 0_usize;
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
                if let Ok(ViewerResponse::Event { .. }) =
                    serde_json::from_str::<ViewerResponse>(trimmed)
                {
                    event_count = event_count.saturating_add(1);
                    if event_count >= 2 {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }

    assert!(
        event_count >= 2,
        "expected play control to stream at least 2 events, got {event_count}"
    );

    drop(reader);
    drop(writer);
    handle.join().expect("server exit");
    let _ = fs::remove_dir_all(&temp_dir);
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("{prefix}-{nanos}"));
    fs::create_dir_all(&dir).expect("create dir");
    dir
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
