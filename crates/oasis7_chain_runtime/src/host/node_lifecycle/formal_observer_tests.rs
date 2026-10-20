use super::super::local_io as io;
use super::*;
use crate::formal_read_fixtures as fixtures;
use fixtures::*;
use oasis7::world_service::{
    SignedReadRequest, authority,
    formal_read::{FormalReadHistoryV1, FormalReadVerifier},
    formal_read_service::{FormalCommittedReadService, FormalReadPublication},
};
use oasis7_client_api::world_service::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn instance(generation: u64) -> LocalInstance {
    LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: h(90),
        generation,
        incarnation: h(generation as u8),
        config_digest: h(91),
        supervisor_pid: 1,
        runtime_pid: None,
        started_unix_ms: 0,
        state: "running".into(),
        exit_code: None,
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) -> SignedReadRequest<ReadWorldViewRequest> {
    // BSD may inherit O_NONBLOCK from the listener. A socket timeout alone
    // does not clear that flag; fragmented requests would fail immediately.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let (offset, length) = loop {
        let mut chunk = [0; 4096];
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0, "client closed before sending its signed request");
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            assert!(header.starts_with("post /v1/world/view "));
            let length: usize = header
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                break (end + 4, length);
            }
        }
    };
    let signed: SignedReadRequest<ReadWorldViewRequest> =
        serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
    authority::verify_read_request("/v1/world/view", &signed).unwrap();
    signed
}

fn write_signed_response(
    stream: &mut std::net::TcpStream,
    signed_request: &SignedReadRequest<ReadWorldViewRequest>,
    payload: ReadWorldViewResponse<oasis7::world_service::projection::WorldServiceProjection>,
) {
    let digest = authority::request_digest("/v1/world/view", signed_request).unwrap();
    let signed_reply =
        authority::sign_service_response("/v1/world/view", digest, payload, &h(9)).unwrap();
    let body = serde_json::to_vec(&signed_reply).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
}

#[test]
fn signed_public_observer_retains_opaque_view_and_restart_revalidates_from_fixed_pin() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let origin = FormalReadPublication::new(anchor.clone(), world.clone()).unwrap();
    let service = FormalCommittedReadService::new(
        trust.clone(),
        FormalReadPublication::new(anchor.clone(), world).unwrap(),
    )
    .unwrap();
    let fixed_anchor = FormalReadVerifier::new(&trust, anchor.clone())
        .unwrap()
        .anchor_ref()
        .unwrap();
    let history = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![],
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let signed = read_http_request(&mut stream);
            assert_eq!(signed.request.scope_id, "public");
            assert_eq!(signed.request.formal_anchor.as_ref(), Some(&fixed_anchor));
            assert_eq!(
                signed.request.min_commit.is_some(),
                index == 1,
                "a live refresh carries the previous opaque view; a new process starts from its external pin"
            );
            let mut payload = service.read(&signed, &origin, &history).unwrap();
            if index == 1 {
                // The service transport signature remains valid, but its
                // projection header no longer agrees with the authenticated
                // CommitRef. The consumer must reject it and retain its last
                // opaque verified observation.
                payload.version.commit.binding.permission_generation += 1;
                payload.continuation.commit = payload.version.commit.clone();
            }
            write_signed_response(&mut stream, &signed, payload);
        }
    });

    let checkpoint = anchor.checkpoint();
    let config = FormalObserverConfigV1 {
        api_version: "oasis7.formal-public-observer/v1".into(),
        endpoint,
        trusted_service_public_key: public(9),
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        genesis_digest: trust.world.genesis_digest.clone(),
        formal_read_trust: FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        },
        poll_interval_ms: 100,
        request_timeout_ms: 5000,
        max_response_bytes: 16 * 1024 * 1024,
    };

    let mut first = Session::new(&config, h(7), &instance(1)).unwrap();
    first.poll();
    assert!(first.observed().read_ready);
    assert!(!first.observed().can_propose);
    assert!(!first.can_propose());
    assert_eq!(first.observed().state_sync_scope, "public_projection_only");
    assert_eq!(first.observed().source_freshness, "unknown");
    assert_eq!(first.observed().full_state_sync_readiness, "unknown");
    assert!(first.observed().public_projection.is_some());
    let first_view = first.current_view().expect("verified opaque view retained");
    assert!(first_view.formal_commit().is_some());
    let previous = first_view.version().commit.clone();
    let previous_projection =
        serde_json::to_value(first.observed().public_projection.as_ref()).unwrap();
    assert_eq!(
        first.observed().last_verified_commit.as_ref(),
        Some(&previous)
    );

    first.poll();
    assert!(!first.observed().read_ready);
    assert_eq!(
        first.observed().last_verified_commit.as_ref(),
        Some(&previous)
    );
    assert_eq!(
        serde_json::to_value(first.observed().public_projection.as_ref()).unwrap(),
        previous_projection
    );
    let retained = first
        .current_view()
        .expect("failed refresh preserves prior view");
    assert_eq!(retained.version().commit, previous);
    assert!(retained.formal_commit().is_some());
    drop(first);

    let mut restarted = Session::new(&config, h(8), &instance(2)).unwrap();
    assert!(restarted.current_view().is_none());
    assert!(restarted.observed().last_verified_commit.is_none());
    restarted.poll();
    assert!(restarted.observed().read_ready);
    assert_eq!(restarted.observed().generation, 2);
    assert_eq!(restarted.current_view().unwrap().version().commit, previous);
    server.join().unwrap();

    let mut wrong = config.clone();
    wrong.chain_id.push('x');
    assert!(validate(&wrong).is_err());
    let mut wrong = config;
    wrong.formal_read_trust.checkpoint_digest = h(93);
    assert!(validate(&wrong).is_err());
}

