use super::*;
use oasis7::runtime::{Action, World};
use oasis7::world_service::projection::WorldServiceProjection;
use oasis7_client_api::world_service::{
    CommitRef, EventCursor, ExecutionBinding, IntentOutcome, IntentResponse, ProjectionVersion,
    ReadWorldViewResponse,
};
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::net::Shutdown;
use std::net::TcpListener;
#[cfg(unix)]
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[cfg(unix)]
use std::thread;

pub(super) fn world_fixture() -> (World, String, WorldIdentity, CommitRef) {
    let private = hex::encode([73; 32]);
    let key = authority::sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = oasis7::runtime_host::world::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-0".into(),
        pos: oasis7::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent-0".into(),
        player_id: "owner".into(),
        public_key: Some(key),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent-0", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("canonical-cli-fixture", "main", 0, None, "pending", 0)
        .unwrap();
    let identity = WorldIdentity {
        world_id: "canonical-cli-fixture".into(),
        genesis_digest: "fixture".into(),
    };
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: identity.world_id.clone(),
            branch_id: "main".into(),
            finality_ref: "fixture".into(),
            reorg_generation: 0,
            governing_manifest_ref: "fixture".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 2,
        execution_block_hash: "fixture".into(),
        state_root_ref: "fixture".into(),
    };
    (world, private, identity, commit)
}
#[cfg(unix)]
fn read_http(socket: &mut std::net::TcpStream) -> (String, Value, Vec<u8>) {
    socket.set_nonblocking(false).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut part = [0; 4096];
        let n = socket.read(&mut part).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&part[..n]);
        if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..i]);
            let length: usize = headers
                .lines()
                .find_map(|v| {
                    v.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() >= i + 4 + length {
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string();
                return (
                    path,
                    serde_json::from_slice(&bytes[i + 4..i + 4 + length]).unwrap(),
                    bytes[i + 4..i + 4 + length].to_vec(),
                );
            }
        }
    }
}
#[cfg(unix)]
fn reply(socket: &mut std::net::TcpStream, body: Value) {
    let bytes = serde_json::to_vec(&body).unwrap();
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )
    .unwrap();
    socket.write_all(&bytes).unwrap();
}

