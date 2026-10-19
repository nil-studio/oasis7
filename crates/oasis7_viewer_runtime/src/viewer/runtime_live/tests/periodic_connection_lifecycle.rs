use super::*;
use crate::viewer::runtime_live::periodic_connection_diagnostic::{ConnectionTrace, TraceSink};
use crate::world_service::{
    authority, client::WorldServiceClientConfig, projection::WorldServiceProjection,
};
use oasis7_client_api::world_service::{
    CHANGES_PATH, CommitRef, EventCursor, ExecutionBinding, ProjectionVersion,
    ReadWorldChangesRequest, ReadWorldChangesResponse, ReadWorldViewRequest, ReadWorldViewResponse,
    SignedReadRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Condvar, Mutex};
use std::thread::JoinHandle;

const PRIVATE_WORLD_SENTINEL: &str = "trace-private-sentinel-world";
const TEST_SOCKET_TIMEOUT: Duration = Duration::from_secs(4);
const TEST_GATE_TIMEOUT: Duration = Duration::from_secs(3);

struct ResponseGate {
    armed: std::sync::atomic::AtomicBool,
    entered: mpsc::Sender<()>,
    released: Mutex<bool>,
    wake: Condvar,
}

impl ResponseGate {
    fn hold_first_view(&self) {
        if !self.armed.swap(false, Ordering::AcqRel) {
            return;
        }
        let _ = self.entered.send(());
        let deadline = Instant::now() + TEST_GATE_TIMEOUT;
        let mut released = self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*released {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, timeout) = self
                .wake
                .wait_timeout(released, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            released = next;
            if timeout.timed_out() {
                break;
            }
        }
    }

    fn release(&self) {
        *self
            .released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.wake.notify_all();
    }
}

struct GateRelease(Option<Arc<ResponseGate>>);

impl Drop for GateRelease {
    fn drop(&mut self) {
        if let Some(gate) = self.0.take() {
            gate.release();
        }
    }
}

struct LiveFixture {
    shared: Arc<Mutex<ViewerRuntimeLiveServer>>,
    sink: Arc<TraceSink>,
    gate_release: GateRelease,
    _http: super::mock_http::RuntimeLiveMockHttpServer,
    entered: Option<Receiver<()>>,
    read_private_key_hex: String,
}

