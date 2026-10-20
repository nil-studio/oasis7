use super::*;
use oasis7::world_service::{
    private_goal_prepare::sign_goal_prepare,
    private_goal_read::*,
    private_goal_store::{PreparedGoalReceiptV1, PrivateGoalStore},
};
use oasis7_agent_api::game::{
    DecimalU64, GAME_GOAL_PUBLISH_DOMAIN, GameGoalPrepareV1, GoalContentV1,
    game_goal_version_digest,
};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub(super) fn http(
    dir: &Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: Option<&PrivateGoalStore>,
    path: &str,
    body: &impl serde::Serialize,
    headers: &str,
) -> (u16, serde_json::Value) {
    http_with_hook(dir, runtime, signer, store, path, body, headers, || {})
}
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit fixture and deterministic delivery hook"
)]
pub(super) fn http_with_hook(
    dir: &Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: Option<&PrivateGoalStore>,
    path: &str,
    body: &impl serde::Serialize,
    headers: &str,
    before_delivery: impl FnOnce(),
) -> (u16, serde_json::Value) {
    let (status, body, _) = http_with_diagnostic_hook(
        dir,
        runtime,
        signer,
        store,
        path,
        body,
        headers,
        before_delivery,
    );
    (status, body)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit fixture, deterministic delivery hook, and bounded stage observation"
)]
pub(super) fn http_with_diagnostic_hook(
    dir: &Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: Option<&PrivateGoalStore>,
    path: &str,
    body: &impl serde::Serialize,
    headers: &str,
    before_delivery: impl FnOnce(),
) -> (u16, serde_json::Value, Option<&'static str>) {
    let body = serde_json::to_vec(body).unwrap();
    let mut bytes=format!("POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\n\r\n",body.len()).into_bytes();
    bytes.extend(body);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    client.write_all(&bytes).unwrap();
    let mut read = vec![0; bytes.len()];
    server.read_exact(&mut read).unwrap();
    let mut read_conflict_stage = None;
    let private = crate::world_service_api::private_goal::maybe_handle_with_delivery_hook(
        &mut server,
        &read,
        runtime,
        "POST",
        path,
        "w1",
        &dir.join("world"),
        &dir.join("records"),
        &dir.join("store"),
        signer,
        store,
        before_delivery,
        |stage| read_conflict_stage = Some(stage),
    )
    .unwrap();
    if !private {
        assert!(
            crate::world_service_api::maybe_handle(
                &mut server,
                &read,
                runtime,
                "POST",
                path,
                "w1",
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                signer
            )
            .unwrap()
        );
    }
    drop(server);
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    if private {
        assert!(response.contains("Cache-Control: no-store"));
        assert!(!response.contains("Access-Control-Allow-Origin"));
    }
    (
        status,
        serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap(),
        read_conflict_stage,
    )
}
pub(super) fn consumer<T>(
    dir: &Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &PrivateGoalStore,
    world: &WorldIdentity,
    call: impl FnOnce(&oasis7::world_service::client::RemoteWorldServiceClient) -> T,
) -> T {
    // Functional recovery requests rebuild canonical worlds and fsync private
    // records, so normal consumer fixtures retain a bounded 30s response budget.
    let (result, outcome) = consumer_with_delivery_hook(
        dir,
        runtime,
        signer,
        store,
        world,
        std::time::Duration::from_secs(30),
        || {},
        call,
    );
    assert_eq!(outcome, "response_written");
    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "Real private client, isolated server, and deterministic delivery delay"
)]
fn consumer_with_delivery_hook<T>(
    dir: &Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &PrivateGoalStore,
    world: &WorldIdentity,
    request_timeout: std::time::Duration,
    before_delivery: impl FnOnce() + Send,
    call: impl FnOnce(&oasis7::world_service::client::RemoteWorldServiceClient) -> T,
) -> (T, &'static str) {
    use oasis7::world_service::client::{RemoteWorldServiceClient, WorldServiceClientConfig};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        endpoint: format!("http://{}", listener.local_addr().unwrap()),
        trusted_service_public_key: signer.public_key_hex.clone(),
        expected_world: world.clone(),
        scope_id: "agent:agent-a".into(),
        read_private_key_hex: signer.private_key_hex.clone(),
        timeout: request_timeout,
        formal_read_trust: None,
        max_response_bytes: 65536,
    })
    .unwrap();
    std::thread::scope(|scope| {
        let serving = scope.spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut byte = [0; 1];
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 65536);
            }
            let headers = std::str::from_utf8(&bytes).unwrap();
            let path = headers.split_whitespace().nth(1).unwrap().to_owned();
            let length = headers
                .split("\r\n")
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse::<usize>()
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            bytes.extend(body);
            let request_started = std::time::Instant::now();
            let route = if path == GOAL_CURRENT_PATH {
                "goal_current"
            } else if path == GOAL_PREPARE_PATH {
                "goal_prepare"
            } else {
                "other"
            };
            let handled = crate::world_service_api::private_goal::maybe_handle_with_delivery_hook(
                &mut stream,
                &bytes,
                runtime,
                "POST",
                &path,
                "w1",
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                signer,
                Some(store),
                before_delivery,
                |stage| {
                    eprintln!("TASK_PRIVATE_CONTEXT_REJECT route={route} stage={stage}");
                },
            );
            let outcome = match handled {
                Ok(true) => "response_written",
                Ok(false) => "unhandled",
                Err(error) if error == "private response write failed" => "write_failed",
                Err(_) => "handler_failed",
            };
            eprintln!(
                "TASK_PRIVATE_HTTP_FIXTURE route={route} outcome={outcome} elapsed_ms={} client_timeout_ms={}",
                request_started.elapsed().as_millis(),
                request_timeout.as_millis()
            );
            outcome
        });
        let result = call(&client);
        let outcome = serving.join().unwrap();
        (result, outcome)
    })
}
#[test]
fn private_goal_native_tcp_prepare_publish_read_restart_and_recovery() {
    let (dir, mut driver, identity, owner, public) = canonical_agent_chat::driver_fixture();
    let create = game_task_tests::create(&identity, &owner, "native-goal-task");
    commit(&mut driver, 3, create);
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private_dir = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private_dir, [7; 32]).unwrap();
    let content = GoalContentV1 {
        goal_text: "native confidential 完整目标".into(),
        soft_constraints: vec!["first".into(), "second".into()],
    };
    let proof = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: control.agent_id.clone(),
            owner_public_key: public.clone(),
            owner_binding: control.owner_binding.clone(),
            owner_generation: control.owner_generation,
            branch_id: control.branch_id.clone(),
            reorg_epoch: control.reorg_epoch,
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "native-prepare".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(1), &content)
                .unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: public.clone(),
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let first = consumer(&dir, &runtime, &signer, &store, &identity, |client| {
        client.prepare_goal(&proof).unwrap()
    });
    // Discard the response and recover from disk using the same operation/proof.
    drop(store);
    let store = PrivateGoalStore::open(&private_dir, [7; 32]).unwrap();
    let (status, replay) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        GOAL_PREPARE_PATH,
        &proof,
        "",
    );
    assert_eq!(status, 200);

    let receipt: SignedServiceResponse<PreparedGoalReceiptV1> =
        serde_json::from_value(replay).unwrap();
    verify_prepare_response(&proof, &receipt, &public).unwrap();
    assert!(first == receipt.payload);
    let publish = publication(&proof, &receipt.payload, "native-publish".into()).unwrap();
    let submit = request(
        &identity,
        WorldServicePayloadV1::GameGoalPublish(
            sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, &owner).unwrap(),
        ),
    );
    let (status, received) = http(&dir, &runtime, &signer, None, SUBMIT_PATH, &submit, "");
    assert_eq!(status, 200);
    let received: SignedServiceResponse<IntentResponse<serde_json::Value>> =
        serde_json::from_value(received).unwrap();
    assert!(matches!(
        received.payload.outcome,
        IntentOutcome::Received { .. }
    ));
    commit(&mut driver, 4, submit.clone());
    let public_snapshot = serde_json::to_string(&driver.execution_world.snapshot()).unwrap();
    for secret in [
        &proof.request.content.goal_text,
        &proof.request.goal_digest.0,
        &proof.signature_hex,
        &proof.request.client_nonce,
        &receipt.payload.reference_salt,
    ] {
        assert!(
            !public_snapshot.contains(secret.as_str()),
            "private material must not enter canonical state"
        );
    }

    assert!(
        game_task_tests::recorded(&driver, &submit)
            .rejected
            .is_none()
    );
    let read = sign_read_request(
        GOAL_CURRENT_PATH,
        CurrentGoalRequest {
            schema_revision: 1,
            world: identity.clone(),
            agent_id: "agent-a".into(),
            task_id: control.task_id.clone(),
        },
        &owner,
    )
    .unwrap();
    let (status, value) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        GOAL_CURRENT_PATH,
        &read,
        "",
    );
    assert_eq!(status, 200);
    let response: SignedServiceResponse<CurrentGoalResponse> =
        serde_json::from_value(value).unwrap();
    verify_current_response(&read, &response, &public).unwrap();
    let consumed = consumer(&dir, &runtime, &signer, &store, &identity, |client| {
        client.read_current_goal(&read).unwrap()
    });
    assert!(consumed == response.payload);
    let (timed_out, server_outcome) = consumer_with_delivery_hook(
        &dir,
        &runtime,
        &signer,
        &store,
        &identity,
        std::time::Duration::from_millis(100),
        || std::thread::sleep(std::time::Duration::from_millis(250)),
        |client| client.read_current_goal(&read),
    );
    assert!(matches!(
        timed_out,
        Err(oasis7::world_service::client::WorldServiceClientError::Transport(ref reason))
            if reason == "private transport timed out"
    ));
    assert!(
        matches!(server_outcome, "response_written" | "write_failed"),
        "a timed-out client leaves server delivery unknown"
    );

    assert_eq!(response.payload.activation_commit.unwrap().position, 4);
    assert_eq!(
        response
            .payload
            .active_goal
            .unwrap()
            .owner_proof
            .request
            .content,
        proof.request.content
    );
    let (status, _) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        GOAL_PREPARE_PATH,
        &proof,
        "",
    );
    assert_eq!(status, 200, "original prepare retry after publication");
    let mut conflict = proof.request.clone();
    conflict.content.goal_text = "different".into();
    conflict.goal_digest =
        game_goal_version_digest(&conflict.task_id, conflict.goal_revision, &conflict.content)
            .unwrap();
    let conflict = sign_goal_prepare(conflict, &owner).unwrap();
    let (status, value) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        GOAL_PREPARE_PATH,
        &conflict,
        "",
    );
    assert_eq!(status, 409);
    assert!(!value.to_string().contains("different"));
    for header in [
        "Origin: https://evil.invalid\r\n",
        "Authorization: secret\r\n",
        "Forwarded: for=127.0.0.1\r\n",
    ] {
        assert_eq!(
            http(
                &dir,
                &runtime,
                &signer,
                Some(&store),
                GOAL_CURRENT_PATH,
                &read,
                header
            )
            .0,
            403
        );
    }
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            &format!("{GOAL_CURRENT_PATH}?token=secret"),
            &read,
            ""
        )
        .0,
        403
    );
    for header in [
        " Origin: https://evil.invalid\r\n",
        "Origin : https://evil.invalid\r\n",
        "\tAuthorization: secret\r\n",
        "Content-Length: 1\r\n",
        "Transfer-Encoding: chunked\r\n",
        "X-Test: value\r\n folded\r\n",
    ] {
        assert_eq!(
            http(
                &dir,
                &runtime,
                &signer,
                Some(&store),
                GOAL_CURRENT_PATH,
                &read,
                header
            )
            .0,
            400
        );
    }
    let outsider = sign_read_request(
        GOAL_CURRENT_PATH,
        read.request.clone(),
        &hex::encode([5; 32]),
    )
    .unwrap();
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            GOAL_CURRENT_PATH,
            &outsider,
            ""
        )
        .0,
        403
    );
    for record in fs::read_dir(&private_dir).unwrap() {
        let record = record.unwrap().path();
        if record
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("prepared-")
        {
            fs::remove_file(record).unwrap();
        }
    }
    let (status, value) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        GOAL_CURRENT_PATH,
        &read,
        "",
    );
    assert_eq!(status, 503);
    assert_eq!(value["error"], "private_goal_recovery_required");
    assert!(!value.to_string().contains(&proof.request.content.goal_text));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn private_goal_native_owner_and_reorg_changes_before_delivery_are_fenced() {
    for phase in [GOAL_PREPARE_PATH, GOAL_CURRENT_PATH] {
        for reorg in [false, true] {
            let (dir, mut driver, identity, owner, public) = canonical_agent_chat::driver_fixture();
            if reorg {
                let policy=serde_json::from_value(serde_json::json!({
                    "schema_version":"scheduler-policy.v1", "max_total_wakes_per_tick":8,
                    "max_wakes_per_agent_per_tick":1, "aging_after_ticks":2,
                    "max_starvation_ticks":4, "initial_priority":0,
                    "comparator":"deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
                    "service_order":"stable_round_robin"
                })).unwrap();
                driver.execution_world = driver
                    .execution_world
                    .clone()
                    .with_cognition_scheduler(policy, 1);
            }
            let create = game_task_tests::create(&identity, &owner, "fresh-fence-task");
            commit(&mut driver, 3, create.clone());
            let control = game_task::current_control(&driver.execution_world, "agent-a")
                .unwrap()
                .unwrap();
            let private = dir.canonicalize().unwrap().join("private-goals");
            let store = PrivateGoalStore::open(&private, [8; 32]).unwrap();
            let content = GoalContentV1 {
                goal_text: "never deliver after ownership or reorg change".into(),
                soft_constraints: vec![],
            };
            let proof = sign_goal_prepare(
                GameGoalPrepareV1 {
                    api_rev: 1,
                    world_id: identity.world_id.clone(),
                    genesis_digest: identity.genesis_digest.clone(),
                    task_id: control.task_id.clone(),
                    agent_id: control.agent_id.clone(),
                    owner_public_key: public.clone(),
                    owner_binding: control.owner_binding.clone(),
                    owner_generation: control.owner_generation,
                    branch_id: control.branch_id.clone(),
                    reorg_epoch: control.reorg_epoch,
                    expected_task_revision: control.task_revision,
                    expected_latest_goal_revision: control.latest_goal_revision,
                    goal_revision: DecimalU64(1),
                    client_operation_id: "fresh-prepare".into(),
                    client_nonce: "cd".repeat(32),
                    goal_digest: game_goal_version_digest(
                        &control.task_id,
                        DecimalU64(1),
                        &content,
                    )
                    .unwrap(),
                    content,
                },
                &owner,
            )
            .unwrap();
            if phase == GOAL_CURRENT_PATH {
                let receipt = store.prepare(&proof).unwrap();
                let public_goal = publication(&proof, &receipt, "fresh-publish".into()).unwrap();
                commit(
                    &mut driver,
                    4,
                    request(
                        &identity,
                        WorldServicePayloadV1::GameGoalPublish(
                            sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, public_goal, &owner)
                                .unwrap(),
                        ),
                    ),
                );
            }
            let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
                private_key_hex: owner.clone(),
                public_key_hex: public.clone(),
            };
            let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
                oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
            )));
            let read = sign_read_request(
                GOAL_CURRENT_PATH,
                CurrentGoalRequest {
                    schema_revision: 1,
                    world: identity.clone(),
                    agent_id: "agent-a".into(),
                    task_id: control.task_id,
                },
                &owner,
            )
            .unwrap();
            let body = if phase == GOAL_PREPARE_PATH {
                serde_json::to_value(&proof).unwrap()
            } else {
                serde_json::to_value(&read).unwrap()
            };
            let (status, value) = http_with_hook(
                &dir,
                &runtime,
                &signer,
                Some(&store),
                phase,
                &body,
                "",
                || {
                    if reorg {
                        driver
                            .execution_world
                            .invalidate_cognition_for_reorg(1)
                            .unwrap();
                        driver
                            .execution_world
                            .bind_cognition_runtime("w1", "main", 0, None, "pending", 1)
                            .unwrap();
                    } else {
                        driver
                            .execution_world
                            .install_capability_agent_identity("agent-a", "owner-a", 2)
                            .unwrap();
                    }
                    commit(
                        &mut driver,
                        if phase == GOAL_PREPARE_PATH { 4 } else { 5 },
                        create.clone(),
                    );
                },
            );
            assert!(status == 403 || status == 409, "must fence before delivery");
            assert!(!value.to_string().contains(&proof.request.content.goal_text));
            assert!(value.get("payload").is_none());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
