//! Actual ordinary cadence and non-reading consumer boundaries; no provider factory injection.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::io::{BufRead, BufReader, Write};

#[test]
fn real_tcp_hosted_ordinary_cadence_commits_two_distinct_turns() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "repeated-turns",
    );
}
#[test]
fn real_tcp_nonreading_viewer_preserves_second_primed_viewer() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "slow-consumer",
    );
}
pub(super) struct Session {
    reader: BufReader<TcpStream>,
    partial_line: String,
    diagnostic: bool,
    created: Instant,
    connection: u16,
    accepted_probe: TcpStream,
    worker: thread::JoinHandle<Result<(), oasis7::viewer::ViewerRuntimeLiveServerError>>,
}
impl Session {
    pub(super) fn start(shared: &Arc<Mutex<ViewerRuntimeLiveServer>>, small: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (accepted, _) = listener.accept().unwrap();
        if small {
            small_buffers(&accepted, &socket);
        }
        let accepted_probe = accepted.try_clone().unwrap();
        let connection = socket.local_addr().unwrap().port();
        let server = shared.clone();
        let worker = thread::spawn(move || {
            ViewerRuntimeLiveServer::test_serve_shared_stream(server, accepted)
        });
        Self {
            reader: BufReader::new(socket),
            partial_line: String::new(),
            diagnostic: !small,
            created: Instant::now(),
            connection,
            accepted_probe,
            worker,
        }
    }
    pub(super) fn send(&mut self, value: serde_json::Value) -> std::io::Result<()> {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        let result = self.reader.get_mut().write_all(&bytes);
        if self.diagnostic {
            println!(
                "stream_observer_send connection={} type={} elapsed_us={} result={:?}",
                self.connection,
                value["type"],
                self.created.elapsed().as_micros(),
                result.as_ref().err().map(std::io::Error::kind)
            );
        }
        result
    }
    pub(super) fn snapshot(&mut self, budget: Duration) -> bool {
        self.snapshot_ordered(budget, false)
    }
    pub(super) fn snapshot_ordered(&mut self, budget: Duration, require_hello: bool) -> bool {
        let deadline = Instant::now() + budget;
        let read_started = Instant::now();
        let mut hello_seen = !require_hello;
        let mut snapshot_seen = false;
        while Instant::now() < deadline {
            self.reader
                .get_mut()
                .set_read_timeout(Some(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(50))
                        .max(Duration::from_millis(1)),
                ))
                .unwrap();
            match self.reader.read_line(&mut self.partial_line) {
                Ok(0) => return false,
                Ok(_) => {
                    if !self.partial_line.ends_with('\n') {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.partial_line)
                    {
                        if self.diagnostic {
                            println!(
                                "stream_observer_frame connection={} type={} bytes={} elapsed_us={} read_elapsed_us={}",
                                self.connection,
                                value["type"],
                                self.partial_line.len(),
                                self.created.elapsed().as_micros(),
                                read_started.elapsed().as_micros()
                            );
                        }
                        if value["type"] == "hello_ack" {
                            hello_seen = true;
                        }
                        if hello_seen
                            && value["type"] == "snapshot"
                            && value["snapshot"]["runtime_snapshot"].is_object()
                        {
                            println!("actual_stream_snapshot_bytes={}", self.partial_line.len());
                            snapshot_seen = true;
                        }
                        if snapshot_seen && value["type"] == "authoritative_recovery_ack" {
                            self.partial_line.clear();
                            return true;
                        }
                    } else if self.diagnostic {
                        println!(
                            "stream_observer_invalid_frame connection={} bytes={} elapsed_us={}",
                            self.connection,
                            self.partial_line.len(),
                            self.created.elapsed().as_micros()
                        );
                    }
                    self.partial_line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if self.diagnostic {
                        println!(
                            "stream_observer_timeout connection={} partial_bytes={} partial_type={} elapsed_us={} read_elapsed_us={}",
                            self.connection,
                            self.partial_line.len(),
                            if self.partial_line.is_empty() {
                                "empty"
                            } else {
                                "incomplete_frame"
                            },
                            self.created.elapsed().as_micros(),
                            read_started.elapsed().as_micros()
                        );
                    }
                }
                Err(_) => return false,
            }
        }
        false
    }
    pub(super) fn warm(&mut self) -> bool {
        self.send(serde_json::json!({"type":"hello_v2","client":"PRE2 actual stream boundary","version":2,"capabilities":[]})).unwrap();
        self.send(serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}))
            .unwrap();
        self.send(serde_json::json!({"type":"request_snapshot"}))
            .unwrap();
        self.snapshot(Duration::from_secs(3))
    }
    pub(super) fn close(self) -> Result<(), oasis7::viewer::ViewerRuntimeLiveServerError> {
        self.reader
            .get_ref()
            .shutdown(std::net::Shutdown::Both)
            .unwrap();
        self.worker.join().unwrap()
    }
}
pub(super) fn verify(client: &RemoteWorldServiceClient, mode: &str) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    application_fresh::preflight(client, &root);
    let ordinary = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join("stream-boundary-private-lineage.json")),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(ordinary));
    if mode == "slow-consumer" {
        slow_consumer(shared);
    } else {
        repeated(shared, &root);
    }
}
fn slow_consumer(shared: Arc<Mutex<ViewerRuntimeLiveServer>>) {
    let mut offender = Session::start(&shared, true);
    let mut observer = Session::start(&shared, false);
    let primed = offender.warm() && observer.warm();
    let baseline = phase_exchange(
        &mut observer,
        "baseline",
        Instant::now() + Duration::from_secs(2),
    );
    println!("slow_consumer_phase_baseline {baseline}");
    // Stop consuming the actual offender socket. Each response uses the production writer.
    let mut requests = 0;
    for _ in 0..128 {
        if offender
            .send(serde_json::json!({"type":"request_snapshot"}))
            .is_err()
        {
            break;
        }
        requests += 1;
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut contended_since = None;
    let mut backpressure = false;
    let mut queue_since = None;
    let mut queue_pressure = false;
    let mut maximum_queued = 0;
    while Instant::now() < deadline {
        let queued = kernel_queued_bytes(&offender.accepted_probe);
        maximum_queued = maximum_queued.max(queued);
        if queued > 0 {
            let since = queue_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(100) {
                queue_pressure = true;
            }
        } else {
            queue_since = None;
        }
        if shared.try_lock().is_err() {
            let since = contended_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(100) {
                backpressure = true;
            }
        } else {
            contended_since = None;
        }
        thread::sleep(Duration::from_millis(2));
        if queue_pressure {
            break;
        }
    }
    let selection_started = Instant::now();
    let selection_deadline = selection_started + Duration::from_millis(1200);
    let mut samples = Vec::new();
    let mut outstanding_complete = baseline["original_response_complete"] == true;
    for (index, offset_ms) in [0u64, 57, 114, 171, 228, 285, 342, 400]
        .into_iter()
        .enumerate()
    {
        let scheduled = selection_started + Duration::from_millis(offset_ms);
        while Instant::now() < scheduled && Instant::now() < selection_deadline {
            thread::sleep(Duration::from_millis(2));
        }
        if Instant::now() >= selection_deadline || !outstanding_complete {
            break;
        }
        let queue_bytes = kernel_queued_bytes(&offender.accepted_probe);
        let connection_alive = !offender.worker.is_finished();
        let mut sample = phase_exchange(
            &mut observer,
            &format!("pressure-{index}"),
            selection_deadline,
        );
        sample["queue_bytes_before"] = serde_json::json!(queue_bytes);
        sample["offender_alive_before"] = serde_json::json!(connection_alive);
        sample["queue_bytes_after"] =
            serde_json::json!(kernel_queued_bytes(&offender.accepted_probe));
        sample["under_pressure"] = serde_json::json!(queue_bytes > 0 && connection_alive);
        outstanding_complete = sample["original_response_complete"] == true;
        println!("slow_consumer_phase_sample {sample}");
        samples.push(sample);
    }
    let selection_elapsed = selection_started.elapsed();
    let offender_result = offender.close();
    let late_response_drained =
        outstanding_complete || observer.snapshot_ordered(Duration::from_secs(2), false);
    let observer_result = observer.close();
    println!(
        "slow_consumer_actual primed={primed} requests={requests} sustained_mutex_contention={backpressure} kernel_queue_pressure={queue_pressure} maximum_queued_bytes={maximum_queued} late_response_drained={late_response_drained} samples={} selection_elapsed_ms={} offender_result={offender_result:?} observer_result={observer_result:?} baseline={baseline}",
        samples.len(),
        selection_elapsed.as_millis()
    );
    assert!(primed, "both actual Viewer streams must be primed");
    assert!(
        queue_pressure,
        "actual kernel queue pressure must be witnessed independently of shared mutex"
    );
    assert!(
        late_response_drained,
        "actual observer late response must finish before shutdown"
    );
    assert!(
        observer_result.is_ok(),
        "healthy observer teardown must succeed"
    );
    assert!(
        baseline["response_within_150ms"] == true,
        "already primed pressure-free response must satisfy the same budget"
    );
    assert!(
        !samples.is_empty(),
        "at least one actual phase sample required"
    );
    assert!(
        samples
            .iter()
            .all(|sample| sample["response_within_150ms"] == true),
        "all actual samples must retain the 150ms useful response budget; no PASS selection"
    );
    println!("PRE2_SLOW_CONSUMER_FAIRNESS_PASSED");
}
fn phase_exchange(
    observer: &mut Session,
    label: &str,
    late_deadline: Instant,
) -> serde_json::Value {
    let started = Instant::now();
    let hello_sent = observer.send(serde_json::json!({"type":"hello_v2","client":format!("PRE2 phase {label}"),"version":2,"capabilities":[]})).is_ok();
    let snapshot_sent = hello_sent
        && observer
            .send(serde_json::json!({"type":"request_snapshot"}))
            .is_ok();
    let response = snapshot_sent
        && observer.snapshot_ordered(
            Duration::from_millis(150).saturating_sub(started.elapsed()),
            true,
        );
    let useful_elapsed = started.elapsed();
    let complete = response
        || (snapshot_sent
            && observer.snapshot_ordered(
                late_deadline.saturating_duration_since(Instant::now()),
                false,
            ));
    serde_json::json!({"label":label,"connection":observer.connection,"hello_sent":hello_sent,"snapshot_sent":snapshot_sent,"response_within_150ms":response && useful_elapsed <= Duration::from_millis(150),"useful_elapsed_us":useful_elapsed.as_micros(),"original_response_complete":complete,"partial_bytes_remaining":observer.partial_line.len()})
}
fn requests(root: &std::path::Path) -> Vec<serde_json::Value> {
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("model-request-")
        })
        .map(|entry| serde_json::from_slice(&fs::read(entry.path()).unwrap()).unwrap())
        .collect()
}
fn repeated(shared: Arc<Mutex<ViewerRuntimeLiveServer>>, root: &std::path::Path) {
    let mut session = Session::start(&shared, false);
    let primed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1101}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 cadence after Play","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(2), true);
    let mut reader = session.reader;
    let mut socket = reader.get_ref().try_clone().unwrap();
    let worker = session.worker;
    let drain_stop = Arc::new(AtomicBool::new(false));
    let reader_stop = drain_stop.clone();
    let draining = thread::spawn(move || {
        let mut line = String::new();
        while !reader_stop.load(Ordering::SeqCst) {
            match reader.read_line(&mut line) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    if line.ends_with('\n') {
                        line.clear();
                    }
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
    let mut completion = application_completion::CompletionWatch::new();
    let mut summary = serde_json::Value::Null;
    let mut first_terminal = false;
    let mut eligible = false;
    loop {
        if let Ok(server) = shared.try_lock() {
            eligible = server.test_agent_service_pump_status()["eligible"] == true;
            summary = server.test_canonical_provider_summary();
            first_terminal |= summary["terminal_states"]["agent-a"]["status"] == "committed";
        }
        let complete = requests(root).len() >= 2
            && requests(root).iter().all(|identity| {
                summary["memory_store"]
                    .to_string()
                    .contains(identity["request_digest"].as_str().unwrap())
            })
            && summary["pending_intent_count"] == 0
            && summary["pending_action_count"] == 0;
        if completion.finished(&summary, complete) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let two_memories = requests(root).len() == 2
        && requests(root).iter().all(|identity| {
            summary["memory_store"]
                .to_string()
                .contains(identity["request_digest"].as_str().unwrap())
        });
    let mut paused_after_two = false;
    if two_memories {
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1102}),
        )
        .unwrap();
        bytes.push(b'\n');
        socket.write_all(&bytes).unwrap();
        let pause_deadline = application_completion::child_deadline();
        while Instant::now() < pause_deadline {
            if let Ok(server) = shared.try_lock()
                && server.test_agent_service_pump_status()["play_enabled"] == false
            {
                paused_after_two = true;
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    socket.shutdown(std::net::Shutdown::Both).unwrap();
    let served = worker.join().unwrap();
    drain_stop.store(true, Ordering::SeqCst);
    let drained = draining.join().unwrap();
    let identities = requests(root);
    assert!(
        !application_completion::has_failure(&summary),
        "actual canonical completion error: {summary}"
    );
    println!(
        "repeated_turn_actual primed={primed} ordered={ordered} eligible={eligible} first_terminal={first_terminal} model_identities={identities:?} worker={served:?} drain={drained:?} memory={}",
        summary["memory_store"]
    );
    assert!(
        primed && ordered && eligible,
        "actual ordinary Play eligibility required"
    );
    assert!(
        served.is_ok() && drained.is_ok(),
        "all actual serving/drain workers must finish"
    );
    assert!(
        first_terminal,
        "first actual canonical terminal receipt required before testing cadence"
    );
    fs::write(
        root.join("repeated-turn-memory.json"),
        serde_json::to_vec(&summary["memory_store"]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        identities.len(),
        2,
        "existing cadence must issue two distinct real model requests"
    );
    assert_ne!(
        identities[0]["decision_request_id"],
        identities[1]["decision_request_id"]
    );
    assert_ne!(
        identities[0]["request_digest"],
        identities[1]["request_digest"]
    );
    if !two_memories || !paused_after_two {
        println!(
            "repeated_turn_failed_diagnostic={}",
            serde_json::json!({
                "hosted_service_phase": summary["hosted_service_phase"],
                "hosted_service_inflight": summary["hosted_service_inflight"],
                "hosted_service_memory_failure": summary["hosted_service_memory_failure"],
                "agent_service_pump_error": summary["agent_service_pump_error"],
                "pending_intent_count": summary["pending_intent_count"],
                "pending_action_count": summary["pending_action_count"],
                "two_memories": two_memories,
                "pause_sent": two_memories,
                "pause_observed": paused_after_two,
            })
        );
    }
    assert!(
        two_memories,
        "both actual receipt memories must complete before issuing genuine Pause"
    );
    assert!(
        paused_after_two,
        "genuine Pause must stop fresh admission after two actual memories"
    );
    let memory = summary["memory_store"].to_string();
    for identity in identities {
        assert!(
            memory.contains(identity["request_digest"].as_str().unwrap()),
            "actual receipt memory for both requests required"
        );
    }
    println!("PRE2_HOSTED_REPEATED_TURNS_PASSED");
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn small_buffers(accepted: &TcpStream, socket: &TcpStream) {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn setsockopt(
            fd: i32,
            level: i32,
            name: i32,
            value: *const std::ffi::c_void,
            len: u32,
        ) -> i32;
        fn getsockopt(
            fd: i32,
            level: i32,
            name: i32,
            value: *mut std::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
    #[cfg(target_os = "macos")]
    let (level, send, receive) = (0xffff, 0x1001, 0x1002);
    #[cfg(target_os = "linux")]
    let (level, send, receive) = (1, 7, 8);
    for (stream, option) in [(accepted, send), (socket, receive)] {
        let value = 1024i32;
        assert_eq!(
            unsafe {
                setsockopt(
                    stream.as_raw_fd(),
                    level,
                    option,
                    (&value as *const i32).cast(),
                    4,
                )
            },
            0
        );
        let mut actual = 0i32;
        let mut length = 4u32;
        assert_eq!(
            unsafe {
                getsockopt(
                    stream.as_raw_fd(),
                    level,
                    option,
                    (&mut actual as *mut i32).cast(),
                    &mut length,
                )
            },
            0
        );
        println!("actual_tcp_buffer option={option} bytes={actual}");
        assert!(actual > 0 && actual <= 8192);
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn small_buffers(_: &TcpStream, _: &TcpStream) {
    panic!("actual small-buffer capability unsupported: not_run");
}

pub(super) fn validate_repeated_receipts(fixture: &Fixture, root: &std::path::Path) {
    let identities = requests(root);
    assert!(
        !identities.is_empty() && identities.len() <= 2,
        "observed real model identities required for canonical correlation"
    );
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .collect::<Vec<_>>();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut request = fixture.view(None);
    request.scope_id = "agent:agent-a".into();
    let view = client.read_view(request).unwrap();
    let memory: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("repeated-turn-memory.json")).unwrap()).unwrap();
    for identity in identities {
        let digest = identity["request_digest"].as_str().unwrap();
        let matching = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Cognition(signed) if signed.request.request.request_digest == digest)).collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "each actual model request needs one real Cognition result"
        );
        let result = matching[0];
        assert!(result.rejected.is_none());
        let IntentOutcome::Committed { receipt, .. } = fixture
            .client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: result.request.correlation.key.clone(),
                },
                result.request.signed_payload.clone(),
            )
            .unwrap()
            .outcome
        else {
            panic!("actual signed Lookup receipt required for each turn")
        };
        assert_eq!(receipt["commit_record"]["request_digest"], digest);
        assert!(
            memory.to_string().contains(digest),
            "memory retains each exact committed request"
        );
        assert!(
            view.projection()
                .cognition_leases
                .iter()
                .any(|lease| lease.request_digest == digest
                    && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled)
        );
        assert!(results.iter().any(|prefix| matches!(&prefix.request.signed_payload,WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,wire::SchedulerOperationV1::ProviderPrefix{request,..} if request.request_digest.to_string()==digest)) && prefix.rejected.is_none() && prefix.committed_height < result.committed_height));
        println!(
            "repeated_turn_actual_signed_receipt request_digest={digest} original_correlation={} committed_height={} exact_lease_settled=true memory_present=true",
            correlation::key_digest(&result.request.correlation.key).unwrap(),
            result.committed_height
        );
    }
}

#[cfg(target_os = "macos")]
fn kernel_queued_bytes(stream: &TcpStream) -> i32 {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn getsockopt(
            fd: i32,
            level: i32,
            name: i32,
            value: *mut std::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
    let mut queued = 0i32;
    let mut length = 4u32;
    assert_eq!(
        unsafe {
            getsockopt(
                stream.as_raw_fd(),
                0xffff,
                0x1024,
                (&mut queued as *mut i32).cast(),
                &mut length,
            )
        },
        0
    );
    assert!(queued >= 0);
    queued
}
#[cfg(target_os = "linux")]
fn kernel_queued_bytes(stream: &TcpStream) -> i32 {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
    }
    let mut queued = 0i32;
    assert_eq!(
        unsafe { ioctl(stream.as_raw_fd(), 0x5411 as std::ffi::c_ulong, &mut queued) },
        0,
        "read actual TCP send queue",
    );
    assert!(queued >= 0);
    queued
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn kernel_queued_bytes(_: &TcpStream) -> i32 {
    panic!("actual kernel queue probe unsupported: not_run");
}
