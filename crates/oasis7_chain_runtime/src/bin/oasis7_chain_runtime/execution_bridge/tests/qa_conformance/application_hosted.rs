//! Native provider actor installation followed only by the real hosted serving loop.
use super::*;
use oasis7::viewer::{
    ControlCompletionStatus, ViewerLiveDecisionMode, ViewerResponse, ViewerRuntimeLiveServer,
    ViewerRuntimeLiveServerConfig,
};
use oasis7::world_service::client::WorldServiceAgentSignerConfig;
use oasis7_simulator::WorldScenario;
use std::io::{BufRead, BufReader, Write};

#[test]
fn real_tcp_hosted_serving_loop_drives_registered_provider_to_canonical_receipt() {
    run_isolated_application(false, false, false, true, false, false);
}

pub(super) fn verify_hosted(client: &RemoteWorldServiceClient) {
    let server = prepare_server(client);
    application_hosted_unpublished_cadence::child_ready();
    verify_hosted_server(server);
}

pub(super) fn verify_hosted_server(server: ViewerRuntimeLiveServer) {
    let shared = Arc::new(Mutex::new(server));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client_socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (accepted, _) = listener.accept().unwrap();
    let serving = shared.clone();
    let worker =
        thread::spawn(move || ViewerRuntimeLiveServer::test_serve_shared_stream(serving, accepted));
    for request in [
        serde_json::json!({"type":"hello_v2","client":"PRE2 hosted Agent QA","version":2,"capabilities":[]}),
        serde_json::json!({"type":"subscribe","streams":["snapshot","events"],"event_kinds":[]}),
        serde_json::json!({"type":"request_snapshot"}),
        serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":701}),
        serde_json::json!({"type":"hello_v2","client":"PRE2 after genuine Play","version":2,"capabilities":[]}),
        serde_json::json!({"type":"request_snapshot"}),
    ] {
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        client_socket.write_all(&bytes).unwrap();
    }
    client_socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut reader = BufReader::new(client_socket.try_clone().unwrap());
    let read_deadline = Instant::now() + Duration::from_secs(3);
    let mut hello_acks = 0;
    let mut play_processed = false;
    let mut play_blocked = false;
    let mut line = String::new();
    while Instant::now() < read_deadline {
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if !line.ends_with('\n') {
                    continue;
                }
                if let Ok(ViewerResponse::ControlCompletionAck { ack }) =
                    serde_json::from_str::<ViewerResponse>(&line)
                    && ack.request_id == 701
                    && ack.status == ControlCompletionStatus::Blocked
                {
                    play_blocked = true;
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if value["type"] == "hello_ack" {
                        hello_acks += 1;
                    }
                    if hello_acks >= 2 && value["type"] == "snapshot" {
                        play_processed = true;
                        line.clear();
                        break;
                    }
                }
                line.clear();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => break,
        }
    }
    println!(
        "hosted_genuine_live_play_ordered_snapshot={play_processed} hello_acks={hello_acks} request701_blocked={play_blocked}"
    );
    let drain_stop = Arc::new(AtomicBool::new(false));
    let reader_stop = drain_stop.clone();
    let draining = thread::spawn(move || {
        while !reader_stop.load(Ordering::SeqCst) {
            match reader.read_line(&mut line) {
                Ok(0) => return Ok(()),
                Ok(_) => {
                    if line.ends_with('\n') {
                        line.clear();
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) => {}
                Err(error) => return Err(error.kind()),
            }
        }
        Ok(())
    });
    let eligibility_deadline = application_completion::child_deadline();
    let eligibility = loop {
        if let Ok(server) = shared.try_lock() {
            break server.test_agent_service_pump_status();
        }
        if Instant::now() >= eligibility_deadline {
            break serde_json::Value::Null;
        }
        thread::sleep(Duration::from_millis(10));
    };
    println!("hosted_actual_eligibility={eligibility}");
    let mut completion = application_completion::CompletionWatch::new();
    let mut summary = serde_json::Value::Null;
    let mut pending_observation = None;
    let summary = loop {
        if let Ok(server) = shared.try_lock() {
            summary = server.test_canonical_provider_diagnostic_summary();
        }
        application_hosted_unpublished_cadence::observe_child(&summary, &mut pending_observation);
        if completion.finished(
            &summary,
            application_completion::canonical_complete(&summary),
        ) {
            break summary;
        }
        thread::sleep(Duration::from_millis(20));
    };
    client_socket.shutdown(std::net::Shutdown::Both).unwrap();
    let joined = worker.join().unwrap();
    drain_stop.store(true, Ordering::SeqCst);
    let drained = draining.join().unwrap();
    println!(
        "hosted_serving_witness worker_ok={} terminal_status={} pending_actions={} pending_intents={} direct_poll_calls=0",
        joined.is_ok(),
        summary["terminal_states"]["agent-a"]["status"],
        summary["pending_action_count"],
        summary["pending_intent_count"]
    );
    if summary["terminal_states"]["agent-a"]["status"] != "committed" {
        println!(
            "hosted_failed_terminal_diagnostic={}",
            serde_json::json!({
                "hosted_service_phase": summary["hosted_service_phase"],
                "hosted_service_inflight": summary["hosted_service_inflight"],
                "hosted_service_memory_failure": summary["hosted_service_memory_failure"],
                "agent_service_pump_error": summary["agent_service_pump_error"],
                "pending_intent_count": summary["pending_intent_count"],
                "pending_action_count": summary["pending_action_count"],
                "native_model_call_count": summary["native_model_call_count"],
                "mirrored_lease_count": summary["mirrored_lease_count"],
            })
        );
    }
    assert!(
        play_processed,
        "genuine LiveControl Play was not followed by ordered protocol response"
    );
    assert!(
        !play_blocked,
        "genuine LiveControl Play request701 was blocked"
    );
    assert_eq!(
        eligibility["eligible"], true,
        "actual hosted eligibility prerequisite was not satisfied"
    );
    assert!(joined.is_ok(), "actual serving loop failed: {joined:?}");
    assert!(
        drained.is_ok(),
        "actual hosted output drain failed: {drained:?}"
    );
    assert!(
        !application_completion::has_failure(&summary),
        "actual canonical completion error: {summary}"
    );
    assert_eq!(
        summary["terminal_states"]["agent-a"]["status"], "committed",
        "eligible hosted serving loop must drive registered native provider to real receipt"
    );
    application_completion::save_completion(&summary);
    println!("PRE2_HOSTED_NATIVE_PROVIDER_CANONICAL_RECEIPT_PASSED direct_poll_calls=0");
}