impl LiveFixture {
    fn new(hold_first_view: bool) -> Self {
        let mut config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
        config.chain_poll_interval = Duration::from_secs(30);
        let mut server = ViewerRuntimeLiveServer::new(config).expect("create fixture viewer");
        server.auto_play_paused = true;

        let identity = WorldIdentity {
            world_id: PRIVATE_WORLD_SENTINEL.into(),
            genesis_digest: "trace-fixture-genesis".into(),
        };
        let base_commit = fixture_commit(&identity, 1);
        let mut projection =
            WorldServiceProjection::from_world(&server.world, None).expect("project fixture world");
        projection.runtime_binding = None;

        let base_request = fixture_view_request(identity.clone());
        let base_response =
            fixture_view_response(projection.clone(), base_commit, server.world.state().time);
        server.verified_world_view = Some(verified_fixture_assertion(base_response, &base_request));

        let service_private_key = hex::encode([9; 32]);
        let (entered_tx, entered_rx) = mpsc::channel();
        let gate = hold_first_view.then(|| {
            Arc::new(ResponseGate {
                armed: std::sync::atomic::AtomicBool::new(true),
                entered: entered_tx,
                released: Mutex::new(false),
                wake: Condvar::new(),
            })
        });
        let handler_gate = gate.clone();
        let handler_identity = identity.clone();
        let handler_projection = projection.clone();
        let handler_private_key = service_private_key.clone();
        let http =
            super::mock_http::spawn_runtime_live_mock_http_server_until_drop(move |request| {
                match request.path.as_str() {
                    crate::world_service::VIEW_PATH => {
                        let signed: SignedReadRequest<ReadWorldViewRequest> =
                            serde_json::from_slice(&request.body).expect("decode signed view");
                        authority::verify_read_request(crate::world_service::VIEW_PATH, &signed)
                            .expect("verify signed view");
                        if let Some(gate) = &handler_gate {
                            gate.hold_first_view();
                        }
                        let response = fixture_view_response(
                            handler_projection.clone(),
                            fixture_commit(&handler_identity, 2),
                            handler_projection.state.time,
                        );
                        let signed_response = authority::sign_service_response(
                            crate::world_service::VIEW_PATH,
                            authority::request_digest(crate::world_service::VIEW_PATH, &signed)
                                .expect("digest signed view"),
                            response,
                            &handler_private_key,
                        )
                        .expect("sign service view");
                        super::mock_http::MockHttpResponse {
                            status_code: 200,
                            body: serde_json::to_string(&signed_response)
                                .expect("encode signed service view"),
                        }
                    }
                    CHANGES_PATH => {
                        let signed: SignedReadRequest<ReadWorldChangesRequest> =
                            serde_json::from_slice(&request.body).expect("decode signed changes");
                        authority::verify_read_request(CHANGES_PATH, &signed)
                            .expect("verify signed changes");
                        let response = ReadWorldChangesResponse::<Value> {
                            formal_profile: None,
                            execution_evidence_scope: None,
                            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                            changes: Vec::new(),
                            next_cursor: signed.request.cursor.clone(),
                        };
                        let signed_response = authority::sign_service_response(
                            CHANGES_PATH,
                            authority::request_digest(CHANGES_PATH, &signed)
                                .expect("digest signed changes"),
                            response,
                            &handler_private_key,
                        )
                        .expect("sign service changes");
                        super::mock_http::MockHttpResponse {
                            status_code: 200,
                            body: serde_json::to_string(&signed_response)
                                .expect("encode signed service changes"),
                        }
                    }
                    _ => super::mock_http::MockHttpResponse {
                        status_code: 404,
                        body: r#"{"error":"unknown fixture route"}"#.into(),
                    },
                }
            });

        let (service_public_key, _) = test_signer(9);
        let (_, read_private_key_hex) = test_signer(7);
        server.config.world_service = Some(WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: http.base_url().into(),
            trusted_service_public_key: service_public_key,
            expected_world: identity,
            scope_id: "public".into(),
            read_private_key_hex: read_private_key_hex.clone(),
            timeout: TEST_SOCKET_TIMEOUT,
            max_response_bytes: 1024 * 1024,
        });
        Self {
            shared: Arc::new(Mutex::new(server)),
            sink: TraceSink::for_test(),
            gate_release: GateRelease(gate),
            _http: http,
            entered: hold_first_view.then_some(entered_rx),
            read_private_key_hex,
        }
    }

    fn wait_for_first_view(&self) {
        self.entered
            .as_ref()
            .expect("fixture has a held first view")
            .recv_timeout(TEST_SOCKET_TIMEOUT)
            .expect("periodic worker reached the signed view endpoint");
    }

    fn release_view(&self) {
        if let Some(gate) = &self.gate_release.0 {
            gate.release();
        }
    }

    fn outstanding(&self) -> usize {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .periodic_service_executor
            .as_ref()
            .map_or(0, |executor| executor.outstanding_for_test())
    }
}

impl Drop for LiveFixture {
    fn drop(&mut self) {
        self.release_view();
    }
}

fn fixture_view_request(world: WorldIdentity) -> ReadWorldViewRequest {
    ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world,
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    }
}

fn fixture_commit(world: &WorldIdentity, position: u64) -> CommitRef {
    CommitRef {
        world: world.clone(),
        binding: ExecutionBinding {
            provider_world_id: world.world_id.clone(),
            branch_id: "trace-fixture-branch".into(),
            finality_ref: "trace-fixture-finality".into(),
            reorg_generation: 0,
            governing_manifest_ref: "trace-fixture-manifest".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position,
        execution_block_hash: format!("trace-fixture-block-{position}"),
        state_root_ref: format!("trace-fixture-state-{position}"),
    }
}

fn fixture_view_response(
    projection: WorldServiceProjection,
    commit: CommitRef,
    logical_tick: u64,
) -> ReadWorldViewResponse<WorldServiceProjection> {
    ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: None,
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: "trace-fixture-projection".into(),
            visibility_scope: "public".into(),
        },
        logical_tick,
        continuation: EventCursor {
            stream_id: "trace-fixture-stream".into(),
            scope_id: "public".into(),
            era: 0,
            sequence: 0,
            commit,
        },
        view: projection,
    }
}

