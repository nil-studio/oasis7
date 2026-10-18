//! Real process loss after canonical Act, before settlement/finalization, with app-private state.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::io::{BufRead, BufReader, Write};

const MEMORY: &str = "native canonical memory survives application process loss";

fn shutdown_application_socket(socket: &TcpStream) {
    match socket.shutdown(std::net::Shutdown::Both) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotConnected => {
            // A completed server may already have closed the connection.
            // Its worker is still joined and its actual Result checked below.
            println!("native_memory_teardown_peer_already_closed=true");
        }
        Err(error) => panic!("actual application socket shutdown failed: {error}"),
    }
}

#[test]
fn real_tcp_native_memory_process_restart_uses_original_receipt_and_private_store() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-crash",
    );
}

pub(super) fn verify_memory_process(client: &RemoteWorldServiceClient, mode: &str) {
    // This same-artifact child is a libtest worker (2 MiB), unlike the ordinary
    // application main thread. Debug restore owns large typed lineage values.
    // Retain real constructor/serving behavior while giving that test boundary
    // the ordinary process stack budget; do not alter production recovery.
    let client = client.clone();
    let mode = mode.to_owned();
    thread::Builder::new()
        .name("pre2-memory-application".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || verify_memory_process_inner(&client, &mode))
        .unwrap()
        .join()
        .unwrap();
}
fn verify_memory_process_inner(client: &RemoteWorldServiceClient, mode: &str) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    let store = std::env::var_os("PRE2_MEMORY_STORE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("native-memory-lineage.json"));
    let config = application_hosted::server_config(
        client,
        true,
        Duration::from_secs(60),
        Some(store.clone()),
    );
    let original_bytes = mode
        .starts_with("memory-reject-")
        .then(|| fs::read(&store).unwrap());
    let created = ViewerRuntimeLiveServer::new(config.clone());
    if matches!(
        mode,
        "memory-reject-response"
            | "memory-reject-intents"
            | "memory-reject-ack-content"
            | "memory-reject-ack-entry"
            | "memory-reject-ack-signature"
            | "memory-reject-ack-signer"
    ) {
        match created {
            Err(oasis7::viewer::ViewerRuntimeLiveServerError::Init(reason))
                if mode == "memory-reject-ack-signature" =>
            {
                assert!(
                    reason.starts_with("feedback ACK checkpoint signature invalid:"),
                    "specific actual signature refusal required, got {reason}"
                )
            }
            Err(oasis7::viewer::ViewerRuntimeLiveServerError::Init(reason))
                if mode == "memory-reject-ack-signer" =>
            {
                assert_eq!(reason, "feedback ACK checkpoint signer/authority mismatch")
            }
            Err(oasis7::viewer::ViewerRuntimeLiveServerError::Init(reason)) => assert_eq!(
                reason,
                if mode == "memory-reject-response" {
                    "pending cognition response artifact invalid: response_digest_mismatch: provider response digest does not match its content"
                } else if mode.starts_with("memory-reject-ack-") {
                    "durable private feedback acceptance binding mismatch"
                } else {
                    "pending cognition response or memory artifact mismatch"
                }
            ),
            Err(other) => panic!("unexpected original artifact restore rejection: {other:?}"),
            Ok(_) => {
                panic!("tampered original artifact must be rejected during constructor restore")
            }
        }
        let retained = fs::read(&store).unwrap();
        assert_eq!(
            Some(&retained),
            original_bytes.as_ref(),
            "constructor refusal must preserve exact private checkpoint bytes"
        );
        let disk: serde_json::Value = serde_json::from_slice(&retained).unwrap();
        assert_eq!(
            disk["provider_service_pending"].as_object().unwrap().len(),
            1
        );
        if !mode.starts_with("memory-reject-ack-") {
            assert_eq!(
                disk["provider_memory_store"],
                serde_json::to_value(oasis7::simulator::MemoryWriteStore::default()).unwrap(),
                "constructor refusal cannot release memory"
            );
        }
        println!(
            "native_memory_process_constructor_rejected kind={mode} private_bytes_unchanged=true memory_unchanged=true listener_started=false"
        );
        println!("PRE2_NATIVE_MEMORY_TAMPER_REJECTED");
        return;
    }
    let mut server = created.unwrap();
    if mode.starts_with("memory-crash") || mode.starts_with("memory-ack-") {
        let action = oasis7_simulator::Action::MoveAgent {
            agent_id: "agent-a".into(),
            to: "runtime:2:2:0".into(),
        };
        let context = server
            .test_prepare_canonical_provider_response_with_memory(
                "agent-a",
                action.clone(),
                vec![oasis7::simulator::MemoryWriteIntent {
                    scope: "session_private".into(),
                    summary: MEMORY.into(),
                    tags: vec!["process-recovery".into()],
                }],
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match server.test_queue_canonical_provider_response(context.clone(), action.clone()) {
                Ok(()) => break,
                Err(error)
                    if error.contains("pending")
                        || error.contains("unresolved")
                        || error.contains("outcome unknown") => {}
                Err(error) => panic!("native memory actor setup failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "native memory actor setup timeout"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    // Recovery constructs a new server from disk without installing an actor or polling a helper.
    let shared = Arc::new(Mutex::new(server));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    let serving = shared.clone();
    let worker =
        thread::spawn(move || ViewerRuntimeLiveServer::test_serve_shared_stream(serving, accepted));
    let mut requests = vec![
        serde_json::json!({"type":"hello_v2","client":"PRE2 native memory process","version":2,"capabilities":[]}),
        serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}),
        serde_json::json!({"type":"request_snapshot"}),
        serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":901}),
        serde_json::json!({"type":"hello_v2","client":"PRE2 memory after genuine Play","version":2,"capabilities":[]}),
        serde_json::json!({"type":"request_snapshot"}),
    ];
    if !mode.starts_with("memory-crash")
        && !matches!(mode, "memory-ack-before" | "memory-ack-after")
    {
        requests.insert(
            4,
            serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":902}),
        );
    }
    for request in requests {
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        socket.write_all(&bytes).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    if mode == "memory-ack-missing-runner" {
        while !root.join("feedback-ack-missing-runner-refused").exists()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            root.join("feedback-ack-missing-runner-refused").exists(),
            "actual missing runner gate not reached"
        );
        let summary = shared.lock().unwrap().test_canonical_provider_summary();
        assert_eq!(summary["pending_intent_count"], 1);
        assert_eq!(summary["native_model_call_count"], 1);
        let refusal: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("feedback-ack-missing-runner-refused")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            refusal["actual_error"],
            "feedback consumption native runner missing"
        );
        assert!(
            summary["memory_store"]["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        shutdown_application_socket(&socket);
        assert!(worker.join().unwrap().is_ok());
        println!(
            "PRE2_PRIVATE_MEMORY_ACK_MISSING_RUNNER_REFUSED pending_retained=true actual_native_runner_absent=true private_memory_empty=true"
        );
        return;
    }
    if matches!(mode, "memory-ack-before" | "memory-ack-after") {
        let marker = if mode == "memory-ack-before" {
            "world-feedback-ack-before-started"
        } else {
            "world-feedback-ack-after-started"
        };
        while !root.join(marker).exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            root.join(marker).exists(),
            "actual ACK crash boundary not reached"
        );
        let summary = shared.lock().unwrap().test_canonical_provider_summary();
        assert_eq!(summary["native_model_call_count"], 1);
        fs::write(
            root.join("feedback-ack-original-model-count.json"),
            serde_json::to_vec(&summary["native_model_call_count"]).unwrap(),
        )
        .unwrap();
        let bytes = fs::read(&store).unwrap();
        let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(checkpoint["provider_service_pending"]["agent-a"]["feedback_ack"].is_object());
        assert!(
            checkpoint["provider_memory_store"]
                .to_string()
                .contains(MEMORY)
        );
        println!(
            "PRE2_PRIVATE_MEMORY_ACK_CRASH actual_native_memory=true full_checkpoint_blake3={}",
            blake3::hash(&bytes)
        );
        std::io::stdout().flush().unwrap();
        std::process::exit(73);
    }
    if mode.starts_with("memory-crash") {
        while !root.join("world-settle-started").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        let reached = root.join("world-settle-started").exists();
        if reached {
            let bytes = fs::read(&store)
                .expect("explicit private lineage checkpoint must exist before Settle HTTP");
            println!(
                "native_memory_real_settle_crash_boundary=true private_checkpoint_blake3={} direct_poll_calls=0",
                blake3::hash(&bytes)
            );
            std::io::stdout().flush().unwrap();
            // Entire application process exits; parent retains only real canonical node/server.
            std::process::exit(73);
        }
        socket.shutdown(std::net::Shutdown::Both).unwrap();
        let result = worker.join().unwrap();
        panic!("native memory crash boundary not reached, actual serving result={result:?}");
    }
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let handshake_deadline = Instant::now() + Duration::from_secs(3);
    let mut hello_acks = 0;
    let mut handshake = false;
    let mut line = String::new();
    while Instant::now() < handshake_deadline {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if value["type"] == "hello_ack" {
                        hello_acks += 1;
                    }
                    if hello_acks >= 2 && value["type"] == "snapshot" {
                        handshake = true;
                        break;
                    }
                }
                line.clear();
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => break,
        }
    }
    println!("native_memory_actual_recovery_ordered_handshake={handshake} hello_acks={hello_acks}");
    let summary = loop {
        let summary = shared.lock().unwrap().test_canonical_provider_summary();
        if summary["terminal_states"]["agent-a"]["status"] == "committed"
            || (mode.starts_with("memory-reject-")
                && summary["hosted_service_memory_failure"] == "restored_memory_checkpoint_invalid")
            || Instant::now() >= deadline
        {
            break summary;
        }
        thread::sleep(Duration::from_millis(10));
    };
    shutdown_application_socket(&socket);
    let result = worker.join().unwrap();
    println!(
        "actual_native_model_call_count={}",
        summary["native_model_call_count"]
    );
    if matches!(mode, "memory-recover" | "memory-repeat") {
        assert_eq!(
            summary["native_model_call_count"], 0,
            "restored process must not call a fresh native provider"
        );
    }
    if mode == "memory-ack-write-failure" {
        assert_eq!(
            summary["native_model_call_count"], 1,
            "retry must not invoke original native model again"
        );
    }
    let memory = summary["memory_store"].to_string().contains(MEMORY);
    println!(
        "native_memory_actual_finalize_witness native_runner_present={} hosted_phase={} memory_failure={}",
        summary["native_runner_present"],
        summary["hosted_service_phase"],
        summary["hosted_service_memory_failure"]
    );
    println!(
        "native_memory_new_process_recovery terminal={} memory_present={memory} pending_intents={} worker_ok={} actor_reinstalled=false direct_poll_calls=0",
        summary["terminal_states"]["agent-a"]["status"],
        summary["pending_intent_count"],
        result.is_ok()
    );
    assert!(
        handshake,
        "actual memory recovery protocol handshake must finish before teardown"
    );
    assert!(
        result.is_ok(),
        "actual recovery serving worker failed: {result:?}"
    );
    if !mode.starts_with("memory-crash") {
        assert_eq!(
            shared.lock().unwrap().test_agent_service_pump_status()["play_enabled"],
            false,
            "issued recovery must finish after genuine Pause without admitting future work"
        );
        println!("native_memory_issued_recovery_actual_pause=true");
    }
    if mode.starts_with("memory-reject-") {
        assert_eq!(
            summary["hosted_service_memory_failure"], "restored_memory_checkpoint_invalid",
            "specific actual restored checkpoint rejection required, timeout alone is not evidence"
        );
        assert!(
            summary["hosted_service_phase"].is_null(),
            "invalid original must be fenced before downstream phase selection"
        );
        assert_eq!(summary["native_runner_present"], true);
        assert_ne!(summary["terminal_states"]["agent-a"]["status"], "committed");
        assert!(
            !memory,
            "tampered original checkpoint cannot release memory"
        );
        assert_eq!(summary["pending_intent_count"], 1);
        let disk: serde_json::Value = serde_json::from_slice(&fs::read(&store).unwrap()).unwrap();
        assert_eq!(
            disk["provider_service_pending"].as_object().unwrap().len(),
            1,
            "invalid original checkpoint must remain durably pending"
        );
        println!(
            "PRE2_NATIVE_MEMORY_TAMPER_REJECTED kind={mode} pending_retained=true memory_unchanged=true"
        );
        return;
    }
    assert_eq!(summary["terminal_states"]["agent-a"]["status"], "committed");
    assert!(
        memory,
        "verified original receipt must apply captured nonzero session_private memory after process loss"
    );
    assert_eq!(summary["pending_intent_count"], 0);
    let revision = blake3::hash(
        serde_json::to_string(&summary["memory_store"])
            .unwrap()
            .as_bytes(),
    )
    .to_string();
    println!("native_memory_revision_blake3={revision}");
    if mode == "memory-repeat" {
        assert_eq!(
            revision,
            std::env::var("PRE2_EXPECT_MEMORY_REVISION").unwrap(),
            "actual repeated process recovery must preserve memory revision exactly"
        );
    }
    let recreated = ViewerRuntimeLiveServer::new(config).unwrap();
    assert_eq!(
        recreated.test_canonical_provider_summary()["memory_store"],
        summary["memory_store"],
        "repeated recreation must preserve exact memory revision"
    );
    println!("PRE2_NATIVE_MEMORY_PROCESS_RECOVERY_PASSED");
}