pub(super) fn prepare_server(client: &RemoteWorldServiceClient) -> ViewerRuntimeLiveServer {
    prepare_server_for_policy(client, true, Duration::from_millis(200))
}

pub(super) fn prepare_server_for_policy(
    client: &RemoteWorldServiceClient,
    autoplay: bool,
    interval: Duration,
) -> ViewerRuntimeLiveServer {
    let mut server = ViewerRuntimeLiveServer::new(server_config(
        client,
        autoplay,
        interval,
        Some(
            std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap())
                .join("provider-lineage.json"),
        ),
    ))
    .unwrap();
    let action = oasis7_simulator::Action::MoveAgent {
        agent_id: "agent-a".into(),
        to: "runtime:2:2:0".into(),
    };
    let cognition = server
        .test_prepare_canonical_provider_response("agent-a", action.clone())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match server.test_queue_canonical_provider_response(cognition.clone(), action.clone()) {
            Ok(()) => break,
            Err(e)
                if e.contains("pending")
                    || e.contains("unresolved")
                    || e.contains("outcome unknown") => {}
            Err(e) => panic!("native actor installation failed: {e}"),
        }
        assert!(Instant::now() < deadline, "native actor install timeout");
        thread::sleep(Duration::from_millis(10));
    }
    server
}

pub(super) fn server_config(
    client: &RemoteWorldServiceClient,
    autoplay: bool,
    interval: Duration,
    lineage_store: Option<std::path::PathBuf>,
) -> ViewerRuntimeLiveServerConfig {
    let mut connection = client.config().clone();
    connection.scope_id = "agent:agent-a".into();
    let mut config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
    config.world_id = "w1".into();
    config.world_service = Some(connection);
    config.world_service_agent_signer = Some(WorldServiceAgentSignerConfig {
        private_key_hex: hex::encode([8u8; 32]),
        delegation_generation: 1,
    });
    config.decision_mode = ViewerLiveDecisionMode::Llm;
    config.auto_play_on_connect = autoplay;
    config.chain_poll_interval = interval;
    config.provider_lineage_store = lineage_store;
    config
}
