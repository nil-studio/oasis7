//! Exercises the shipped viewer main and newline protocol with no node filesystem access.
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Stdio};

struct ViewerProcess {
    child: Child,
    addr: String,
    root: std::path::PathBuf,
}
impl Drop for ViewerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if std::thread::panicking() {
            println!("shipped_viewer_failure_diagnostics={}", self.diagnostics());
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
impl ViewerProcess {
    fn start(fixture: &Fixture, trust: &str, world: &str, scope: &str) -> Self {
        let binary = std::env::var("PRE2_VIEWER_BINARY")
            .expect("runner must supply actual built oasis7_viewer_live executable");
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = socket.local_addr().unwrap().to_string();
        drop(socket);
        let root = temp_dir("qa-shipped-viewer");
        fs::create_dir_all(&root).unwrap();
        let denied = fs::canonicalize(&fixture.root).unwrap();
        let profile = format!(
            "(version 1)(allow default)(deny file-read* file-write* (subpath \"{}\"))",
            denied.display()
        );
        // This independent probe proves the exact launch profile denies an existing node file.
        // The shipped executable is launched under that same profile, without a test-only probe.
        let probe = application_sandbox::command(std::path::Path::new("/bin/cat"), &root, &profile)
            .arg(denied.join("world/world-service-identity.json"))
            .output()
            .unwrap();
        assert!(!probe.status.success());
        let stderr = String::from_utf8_lossy(&probe.stderr);
        assert!(
            stderr.contains("Operation not permitted") || stderr.contains("Permission denied"),
            "{stderr}"
        );
        println!(
            "shipped_viewer_artifact_blake3={} sandbox_profile_blake3={} os_denial_probe=independent_same_profile isolated_cwd=true",
            blake3::hash(&fs::read(&binary).unwrap()),
            application_sandbox::profile_digest(std::path::Path::new(&binary), &root, &profile)
        );
        println!(
            "shipped_viewer_public_configuration={}",
            serde_json::json!({"endpoint":fixture.client.config().endpoint,"trusted_service_public_key":trust,"world_id":world,"genesis_digest":"fixture-genesis-v1","scope":scope,"legacy_bind":false})
        );
        let stdout = fs::File::create(root.join("stdout.log")).unwrap();
        let stderr = fs::File::create(root.join("stderr.log")).unwrap();
        let child = application_sandbox::command(std::path::Path::new(&binary), &root, &profile)
            .args([
                "--bind",
                &addr,
                "--no-web-bind",
                "--no-auto-play",
                "--no-llm",
            ])
            .current_dir(&root)
            .env(
                "OASIS7_WORLD_SERVICE_ENDPOINT",
                &fixture.client.config().endpoint,
            )
            .env("OASIS7_WORLD_SERVICE_PUBLIC_KEY", trust)
            .env("OASIS7_WORLD_SERVICE_WORLD_ID", world)
            .env("OASIS7_WORLD_SERVICE_GENESIS_DIGEST", "fixture-genesis-v1")
            .env("OASIS7_WORLD_SERVICE_SCOPE", scope)
            .env(
                "OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY",
                &fixture.client.config().read_private_key_hex,
            )
            .env_remove("OASIS7_WORLD_SERVICE_AGENT_PRIVATE_KEY")
            .env_remove("OASIS7_WORLD_SERVICE_AGENT_DELEGATION_GENERATION")
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .unwrap();
        Self { child, addr, root }
    }
    fn connect(&mut self) -> Protocol {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(stream) = TcpStream::connect(&self.addr) {
                stream
                    .set_read_timeout(Some(Duration::from_millis(200)))
                    .unwrap();
                return Protocol(BufReader::new(stream));
            }
            if let Some(status) = self.child.try_wait().unwrap() {
                panic!("shipped viewer exited {status}: {}", self.diagnostics());
            }
            assert!(
                Instant::now() < deadline,
                "shipped viewer bind timeout: {}",
                self.diagnostics()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn diagnostics(&self) -> String {
        fs::read_to_string(self.root.join("stderr.log")).unwrap_or_default()
    }
}
struct Protocol(BufReader<TcpStream>);
impl Protocol {
    fn send(&mut self, value: serde_json::Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.0.get_mut().write_all(&bytes).unwrap();
    }
    fn receive(&mut self, kind: &str) -> serde_json::Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut line = String::new();
            match self.0.read_line(&mut line) {
                Ok(0) => panic!("viewer closed before {kind}"),
                Ok(_) => {
                    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                    if value["type"] == kind {
                        return value;
                    }
                    if value["type"]
                        .as_str()
                        .is_some_and(|kind| kind.ends_with("_error"))
                    {
                        panic!(
                            "viewer rejected before {kind}: code={} message={}",
                            value["error"]["code"],
                            value["error"]["message"]
                                .as_str()
                                .unwrap_or("")
                                .chars()
                                .take(256)
                                .collect::<String>()
                        );
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("viewer protocol read: {e}"),
            }
            assert!(Instant::now() < deadline, "viewer response timeout: {kind}");
        }
    }
    fn subscribe(&mut self) -> serde_json::Value {
        self.send(serde_json::json!({"type":"hello_v2","client":"PRE2 shipped viewer QA","version":2,"capabilities":[]}));
        assert_eq!(self.receive("hello_ack")["world_id"], "w1");
        self.send(serde_json::json!({"type":"subscribe","streams":["snapshot","events"],"event_kinds":[]}));
        self.send(serde_json::json!({"type":"request_snapshot"}));
        self.receive("snapshot")["snapshot"].clone()
    }

    fn request_snapshot(&mut self) -> oasis7::simulator::WorldSnapshot {
        self.send(serde_json::json!({"type":"request_snapshot"}));
        let response: oasis7::viewer::ViewerResponse =
            serde_json::from_value(self.receive("snapshot")).expect("valid Viewer response");
        let oasis7::viewer::ViewerResponse::Snapshot { snapshot } = response else {
            panic!("expected the requested snapshot")
        };
        snapshot
    }
}

#[test]
fn real_tcp_shipped_viewer_initial_periodic_controls_and_reconnect() {
    let fixture = Fixture::with_options(true, false);
    let mut process = ViewerProcess::start(
        &fixture,
        &fixture.client.config().trusted_service_public_key,
        "w1",
        "public",
    );
    let mut protocol = process.connect();
    let initial = protocol.subscribe();
    let canonical = fixture.client.read_view(fixture.view(None)).unwrap();
    let cursor = protocol.receive("authoritative_recovery_ack")["ack"].clone();
    assert_eq!(
        cursor["snapshot_height"],
        serde_json::json!(canonical.version().commit.position)
    );
    assert_eq!(
        cursor["snapshot_hash"],
        canonical.version().commit.state_root_ref
    );
    assert_eq!(
        cursor["log_cursor"],
        serde_json::json!(canonical.continuation().sequence)
    );
    assert_eq!(
        initial["time"],
        serde_json::json!(canonical.projection().state.time)
    );
    assert!(initial["runtime_snapshot"].is_object());
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    protocol.send(
        serde_json::json!({"type":"live_control","mode":{"mode":"step","count":1},"request_id":1}),
    );
    let step_ack = protocol.receive("control_completion_ack")["ack"].clone();
    assert_eq!(step_ack["delta_logical_time"], 0);
    assert_eq!(step_ack["delta_event_seq"], 0);
    protocol.send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":2}));
    thread::sleep(Duration::from_millis(300));
    protocol
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":3}));
    // Play/Pause have no completion ACK in the shipped protocol. A subsequent
    // snapshot request establishes that this ordered stream processed them.
    protocol.send(serde_json::json!({"type":"request_snapshot"}));
    protocol.receive("snapshot");
    assert_eq!(
        fixture
            .driver
            .lock()
            .unwrap()
            .state
            .last_applied_committed_height,
        before,
        "viewer controls advanced canonical execution"
    );
    let after_controls = fixture.client.read_view(fixture.view(None)).unwrap();
    assert_eq!(after_controls.version().commit, canonical.version().commit);
    commit_request(&mut fixture.driver.lock().unwrap(), 0, None);
    let next = fixture.client.read_view(fixture.view(None)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        protocol.send(serde_json::json!({"type":"request_snapshot"}));
        let snapshot = protocol.receive("snapshot")["snapshot"].clone();
        if snapshot["time"] == serde_json::json!(next.projection().state.time) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "periodic projection did not catch canonical commit"
        );
        thread::sleep(Duration::from_millis(100));
    }
    drop(protocol);
    let mut reconnected = process.connect();
    let recovered = reconnected.subscribe();
    assert_eq!(
        recovered["time"],
        serde_json::json!(next.projection().state.time)
    );
    let recovered_cursor = reconnected.receive("authoritative_recovery_ack")["ack"].clone();
    assert_eq!(
        recovered_cursor["snapshot_height"],
        serde_json::json!(next.version().commit.position)
    );
    assert_eq!(
        recovered_cursor["snapshot_hash"],
        next.version().commit.state_root_ref
    );
    assert_eq!(
        recovered_cursor["log_cursor"],
        serde_json::json!(next.continuation().sequence)
    );
    println!("PRE2_SHIPPED_VIEWER_INITIAL_PERIODIC_CONTROLS_RECONNECT_PASSED");
}

#[test]
fn real_tcp_shipped_viewer_signed_collect_data_handler() {
    verify_signed_collect_data(false);
}

#[test]
fn real_tcp_shipped_viewer_lost_ack_preserves_original_lookup() {
    verify_signed_collect_data(true);
}

fn verify_signed_collect_data(lost_ack: bool) {
    use oasis7::viewer::{CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof};
    let fixture = Fixture::with_options(true, false);
    fixture
        .controlled_submit_commit
        .store(true, Ordering::SeqCst);
    let mut process = ViewerProcess::start(
        &fixture,
        &fixture.client.config().trusted_service_public_key,
        "w1",
        "public",
    );
    let mut protocol = process.connect();
    protocol.subscribe();
    let public = sign_read_request("owner", (), &fixture.owner)
        .unwrap()
        .subject_public_key;
    let mut registration = oasis7::viewer::AuthoritativeSessionRegisterRequest {
        player_id: "owner-a".into(),
        public_key: Some(public.clone()),
        registration_grant: None,
        auth: None,
        requested_agent_id: Some("agent-a".into()),
        force_rebind: false,
    };
    registration.auth = Some(
        oasis7::viewer::sign_session_register_auth_proof(
            &registration,
            54,
            &public,
            &fixture.owner,
        )
        .unwrap(),
    );
    protocol.send(serde_json::json!({"type":"authoritative_recovery","command":{"mode":"register_session","request":registration}}));
    loop {
        let ack = protocol.receive("authoritative_recovery_ack");
        if ack["ack"]["status"] == "session_registered" {
            break;
        }
    }
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let auth = sign_collect_data_auth_proof(&command, 55, &public, &fixture.owner).unwrap();
    if let CollectDataCommand::Submit { request } = &mut command {
        request.auth = Some(auth);
    }
    let original = fixture.request(WorldServicePayloadV1::GameplayJson(
        serde_json::to_vec(&command).unwrap(),
    ));
    let original_digest = correlation::key_digest(&original.correlation.key).unwrap();
    fixture.lose_next_submit.store(lost_ack, Ordering::SeqCst);
    protocol.send(serde_json::json!({"type":"collect_data","command":command}));
    if lost_ack {
        let rejection = protocol.receive("gameplay_action_error");
        assert_eq!(rejection["error"]["code"], "world_service_outcome_unknown");
    } else {
        let ack = protocol.receive("gameplay_action_ack");
        assert!(ack["ack"].is_object());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let world = fixture.driver.lock().unwrap().execution_world.clone();
        if world
            .state()
            .authenticated_collect_data_last_nonces
            .get("owner-a")
            .and_then(|nonces| nonces.get(&public))
            .copied()
            == Some(55)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "signed CollectData did not commit through shipped handler"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let matching = world.journal().events.iter().filter(|event| matches!(&event.body,
        oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::DataCollectedAuthenticated {
            collector_agent_id, electricity_cost: 7, data_amount: 11, player_id, nonce: 55, ..
        }) if collector_agent_id == "agent-a" && player_id == "owner-a"
    )).count();
    assert_eq!(
        matching, 1,
        "shipped signed gameplay handler canonical event multiplicity"
    );
    if lost_ack {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !fixture
            .lookup_digests
            .lock()
            .unwrap()
            .contains(&original_digest)
        {
            assert!(
                Instant::now() < deadline,
                "actual viewer did not Lookup the original pending identity"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let result = fixture
            .client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key,
                },
                original.signed_payload,
            )
            .unwrap();
        assert!(matches!(result.outcome, IntentOutcome::Committed { .. }));
        println!(
            "PRE2_SHIPPED_VIEWER_LOST_ACK_ORIGINAL_LOOKUP_PASSED correlation_digest={original_digest}"
        );
    } else {
        println!("PRE2_SHIPPED_VIEWER_SIGNED_COLLECT_DATA_HANDLER_PASSED");
    }
    verify_collect_data_feedback_isolation(&mut process, &mut protocol);
}

fn verify_collect_data_feedback_isolation(process: &mut ViewerProcess, owner: &mut Protocol) {
    // This connection used the shipped registration and signed Submit handlers.
    // It must receive its own terminal result without an additional Preflight or nonce.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let gameplay = owner
            .request_snapshot()
            .player_gameplay
            .expect("actual owner snapshot contains gameplay");
        if gameplay.recent_feedback.as_ref().is_some_and(|feedback| {
            feedback.action == "world_service_intent" && feedback.stage == "committed"
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original signed Submit did not expose its canonical terminal feedback: {:?}",
            gameplay.recent_feedback
        );
    }

    let mut observer = process.connect();
    let initial: oasis7::simulator::WorldSnapshot =
        serde_json::from_value(observer.subscribe()).expect("valid anonymous WorldSnapshot");
    assert_snapshot_feedback_absent(initial, "fresh anonymous connection");
    let request = oasis7::viewer::AuthoritativeReconnectSyncRequest {
        player_id: "owner-a".into(),
        session_pubkey: None,
        last_known_log_cursor: None,
        expected_reorg_epoch: None,
    };
    observer.send(serde_json::json!({"type":"authoritative_recovery","command":{"mode":"reconnect_sync","request":request}}));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let response: oasis7::viewer::ViewerResponse =
            serde_json::from_value(observer.receive("authoritative_recovery_ack"))
                .expect("valid reconnect response");
        let oasis7::viewer::ViewerResponse::AuthoritativeRecoveryAck { ack } = response else {
            panic!("expected reconnect acknowledgement")
        };
        if ack.status == oasis7::viewer::AuthoritativeRecoveryStatus::CatchUpReady
            && ack.player_id.as_deref() == Some("owner-a")
        {
            assert!(ack.session_pubkey.is_none());
            assert!(ack.session_epoch.is_none());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "claimed player reconnect was not acknowledged"
        );
    }
    assert_snapshot_feedback_absent(
        observer.request_snapshot(),
        "ReconnectSync player ID without authentication",
    );
    let retained = owner
        .request_snapshot()
        .player_gameplay
        .expect("actual owner snapshot contains gameplay")
        .recent_feedback
        .expect("anonymous reads must not clear the original connection's feedback");
    assert_eq!(retained.action, "world_service_intent");
    assert_eq!(retained.stage, "committed");
    println!("PRE2_SHIPPED_VIEWER_ORIGINAL_TERMINAL_FEEDBACK_ISOLATION_PASSED");
}

fn assert_snapshot_feedback_absent(snapshot: oasis7::simulator::WorldSnapshot, context: &str) {
    let gameplay = snapshot
        .player_gameplay
        .expect("actual observer snapshot contains gameplay");
    assert!(
        gameplay.recent_feedback.is_none(),
        "{context} exposed another connection's private feedback: {:?}",
        gameplay.recent_feedback
    );
}

#[test]
fn real_tcp_shipped_viewer_wrong_trust_world_scope_fail_closed() {
    let fixture = Fixture::with_options(true, false);
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    for (trust, world, scope) in [
        (hex::encode([0u8; 32]), "w1", "public"),
        (
            fixture.client.config().trusted_service_public_key.clone(),
            "other-world",
            "public",
        ),
        (
            fixture.client.config().trusted_service_public_key.clone(),
            "w1",
            "agent:unowned-agent",
        ),
    ] {
        let mut process = ViewerProcess::start(&fixture, &trust, world, scope);
        let mut protocol = process.connect();
        protocol.send(serde_json::json!({"type":"hello","client":"PRE2 negative","version":1}));
        protocol.receive("hello_ack");
        protocol
            .send(serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}));
        protocol.send(serde_json::json!({"type":"request_snapshot"}));
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let mut line = String::new();
            match protocol.0.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
                    assert_ne!(
                        response["type"], "snapshot",
                        "untrusted service produced local snapshot"
                    );
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("negative protocol transport: {e}"),
            }
        }
        println!(
            "shipped_viewer_rejection world={world} scope={scope} diagnostics={}",
            process.diagnostics()
        );
        assert_eq!(
            fixture
                .driver
                .lock()
                .unwrap()
                .state
                .last_applied_committed_height,
            before
        );
    }
    println!("PRE2_SHIPPED_VIEWER_TRUST_WORLD_SCOPE_FAIL_CLOSED_PASSED");
}

