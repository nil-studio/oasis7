use super::*;
#[cfg(test)]
use oasis7::simulator::WorldSnapshot;
use oasis7::viewer::{VIEWER_PROTOCOL_VERSION, ViewerRequest, ViewerResponse};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

// Runtime snapshots in the hosted W3 world are approximately 400 KiB and
// are produced behind the live runtime lock. The observed commit path takes
// up to about 1 second, so 300 ms expires probes while the runtime is healthy.
const HOSTED_SESSION_RUNTIME_PROBE_TIMEOUT_MS: u64 = 2_000;
const HOSTED_SESSION_RUNTIME_PROBE_INTERVAL_MS: u64 = 1_000;
const RUNTIME_PRESENCE_PROBE_CLIENT: &str = "oasis7_game_launcher_hosted_session_probe";
static NEXT_RUNTIME_PRESENCE_PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn run_runtime_presence_monitor(
    stop_requested: Arc<AtomicBool>,
    live_bind: Arc<String>,
    hosted_session_issuer: Arc<Mutex<HostedPlayerSessionIssuer>>,
) {
    run_runtime_presence_monitor_with_interval(
        stop_requested,
        live_bind,
        hosted_session_issuer,
        Duration::from_millis(HOSTED_SESSION_RUNTIME_PROBE_INTERVAL_MS),
    );
}

#[cfg(test)]
pub(super) fn query_runtime_bound_players(live_bind: &str) -> Result<BTreeSet<String>, String> {
    query_runtime_bound_players_with_probe_sequence(live_bind).map(|(_, players)| players)
}

pub(super) fn query_runtime_bound_players_with_probe_sequence(
    live_bind: &str,
) -> Result<(u64, BTreeSet<String>), String> {
    let probe_sequence = NEXT_RUNTIME_PRESENCE_PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut client = ViewerRuntimeProbeClient::connect(
        live_bind,
        Duration::from_millis(HOSTED_SESSION_RUNTIME_PROBE_TIMEOUT_MS),
        RUNTIME_PRESENCE_PROBE_CLIENT,
    )?;
    client.request_snapshot()?;
    client
        .wait_for_snapshot()
        .map(|players| (probe_sequence, players))
}

fn run_runtime_presence_monitor_with_interval(
    stop_requested: Arc<AtomicBool>,
    live_bind: Arc<String>,
    hosted_session_issuer: Arc<Mutex<HostedPlayerSessionIssuer>>,
    probe_interval: Duration,
) {
    while !stop_requested.load(Ordering::SeqCst) {
        let probe_started_at = Instant::now();
        match query_runtime_bound_players_with_probe_sequence(live_bind.as_str()) {
            Ok((probe_sequence, active_players)) => {
                observe_runtime_presence_snapshot(
                    &hosted_session_issuer,
                    probe_sequence,
                    &active_players,
                );
            }
            Err(err) => {
                if let Ok(mut issuer) = hosted_session_issuer.lock() {
                    issuer.record_runtime_probe_failure(err);
                }
            }
        }
        sleep_until_next_probe(stop_requested.as_ref(), probe_started_at, probe_interval);
    }
}

fn observe_runtime_presence_snapshot(
    hosted_session_issuer: &Arc<Mutex<HostedPlayerSessionIssuer>>,
    probe_sequence: u64,
    active_players: &BTreeSet<String>,
) {
    if let Ok(mut issuer) = hosted_session_issuer.lock() {
        issuer.observe_runtime_active_players_for_probe(
            probe_sequence,
            active_players.iter().map(String::as_str),
        );
    }
}

