use super::super::{application_completion, cleanup_deadline::CleanupDeadline, http_fixture, temp_dir};
use super::*;
use std::{
    fs,
    net::{TcpListener, TcpStream},
    sync::{atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

const PROBE_ENV: &str = "PRE2_LISTENER_FAILURE_DIAGNOSTIC_PROBE";
const PRIVATE_PANIC: &str = "private-request-signature-must-not-be-logged";

fn failed_listener(concurrent: bool) -> Arc<Diagnostics> {
    let cleanup = CleanupDeadline::start("fixture_http_workers");
    let root = temp_dir("qa-listener-diagnostic-probe");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("world-concurrent-ready"), b"ready").unwrap();
    let gate = Arc::new(http_fixture::WorldGate::default());
    *gate.root.lock().unwrap() = Some(root.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let halt = Arc::new(AtomicBool::new(false));
    let diagnostics = Arc::new(Diagnostics::default());
    diagnostics.set_scenario("diagnostic-probe");
    let (entered, observed) = mpsc::channel();
    let handler = Arc::new(move |_: TcpStream| {
        at(Stage::ReadRequest);
        route(super::super::SUBMIT_PATH);
        let error = io::Error::from_raw_os_error(libc::ECONNRESET);
        let result: io::Result<()> = observe_io(Err(error));
        // Instrumentation must leave the strict transport outcome unchanged.
        assert_eq!(
            result.as_ref().unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
        entered.send(()).unwrap();
        panic!("{PRIVATE_PANIC}");
    });
    let worker_halt = halt.clone();
    let worker_diagnostics = diagnostics.clone();
    let worker = thread::spawn(move || {
        http_fixture::serve_listener(
            listener,
            worker_halt,
            Arc::new(AtomicBool::new(concurrent)),
            gate,
            handler,
            worker_diagnostics,
        );
    });
    let socket = TcpStream::connect(address).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    halt.store(true, Ordering::SeqCst);
    let joined = worker.join();
    drop(socket);
    fs::remove_dir_all(root).unwrap();
    assert!(
        joined.is_err(),
        "actual listener must fail after its connection panics"
    );
    cleanup.complete();
    diagnostics
}

#[test]
fn listener_failure_diagnostic_process_probe() {
    let Ok(mode) = std::env::var(PROBE_ENV) else {
        return;
    };
    let concurrent = match mode.as_str() {
        "connection" => true,
        "serial" => false,
        _ => panic!("unknown listener diagnostic probe"),
    };
    let diagnostics = failed_listener(concurrent);
    // Same fail-closed endpoint as Fixture::finish_http_workers. Ordinary
    // libtest capture must not leak the original arbitrary panic payload.
    CleanupDeadline::start("fixture_http_workers")
        .fail_with_diagnostics("listener_join_failed", &diagnostics);
}

fn probe_command(mode: &str) -> std::process::Command {
    let name = concat!(module_path!(), "::listener_failure_diagnostic_process_probe");
    let name = name.split_once("::").unwrap().1;
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name])
        .env(PROBE_ENV, mode)
        .env_remove("RUST_TEST_NOCAPTURE");
    command
}

#[test]
fn original_connection_failure_survives_aggregation() {
    for (concurrent, role) in [(true, "Connection"), (false, "SerialConnection")] {
        let diagnostics = failed_listener(concurrent);
        let mut bytes = Vec::new();
        diagnostics.write(&mut bytes).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        assert!(output.contains("scenario=diagnostic-probe"));
        assert!(output.contains(&format!(
            "role={role} stage=ReadRequest route=Submit errno={}",
            libc::ECONNRESET
        )));
        assert!(output.contains("io_kind=ConnectionReset io_source=Typed read_bytes=0"));
        assert!(output.contains("site=probe:"));
        assert!(output.contains("role=Listener"));
        assert!(!output.contains(PRIVATE_PANIC));
        assert!(output.len() < 4096);
    }
}

#[test]
fn listener_failure_exits_without_publishing_captured_panic_payload() {
    let root = temp_dir("qa-listener-diagnostic-results");
    fs::create_dir_all(&root).unwrap();
    for mode in ["connection", "serial"] {
        let output = application_completion::run_child_phase(&mut probe_command(mode), &root, mode);
        assert_eq!(output.status.code(), Some(124));
        let stderr = String::from_utf8(output.stderr).unwrap();
        // The 100 ms fallback may truncate or preempt any diagnostic. The
        // controlled writer test above checks the complete record separately.
        assert!(!stderr.contains(PRIVATE_PANIC));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(PRIVATE_PANIC));
        assert!(stderr.len() < 4096);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn diagnostic_capacity_and_labels_cannot_publish_arbitrary_text() {
    let diagnostics = Arc::new(Diagnostics::default());
    diagnostics.set_scenario(PRIVATE_PANIC);
    for _ in 0..CAPACITY + 3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = Scope::listener(diagnostics.clone());
            at(Stage::ResponseRelay);
            route(PRIVATE_PANIC);
            panic!("{PRIVATE_PANIC}");
        }));
        assert!(result.is_err());
    }
    let mut bytes = Vec::new();
    diagnostics.write(&mut bytes).unwrap();
    let output = String::from_utf8(bytes).unwrap();
    assert_eq!(output.matches("fixture_worker_failure ").count(), CAPACITY);
    assert!(output.contains("scenario=other failures=11 capacity=8"));
    assert!(output.contains("route=Other"));
    assert!(!output.contains(PRIVATE_PANIC));
    assert!(output.len() < 4096);
}

