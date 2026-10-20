//! Real metadata/control/checkpoint boundaries for ordinary fresh hosted admission.
use super::*;
use oasis7_viewer_runtime::viewer::{
    ControlCompletionStatus, ViewerResponse, ViewerRuntimeLiveServer,
};
use std::io::{BufRead, BufReader, Write};

#[test]
fn real_tcp_fresh_admission_waits_for_successful_metadata() {
    run("fresh-metadata");
}
#[test]
fn real_tcp_fresh_admission_pause_before_metadata_release_does_not_start() {
    run("fresh-paused");
}
#[test]
fn real_tcp_fresh_admission_checkpoint_io_failure_precedes_submit_and_model() {
    run("fresh-bad-store");
}
fn run(mode: &str) {
    application_harness::run_isolated_application_mode(
        false, false, false, true, false, false, mode,
    );
}

pub(super) fn verify(client: &RemoteWorldServiceClient, mode: &str) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    application_fresh::preflight(client, &root);
    let mut independent_config = client.config().clone();
    independent_config.scope_id = "agent:agent-a".into();
    let independent_client = RemoteWorldServiceClient::new(independent_config).unwrap();
    let canonical = independent_client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: "agent:agent-a".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    assert_eq!(canonical.version().visibility_scope, "agent:agent-a");
    assert_eq!(canonical.continuation().scope_id, "agent:agent-a");
    let store = if mode == "fresh-bad-store" {
        let parent = root.join("regular-file-checkpoint-parent");
        fs::write(&parent, b"actual regular file").unwrap();
        parent.join("lineage.json")
    } else {
        root.join("boundary-private-lineage.json")
    };
    let created = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(store),
    ));
    if mode == "fresh-bad-store" {
        match created {
            Err(oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServerError::Init(reason)) => {
                assert!(
                    reason.contains("Not a directory") || reason.contains("not a directory"),
                    "actual regular-file parent filesystem rejection required"
                )
            }
            Err(other) => panic!("unexpected checkpoint constructor failure: {other:?}"),
            Ok(_) => panic!(
                "regular-file checkpoint parent must fail before model or canonical admission"
            ),
        }
        assert_zero(&observe(&root, "bad-store").unwrap());
        println!("PRE2_FRESH_CHECKPOINT_IO_PRECEDES_ADMISSION_PASSED listener_started=false");
        return;
    }
    fs::write(
        root.join("provider-admission-gates-arm"),
        b"actual metadata only",
    )
    .unwrap();
    let shared = Arc::new(Mutex::new(created.unwrap()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let serving = shared.clone();
    let worker =
        thread::spawn(move || ViewerRuntimeLiveServer::test_serve_shared_stream(serving, accepted));
    send(
        &mut socket,
        serde_json::json!({"type":"hello_v2","client":"PRE2 admission boundary","version":2,"capabilities":[]}),
    );
    send(
        &mut socket,
        serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}),
    );
    send(&mut socket, serde_json::json!({"type":"request_snapshot"}));
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let initial_deadline = Instant::now() + Duration::from_secs(1);
    let (mut initial_snapshot, mut initial_recovery, mut initial_acks) = (false, false, 0);
    let mut recovery_observed = serde_json::Value::Null;
    // read_line may append an incomplete frame before a socket timeout.
    // Retain it across polling and reader handoff until its newline arrives.
    let mut line = String::new();
    while Instant::now() < initial_deadline && !(initial_snapshot && initial_recovery) {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if value["type"] == "hello_ack" {
                        initial_acks += 1;
                    }
                    if value["type"] == "snapshot" {
                        initial_snapshot = value["snapshot"]["runtime_snapshot"].is_object()
                            && value["snapshot"]["time"]
                                == serde_json::json!(canonical.projection().state.time);
                    }
                    if value["type"] == "authoritative_recovery_ack" {
                        recovery_observed = serde_json::json!({
                            "snapshot_height": value["ack"]["snapshot_height"],
                            "snapshot_hash": value["ack"]["snapshot_hash"],
                            "log_cursor": value["ack"]["log_cursor"]
                        });
                        initial_recovery = value["ack"]["snapshot_height"]
                            == serde_json::json!(canonical.version().commit.position)
                            && value["ack"]["snapshot_hash"].as_str()
                                == Some(canonical.version().commit.state_root_ref.as_str())
                            && value["ack"]["log_cursor"]
                                == serde_json::json!(canonical.continuation().sequence);
                    }
                }
                line.clear();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => break,
        }
    }
    println!(
        "fresh_admission_initial_cursor observed={recovery_observed} expected={} height_matches={} root_matches={} cursor_matches={} subscription=snapshot",
        serde_json::json!({"snapshot_height":canonical.version().commit.position,"snapshot_hash":canonical.version().commit.state_root_ref,"log_cursor":canonical.continuation().sequence}),
        recovery_observed["snapshot_height"]
            == serde_json::json!(canonical.version().commit.position),
        recovery_observed["snapshot_hash"].as_str()
            == Some(canonical.version().commit.state_root_ref.as_str()),
        recovery_observed["log_cursor"] == serde_json::json!(canonical.continuation().sequence)
    );
    let initial_projection = initial_snapshot && initial_recovery;
    println!(
        "fresh_admission_initial_projection snapshot={initial_snapshot} trusted_height_root_cursor={initial_recovery} hello_acks={initial_acks}"
    );
    send(
        &mut socket,
        serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1001}),
    );
    if mode == "fresh-paused" {
        send(
            &mut socket,
            serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1002}),
        );
    }
    send(
        &mut socket,
        serde_json::json!({"type":"hello_v2","client":"PRE2 ordered admission control","version":2,"capabilities":[]}),
    );
    send(&mut socket, serde_json::json!({"type":"request_snapshot"}));
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let (mut acks, mut ordered, mut blocked) = (initial_acks, false, false);
    let control_witness = Arc::new(Mutex::new(serde_json::Value::Null));
    while Instant::now() < deadline {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                if let Ok(ViewerResponse::ControlCompletionAck { ack }) =
                    serde_json::from_str(&line)
                    && ack.request_id == 1001
                    && ack.status == ControlCompletionStatus::Blocked
                {
                    *control_witness.lock().unwrap() = control_diagnostic(&ack);
                    blocked = true;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if v["type"] == "hello_ack" {
                        acks += 1;
                    }
                    if acks >= 2 && v["type"] == "snapshot" {
                        ordered = true;
                        line.clear();
                        break;
                    }
                }
                line.clear();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => break,
        }
    }
    let drain_stop = Arc::new(AtomicBool::new(false));
    let compatible = Arc::new(AtomicBool::new(false));
    let draining = drain_stop.clone();
    let ready_flag = compatible.clone();
    let drained_control = control_witness.clone();
    let all_acks = Arc::new(std::sync::atomic::AtomicUsize::new(acks));
    let drained_acks = all_acks.clone();
    let snapshot_completions = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let drained_completions = snapshot_completions.clone();
    let drain = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(7);
        while !draining.load(Ordering::SeqCst) && Instant::now() < deadline {
            match reader.read_line(&mut line) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    if !line.ends_with('\n') {
                        continue;
                    }
                    if let Ok(ViewerResponse::ControlCompletionAck { ack }) =
                        serde_json::from_str::<ViewerResponse>(&line)
                        && ack.request_id == 1001
                    {
                        *drained_control.lock().unwrap() = control_diagnostic(&ack);
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value["type"] == "hello_ack" {
                            drained_acks.fetch_add(1, Ordering::SeqCst);
                        }
                        if value["type"] == "authoritative_recovery_ack" {
                            drained_completions.fetch_add(1, Ordering::SeqCst);
                        }
                        if ready(&value) {
                            ready_flag.store(true, Ordering::SeqCst);
                        }
                    }
                    line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) => return Err(e.kind()),
            }
        }
        Ok(())
    });
    let status_deadline = Instant::now() + Duration::from_millis(500);
    let status = loop {
        if let Ok(server) = shared.try_lock() {
            break server.test_agent_service_pump_status();
        }
        if Instant::now() >= status_deadline {
            break serde_json::Value::Null;
        }
        thread::sleep(Duration::from_millis(2));
    };
    let gate_deadline = Instant::now() + Duration::from_millis(500);
    while !root.join("provider-info-started").exists() && Instant::now() < gate_deadline {
        thread::sleep(Duration::from_millis(2));
    }
    let started = root.join("provider-info-started").exists();
    let before = observe(&root, "metadata-held");
    // Every fault gate releases before any assertion or worker join.
    for kind in ["info", "health"] {
        fs::write(
            root.join(format!("provider-{kind}-release")),
            b"actual release",
        )
        .unwrap();
    }
    // Each explicit snapshot completes with its ordered recovery Ack before another request.
    let initial_completion_deadline = Instant::now() + Duration::from_millis(500);
    while snapshot_completions.load(Ordering::SeqCst) == 0
        && Instant::now() < initial_completion_deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    let mut observed_completions = snapshot_completions.load(Ordering::SeqCst);
    let initial_request_complete = observed_completions > 0;
    if initial_request_complete {
        send(&mut socket, serde_json::json!({"type":"request_snapshot"}));
    }
    let mut snapshot_requests = usize::from(initial_request_complete);
    // Cover the full observation window rather than consuming every bounded
    // request before a failed metadata probe can recover after its cache TTL.
    let mut last_snapshot_request = Instant::now();
    // Paused negative admission remains a bounded observation. Eligible work
    // waits for its canonical completion evidence under the parent budget.
    let terminal_deadline = Instant::now() + Duration::from_secs(4);
    let mut completion = application_completion::CompletionWatch::new();
    let mut summary = serde_json::Value::Null;
    loop {
        if let Ok(server) = shared.try_lock() {
            summary = server.test_canonical_provider_summary();
        }
        let ready = compatible.load(Ordering::SeqCst);
        let finished = if mode == "fresh-paused" {
            ready || Instant::now() >= terminal_deadline
        } else {
            completion.finished(
                &summary,
                ready && application_completion::canonical_complete(&summary),
            )
        };
        if finished {
            break;
        }
        let completed = snapshot_completions.load(Ordering::SeqCst);
        if initial_request_complete
            && completed > observed_completions
            && snapshot_requests < 16
            && last_snapshot_request.elapsed() >= Duration::from_millis(250)
        {
            observed_completions = completed;
            send(&mut socket, serde_json::json!({"type":"request_snapshot"}));
            snapshot_requests += 1;
            last_snapshot_request = Instant::now();
        }
        thread::sleep(Duration::from_millis(10));
    }
    let metadata_ready = compatible.load(Ordering::SeqCst);
    let after = observe(&root, "metadata-released");
    socket.shutdown(std::net::Shutdown::Both).unwrap();
    let joined = worker.join().unwrap();
    drain_stop.store(true, Ordering::SeqCst);
    let drained = drain.join().unwrap();
    println!(
        "fresh_admission_explicit_snapshot_requests={snapshot_requests} initial_request_complete={initial_request_complete}"
    );
    assert!(
        initial_request_complete,
        "actual initial snapshot request must complete before another request"
    );
    println!(
        "fresh_admission_control_witness initial_hello_acks={acks} all_hello_acks={} ordered={ordered} initial_blocked={blocked} request1001={} actual_status={status} worker_result={joined:?} drain_result={drained:?} metadata_ready={metadata_ready} info_gate_started={started}",
        all_acks.load(Ordering::SeqCst),
        control_witness.lock().unwrap()
    );
    assert!(
        drained.is_ok(),
        "actual Viewer reader teardown failed: {drained:?}"
    );
    assert!(
        initial_projection,
        "actual trusted canonical projection handshake must precede genuine Play"
    );
    assert!(
        ordered && !blocked,
        "genuine Play and ordered control prerequisite"
    );
    assert!(started, "real Info request must be parsed while held");
    assert_eq!(status["eligible"], mode == "fresh-metadata");
    assert_zero(&before.unwrap());
    assert!(
        metadata_ready,
        "actual successful same-config provider compatibility snapshot required after release"
    );
    assert!(
        joined.is_ok(),
        "actual admission boundary serving worker: {joined:?}"
    );
    if mode == "fresh-paused" {
        assert_zero(&after.unwrap());
        assert_ne!(summary["terminal_states"]["agent-a"]["status"], "committed");
        println!("PRE2_FRESH_PAUSE_METADATA_NO_ADMISSION_PASSED");
    } else {
        if summary["terminal_states"]["agent-a"]["status"] != "committed" {
            println!(
                "fresh_metadata_failed_terminal_diagnostic={}",
                serde_json::json!({
                    "hosted_service_phase": summary["hosted_service_phase"],
                    "hosted_service_inflight": summary["hosted_service_inflight"],
                    "hosted_service_memory_failure": summary["hosted_service_memory_failure"],
                    "agent_service_pump_error": summary["agent_service_pump_error"],
                    "pending_intent_count": summary["pending_intent_count"],
                    "pending_action_count": summary["pending_action_count"],
                    "native_model_call_count": summary["native_model_call_count"],
                    "metadata_ready": metadata_ready,
                })
            );
        }
        assert!(
            !application_completion::has_failure(&summary),
            "actual canonical completion error: {summary}"
        );
        assert_eq!(summary["terminal_states"]["agent-a"]["status"], "committed");
        assert!(after.unwrap()["model_decisions"].as_u64().unwrap() > 0);
        application_completion::save_completion(&summary);
        println!("PRE2_FRESH_METADATA_BEFORE_ADMISSION_PASSED");
    }
}
fn send(socket: &mut TcpStream, value: serde_json::Value) {
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    socket.write_all(&bytes).unwrap();
}
fn assert_zero(v: &serde_json::Value) {
    assert_eq!(v["new_canonical_submits"], 0);
    assert_eq!(v["model_decisions"], 0);
}
fn observe(root: &std::path::Path, label: &str) -> Result<serde_json::Value, String> {
    fs::write(
        root.join(format!("admission-observe-{label}")),
        b"actual parent counters",
    )
    .map_err(|e| e.to_string())?;
    let path = root.join(format!("admission-observed-{label}.json"));
    let deadline = Instant::now() + Duration::from_secs(1);
    while !path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let value = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    println!("fresh_admission_boundary label={label} actual_counts={value}");
    Ok(value)
}

