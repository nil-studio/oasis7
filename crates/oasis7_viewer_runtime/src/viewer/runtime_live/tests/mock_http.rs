use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub(super) struct RecordedHttpRequest {
    pub(super) method: String,
    pub(super) path: String,
    pub(super) headers: BTreeMap<String, String>,
    pub(super) body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(super) struct MockHttpResponse {
    pub(super) status_code: u16,
    pub(super) body: String,
}

pub(super) struct RuntimeLiveMockHttpServer {
    base_url: String,
    shutdown: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    diagnostic_tx: std::sync::mpsc::Sender<String>,
    diagnostic_rx: std::sync::mpsc::Receiver<String>,
}

impl RuntimeLiveMockHttpServer {
    pub(super) fn base_url(&self) -> &str {
        self.base_url.as_str()
    }
}

impl Drop for RuntimeLiveMockHttpServer {
    fn drop(&mut self) {
        let dropping_during_unwind = std::thread::panicking();
        let _ = self.diagnostic_tx.send(format!(
            "owner_drop during_unwind={dropping_during_unwind} base_url={}",
            self.base_url
        ));
        let _ = self.shutdown.send(());
        let thread_panic = self
            .thread
            .take()
            .and_then(|thread| thread.join().err())
            .map(mock_http_thread_panic_message);
        if dropping_during_unwind || thread_panic.is_some() {
            let events = self.diagnostic_rx.try_iter().collect::<Vec<_>>();
            eprintln!(
                "runtime live mock HTTP server diagnostic: base_url={} thread_panic={thread_panic:?} events={events:?}",
                self.base_url
            );
        }
    }
}

fn mock_http_thread_panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "non-string panic payload".to_string()
}

impl RuntimeLiveMockHttpServer {
    pub(super) fn drain_diagnostics(&self) -> Vec<String> {
        self.diagnostic_rx.try_iter().collect()
    }
}

pub(super) fn provider_context_response(
    context: &crate::simulator::ContinuousAgentRequestContextV1,
    response: crate::simulator::DecisionResponse,
) -> crate::simulator::ContinuousAgentResponseContextV1 {
    crate::simulator::ContinuousAgentResponseContextV1 {
        response_digest: crate::simulator::cognition_response_digest(&response),
        base_decision_response: response,
        context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_digest.clone(),
    }
}

pub(super) fn spawn_runtime_live_mock_http_server<F>(
    expected_connections: usize,
    handler: F,
) -> String
where
    F: Fn(RecordedHttpRequest) -> MockHttpResponse + Send + Sync + 'static,
{
    spawn_runtime_live_mock_http_server_inner(expected_connections, handler, false)
}

/// Spawn a mock that lets the handler observe the provider capability probes.
/// Most runtime-live fixtures only serve POST requests.  An unrelated
/// compatibility snapshot can still issue a GET against a fixture whose
/// provider URL it observes while another test owns the environment lock.  A
/// default 404 for those GETs keeps that connection isolated instead of
/// feeding an empty body to a decision decoder.  The agent-chat fixtures opt
/// into handling the two provider probe routes explicitly.
pub(super) fn spawn_runtime_live_mock_http_server_with_provider_probes<F>(
    expected_connections: usize,
    handler: F,
) -> String
where
    F: Fn(RecordedHttpRequest) -> MockHttpResponse + Send + Sync + 'static,
{
    spawn_runtime_live_mock_http_server_inner(expected_connections, handler, true)
}

/// Spawn a mock server whose listener remains available until its owner drops.
/// Multi-phase runtime tests use this when their provider interaction count is
/// not a stable contract of the scenario itself.
pub(super) fn spawn_runtime_live_mock_http_server_until_drop<F>(
    handler: F,
) -> RuntimeLiveMockHttpServer
where
    F: Fn(RecordedHttpRequest) -> MockHttpResponse + Send + Sync + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock http server");
    listener
        .set_nonblocking(true)
        .expect("set mock http listener nonblocking");
    let bind = listener.local_addr().expect("listener addr");
    let handler = Arc::new(handler);
    let (shutdown, shutdown_rx) = std::sync::mpsc::channel();
    let (diagnostic_tx, diagnostic_rx) = std::sync::mpsc::channel();
    let thread_diagnostic_tx = diagnostic_tx.clone();
    let thread = std::thread::spawn(move || {
        let record = |event: String| {
            let _ = thread_diagnostic_tx.send(event);
        };
        record(format!("listener_bound={bind}"));
        loop {
            match shutdown_rx.try_recv() {
                Ok(()) => {
                    record("listener_exit=shutdown".to_string());
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    record("listener_exit=owner_disconnected".to_string());
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    record(format!("accept_error={error}"));
                    panic!("accept mock request: {error}");
                }
            };
            stream
                .set_nonblocking(false)
                .expect("set accepted mock stream blocking");
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let request = match read_runtime_live_http_request(&mut stream) {
                Ok(request) => request,
                Err(error_kind) => {
                    record("request_incomplete_or_closed=true".to_string());
                    record(format!("request_read_error={error_kind:?}"));
                    continue;
                }
            };
            record(format!(
                "request method={} path={} body_bytes={}",
                request.method,
                request.path,
                request.body.len()
            ));
            if request.method == "GET" {
                write_runtime_live_json_response(
                    &mut stream,
                    404,
                    r#"{"ok":false,"error":"mock route unavailable"}"#,
                );
                record(format!("response status=404 path={}", request.path));
                continue;
            }
            let request_path = request.path.clone();
            let response = handler(request);
            record(format!(
                "response status={} path={request_path}",
                response.status_code
            ));
            write_runtime_live_json_response(
                &mut stream,
                response.status_code,
                response.body.as_str(),
            );
        }
    });

    RuntimeLiveMockHttpServer {
        base_url: format!("http://{bind}"),
        shutdown,
        thread: Some(thread),
        diagnostic_tx,
        diagnostic_rx,
    }
}

