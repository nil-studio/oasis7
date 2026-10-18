//! Held real service I/O must not block an already primed Viewer or replace pending identity.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::io::{BufRead, BufReader, Write};
struct Session {
    reader: BufReader<TcpStream>,
    accepted_probe: TcpStream,
    normal_send_buffer: Option<i32>,
    worker: Option<thread::JoinHandle<Result<(), oasis7::viewer::ViewerRuntimeLiveServerError>>>,
}
impl Session {
    fn start(shared: &Arc<Mutex<ViewerRuntimeLiveServer>>, small_buffers: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (accepted, _) = listener.accept().unwrap();
        let normal_send_buffer = small_buffers.then(|| send_buffer(&accepted));
        if small_buffers {
            set_small_send_buffer(&accepted);
        }
        let accepted_probe = accepted.try_clone().unwrap();
        let server = shared.clone();
        let worker = thread::spawn(move || {
            ViewerRuntimeLiveServer::test_serve_shared_stream(server, accepted)
        });
        Self {
            reader: BufReader::new(socket),
            accepted_probe,
            normal_send_buffer,
            worker: Some(worker),
        }
    }
    fn send(&mut self, value: serde_json::Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        if let Err(error) = self.reader.get_mut().write_all(&bytes) {
            println!("hosted_service_viewer_send_error kind={:?}", error.kind());
        }
    }
    fn warm(&mut self) -> bool {
        self.send(serde_json::json!({"type":"hello_v2","client":"PRE2 primed Viewer","version":2,"capabilities":[]}));
        self.send(serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}));
        self.send(serde_json::json!({"type":"request_snapshot"}));
        self.receive_snapshot(Duration::from_secs(3), false)
    }
    fn receive_snapshot(&mut self, budget: Duration, require_ack: bool) -> bool {
        let deadline = Instant::now() + budget;
        let mut ack = !require_ack;
        while Instant::now() < deadline {
            self.reader
                .get_mut()
                .set_read_timeout(Some(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(1)),
                ))
                .unwrap();
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => return false,
                Ok(_) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value["type"] == "hello_ack" {
                            ack = true;
                        }
                        if ack && value["type"] == "snapshot" {
                            return true;
                        }
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return false,
            }
        }
        false
    }
    fn send_snapshot_burst(&mut self, count: usize) -> usize {
        let mut bytes =
            serde_json::to_vec(&serde_json::json!({"type":"request_snapshot"})).unwrap();
        bytes.push(b'\n');
        let mut sent = 0;
        for _ in 0..count {
            if let Err(error) = self.reader.get_mut().write_all(&bytes) {
                println!(
                    "hosted_periodic_pressure_send_error kind={:?} sent={sent}",
                    error.kind()
                );
                break;
            }
            sent += 1;
        }
        sent
    }
    fn wait_for_kernel_pressure(&self, budget: Duration) -> (bool, i32) {
        let deadline = Instant::now() + budget;
        let mut pressure_since = None;
        let mut maximum_queued = 0;
        while Instant::now() < deadline {
            let queued = kernel_queued_bytes(&self.accepted_probe);
            maximum_queued = maximum_queued.max(queued);
            let connection_alive = self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished());
            if queued > 0 && connection_alive {
                let since = pressure_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(100) {
                    return (true, maximum_queued);
                }
            } else {
                pressure_since = None;
            }
            thread::sleep(Duration::from_millis(2));
        }
        (false, maximum_queued)
    }
    fn drain_through_hello_ack(
        &mut self,
        expected_snapshots: usize,
        budget: Duration,
    ) -> (bool, usize, bool) {
        // Pressure and primed-observer proofs are already complete. Restore
        // the original send buffer after pressure release. The client receive
        // window stays at its normal size throughout this real nonreader test.
        if let Some(size) = self.normal_send_buffer.take() {
            restore_send_buffer(&self.accepted_probe, size);
        }
        self.send(serde_json::json!({"type":"hello_v2","client":"PRE2 pressure drain sentinel","version":2,"capabilities":[]}));
        let deadline = Instant::now() + budget;
        let mut snapshots = 0;
        let mut hello_ack_seen = false;
        let mut line = String::new();
        while Instant::now() < deadline {
            self.reader
                .get_mut()
                .set_read_timeout(Some(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(100))
                        .max(Duration::from_millis(1)),
                ))
                .unwrap();
            match self.reader.read_line(&mut line) {
                Ok(0) => return (false, snapshots, hello_ack_seen),
                Ok(_) => {
                    if !line.ends_with('\n') {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                        if value["type"] == "snapshot" {
                            snapshots += 1;
                        }
                        if value["type"] == "hello_ack" {
                            hello_ack_seen = true;
                        }
                        if hello_ack_seen && snapshots >= expected_snapshots {
                            return (true, snapshots, hello_ack_seen);
                        }
                    }
                    line.clear();
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(_) => return (false, snapshots, hello_ack_seen),
            }
        }
        (false, snapshots, hello_ack_seen)
    }
    fn shutdown(&mut self) {
        let _ = self.reader.get_mut().shutdown(std::net::Shutdown::Both);
    }
    fn finish(&mut self) -> Result<(), String> {
        self.shutdown();
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        match worker.join() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("Viewer worker returned {error:?}")),
            Err(_) => Err("Viewer worker panicked".into()),
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(worker) = self.worker.take() {
            println!("hosted_service_viewer_worker_result={:?}", worker.join());
        }
    }
}