#[test]
fn real_tcp_shipped_viewer_transport_outage_and_recovery_preserve_authority() {
    let fixture = Fixture::with_options(true, false);
    let mut process = ViewerProcess::start(
        &fixture,
        &fixture.client.config().trusted_service_public_key,
        "w1",
        "public",
    );
    let mut protocol = process.connect();
    protocol.subscribe();
    let before = fixture.client.read_view(fixture.view(None)).unwrap();
    fixture.outage.active.store(true, Ordering::SeqCst);
    protocol.send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":9}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.outage.observed.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "viewer did not attempt actual transport during outage"
        );
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(250));
    fixture.outage.active.store(false, Ordering::SeqCst);
    let after = fixture.client.read_view(fixture.view(None)).unwrap();
    assert_eq!(
        after.version().commit,
        before.version().commit,
        "viewer advanced canonical state while service transport unavailable"
    );
    drop(protocol);
    let mut reconnected = process.connect();
    let recovered = reconnected.subscribe();
    assert_eq!(
        recovered["time"],
        serde_json::json!(after.projection().state.time)
    );
    let cursor = reconnected.receive("authoritative_recovery_ack")["ack"].clone();
    assert_eq!(
        cursor["snapshot_hash"],
        after.version().commit.state_root_ref
    );
    assert_eq!(
        cursor["snapshot_height"],
        serde_json::json!(after.version().commit.position)
    );
    assert_eq!(
        cursor["log_cursor"],
        serde_json::json!(after.continuation().sequence)
    );
    println!(
        "PRE2_SHIPPED_VIEWER_TRANSPORT_OUTAGE_RECOVERY_PASSED mechanism=accepted_socket_shutdown_before_dispatch"
    );
}