struct StreamClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
    finished: Receiver<bool>,
    handler: Option<JoinHandle<()>>,
    release_gate_on_drop: Option<Arc<ResponseGate>>,
}

impl StreamClient {
    fn start(
        shared: Arc<Mutex<ViewerRuntimeLiveServer>>,
        connection: Option<ConnectionTrace>,
        sink_session_is_primed: bool,
        gate: Option<Arc<ResponseGate>>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind viewer test listener");
        let addr = listener.local_addr().expect("viewer listener address");
        let client = TcpStream::connect(addr).expect("connect viewer test peer");
        let (server, _) = listener.accept().expect("accept viewer test peer");
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("set peer read timeout");
        client
            .set_write_timeout(Some(TEST_SOCKET_TIMEOUT))
            .expect("set peer write timeout");
        let reader = BufReader::new(client.try_clone().expect("clone peer reader"));
        let writer = BufWriter::new(client.try_clone().expect("clone peer writer"));
        let (finished_tx, finished) = mpsc::channel();
        let handler = std::thread::spawn(move || {
            let result = ViewerRuntimeLiveServer::test_serve_shared_stream_with_connection(
                shared,
                server,
                connection,
                sink_session_is_primed,
            );
            let _ = finished_tx.send(result.is_ok());
        });
        Self {
            stream: client,
            reader,
            writer,
            finished,
            handler: Some(handler),
            release_gate_on_drop: gate,
        }
    }

    fn begin_snapshot_stream(&mut self) {
        send_runtime_live_request(
            &mut self.writer,
            &ViewerRequest::Subscribe {
                streams: vec![ViewerStream::Snapshot],
                event_kinds: Vec::new(),
            },
        );
        send_runtime_live_request(&mut self.writer, &ViewerRequest::RequestSnapshot);
        assert!(matches!(
            self.read_response(),
            ViewerResponse::Snapshot { .. }
        ));
        assert!(matches!(
            self.read_response(),
            ViewerResponse::AuthoritativeRecoveryAck { .. }
        ));
    }

    fn read_response(&mut self) -> ViewerResponse {
        let deadline = Instant::now() + TEST_SOCKET_TIMEOUT;
        let mut line = String::new();
        loop {
            match self.reader.read_line(&mut line) {
                Ok(0) => panic!("viewer connection closed before response"),
                Ok(_) => {
                    return serde_json::from_str(line.trim_end())
                        .expect("decode viewer response line");
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) && Instant::now() < deadline => {}
                Err(_) => panic!("viewer response read failed before bounded deadline"),
            }
        }
    }

