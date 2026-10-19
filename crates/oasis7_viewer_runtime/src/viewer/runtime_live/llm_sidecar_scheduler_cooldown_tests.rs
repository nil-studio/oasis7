use super::*;

#[test]
fn scheduler_query_cooldown_survives_client_recreation_without_submit_replay() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "scheduler sent no HTTP request before bounded accept deadline"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("scheduler fixture accept failed: {error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0u8; 4096];
            let count = socket.read(&mut chunk).unwrap();
            assert!(count > 0 && bytes.len() + count <= 65536);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        assert!(String::from_utf8_lossy(&bytes).starts_with("POST /v1/world/submit "));
        socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let mut context = test_provider_context("agent-1", "cooldown-turn", "cooldown-request", 1);
    let request = &mut context.request_context;
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.observation.v1",
        &request.base_decision_request.observation,
    );
    request.capability_catalog_digest =
        crate::simulator::h_v1("oasis7.cognition.test.catalog.v1", &Option::<()>::None);
    request.capability_invocation_context_digest =
        crate::simulator::h_v1("oasis7.cognition.test.invocation.v1", &Option::<()>::None);
    request.memory_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.memory.v1",
        &context.turn_context.memory_snapshot,
    );
    request.goal_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.goal.v1",
        &context.turn_context.goal_snapshot,
    );
    request.continuation_digest =
        crate::simulator::h_v1("oasis7.cognition.test.continuation.v1", &Option::<()>::None);
    request.runtime_binding.base_world_hash =
        crate::simulator::h_v1("oasis7.cognition.test.world.v1", &"cooldown");
    request.runtime_binding.runtime_manifest_hash =
        crate::simulator::h_v1("oasis7.cognition.test.manifest.v1", &"cooldown");
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_service_projection =
        Some(crate::world_service::projection::WorldServiceProjection {
            formal_commit_header: None,
            canonical_agent_owner: None,
            canonical_agent_chat: None,
            game_task_control: None,
            native_delegation_authorizations: Vec::new(),
            state: crate::runtime_host::world::new().state().clone(),
            events: vec![],
            runtime_binding: Some(context.request_context.runtime_binding.clone()),
            agent_context: None,
            feedback_history: None,
            scheduler_wakes: vec![],
            continuations: vec![],
            cognition_leases: vec![],
            continuation_contexts: BTreeMap::new(),
        });
    sidecar.provider_service_config =
        Some(crate::world_service::client::WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint,
            trusted_service_public_key: crate::world_service::authority::sign_read_request(
                "cooldown-test",
                (),
                &"11".repeat(32),
            )
            .unwrap()
            .subject_public_key,
            expected_world: oasis7_client_api::world_service::WorldIdentity {
                world_id: context.request_context.runtime_binding.world_id.clone(),
                genesis_digest: "fixture-genesis".into(),
            },
            scope_id: "agent:agent-1".into(),
            read_private_key_hex: "11".repeat(32),
            timeout: std::time::Duration::from_secs(2),
            max_response_bytes: 4096,
        });
    sidecar.provider_service_signer = Some(
        crate::world_service::client::WorldServiceAgentSignerConfig {
            private_key_hex: "11".repeat(32),
            delegation_generation: 1,
        },
    );
    let operation = crate::world_service::wire::SchedulerOperationV1::ReleaseLease {
        lease_id: "original-lease".into(),
    };
    // A submitted overload is ambiguous; retain its exact signed checkpoint.
    let first_error = sidecar
        .provider_scheduler_operation(&context.request_context, "release", operation.clone())
        .unwrap_err();
    eprintln!(
        "scheduler first operation error: {}",
        first_error.chars().take(512).collect::<String>()
    );
    assert!(
        worker.join().is_ok(),
        "bounded HTTP fixture failed; first operation error: {first_error}"
    );
    assert!(
        first_error.to_ascii_lowercase().contains("outcome unknown"),
        "unexpected first operation error: {first_error}"
    );
    let before = serde_json::to_value(&sidecar.provider_scheduler_pending).unwrap();
    let error = sidecar
        .provider_scheduler_operation(&context.request_context, "release", operation)
        .unwrap_err();
    assert!(
        error.to_ascii_lowercase().contains("cooldown"),
        "unexpected retry error: {error}"
    );
    assert_eq!(
        serde_json::to_value(&sidecar.provider_scheduler_pending).unwrap(),
        before
    );
}