#[test]
fn restart_does_not_reuse_persisted_head_when_formal_history_is_unavailable() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let signed = read_http_request(&mut stream);
        assert_eq!(signed.request.scope_id, "public");
        assert!(signed.request.min_commit.is_none());
        write!(
            stream,
            "HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let checkpoint = anchor.checkpoint();
    let config = FormalObserverConfigV1 {
        api_version: "oasis7.formal-public-observer/v1".into(),
        endpoint,
        trusted_service_public_key: public(9),
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        genesis_digest: trust.world.genesis_digest.clone(),
        formal_read_trust: FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        },
        poll_interval_ms: 100,
        request_timeout_ms: 5000,
        max_response_bytes: 16 * 1024 * 1024,
    };
    let data = std::fs::canonicalize("/tmp").unwrap().join(format!(
        "oasis7-formal-observer-test-{}",
        io::random_id().unwrap()
    ));
    io::private_dir(&data).unwrap();
    let keys = data.join("keys");
    io::private_dir(&keys).unwrap();
    super::super::super::node_keypair_config::ensure_node_keypair_in_secure_config_dir(&keys)
        .unwrap();
    // A saved diagnostic status may contain a CommitRef, but startup must not
    // read it as a formal anchor or minimum.
    let mut stale = status(&instance(1));
    stale.last_verified_commit = Some(commit_ref(
        &anchor,
        &oasis7::world_service::projection::WorldServiceProjection::from_formal_committed_world(
            &world,
            None,
            &public(8),
        )
        .unwrap(),
    ));
    io::write(&data.join(STATUS_FILE), &stale).unwrap();
    let mut restarted = Session::open(&config, &data, &instance(2)).unwrap();
    assert!(restarted.current_view().is_none());
    assert!(restarted.observed().last_verified_commit.is_none());
    restarted.poll();
    assert!(!restarted.observed().read_ready);
    assert_eq!(restarted.observed().read_state, "history_unavailable");
    assert!(restarted.current_view().is_none());
    assert!(restarted.observed().last_verified_commit.is_none());
    assert!(
        restarted
            .observed()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("410"))
    );
    server.join().unwrap();
    std::fs::remove_dir_all(data).unwrap();
}