struct ViewerRuntimeProbeClient {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl ViewerRuntimeProbeClient {
    fn connect(live_bind: &str, timeout: Duration, client_name: &str) -> Result<Self, String> {
        let (host, port) = parse_host_port(live_bind, "--live-bind")?;
        let stream = TcpStream::connect((host.as_str(), port))
            .map_err(|err| format!("connect runtime live {live_bind} failed: {err}"))?;
        stream
            .set_nodelay(true)
            .map_err(|err| format!("set_nodelay on runtime presence client failed: {err}"))?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|err| format!("set_read_timeout on runtime presence client failed: {err}"))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|err| format!("set_write_timeout on runtime presence client failed: {err}"))?;
        let reader_stream = stream
            .try_clone()
            .map_err(|err| format!("clone runtime presence stream failed: {err}"))?;
        let mut client = Self {
            reader: BufReader::new(reader_stream),
            writer: BufWriter::new(stream),
        };
        client.write_request_line(&ViewerRequest::Hello {
            client: client_name.to_string(),
            version: VIEWER_PROTOCOL_VERSION,
        })?;
        client.wait_for_hello_ack()?;
        Ok(client)
    }

    fn request_snapshot(&mut self) -> Result<(), String> {
        self.write_request_line(&ViewerRequest::RequestSnapshot)
    }

    fn wait_for_snapshot(&mut self) -> Result<BTreeSet<String>, String> {
        loop {
            match self.read_response_line()? {
                ViewerResponseLine::Snapshot(active_players) => return Ok(active_players),
                ViewerResponseLine::Response(_) => {}
                ViewerResponseLine::Timeout => {
                    return Err("runtime probe timed out waiting for snapshot".to_string());
                }
                ViewerResponseLine::Closed => {
                    return Err("runtime probe closed before snapshot".to_string());
                }
            }
        }
    }

    fn wait_for_hello_ack(&mut self) -> Result<(), String> {
        loop {
            match self.read_response_line()? {
                ViewerResponseLine::Response(ViewerResponse::HelloAck { .. }) => return Ok(()),
                ViewerResponseLine::Response(_) => {}
                ViewerResponseLine::Snapshot(_) => {}
                ViewerResponseLine::Timeout => {
                    return Err("runtime probe timed out waiting for hello_ack".to_string());
                }
                ViewerResponseLine::Closed => {
                    return Err("runtime probe closed before hello_ack".to_string());
                }
            }
        }
    }

    fn write_request_line(&mut self, request: &ViewerRequest) -> Result<(), String> {
        let payload = serde_json::to_string(request)
            .map_err(|err| format!("serialize runtime presence request failed: {err}"))?;
        self.writer
            .write_all(payload.as_bytes())
            .map_err(|err| format!("write runtime presence request failed: {err}"))?;
        self.writer
            .write_all(b"\n")
            .map_err(|err| format!("write runtime presence delimiter failed: {err}"))?;
        self.writer
            .flush()
            .map_err(|err| format!("flush runtime presence request failed: {err}"))
    }

    fn read_response_line(&mut self) -> Result<ViewerResponseLine, String> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Ok(ViewerResponseLine::Closed),
            Ok(_) => decode_runtime_presence_response(line.trim_end()),
            Err(err) if is_timeout_error(&err) => Ok(ViewerResponseLine::Timeout),
            Err(err) => Err(format!("read runtime presence response failed: {err}")),
        }
    }
}

#[expect(
    clippy::large_enum_variant,
    reason = "Stable runtime enum preserves established variant payload ownership and wire behavior."
)]
enum ViewerResponseLine {
    Response(ViewerResponse),
    Snapshot(BTreeSet<String>),
    Timeout,
    Closed,
}

fn decode_runtime_presence_response(line: &str) -> Result<ViewerResponseLine, String> {
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|err| format!("decode runtime presence response failed: {err}"))?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("snapshot") {
        return decode_runtime_presence_snapshot(&value).map(ViewerResponseLine::Snapshot);
    }
    serde_json::from_value(value)
        .map(ViewerResponseLine::Response)
        .map_err(|err| format!("decode runtime presence response failed: {err}"))
}