#[test]
fn dispatcher_error_classification_is_exact_and_does_not_keep_message() {
    for (message, expected) in [
        (
            io::Error::from_raw_os_error(libc::EPIPE).to_string(),
            "io_kind=BrokenPipe io_source=KnownMessage",
        ),
        (
            format!("prefix {}", io::Error::from_raw_os_error(libc::ECONNRESET)),
            "io_kind=None io_source=None",
        ),
        (PRIVATE_PANIC.to_owned(), "io_kind=None io_source=None"),
    ] {
        let diagnostics = Arc::new(Diagnostics::default());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = Scope::listener(diagnostics.clone());
            at(Stage::DispatcherResult);
            route(super::super::SUBMIT_PATH);
            request_bytes(37);
            dispatcher_error(&message);
            panic!("{PRIVATE_PANIC}");
        }));
        assert!(result.is_err());
        let mut bytes = Vec::new();
        diagnostics.write(&mut bytes).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        assert!(output.contains(expected));
        assert!(output.contains("read_bytes=37"));
        assert!(!output.contains(PRIVATE_PANIC));
        assert!(!output.contains(&message));
    }
}

#[test]
fn nested_capture_failure_keeps_its_parent_and_original_stage() {
    let diagnostics = Arc::new(Diagnostics::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = Scope::listener(diagnostics.clone());
        route(super::super::VIEW_PATH);
        request_bytes(41);
        let context = child(Worker::CaptureReader);
        let worker = thread::spawn(move || {
            let _scope = Scope::enter(context);
            at(Stage::CaptureRead);
            io_error(&io::Error::from_raw_os_error(libc::ETIMEDOUT));
            panic!("{PRIVATE_PANIC}");
        });
        at(Stage::CaptureJoin);
        assert!(worker.join().is_err());
        panic!("listener aggregation");
    }));
    assert!(result.is_err());
    let mut bytes = Vec::new();
    diagnostics.write(&mut bytes).unwrap();
    let output = String::from_utf8(bytes).unwrap();
    assert!(output.contains("worker=1 parent=0 role=CaptureReader stage=CaptureRead route=View"));
    assert!(output.contains("role=Listener stage=CaptureJoin route=View"));
    assert!(output.contains("io_kind=TimedOut io_source=Typed read_bytes=41"));
    assert!(!output.contains(PRIVATE_PANIC));
}

#[test]
fn truncated_actual_request_keeps_read_bytes_and_remains_a_failure() {
    let cleanup = CleanupDeadline::start("request_diagnostic_probe");
    let diagnostics = Arc::new(Diagnostics::default());
    let _scope = Scope::listener(diagnostics.clone());
    let context = child(Worker::Connection);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    let worker = thread::spawn(move || {
        let _scope = Scope::enter(context);
        http_fixture::configure_accepted_stream(&accepted);
        http_fixture::read_request(&mut accepted)
    });
    let partial = b"POST /v1/world/view HTTP/1.1\r\nContent-Length: 4\r\n\r\nx";
    socket.write_all(partial).unwrap();
    socket.shutdown(std::net::Shutdown::Write).unwrap();
    let joined = worker.join();
    cleanup.complete();
    assert!(
        joined.is_err(),
        "truncated request must not become an empty speculative connection"
    );
    let mut bytes = Vec::new();
    diagnostics.write(&mut bytes).unwrap();
    let output = String::from_utf8(bytes).unwrap();
    assert!(output.contains("stage=ReadRequest"));
    assert!(output.contains(&format!("read_bytes={}", partial.len())));
    assert!(output.contains("site=http:"));
    assert!(!output.contains("POST /v1/world/view"));
}

#[cfg(unix)]
#[test]
fn blocked_failure_diagnostic_stderr_keeps_bounded_exit() {
    use std::{
        io::Read,
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::Stdio,
    };
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    writer.set_nonblocking(true).unwrap();
    let mut filled = 0;
    loop {
        match writer.write(b"x") {
            Ok(1) => filled += 1,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            result => panic!("fill diagnostic stderr failed: {result:?}"),
        }
    }
    assert!(filled > 0);
    writer.set_nonblocking(false).unwrap();
    let mut command = probe_command("connection");
    command.stdout(Stdio::null()).stderr(OwnedFd::from(writer));
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    drop(command);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("listener failure could not exit with blocked stderr");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(124));
    reader
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, vec![b'x'; filled]);
}