#[test]
fn schema2_process_hook_reports_verified_public_readiness_and_stops_cleanly() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let origin = FormalReadPublication::new(anchor.clone(), world.clone()).unwrap();
    let service = FormalCommittedReadService::new(
        trust.clone(),
        FormalReadPublication::new(anchor.clone(), world).unwrap(),
    )
    .unwrap();
    let fixed_anchor = FormalReadVerifier::new(&trust, anchor.clone())
        .unwrap()
        .anchor_ref()
        .unwrap();
    let history = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![],
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop_server = Arc::new(AtomicBool::new(false));
    let server_stop = stop_server.clone();
    let server = thread::spawn(move || {
        let mut request_count = 0;
        while !server_stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let signed = read_http_request(&mut stream);
                    assert_eq!(signed.request.scope_id, "public");
                    assert_eq!(signed.request.formal_anchor.as_ref(), Some(&fixed_anchor));
                    assert_eq!(signed.request.min_commit.is_some(), request_count > 0);
                    let payload = service.read(&signed, &origin, &history).unwrap();
                    write_signed_response(&mut stream, &signed, payload);
                    request_count += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("formal observer test service failed: {error}"),
            }
        }
        request_count
    });

    let checkpoint = anchor.checkpoint();
    let observer = FormalObserverConfigV1 {
        api_version: "oasis7.formal-public-observer/v1".into(),
        endpoint,
        trusted_service_public_key: public(9),
        world_id: trust.world.world_id.clone(),
        chain_id: trust.world.chain_id.clone(),
        genesis_digest: trust.world.genesis_digest.clone(),
        formal_read_trust: FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        },
        poll_interval_ms: 100,
        request_timeout_ms: 5000,
        max_response_bytes: 16 * 1024 * 1024,
    };
    let root = std::fs::canonicalize("/tmp").unwrap().join(format!(
        "oasis7-formal-process-test-{}",
        io::random_id().unwrap()
    ));
    io::private_dir(&root).unwrap();
    let chain_path = root.join("chain.json");
    io::write(
        &chain_path,
        &serde_json::json!({
            "api_version": "oasis7.chain-config/v2",
            "runtime_profile": PROFILE,
            "formal_observer": observer,
        }),
    )
    .unwrap();
    let chain = super::super::model::ChainConfig::load(&chain_path).unwrap();
    let data = root.join("node");
    let config = super::super::model::initialize(
        chain,
        &data,
        Some("formal-public-test-observer"),
        "127.0.0.1:19083",
    )
    .unwrap();
    assert_eq!(config.api_version, "oasis7.local-node/v2");
    assert!(!data.join("state").exists());

    let intent = super::super::process::test_intent(&config).unwrap();
    let incarnation = intent.target.incarnation.clone();
    io::write(&data.join("start-action.json"), &intent).unwrap();
    super::super::process::save_action(&data, &intent).unwrap();
    let config_path = data.join("node.json");
    let supervisor =
        thread::spawn(move || super::super::process::supervise(&config_path, &incarnation));
    let started = Instant::now();
    let ready = loop {
        let status = super::super::process::status(&data).unwrap();
        if status["formal_readiness"] == true {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "formal process did not become read-ready: {status}"
        );
        assert!(!supervisor.is_finished(), "formal supervisor exited early");
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(ready["instance"]["state"], "running");
    assert_eq!(
        ready["formal_public_observer"]["read_state"],
        "qualified_public_read"
    );
    assert!(
        ready["formal_public_observer"]["read_ready"]
            .as_bool()
            .unwrap()
    );
    assert!(
        !ready["formal_public_observer"]["can_propose"]
            .as_bool()
            .unwrap()
    );
    assert!(ready["formal_public_observer"]["public_projection"].is_object());
    assert_eq!(ready["verified_sync_readiness"], serde_json::Value::Null);
    assert_eq!(ready["can_propose"], false);
    assert_eq!(ready["blocker"], serde_json::Value::Null);

    let current: LocalInstance = serde_json::from_value(ready["instance"].clone()).unwrap();
    let stopped = super::super::process::stop_instance(&config, &current, 2000).unwrap();
    assert_eq!(stopped["instance"]["state"], "stopped");
    assert_eq!(stopped["formal_readiness"], false);
    supervisor.join().unwrap().unwrap();
    stop_server.store(true, Ordering::SeqCst);
    assert!(server.join().unwrap() > 0);
    let final_status = super::super::process::status(&data).unwrap();
    assert_eq!(final_status["instance"]["state"], "stopped");
    assert_eq!(final_status["formal_readiness"], false);
    assert_eq!(final_status["formal_public_observer"]["read_ready"], false);
    assert_eq!(
        final_status["formal_public_observer"]["read_state"],
        "stopped"
    );
    assert!(final_status["formal_public_observer"]["last_verified_commit"].is_object());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn formal_observer_fragmented_request_clears_inherited_nonblocking_stream() {
    let (_, trust, anchor, _) = actual_world_initial_fixture();
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world: WorldIdentity {
            world_id: trust.world.world_id.clone(),
            genesis_digest: trust.world.genesis_digest.clone(),
        },
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
        formal_anchor: Some(
            FormalReadVerifier::new(&trust, anchor)
                .unwrap()
                .anchor_ref()
                .unwrap(),
        ),
    };
    let signed = authority::sign_read_request("/v1/world/view", request, &h(7)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    // connect() completing does not guarantee the nonblocking listener's
    // accept queue has already become readable, especially on BSD.
    let accept_deadline = Instant::now() + Duration::from_secs(5);
    let (mut accepted, _) = loop {
        match listener.accept() {
            Ok(connection) => break connection,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < accept_deadline,
                    "test accept queue deadline exceeded"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("fragmented observer test accept failed: {error}"),
        }
    };
    // Make the inherited BSD behavior deterministic on Linux as well.
    accepted.set_nonblocking(true).unwrap();
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        ready_sender.send(()).unwrap();
        read_http_request(&mut accepted)
    });
    ready_receiver.recv().unwrap();
    let body = serde_json::to_vec(&signed).unwrap();
    write!(
        client,
        "POST /v1/world/view HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    client.flush().unwrap();
    thread::sleep(Duration::from_millis(50));
    client.write_all(&body).unwrap();
    let decoded = server.join().unwrap();
    assert_eq!(decoded.request, signed.request);
    assert_eq!(decoded.subject_public_key, signed.subject_public_key);
}