#[cfg(unix)]
pub(super) fn canonical_fixture(recover_after_timeout: bool, key_format: Option<&'static str>) {
    let name = std::thread::current().name().unwrap().to_string();
    if std::env::var("OASIS7_ISOLATED_CHAT_TEST").ok().as_deref() != Some(&name) {
        let state = private_state_dir();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env("OASIS7_ISOLATED_CHAT_TEST", &name)
            .env("XDG_STATE_HOME", &state)
            .output()
            .unwrap();
        std::fs::remove_dir_all(state).unwrap();
        assert!(
            output.status.success(),
            "isolated canonical fixture: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let (world, private, identity, commit) = world_fixture();
    let canonical = Arc::new(Mutex::new(world));
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let service_private = hex::encode([9; 32]);
    let service_key = authority::sign_read_request("fixture", (), &service_private)
        .unwrap()
        .subject_public_key;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let service_world = canonical.clone();
    let submitted = received.clone();
    let retry_enabled = Arc::new(AtomicBool::new(!recover_after_timeout));
    let service_retry_enabled = retry_enabled.clone();
    let (submit_observed, observed_submit) = std::sync::mpsc::channel();
    let service = thread::spawn(move || {
        let mut committed: Option<Value> = None;
        let mut first_submit_bytes = None;
        while !stopped.load(Ordering::Relaxed) {
            let (mut socket, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            let (path, body, original_bytes) = read_http(&mut socket);
            let digest = authority::request_digest(&path, &body).unwrap();
            let payload = if path == VIEW_PATH {
                let signed: SignedReadRequest<ReadWorldViewRequest> =
                    serde_json::from_value(body).unwrap();
                authority::verify_read_request(VIEW_PATH, &signed).unwrap();
                let world = service_world.lock().unwrap();
                let owner = signed.request.scope_id.strip_prefix("agent:");
                if owner.is_some() {
                    assert_eq!(
                        signed.subject_public_key,
                        authority::sign_read_request("fixture", (), &hex::encode([73; 32]))
                            .unwrap()
                            .subject_public_key
                    );
                }
                let mut projection = WorldServiceProjection::from_authenticated_world(
                    &world,
                    owner,
                    &signed.subject_public_key,
                )
                .unwrap();
                projection.runtime_binding = None;
                serde_json::to_value(ReadWorldViewResponse {
                    formal_profile: None,
                    formal_proof: None,
                    execution_evidence_scope: None,
                    contract_version: 1,
                    version: ProjectionVersion {
                        commit: commit.clone(),
                        projection_revision: "fixture".into(),
                        visibility_scope: signed.request.scope_id.clone(),
                    },
                    logical_tick: world.state().time,
                    continuation: EventCursor {
                        stream_id: "fixture".into(),
                        scope_id: signed.request.scope_id,
                        era: 0,
                        sequence: 0,
                        commit: commit.clone(),
                    },
                    view: projection,
                })
                .unwrap()
            } else if path == CHANGES_PATH {
                let signed: SignedReadRequest<ReadWorldChangesRequest> =
                    serde_json::from_value(body).unwrap();
                authority::verify_read_request(CHANGES_PATH, &signed).unwrap();
                serde_json::to_value(ReadWorldChangesResponse::<Value> {
                    formal_profile: None,
                    execution_evidence_scope: None,
                    contract_version: 1,
                    changes: Vec::new(),
                    next_cursor: signed.request.cursor,
                })
                .unwrap()
            } else if path == LOOKUP_PATH {
                let originals = submitted.lock().unwrap();
                assert_eq!(body["original"], originals[0]["signed_payload"]);
                assert_eq!(body["request"]["key"], originals[0]["correlation"]["key"]);
                let mut pending = committed.as_ref().unwrap().clone();
                pending["outcome"] = json!({"status":"pending"});
                pending
            } else if path == SUBMIT_PATH {
                submitted.lock().unwrap().push(body.clone());
                submit_observed.send(()).unwrap();
                if let Some(result) = &committed {
                    let originals = submitted.lock().unwrap();
                    assert_eq!(originals[0], originals[1]);
                    assert_eq!(first_submit_bytes.as_ref().unwrap(), &original_bytes);
                    if service_retry_enabled.load(Ordering::Acquire) {
                        result.clone()
                    } else {
                        let mut pending = result.clone();
                        pending["outcome"] = json!({"status":"pending"});
                        pending
                    }
                } else {
                    first_submit_bytes = Some(original_bytes);
                    let request: SubmitIntentRequest<WorldServicePayloadV1> =
                        serde_json::from_value(body).unwrap();
                    let WorldServicePayloadV1::AgentChat(chat) = &request.signed_payload else {
                        panic!("unexpected payload")
                    };
                    assert_eq!(chat.world_id.as_deref(), Some("canonical-cli-fixture"));
                    assert_eq!(
                        chat.canonical_authority
                            .as_ref()
                            .unwrap()
                            .agent_identity_generation,
                        1
                    );
                    let mut world = service_world.lock().unwrap();
                    let receipt = world.apply_authenticated_agent_chat(chat).unwrap();
                    world
                        .record_world_service_result(CanonicalIntentResultV1 {
                            request: request.clone(),
                            action_id: 1,
                            committed_height: 2,
                            receipt: receipt.clone(),
                            rejected: None,
                        })
                        .unwrap();
                    committed = Some(
                        serde_json::to_value(IntentResponse {
                            formal_profile: None,
                            execution_evidence_scope: None,
                            contract_version: 1,
                            correlation: request.correlation.clone(),
                            outcome: IntentOutcome::Committed {
                                commit: Box::new(commit.clone()),
                                receipt,
                            },
                        })
                        .unwrap(),
                    );
                    serde_json::to_value(IntentResponse::<Value> {
                        formal_profile: None,
                        execution_evidence_scope: None,
                        contract_version: 1,
                        correlation: request.correlation,
                        outcome: IntentOutcome::Pending,
                    })
                    .unwrap()
                }
            } else {
                panic!("unexpected service path {path}")
            };
            reply(
                &mut socket,
                serde_json::to_value(
                    authority::sign_service_response(&path, digest, payload, &service_private)
                        .unwrap(),
                )
                .unwrap(),
            );
        }
    });
    let mut config = oasis7::viewer::ViewerRuntimeLiveServerConfig::new(
        oasis7_simulator::WorldScenario::Minimal,
    );
    config.world_id = identity.world_id.clone();
    config.world_service = Some(client::WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint,
        trusted_service_public_key: service_key,
        expected_world: identity,
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(3),
        max_response_bytes: 1024 * 1024,
    });
    let server = Arc::new(Mutex::new(
        oasis7::viewer::ViewerRuntimeLiveServer::new(config).unwrap(),
    ));
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp.local_addr().unwrap().to_string();
    let serving = thread::spawn(move || {
        for _ in
            0..if recover_after_timeout || key_format.is_some_and(|key| key.ends_with("failure")) {
                3
            } else {
                2
            }
        {
            let (socket, _) = tcp.accept().unwrap();
            oasis7::viewer::ViewerRuntimeLiveServer::test_serve_shared_stream(
                server.clone(),
                socket,
            )
            .unwrap();
        }
    });
    let timeout = Duration::from_secs(10);
    let mut registration =
        ViewerConnection::connect(&addr, "fixture-registration", timeout).unwrap();
    registration.send(&ViewerRequest::RequestSnapshot).unwrap();
    registration
        .collect_until(timeout, terminal_snapshot, "prime")
        .unwrap();
    let request = build_signed_session_register_request(
        "owner",
        &private,
        None,
        Some("agent-0".into()),
        None,
    )
    .unwrap();
    registration
        .send(&ViewerRequest::AuthoritativeRecovery {
            command: AuthoritativeRecoveryCommand::RegisterSession { request },
        })
        .unwrap();
    let registered = registration
        .collect_until(timeout, terminal_recovery, "register")
        .unwrap();
    assert!(
        matches!(
            registered.last().unwrap().response,
            ViewerResponse::AuthoritativeRecoveryAck { .. }
        ),
        "{:?}",
        registered.last().unwrap().raw
    );
    registration
        .writer
        .get_ref()
        .shutdown(Shutdown::Both)
        .unwrap();
    drop(registration);
    let mut conn = ViewerConnection::connect(&addr, "fixture-chat", timeout).unwrap();
    subscribe_for_control(&mut conn, true, false).unwrap();
    let canonical_key = oasis7_pure_api_client_support::derive_public_key_hex(&private).unwrap();
    let supplied_key = key_format.map(|format| match format {
        "uppercase" => canonical_key.to_uppercase(),
        "whitespace" => format!("  {canonical_key}\n"),
        "post_commit_failure" | "write_failure" | "flush_failure" => canonical_key.clone(),
        _ => panic!("unknown key format"),
    });
    let input = ChatInput {
        agent_id: "agent-0",
        player_id: "owner",
        message: "private canonical fixture goal",
        private_key_hex: &private,
        public_key_hex: supplied_key.as_deref(),
        intent_tick: None,
        intent_seq: None,
    };
    let first = if recover_after_timeout {
        let (chat, responses, artifact) =
            prepare_canonical(&mut conn, Instant::now() + timeout, input).unwrap();
        conn.send(&ViewerRequest::AgentChat {
            request: chat.clone(),
        })
        .unwrap();
        observed_submit
            .recv_timeout(timeout)
            .expect("service must receive original canonical Submit before timeout phase");
        submit_original(
            &mut conn,
            Instant::now() + Duration::from_millis(300),
            chat,
            responses,
            &artifact,
        )
    } else {
        exchange(&mut conn, timeout, input)
    };
    let mut outcome = if recover_after_timeout {
        let error = match first {
            Err(error) => error,
            Ok(_) => panic!("pending is not committed success"),
        };
        let path = std::path::PathBuf::from(
            error
                .split("retained at ")
                .nth(1)
                .unwrap_or_else(|| {
                    panic!("timeout must retain persisted original request: {error}")
                })
                .split(". Retry with")
                .next()
                .unwrap(),
        );
        let saved = pending::load(&path).unwrap();
        assert_eq!(saved.endpoint, addr);
        let WorldServicePayloadV1::AgentChat(original) =
            serde_json::from_value::<WorldServicePayloadV1>(
                received.lock().unwrap()[0]["signed_payload"].clone(),
            )
            .unwrap()
        else {
            panic!("chat")
        };
        assert_eq!(saved.request, original);
        conn.writer.get_ref().shutdown(Shutdown::Both).unwrap();
        drop(conn);
        conn = ViewerConnection::connect(&addr, "fixture-retry", timeout).unwrap();
        retry_enabled.store(true, Ordering::Release);
        let result = retry(&mut conn, timeout, &path).unwrap();
        assert!(
            path.exists(),
            "ACK alone must retain pending artifact until reporting succeeds"
        );
        result
    } else {
        first.unwrap()
    };
    if key_format.is_some_and(|key| key.ends_with("failure")) {
        let artifact = outcome.artifact.clone().unwrap();
        let original = pending::load(&artifact).unwrap().request;
        let error = if key_format == Some("post_commit_failure") {
            conn.writer.get_ref().shutdown(Shutdown::Both).unwrap();
            finish_report(&mut conn, outcome, true, timeout, &mut Vec::new()).unwrap_err()
        } else {
            let mut writer = FailingOutput {
                flush_only: key_format == Some("flush_failure"),
            };
            finish_report(&mut conn, outcome, false, timeout, &mut writer).unwrap_err()
        };
        assert!(
            error.contains("canonical chat committed")
                && error.contains(&artifact.display().to_string()),
            "{error}"
        );
        assert!(
            artifact.exists(),
            "known committed request must remain recoverable after snapshot/output failure"
        );
        assert_eq!(pending::load(&artifact).unwrap().request, original);
        if key_format != Some("post_commit_failure") {
            conn.writer.get_ref().shutdown(Shutdown::Both).unwrap();
        }
        drop(conn);
        conn = ViewerConnection::connect(&addr, "fixture-postcommit-retry", timeout).unwrap();
        outcome = retry(&mut conn, timeout, &artifact).unwrap();
    }
    assert!(
        matches!(&outcome.responses.last().unwrap().response,ViewerResponse::AgentChatAck{ack}if ack.intent_id.is_some()&&ack.auth_nonce.is_some())
    );
    let artifact = outcome.artifact.clone().unwrap();
    finish_report(&mut conn, outcome, false, timeout, &mut Vec::new()).unwrap();
    assert!(
        !artifact.exists(),
        "successful report removes pending artifact"
    );
    assert_eq!(
        received.lock().unwrap().len(),
        if recover_after_timeout || key_format.is_some_and(|key| key.ends_with("failure")) {
            3
        } else {
            2
        }
    );
    conn.writer.get_ref().shutdown(Shutdown::Both).unwrap();
    drop(conn);
    serving.join().unwrap();
    stop.store(true, Ordering::Relaxed);
    service.join().unwrap();
}

#[cfg(unix)]
pub(super) fn private_state_dir() -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-cli-state-fixture-{}-{}-{}",
        std::process::id(),
        next_u64_id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    root
}

#[cfg(unix)]
struct FailingOutput {
    flush_only: bool,
}
#[cfg(unix)]
impl Write for FailingOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.flush_only {
            Ok(bytes.len())
        } else {
            Err(std::io::Error::other("fixture output failure"))
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("fixture flush failure"))
    }
}
