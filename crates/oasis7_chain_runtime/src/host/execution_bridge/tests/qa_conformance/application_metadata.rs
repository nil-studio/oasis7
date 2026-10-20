//! Real Info/Health fault gates test a second hosted Viewer before each gate releases.
use super::*;
use oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer;
use std::io::{BufRead, BufReader, Write};
#[test]
fn real_tcp_hosted_slow_provider_metadata_preserves_second_viewer_snapshot() {
    run_isolated_application(false, false, false, true, true, false);
}
fn stream(
    shared: &Arc<Mutex<ViewerRuntimeLiveServer>>,
) -> (
    TcpStream,
    thread::JoinHandle<Result<(), oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServerError>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    let server = shared.clone();
    (
        client,
        thread::spawn(move || ViewerRuntimeLiveServer::test_serve_shared_stream(server, accepted)),
    )
}
fn subscribe(socket: &mut TcpStream) {
    // Keep identical newline-delimited frames in one write so the probe does
    // not measure client Nagle/delayed-ACK scheduling between tiny requests.
    let mut batch = Vec::new();
    for request in [
        serde_json::json!({"type":"hello_v2","client":"PRE2 metadata concurrent QA","version":2,"capabilities":[]}),
        serde_json::json!({"type":"subscribe","streams":["snapshot","events"],"event_kinds":[]}),
        serde_json::json!({"type":"request_snapshot"}),
    ] {
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        batch.extend_from_slice(&bytes);
    }
    socket.write_all(&batch).unwrap();
}
pub(super) fn verify_metadata_responsiveness(client: &RemoteWorldServiceClient) {
    let shared = Arc::new(Mutex::new(application_hosted::prepare_server(client)));
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    let (mut first, worker) = stream(&shared);
    subscribe(&mut first);
    let mut observations = Vec::new();
    let mut secondary_workers = Vec::new();
    for kind in ["info", "health"] {
        let wait = Instant::now() + Duration::from_secs(3);
        while !root.join(format!("provider-{kind}-started")).exists() && Instant::now() < wait {
            thread::sleep(Duration::from_millis(2));
        }
        let started = root.join(format!("provider-{kind}-started")).exists();
        let (mut second, second_worker) = stream(&shared);
        subscribe(&mut second);
        second
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        let mut reader = BufReader::new(second);
        let began = Instant::now();
        let deadline = began + Duration::from_millis(150);
        let mut snapshot = false;
        let mut line = String::new();
        let mut frames = Vec::new();
        while Instant::now() < deadline {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1));
            reader.get_mut().set_read_timeout(Some(remaining)).unwrap();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if !line.ends_with('\n') {
                        continue;
                    }
                    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                    frames.push(serde_json::json!({"type":value["type"],"bytes":line.len(),"elapsed_us":began.elapsed().as_micros()}));
                    if value["type"] == "snapshot" {
                        snapshot = Instant::now() < deadline;
                        break;
                    }
                    line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => break,
            }
        }
        let elapsed = began.elapsed();
        let still_held = !root.join(format!("provider-{kind}-release")).exists();
        // Always release the real 2s fault gate before shutdown/join or any assertion.
        fs::write(root.join(format!("provider-{kind}-release")), b"release").unwrap();
        let _ = reader.get_mut().shutdown(std::net::Shutdown::Both);
        secondary_workers.push(second_worker);
        println!(
            "hosted_metadata_frames kind={kind} frames={frames:?} partial_bytes={}",
            line.len()
        );
        println!(
            "hosted_metadata_concurrency kind={kind} parsed_request={started} snapshot_before_release={snapshot} gate_held={still_held} elapsed_ms={} read_budget_ms=150 test_gate_deadline_ms=2000 worker_join_deferred=true",
            elapsed.as_millis()
        );
        observations.push((kind, started, snapshot && still_held));
    }
    let _ = first.shutdown(std::net::Shutdown::Both);
    let joined = worker.join();
    assert!(
        secondary_workers
            .into_iter()
            .all(|worker| matches!(worker.join(), Ok(Ok(())))),
        "secondary serving worker panicked"
    );
    assert!(
        matches!(joined, Ok(Ok(()))),
        "primary serving worker failed: {joined:?}"
    );
    assert!(
        observations
            .iter()
            .all(|(_, started, snapshot)| *started && *snapshot),
        "slow metadata blocked second actual Viewer snapshot: {observations:?}"
    );
    println!("PRE2_HOSTED_SLOW_METADATA_SECOND_VIEWER_PASSED");
}