fn decode_runtime_presence_snapshot(
    response: &serde_json::Value,
) -> Result<BTreeSet<String>, String> {
    let snapshot = response
        .get("snapshot")
        .ok_or_else(|| "runtime presence snapshot is missing snapshot payload".to_string())?;
    let wire: RuntimePresenceSnapshotWire = serde_json::from_value(snapshot.clone())
        .map_err(|err| format!("decode runtime presence snapshot structure failed: {err}"))?;
    Ok(wire
        .model
        .agent_player_bindings
        .values()
        .map(|player_id| player_id.trim())
        .filter(|player_id| !player_id.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[derive(Debug, serde::Deserialize)]
struct RuntimePresenceSnapshotWire {
    model: RuntimePresenceModelWire,
}

#[derive(Debug, serde::Deserialize)]
struct RuntimePresenceModelWire {
    agent_player_bindings: BTreeMap<String, String>,
}

fn is_timeout_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

fn sleep_until_next_probe(
    stop_requested: &AtomicBool,
    probe_started_at: Instant,
    probe_interval: Duration,
) {
    let Some(remaining) = probe_interval.checked_sub(probe_started_at.elapsed()) else {
        return;
    };
    let sleep_chunk = Duration::from_millis(50);
    let deadline = Instant::now() + remaining;
    while !stop_requested.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        thread::sleep(std::cmp::min(sleep_chunk, deadline - now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_simulator::{WorldConfig, WorldModel};
    use std::io::{BufRead, BufReader, BufWriter, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn runtime_presence_decoder_accepts_decimal_string_snapshot_numbers() {
        let response = serde_json::json!({
            "type": "snapshot",
            "snapshot": {
                "version": 1,
                "time": "0",
                "next_event_id": "0",
                "journal_len": "0",
                "model": {
                    "agent_player_bindings": {
                        "agent-0": "player-wire",
                        "agent-empty": "  "
                    }
                }
            }
        });

        let decoded = decode_runtime_presence_response(response.to_string().as_str())
            .expect("decode runtime presence snapshot");
        let ViewerResponseLine::Snapshot(active_players) = decoded else {
            panic!("expected snapshot response");
        };
        assert_eq!(active_players, BTreeSet::from(["player-wire".to_string()]));
    }

    #[test]
    fn query_runtime_bound_players_reads_snapshot() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock");
        let addr = listener.local_addr().expect("local addr");
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let reader_stream = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(reader_stream);
            let mut writer = BufWriter::new(stream);

            expect_request_type(&mut reader, |request| {
                matches!(
                    request,
                    ViewerRequest::Hello {
                        version: VIEWER_PROTOCOL_VERSION,
                        ..
                    }
                )
            });
            write_response(
                &mut writer,
                &ViewerResponse::HelloAck {
                    server: "oasis7".to_string(),
                    version: VIEWER_PROTOCOL_VERSION,
                    min_version: 1,
                    max_version: VIEWER_PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    world_id: "test-world".to_string(),
                    control_profile: oasis7::viewer::ViewerControlProfile::Live,
                    authority_epoch: None,
                },
            );
            expect_request_type(&mut reader, |request| {
                matches!(request, ViewerRequest::RequestSnapshot)
            });
            write_response(
                &mut writer,
                &ViewerResponse::Snapshot {
                    snapshot: world_snapshot(["player-a", "player-b"]),
                },
            );
        });

        let active_players =
            query_runtime_bound_players(format!("{addr}").as_str()).expect("read snapshot");
        assert_eq!(
            active_players,
            BTreeSet::from(["player-a".to_string(), "player-b".to_string()])
        );

        handle.join().expect("join mock");
    }

    #[test]
    fn query_runtime_bound_players_allows_runtime_snapshot_generation_delay() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock");
        let addr = listener.local_addr().expect("local addr");
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let reader_stream = stream.try_clone().expect("clone");
            let mut reader = BufReader::new(reader_stream);
            let mut writer = BufWriter::new(stream);

            expect_request_type(&mut reader, |request| {
                matches!(
                    request,
                    ViewerRequest::Hello {
                        version: VIEWER_PROTOCOL_VERSION,
                        ..
                    }
                )
            });
            write_response(
                &mut writer,
                &ViewerResponse::HelloAck {
                    server: "oasis7".to_string(),
                    version: VIEWER_PROTOCOL_VERSION,
                    min_version: 1,
                    max_version: VIEWER_PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    world_id: "test-world".to_string(),
                    control_profile: oasis7::viewer::ViewerControlProfile::Live,
                    authority_epoch: None,
                },
            );
            expect_request_type(&mut reader, |request| {
                matches!(request, ViewerRequest::RequestSnapshot)
            });
            thread::sleep(Duration::from_millis(350));
            write_response(
                &mut writer,
                &ViewerResponse::Snapshot {
                    snapshot: world_snapshot(["player-delayed"]),
                },
            );
        });

        let active_players =
            query_runtime_bound_players(format!("{addr}").as_str()).expect("delayed snapshot");
        assert_eq!(
            active_players,
            BTreeSet::from(["player-delayed".to_string()])
        );
        handle.join().expect("join mock");
    }

    #[test]
    fn runtime_presence_monitor_uses_short_lived_snapshot_probes() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind mock");
        let addr = listener.local_addr().expect("local addr");
        let (accept_tx, accept_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().expect("accept");
                let reader_stream = stream.try_clone().expect("clone");
                let mut reader = BufReader::new(reader_stream);
                let mut writer = BufWriter::new(stream);

                expect_request_type(&mut reader, |request| {
                    matches!(
                        request,
                        ViewerRequest::Hello {
                            version: VIEWER_PROTOCOL_VERSION,
                            ..
                        }
                    )
                });
                write_response(
                    &mut writer,
                    &ViewerResponse::HelloAck {
                        server: "oasis7".to_string(),
                        version: VIEWER_PROTOCOL_VERSION,
                        min_version: 1,
                        max_version: VIEWER_PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        world_id: "test-world".to_string(),
                        control_profile: oasis7::viewer::ViewerControlProfile::Live,
                        authority_epoch: None,
                    },
                );
                expect_request_type(&mut reader, |request| {
                    matches!(request, ViewerRequest::RequestSnapshot)
                });
                write_response(
                    &mut writer,
                    &ViewerResponse::Snapshot {
                        snapshot: world_snapshot(["player-a"]),
                    },
                );
                accept_tx.send(()).expect("send accept");
            }
        });

        let stop_requested = Arc::new(AtomicBool::new(false));
        let live_bind = Arc::new(addr.to_string());
        let hosted_session_issuer = Arc::new(Mutex::new(HostedPlayerSessionIssuer::default()));
        let monitor = {
            let stop_requested = Arc::clone(&stop_requested);
            let live_bind = Arc::clone(&live_bind);
            let hosted_session_issuer = Arc::clone(&hosted_session_issuer);
            thread::spawn(move || {
                run_runtime_presence_monitor_with_interval(
                    stop_requested,
                    live_bind,
                    hosted_session_issuer,
                    Duration::from_millis(20),
                )
            })
        };

        accept_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first probe");
        accept_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("second probe");
        stop_requested.store(true, Ordering::SeqCst);

        monitor.join().expect("join monitor");
        server.join().expect("join server");

        let admission = hosted_session_issuer
            .lock()
            .expect("lock issuer")
            .admission(DeploymentMode::HostedPublicJoin);
        assert_eq!(admission.admission.runtime_bound_player_sessions, 1);
        assert_eq!(admission.admission.runtime_probe_status, "ok");
    }

    fn expect_request_type<F>(reader: &mut BufReader<TcpStream>, predicate: F)
    where
        F: FnOnce(&ViewerRequest) -> bool,
    {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request");
        let request: ViewerRequest = serde_json::from_str(line.trim_end()).expect("decode request");
        assert!(predicate(&request), "unexpected request: {request:?}");
    }

    fn write_response(writer: &mut BufWriter<TcpStream>, response: &ViewerResponse) {
        serde_json::to_writer(&mut *writer, response).expect("write response");
        writer.write_all(b"\n").expect("write newline");
        writer.flush().expect("flush response");
    }

    fn world_snapshot<const N: usize>(players: [&str; N]) -> WorldSnapshot {
        let mut model = WorldModel::default();
        for (index, player_id) in players.iter().enumerate() {
            model
                .agent_player_bindings
                .insert(format!("agent-{index}"), (*player_id).to_string());
        }
        WorldSnapshot {
            version: 1,
            chunk_generation_schema_version: 1,
            time: 0,
            config: WorldConfig::default(),
            model,
            runtime_snapshot: None,
            player_gameplay: None,
            chain_resource_manifest: Default::default(),
            latest_chain_resource_delta: Default::default(),
            chunk_runtime: Default::default(),
            intel_ttl_ticks: 0,
            next_event_id: 0,
            next_action_id: 0,
            pending_actions: Vec::new(),
            journal_len: 0,
        }
    }
}