struct PeriodicGateRelease {
    path: std::path::PathBuf,
    released: bool,
}
impl PeriodicGateRelease {
    fn new(root: &std::path::Path) -> Self {
        Self {
            path: root.join("world-periodic-view-release"),
            released: false,
        }
    }
    fn release(&mut self) {
        fs::write(&self.path, b"release").unwrap();
        self.released = true;
    }
}
impl Drop for PeriodicGateRelease {
    fn drop(&mut self) {
        if !self.released {
            let _ = fs::write(&self.path, b"release during cleanup");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PublicCursor {
    commit: CommitRef,
    continuation: EventCursor,
}

fn read_public_cursor(client: &RemoteWorldServiceClient) -> PublicCursor {
    read_public_cursor_at(client, None)
}

fn read_public_cursor_at(
    client: &RemoteWorldServiceClient,
    fixed_commit: Option<CommitRef>,
) -> PublicCursor {
    let config = client.config();
    assert_eq!(
        config.scope_id, "public",
        "cursor read must use public scope"
    );
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: config.expected_world.clone(),
            scope_id: config.scope_id.clone(),
            min_commit: None,
            fixed_commit,
            deadline_unix_ms: None,
        })
        .unwrap();
    assert_eq!(view.version().visibility_scope, "public");
    assert_eq!(view.continuation().scope_id, "public");
    assert_eq!(
        view.continuation().commit,
        view.version().commit,
        "public event cursor must bind to the exact projection commit"
    );
    PublicCursor {
        commit: view.version().commit.clone(),
        continuation: view.continuation().clone(),
    }
}

pub(crate) fn verify_periodic_view_gate(client: &RemoteWorldServiceClient) {
    let server =
        application_hosted::prepare_server_for_policy(client, false, Duration::from_millis(250));
    // Snapshot priming may reuse the exact certified version already known by
    // this server. Independently authenticate that full setup commit, rather
    // than assuming a new observer must synchronously fetch the remote latest.
    let known_commit: CommitRef = serde_json::from_value(
        server.test_agent_service_pump_status()["known_service_commit"].clone(),
    )
    .unwrap();
    let baseline = read_public_cursor_at(client, Some(known_commit.clone()));
    assert_eq!(baseline.commit, known_commit);
    let shared = Arc::new(Mutex::new(server));
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    fs::write(root.join("world-concurrent-ready"), b"ready").unwrap();
    let mut observer = Session::start(&shared, false);
    let observer_primed = observer.warm();
    fs::write(root.join("world-periodic-view-arm"), b"arm after prime").unwrap();
    let gate_deadline = Instant::now() + Duration::from_secs(3);
    while !root.join("world-periodic-view-started").exists() && Instant::now() < gate_deadline {
        thread::sleep(Duration::from_millis(2));
    }
    let gate_started = root.join("world-periodic-view-started").exists();
    let gate_record = fs::read(root.join("world-periodic-view-started"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or_default();
    let mut gate_release = PeriodicGateRelease::new(&root);
    // Session priming may legitimately advance beyond the earlier public View.
    // Authenticate the exact full commit carried by the real signed periodic
    // request independently; never replace this with a position-only check.
    let requested_commit: CommitRef =
        serde_json::from_value(gate_record["min_commit"].clone()).unwrap();
    assert!(
        requested_commit
            .satisfies_minimum(&baseline.commit)
            .unwrap(),
        "periodic minimum cannot precede the independently verified setup View"
    );
    let pinned = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: Some(requested_commit.clone()),
            deadline_unix_ms: None,
        })
        .unwrap();
    assert_eq!(pinned.version().visibility_scope, "public");
    assert_eq!(pinned.continuation().scope_id, "public");
    assert_eq!(pinned.version().commit, requested_commit);
    assert_eq!(pinned.continuation().commit, requested_commit);

    let gate_is_held = gate_started && !root.join("world-periodic-view-release").exists();

    let mut slow = Session::start(&shared, true);
    let slow_primed = slow.warm();
    let requests = slow.send_snapshot_burst(128);
    let (kernel_pressure, maximum_queued) = slow.wait_for_kernel_pressure(Duration::from_secs(1));

    observer.send(serde_json::json!({"type":"hello_v2","client":"PRE2 periodic in-flight observer","version":2,"capabilities":[]}));
    observer.send(serde_json::json!({"type":"request_snapshot"}));
    let began = Instant::now();
    let observer_responsive = observer.receive_snapshot(Duration::from_millis(150), true);
    let observer_elapsed = began.elapsed();
    let still_held_during_observer = !root.join("world-periodic-view-release").exists();

    gate_release.release();
    let completion_deadline = Instant::now() + Duration::from_secs(2);
    while !root.join("world-periodic-view-completed").exists()
        && Instant::now() < completion_deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    let handler_completed = root.join("world-periodic-view-completed").exists();
    let gate_returned = root.join("world-periodic-view-gate-returned").exists();

    let (slow_drained, snapshots_drained, drain_hello_ack_seen) =
        slow.drain_through_hello_ack(requests, Duration::from_secs(5));
    let observer_join = observer.finish();
    let slow_join = slow.finish();
    let periodic_arm_removed = fs::remove_file(root.join("world-periodic-view-arm")).is_ok();

    println!(
        "hosted_periodic_view_fairness primed={observer_primed} gate_started={gate_started} signature_verified={} min_commit_position={:?} baseline_position={} gate_held={gate_is_held} slow_primed={slow_primed} slow_requests={requests} kernel_pressure={kernel_pressure} maximum_queued_bytes={maximum_queued} observer_snapshot_before_release={observer_responsive} observer_elapsed_ms={} gate_still_held={still_held_during_observer} gate_returned={gate_returned} handler_completed={handler_completed} drained_snapshots={snapshots_drained} drain_hello_ack_seen={drain_hello_ack_seen} drain_complete={slow_drained} observer_join={observer_join:?} slow_join={slow_join:?}",
        gate_record["signature_verified"],
        gate_record["min_commit_position"],
        baseline.commit.position,
        observer_elapsed.as_millis()
    );
    assert!(observer_primed, "the actual observer must be primed first");
    assert!(
        gate_started,
        "an actual periodic signed View must reach the gate"
    );
    assert_eq!(gate_record["signature_verified"], true);
    assert_eq!(
        gate_record["min_commit"],
        serde_json::to_value(&requested_commit).unwrap(),
        "periodic read must carry the complete fresh commit reference and binding"
    );
    assert_eq!(
        gate_record["min_commit_position"].as_u64(),
        Some(requested_commit.position)
    );
    assert_eq!(
        baseline.commit, baseline.continuation.commit,
        "fresh verified public view version and continuation must share the complete commit"
    );
    assert!(
        gate_is_held,
        "periodic View gate must remain held during the sample"
    );
    assert!(slow_primed, "the actual nonreading peer must be primed");
    assert_eq!(
        requests, 128,
        "the nonreading peer must issue all pressure requests"
    );
    assert!(
        kernel_pressure,
        "actual kernel send-queue pressure must be held for 100ms"
    );
    assert!(
        observer_responsive && observer_elapsed <= Duration::from_millis(150),
        "same primed observer must receive Hello/Snapshot within 150ms under pressure"
    );
    assert!(
        still_held_during_observer,
        "periodic read must remain blocked through the observer response"
    );
    assert!(
        gate_returned && handler_completed,
        "released periodic View must drain through its real HTTP handler"
    );
    assert!(
        drain_hello_ack_seen && slow_drained && snapshots_drained >= requests,
        "sentinel and all real nonreader responses must drain within the bounded window"
    );
    assert!(
        observer_join.is_ok(),
        "observer serving worker must join successfully"
    );
    assert!(
        slow_join.is_ok(),
        "nonreader serving worker must join successfully"
    );
    assert!(
        periodic_arm_removed,
        "periodic gate arm must be removed before legacy phases"
    );
    println!("PRE2_HOSTED_PERIODIC_VIEW_FAIRNESS_PASSED");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn set_small_send_buffer(accepted: &TcpStream) {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn setsockopt(
            fd: i32,
            level: i32,
            name: i32,
            value: *const std::ffi::c_void,
            len: u32,
        ) -> i32;
    }
    #[cfg(target_os = "macos")]
    let (level, send) = (0xffff, 0x1001);
    #[cfg(target_os = "linux")]
    let (level, send) = (1, 7);
    let value = 1024i32;
    assert_eq!(
        unsafe {
            setsockopt(
                accepted.as_raw_fd(),
                level,
                send,
                (&value as *const i32).cast(),
                4,
            )
        },
        0,
        "set actual small TCP send buffer"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn send_buffer(socket: &TcpStream) -> i32 {
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
    #[cfg(target_os = "macos")]
    let (level, option) = (0xffff, 0x1001);
    #[cfg(target_os = "linux")]
    let (level, option) = (1, 7);
    let mut size = 0i32;
    let mut len = 4u32;
    assert_eq!(
        unsafe {
            getsockopt(
                socket.as_raw_fd(),
                level,
                option,
                (&mut size as *mut i32).cast(),
                &mut len,
            )
        },
        0
    );
    assert_eq!(len, 4);
    assert!(size > 0);
    size
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn restore_send_buffer(socket: &TcpStream, size: i32) {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn setsockopt(
            fd: i32,
            level: i32,
            name: i32,
            value: *const std::ffi::c_void,
            len: u32,
        ) -> i32;
    }
    #[cfg(target_os = "macos")]
    let (level, option, size) = (0xffff, 0x1001, size);
    // Linux getsockopt reports twice the setsockopt requested buffer.
    #[cfg(target_os = "linux")]
    let (level, option, size) = (1, 7, size / 2);
    assert_eq!(
        unsafe {
            setsockopt(
                socket.as_raw_fd(),
                level,
                option,
                (&size as *const i32).cast(),
                4,
            )
        },
        0
    );
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn send_buffer(_: &TcpStream) -> i32 {
    0
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn restore_send_buffer(_: &TcpStream, _: i32) {}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn set_small_send_buffer(_: &TcpStream) {
    panic!("actual kernel queue pressure probe unsupported on this target");
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
        0,
        "read actual TCP send queue"
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
        "read actual TCP send queue"
    );
    assert!(queued >= 0);
    queued
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn kernel_queued_bytes(_: &TcpStream) -> i32 {
    panic!("actual kernel queue probe unsupported on this target");
}

#[test]
fn real_tcp_hosted_service_io_preserves_primed_viewer_and_reconnect_identity() {
    run_isolated_application(false, false, false, true, false, true);
}

#[test]
fn real_tcp_hosted_periodic_view_gate_preserves_primed_observer_under_pressure() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "periodic-fairness",
    );
}

pub(super) fn verify_service_fairness(client: &RemoteWorldServiceClient) {
    let server =
        application_hosted::prepare_server_for_policy(client, false, Duration::from_secs(60));
    // Native-provider setup above performs real Reserve and Prefix commits.
    // Capture this case's verified baseline only after its own setup effects.
    let service_baseline = read_public_cursor(client);
    let shared = Arc::new(Mutex::new(server));
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    fs::write(root.join("world-submit-arm"), b"arm").unwrap();
    fs::write(root.join("world-concurrent-ready"), b"ready").unwrap();
    let mut driving = Session::start(&shared, false);
    let mut observer = Session::start(&shared, false);
    let warmed = driving.warm() && observer.warm();
    println!("hosted_service_viewers_warmed={warmed}");
    if !warmed {
        for kind in ["submit", "lookup", "view"] {
            fs::write(root.join(format!("world-{kind}-release")), b"release").unwrap();
        }
        drop(driving);
        drop(observer);
        panic!("Viewer warmup must establish genuine verified snapshots before gating");
    }
    driving
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":801}));
    let mut evidence = Vec::new();
    let mut retired = Vec::new();
    for (index, kind) in ["submit", "lookup", "view"].into_iter().enumerate() {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join(format!("world-{kind}-started")).exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        let started = root.join(format!("world-{kind}-started")).exists();
        observer.send(serde_json::json!({"type":"hello_v2","client":format!("PRE2 {kind} query"),"version":2,"capabilities":[]}));
        observer.send(serde_json::json!({"type":"request_snapshot"}));
        let began = Instant::now();
        let responsive = observer.receive_snapshot(Duration::from_millis(150), true);
        let elapsed = began.elapsed();
        let held = !root.join(format!("world-{kind}-release")).exists();
        if let Some(next) = ["submit", "lookup", "view"].get(index + 1) {
            fs::write(root.join(format!("world-{next}-arm")), b"arm").unwrap();
        }
        fs::write(root.join(format!("world-{kind}-release")), b"release").unwrap();
        println!(
            "hosted_service_io_concurrency phase={kind} parsed_request={started} snapshot_before_release={responsive} gate_held={held} elapsed_ms={} absolute_budget_ms=150",
            elapsed.as_millis()
        );
        evidence.push((kind, started, responsive && held));
        if kind == "lookup" {
            driving.shutdown();
            retired.push(driving);
            driving = Session::start(&shared, false);
            driving.send(serde_json::json!({"type":"hello_v2","client":"PRE2 reconnect original operation","version":2,"capabilities":[]}));
            driving.send(
                serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":802}),
            );
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let summary = loop {
        let summary = shared.lock().unwrap().test_canonical_provider_summary();
        if summary["terminal_states"]["agent-a"]["status"] == "committed"
            || Instant::now() >= deadline
        {
            break summary;
        }
        thread::sleep(Duration::from_millis(20));
    };
    // Release any unobserved phase before finite stream cleanup and assertions.
    for kind in ["submit", "lookup", "view"] {
        fs::write(root.join(format!("world-{kind}-release")), b"release").unwrap();
    }
    let driving_join = driving.finish();
    let observer_join = observer.finish();
    let retired_join = retired.iter_mut().map(Session::finish).collect::<Vec<_>>();
    assert!(
        warmed,
        "Viewer warmup must establish genuine verified snapshots"
    );
    assert!(
        evidence
            .iter()
            .all(|(_, started, responsive)| *started && *responsive),
        "held service I/O blocked primed Viewer: {evidence:?}"
    );
    assert_eq!(summary["terminal_states"]["agent-a"]["status"], "committed");
    assert!(
        driving_join.is_ok(),
        "driving Viewer worker must join successfully"
    );
    assert!(
        observer_join.is_ok(),
        "observer Viewer worker must join successfully"
    );
    assert!(
        retired_join.iter().all(Result::is_ok),
        "retired Viewer workers must join successfully: {retired_join:?}"
    );
    let final_cursor = read_public_cursor(client);
    assert!(
        final_cursor
            .commit
            .satisfies_minimum(&service_baseline.commit)
            .unwrap(),
        "final public view must remain in the baseline world and execution binding"
    );
    assert!(
        final_cursor.commit.position > service_baseline.commit.position,
        "genuine canonical commit must advance after the service fairness phases: baseline={} final={}",
        service_baseline.commit.position,
        final_cursor.commit.position
    );
    assert_eq!(
        final_cursor.continuation.sequence, service_baseline.continuation.sequence,
        "public event cursor must remain stable when only agent-scoped Domain events are added"
    );
    assert_eq!(
        final_cursor.continuation.sequence, 0,
        "this fixture has no public-scope events"
    );
    println!(
        "PRE2_HOSTED_SERVICE_IO_RECONNECT_FAIRNESS_PASSED direct_poll_calls=0 service_baseline={service_baseline:?} final_cursor={final_cursor:?}"
    );
}

pub(super) fn validate_canonical_identity(fixture: &Fixture) {
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .filter(|result| {
            matches!(
                result.request.signed_payload,
                WorldServicePayloadV1::Cognition(_)
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        results.len(),
        1,
        "reconnect must retain one original Cognition operation"
    );
    let result = &results[0];
    assert!(result.rejected.is_none());
    let key = correlation::key_digest(&result.request.correlation.key).unwrap();
    let trace = fixture.lookup_digests.lock().unwrap();
    let submit_count = trace
        .iter()
        .filter(|entry| **entry == format!("submit:{key}"))
        .count();
    let lookup_count = trace.iter().filter(|entry| *entry == &key).count();
    assert!(
        submit_count > 0,
        "original Cognition requires actual Submit"
    );
    let cognition_submits = trace
        .iter()
        .filter_map(|entry| entry.strip_prefix("cognition_submit:"))
        .map(|bytes| {
            serde_json::from_str::<SubmitIntentRequest<WorldServicePayloadV1>>(bytes)
                .expect("complete typed HTTP Cognition witness")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cognition_submits.len(),
        submit_count,
        "every Cognition HTTP Submit must use the sole original key"
    );
    for observed in cognition_submits {
        observed.validate().expect("captured Submit contract");
        assert_eq!(observed.correlation, result.request.correlation);
        assert_eq!(observed.signed_payload, result.request.signed_payload);
        assert_eq!(
            serde_json::to_vec(&observed.signed_payload).unwrap(),
            serde_json::to_vec(&result.request.signed_payload).unwrap(),
            "every replay must retain complete original signed bytes"
        );
    }
    assert!(
        lookup_count > 0,
        "original canonical operation needs actualLookup"
    );
    drop(trace);
    let WorldServicePayloadV1::Cognition(signed) = &result.request.signed_payload else {
        unreachable!()
    };
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut request = fixture.view(None);
    request.scope_id = "agent:agent-a".into();
    let view = client.read_view(request).unwrap();
    assert_eq!(
        view.projection().state.agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(2, 2, 0)
    );
    assert!(
        view.projection()
            .cognition_leases
            .iter()
            .any(
                |lease| lease.request_digest == signed.request.request.request_digest
                    && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
            )
    );
    println!(
        "hosted_reconnect_original_identity correlation_digest={key} submit_count={submit_count} lookup_count={lookup_count} canonical_result_count=1 exact_lease_settled=true"
    );
}