    fn close(&self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    fn finish(&mut self) -> bool {
        let completed = self.finished.recv_timeout(TEST_SOCKET_TIMEOUT).ok();
        if completed.is_some()
            && let Some(handler) = self.handler.take()
        {
            let _ = handler.join();
        }
        completed.unwrap_or(false)
    }
}

impl Drop for StreamClient {
    fn drop(&mut self) {
        self.close();
        if self.handler.is_some() {
            if let Some(gate) = self.release_gate_on_drop.take() {
                gate.release();
            }
            if let Some(handler) = self.handler.take() {
                let _ = handler.join();
            }
        }
    }
}

fn wait_for_event(
    sink: &TraceSink,
    timeout: Duration,
    predicate: impl Fn(&BTreeMap<String, String>) -> bool,
) -> Option<BTreeMap<String, String>> {
    let deadline = Instant::now() + timeout;
    loop {
        for line in sink.observed() {
            if let Some(fields) = trace_fields(&line)
                && predicate(&fields)
            {
                return Some(fields);
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn trace_fields(line: &str) -> Option<BTreeMap<String, String>> {
    let message = line.strip_prefix("OASIS7_VIEWER_CONNECTION_TRACE ")?;
    let mut fields = BTreeMap::new();
    for token in message.split_whitespace() {
        let (key, value) = token.split_once('=')?;
        fields.insert(key.to_string(), value.to_string());
    }
    Some(fields)
}

fn admitted_jobs(sink: &TraceSink) -> Vec<(u64, u64)> {
    sink.observed()
        .iter()
        .filter_map(|line| {
            let fields = trace_fields(line)?;
            if fields.get("event")?.as_str() != "job" || fields.get("stage")?.as_str() != "admitted"
            {
                return None;
            }
            Some((
                fields.get("connection")?.parse().ok()?,
                fields.get("job")?.parse().ok()?,
            ))
        })
        .collect()
}

fn wait_for_outstanding(fixture: &LiveFixture, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if fixture.outstanding() == 0 {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    fixture.outstanding() == 0
}

#[test]
fn traced_stream_delivers_a_signed_verified_advance_over_real_tcp() {
    let _env_guard = lock_test_llm_env();
    let fixture = LiveFixture::new(false);
    let connection = fixture.sink.connection();
    let mut peer = StreamClient::start(fixture.shared.clone(), Some(connection), true, None);
    peer.begin_snapshot_stream();

    let delivered = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields.get("event").is_some_and(|value| value == "job")
            && fields
                .get("stage")
                .is_some_and(|value| value == "consumed_delivery")
            && fields
                .get("status")
                .is_some_and(|value| value == "write_success")
            && fields.get("apply").is_some_and(|value| value == "advanced")
    })
    .expect("real signed read reaches a verified advancing delivery");
    assert_eq!(delivered.get("schema").map(String::as_str), Some("v3"));
    assert!(matches!(
        peer.read_response(),
        ViewerResponse::Snapshot { .. }
    ));
    assert!(wait_for_outstanding(&fixture, TEST_SOCKET_TIMEOUT));
    assert!(
        fixture
            .sink
            .observed()
            .iter()
            .all(|line| !line.contains(PRIVATE_WORLD_SENTINEL)
                && !line.contains(&fixture.read_private_key_hex)),
        "diagnostics must omit world identity and signing material"
    );
}

#[test]
fn eof_captures_pending_job_before_receiver_drop_then_worker_reports_disconnected() {
    let _env_guard = lock_test_llm_env();
    let fixture = LiveFixture::new(true);
    let connection = fixture.sink.connection();
    let mut peer = StreamClient::start(
        fixture.shared.clone(),
        Some(connection),
        true,
        fixture.gate_release.0.clone(),
    );
    peer.begin_snapshot_stream();
    fixture.wait_for_first_view();

    let admitted = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields.get("stage").is_some_and(|stage| stage == "admitted")
    })
    .expect("periodic job is admitted before its HTTP response is held");
    let expected_job = admitted.get("job").expect("admitted job sequence").clone();
    peer.close();
    assert!(peer.finish(), "EOF handler exits within the test bound");

    let closed = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields
            .get("event")
            .is_some_and(|event| event == "connection_close")
    })
    .expect("connection close is emitted after handler-owned state drops");
    assert_eq!(
        closed.get("pending_at_close_present").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        closed.get("pending_at_close_job").map(String::as_str),
        Some(expected_job.as_str())
    );

    fixture.release_view();
    let terminal = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields.get("stage").is_some_and(|stage| stage == "terminal")
            && fields
                .get("status")
                .is_some_and(|status| status == "disconnected")
    })
    .expect("completed HTTP read observes the dropped result mailbox");
    assert_eq!(
        terminal.get("job").map(String::as_str),
        Some(expected_job.as_str())
    );
    assert!(wait_for_outstanding(&fixture, TEST_SOCKET_TIMEOUT));
}