fn spawn_runtime_live_mock_http_server_inner<F>(
    expected_connections: usize,
    handler: F,
    pass_provider_probes_to_handler: bool,
) -> String
where
    F: Fn(RecordedHttpRequest) -> MockHttpResponse + Send + Sync + 'static,
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock http server");
    let bind = listener.local_addr().expect("listener addr");
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        let mut handled_connections = 0;
        while handled_connections < expected_connections {
            let (mut stream, _) = listener.accept().expect("accept mock request");
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let request = match read_runtime_live_http_request(&mut stream) {
                Ok(request) => request,
                Err(_) => continue,
            };
            if request.method == "GET" && !pass_provider_probes_to_handler {
                write_runtime_live_json_response(
                    &mut stream,
                    404,
                    r#"{"ok":false,"error":"mock route unavailable"}"#,
                );
                continue;
            }
            handled_connections += 1;
            let response = handler(request);
            write_runtime_live_json_response(
                &mut stream,
                response.status_code,
                response.body.as_str(),
            );
        }
    });
    format!("http://{}", bind)
}

fn read_runtime_live_http_request(
    stream: &mut std::net::TcpStream,
) -> Result<RecordedHttpRequest, std::io::ErrorKind> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let mut header_end = None;
    let mut content_length = 0_usize;

    loop {
        let bytes = match stream.read(&mut chunk) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.kind()),
        };
        if bytes == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof);
        }
        buffer.extend_from_slice(&chunk[..bytes]);
        if header_end.is_none() {
            header_end = find_runtime_live_header_terminator(buffer.as_slice());
            if let Some(boundary) = header_end {
                let header = std::str::from_utf8(&buffer[..boundary])
                    .map_err(|_| std::io::ErrorKind::InvalidData)?;
                content_length = header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
            }
        }
        if let Some(boundary) = header_end
            && buffer.len() >= boundary + 4 + content_length
        {
            break;
        }
    }

    let boundary = header_end.ok_or(std::io::ErrorKind::InvalidData)?;
    let header =
        std::str::from_utf8(&buffer[..boundary]).map_err(|_| std::io::ErrorKind::InvalidData)?;
    let mut lines = header.lines();
    let request_line = lines.next().ok_or(std::io::ErrorKind::InvalidData)?;
    let mut request_line_parts = request_line.split_whitespace();
    let method = request_line_parts
        .next()
        .ok_or(std::io::ErrorKind::InvalidData)?
        .to_string();
    let path = request_line_parts
        .next()
        .ok_or(std::io::ErrorKind::InvalidData)?
        .to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let body = buffer[(boundary + 4)..(boundary + 4 + content_length)].to_vec();

    Ok(RecordedHttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_runtime_live_header_terminator(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_runtime_live_json_response(
    stream: &mut std::net::TcpStream,
    status_code: u16,
    body: &str,
) {
    let status_text = match status_code {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

#[test]
fn owned_runtime_live_mock_http_server_serves_beyond_eight_requests_until_drop() {
    let handled_requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server = spawn_runtime_live_mock_http_server_until_drop({
        let handled_requests = std::sync::Arc::clone(&handled_requests);
        move |_| {
            handled_requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            MockHttpResponse {
                status_code: 200,
                body: "{}".to_string(),
            }
        }
    });
    let address = server
        .base_url()
        .strip_prefix("http://")
        .expect("loopback URL")
        .to_string();

    for request_number in 0..9 {
        let mut stream = std::net::TcpStream::connect(address.as_str())
            .unwrap_or_else(|error| panic!("connect request {request_number}: {error}"));
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set mock client read timeout");
        stream
            .write_all(
                b"POST /fixture HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .expect("write mock request");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .expect("read mock response");
        assert!(
            response.starts_with(b"HTTP/1.1 200 OK"),
            "request {request_number} received {}",
            String::from_utf8_lossy(response.as_slice())
        );
    }

    assert_eq!(
        handled_requests.load(std::sync::atomic::Ordering::SeqCst),
        9
    );
    let diagnostics = server.drain_diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .filter(|event| event.starts_with("request method=POST path=/fixture"))
            .count(),
        9,
        "the owned server records each accepted multi-phase request"
    );
    assert!(
        diagnostics
            .iter()
            .all(|event| !event.starts_with("request_read_error=")),
        "the owned server must not drop complete fixture requests"
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|event| event.starts_with("response status=200 path=/fixture"))
            .count(),
        9,
        "the owned server records each response before the client observes it"
    );
}