pub(super) fn observe_parent(
    fixture: &Fixture,
    root: &std::path::Path,
    models: Arc<std::sync::atomic::AtomicUsize>,
    baseline: usize,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let trace = fixture.lookup_digests.clone();
    let root = root.to_path_buf();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(12);
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            for label in ["metadata-held", "metadata-released", "bad-store"] {
                let output = root.join(format!("admission-observed-{label}.json"));
                if root.join(format!("admission-observe-{label}")).exists() && !output.exists() {
                    let count = trace
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|v| v.starts_with("submit:"))
                        .count()
                        - baseline;
                    fs::write(output,serde_json::to_vec(&serde_json::json!({"new_canonical_submits":count,"model_decisions":models.load(Ordering::SeqCst)})).unwrap()).unwrap();
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
    })
}

fn ready(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => {
            (object
                .get("provider_check_status")
                .is_some_and(|v| v == "ready")
                && object
                    .get("provider_check_source")
                    .is_some_and(|v| v == "runtime_live_probe"))
                || object.values().any(ready)
        }
        serde_json::Value::Array(array) => array.iter().any(ready),
        _ => false,
    }
}

fn control_diagnostic(
    ack: &oasis7_viewer_runtime::viewer::ControlCompletionAck,
) -> serde_json::Value {
    serde_json::json!({"request_id":ack.request_id,"status":ack.status,"error_code":ack.error_code,"error_message":ack.error_message.as_ref().map(|s|s.chars().take(256).collect::<String>())})
}