#[test]
fn queued_peer_eof_is_correlated_while_active_peer_still_advances() {
    let _env_guard = lock_test_llm_env();
    let fixture = LiveFixture::new(true);
    let mut active = StreamClient::start(
        fixture.shared.clone(),
        Some(fixture.sink.connection()),
        true,
        fixture.gate_release.0.clone(),
    );
    active.begin_snapshot_stream();
    fixture.wait_for_first_view();

    let mut queued = StreamClient::start(
        fixture.shared.clone(),
        Some(fixture.sink.connection()),
        true,
        fixture.gate_release.0.clone(),
    );
    queued.begin_snapshot_stream();
    let deadline = Instant::now() + TEST_SOCKET_TIMEOUT;
    let mut jobs = admitted_jobs(&fixture.sink);
    while jobs.len() < 2 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
        jobs = admitted_jobs(&fixture.sink);
    }
    assert_eq!(jobs.len(), 2, "one active and one queued job are admitted");
    let (active_connection, _) = jobs[0];
    let (queued_connection, queued_job) = jobs[1];

    queued.close();
    assert!(
        queued.finish(),
        "queued EOF handler exits within the test bound"
    );
    let closed = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields
            .get("event")
            .is_some_and(|event| event == "connection_close")
            && fields
                .get("connection")
                .is_some_and(|value| value.parse::<u64>().ok() == Some(queued_connection))
    })
    .expect("queued connection close is emitted");
    let queued_job_text = queued_job.to_string();
    assert_eq!(
        closed.get("pending_at_close_job").map(String::as_str),
        Some(queued_job_text.as_str())
    );

    fixture.release_view();
    let active_delivery = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields
            .get("stage")
            .is_some_and(|stage| stage == "consumed_delivery")
            && fields
                .get("status")
                .is_some_and(|status| status == "write_success")
            && fields.get("apply").is_some_and(|apply| apply == "advanced")
    })
    .expect("active peer still receives the verified advance");
    let delivered_connection = jobs
        .iter()
        .find(|(_, job)| active_delivery.get("job").and_then(|v| v.parse().ok()) == Some(*job))
        .map(|(connection, _)| *connection)
        .expect("delivery belongs to one admitted connection");
    assert_eq!(delivered_connection, active_connection);
    assert_ne!(delivered_connection, queued_connection);
    assert!(matches!(
        active.read_response(),
        ViewerResponse::Snapshot { .. }
    ));

    let disconnected = wait_for_event(&fixture.sink, TEST_SOCKET_TIMEOUT, |fields| {
        fields.get("stage").is_some_and(|stage| stage == "terminal")
            && fields
                .get("status")
                .is_some_and(|status| status == "disconnected")
            && fields
                .get("job")
                .is_some_and(|value| value.parse::<u64>().ok() == Some(queued_job))
    })
    .expect("queued job reports a disconnected destination after its read ends");
    let queued_connection_text = queued_connection.to_string();
    assert_eq!(
        disconnected.get("connection").map(String::as_str),
        Some(queued_connection_text.as_str())
    );
    assert!(wait_for_outstanding(&fixture, TEST_SOCKET_TIMEOUT));
}

#[test]
fn disabled_trace_keeps_real_snapshot_delivery_and_eof_without_diagnostic_records() {
    let _env_guard = lock_test_llm_env();
    let fixture = LiveFixture::new(false);
    let mut peer = StreamClient::start(fixture.shared.clone(), None, true, None);
    peer.begin_snapshot_stream();
    assert!(matches!(
        peer.read_response(),
        ViewerResponse::Snapshot { .. }
    ));
    peer.close();
    assert!(
        peer.finish(),
        "untraced EOF handler exits within the test bound"
    );
    assert!(fixture.sink.observed().is_empty());
    assert_eq!(fixture.sink.attempted(), 0);
}

fn verified_fixture_assertion(
    response: oasis7_client_api::world_service::ReadWorldViewResponse<
        crate::world_service::projection::WorldServiceProjection,
    >,
    request: &oasis7_client_api::world_service::ReadWorldViewRequest,
) -> crate::world_service::verified_view::VerifiedWorldView {
    use crate::world_service::authority;
    let path = oasis7_client_api::world_service::VIEW_PATH;
    let request =
        authority::sign_read_request(path, request.clone(), &hex::encode([7; 32])).unwrap();
    let digest = authority::request_digest(path, &request).unwrap();
    let response =
        authority::sign_service_response(path, digest, response, &hex::encode([9; 32])).unwrap();
    let key = authority::sign_read_request("fixture", (), &hex::encode([9; 32]))
        .unwrap()
        .subject_public_key;
    crate::world_service::verified_view::VerifiedWorldView::verify_service_assertion(
        response,
        &request,
        &key,
        crate::world_service::verified_view::ReadEvidencePolicy::Ordinary,
    )
    .unwrap()
}
