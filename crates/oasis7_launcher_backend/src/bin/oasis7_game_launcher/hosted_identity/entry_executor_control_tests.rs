use super::super::participant_entry::{EntryTrust, ParticipantEntry};
use super::*;
use oasis7_client_api::world_service_wire::{
    AuthenticatedLookup, SchedulerIntentV1, TaskSchedulerIntentV1,
};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn key(seed: u8) -> String {
    sign_read_request("fixture", (), &hex::encode([seed; 32]))
        .unwrap()
        .subject_public_key
}
fn world() -> WorldIdentity {
    WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    }
}
fn control() -> GameTaskControlV1 {
    serde_json::from_value(json!({"schema_revision":1,"task_id":"task-a","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"3","agent_id":"agent-a","owner_binding":"owner-a","owner_generation":"2","owner_public_key":key(93),"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"ab".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"1"},"executor_epoch":"2","executor_id":"executor-a","executor_binding":{"executor_id":"executor-a","executor_public_key":key(92),"delegation_generation":"4","executor_epoch":"2"},"created_operation_id":"task-create"})).unwrap()
}
fn access(fence: GameTaskFenceV1, seed: u8) -> EntryExecutorAccessV1 {
    EntryExecutorAccessV1 {
        read: sign_read_request(
            VIEW_JSON_PATH,
            ReadWorldViewRequest {
                contract_version: 1,
                observation_nonce: Some("c".repeat(64)),
                formal_anchor: None,
                world: world(),
                scope_id: format!("agent:{}", fence.agent_id),
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            },
            &hex::encode([seed; 32]),
        )
        .unwrap(),
        fence,
    }
}
fn request() -> EntryExecutorControlRequestV1 {
    EntryExecutorControlRequestV1 {
        access: access(GameTaskFenceV1::from_control(&control()), 92),
        operation: EntryExecutorOperationV1::Describe(
            sign_read_request(
                DESCRIBE_PATH,
                DescribeWorldRequest {
                    contract_version: 1,
                    expected_world: world(),
                    trust_config_ref: key(94),
                },
                &hex::encode([92; 32]),
            )
            .unwrap(),
        ),
    }
}
fn entry(endpoint: String, enabled: bool) -> ParticipantEntry {
    let mut entry = ParticipantEntry::new(
        EntryTrust {
            endpoint,
            service_public_key: key(94),
            world: world(),
        },
        zeroize::Zeroizing::new(hex::encode([95; 32])),
    )
    .unwrap();
    assert!(
        !entry.executor_control_enabled,
        "new deployments must remain closed"
    );
    entry.executor_control_enabled = enabled;
    entry
}
fn rejected<T>(result: Result<T, StoreError>, expected: &str) {
    assert!(
        matches!(result,Err(StoreError::Rejected(ref code)) if code==expected),
        "unexpected result for {expected}"
    );
}
#[test]
fn executor_gateway_is_closed_by_default_and_requires_session_and_post() {
    let mut f = IdentityService::model_executor_test_fixture();
    let now = super::super::now_ms();
    rejected(
        f.service
            .entry_executor_route(
                "POST",
                "/entry-executor/control",
                Some(&f.token),
                &json!({}),
                now,
            )
            .unwrap(),
        "executor_control_unavailable",
    );
    f.service.participant_entry = Some(entry("http://127.0.0.1:1".into(), false));
    rejected(
        f.service
            .entry_executor_route(
                "POST",
                "/entry-executor/control",
                Some(&f.token),
                &json!({}),
                now,
            )
            .unwrap(),
        "executor_control_unavailable",
    );
    rejected(
        f.service
            .entry_executor_route("POST", "/entry-executor/control", None, &json!({}), now)
            .unwrap(),
        "session_expired",
    );
    rejected(
        f.service
            .entry_executor_route(
                "GET",
                "/entry-executor/control",
                Some(&f.token),
                &json!({}),
                now,
            )
            .unwrap(),
        "method_not_allowed",
    );
    assert!(
        f.service
            .entry_executor_route("POST", "/other", Some(&f.token), &json!({}), now)
            .is_none()
    );
}
#[test]
fn malformed_unscoped_and_owner_actor_requests_fail_before_upstream() {
    let mut f = IdentityService::model_executor_test_fixture();
    f.service.participant_entry = Some(entry("http://127.0.0.1:1".into(), true));
    let now = super::super::now_ms();
    for body in [
        json!({}),
        json!({"access":{},"operation":{"kind":"owner_control","request":{}}}),
        json!({"padding":"x".repeat(65536)}),
    ] {
        rejected(
            f.service
                .entry_executor_route(
                    "POST",
                    "/entry-executor/control",
                    Some(&f.token),
                    &body,
                    now,
                )
                .unwrap(),
            "invalid_request",
        );
    }
    let original = request();
    for bad in [
        {
            let mut bad = original.clone();
            bad.access = access(bad.access.fence.clone(), 93);
            bad
        },
        {
            let mut bad = original.clone();
            let mut read = bad.access.read.request.clone();
            read.scope_id = "world".into();
            bad.access.read =
                sign_read_request(VIEW_JSON_PATH, read, &hex::encode([92; 32])).unwrap();
            bad
        },
        {
            let mut bad = original.clone();
            let mut read = bad.access.read.request.clone();
            read.observation_nonce = None;
            bad.access.read =
                sign_read_request(VIEW_JSON_PATH, read, &hex::encode([92; 32])).unwrap();
            bad
        },
        {
            let mut bad = original.clone();
            bad.access.fence.genesis_digest = "another-genesis".into();
            bad
        },
    ] {
        rejected(
            f.service
                .entry_executor_route(
                    "POST",
                    "/entry-executor/control",
                    Some(&f.token),
                    &serde_json::to_value(bad).unwrap(),
                    now,
                )
                .unwrap(),
            "executor_context_mismatch",
        );
    }
}
fn scheduler(fence: GameTaskFenceV1) -> WorldServicePayloadV1 {
    let request: TaskSchedulerIntentV1 = SchedulerIntentV1 {
        agent_id: fence.agent_id.clone(),
        request_id: "original-prefix".into(),
        delegation_generation: fence
            .executor_binding
            .as_ref()
            .unwrap()
            .delegation_generation
            .0,
        captured_base_binding: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1 {
            world_id: fence.world_id.clone(),
            branch_id: fence.branch_id.clone(),
            finality_epoch: 1,
            finality_block_hash: None,
            finality_status: "provisional".into(),
            base_tick: 0,
            base_world_hash: format!("blake3:{}", "a".repeat(64)),
            reorg_epoch: fence.reorg_epoch.0,
            runtime_manifest_hash: format!("blake3:{}", "b".repeat(64)),
        },
        operation: SchedulerOperationV1::TaskProviderPrefix {
            context: TaskContextIdentityV1 {
                schema_revision: 1,
                fence,
                agent_session_id: "session".into(),
                agent_turn_id: "turn".into(),
                decision_request_id: "decision".into(),
                retry_seq: 1,
                transport_attempt: 1,
                request_commitment: format!("blake3:{}", "1".repeat(64)),
                context_commitment: format!("blake3:{}", "2".repeat(64)),
                provider_invocation_commitment: format!("blake3:{}", "3".repeat(64)),
                valid_until_tick: DecimalU64(16),
            },
        },
    };
    WorldServicePayloadV1::Scheduler(
        sign_read_request("scheduler", request, &hex::encode([92; 32])).unwrap(),
    )
}
#[test]
fn executor_dispatch_denies_owner_intents_and_checks_current_delegation_and_complete_fence() {
    let current = GameTaskFenceV1::from_control(&control());
    let actor = key(92);
    let good = scheduler(current.clone());
    assert!(check_payload(&good, &actor, &current, false).is_ok());
    let mut wrong_generation = good.clone();
    if let WorldServicePayloadV1::Scheduler(s) = &mut wrong_generation {
        s.request.delegation_generation += 1;
    }
    rejected(
        check_payload(&wrong_generation, &actor, &current, false),
        "executor_context_mismatch",
    );
    let mut wrong_task = current.clone();
    wrong_task.task_revision.0 += 1;
    rejected(
        check_payload(&good, &actor, &wrong_task, false),
        "executor_context_mismatch",
    );
    rejected(
        check_payload(&good, &key(93), &current, false),
        "executor_context_mismatch",
    );
    let owner = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "owner-task".into(),
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(3),
                agent_id: "agent-a".into(),
                owner_binding: "owner-a".into(),
                owner_generation: DecimalU64(2),
            },
            &hex::encode([93; 32]),
        )
        .unwrap(),
    );
    rejected(
        check_payload(&owner, &actor, &current, false),
        "executor_operation_not_allowed",
    );
    rejected(
        check_payload(&owner, &actor, &current, true),
        "executor_operation_not_allowed",
    );
    rejected(
        check_payload(
            &WorldServicePayloadV1::GameplayJson(vec![]),
            &actor,
            &current,
            false,
        ),
        "executor_operation_not_allowed",
    );
}
#[test]
fn historical_lookup_preserves_original_fence_but_cannot_switch_stable_resource_or_signer() {
    let current = GameTaskFenceV1::from_control(&control());
    let actor = key(92);
    let mut old = current.clone();
    old.branch_id = "old-branch".into();
    old.reorg_epoch = DecimalU64(1);
    old.task_revision = DecimalU64(2);
    old.executor_epoch = DecimalU64(1);
    let binding = old.executor_binding.as_mut().unwrap();
    binding.executor_epoch = DecimalU64(1);
    binding.delegation_generation = DecimalU64(1);
    let original = scheduler(old.clone());
    assert!(check_payload(&original, &actor, &current, true).is_ok());
    rejected(
        check_payload(&original, &actor, &current, false),
        "executor_context_mismatch",
    );
    for field in [
        "world",
        "genesis",
        "agent",
        "task",
        "owner_binding",
        "owner_generation",
        "owner_key",
    ] {
        let mut other = current.clone();
        match field {
            "world" => other.world_id = "other".into(),
            "genesis" => other.genesis_digest = "other".into(),
            "agent" => other.agent_id = "agent-b".into(),
            "task" => other.task_id = "task-b".into(),
            "owner_binding" => other.owner_binding = "owner-b".into(),
            "owner_generation" => other.owner_generation.0 += 1,
            _ => other.owner_public_key = key(96),
        };
        assert!(
            check_payload(&original, &actor, &other, true).is_err(),
            "historical lookup crossed {field}"
        );
    }
    assert!(check_payload(&original, &key(96), &current, true).is_err());
    let correlation =
        oasis7_client_api::world_service_correlation::derive_correlation(world(), &original)
            .unwrap();
    let lookup = AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: 1,
            key: correlation.key,
        },
        original,
    };
    let encoded = serde_json::to_value(&lookup).unwrap();
    let decoded: AuthenticatedLookup = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(encoded, serde_json::to_value(decoded).unwrap());
}
fn read_http(socket: &mut std::net::TcpStream) -> (String, Value) {
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = socket.read(&mut chunk).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        assert!(bytes.len() < 65536);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length: usize = header
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return (
                    header
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .into(),
                    parse_request(&bytes[end + 4..end + 4 + length]).unwrap(),
                );
            }
        }
    }
}
#[test]
fn changed_current_delegate_or_fence_blocks_second_upstream_operation() {
    for change in [
        "delegate",
        "task_revision",
        "executor_epoch",
        "owner_generation",
        "suspended",
    ] {
        let mut f = IdentityService::model_executor_test_fixture();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        f.service.participant_entry = Some(entry(
            format!("http://{}", listener.local_addr().unwrap()),
            true,
        ));
        let original = request();
        let mut current = control();
        match change {
            "delegate" => {
                current
                    .executor_binding
                    .as_mut()
                    .unwrap()
                    .executor_public_key = key(96)
            }
            "task_revision" => current.task_revision.0 += 1,
            "executor_epoch" => current.executor_epoch.0 += 1,
            "owner_generation" => current.owner_generation.0 += 1,
            _ => {
                current.task_status = GameTaskStatusV1::Suspended;
                current.executor_binding = None;
                current.executor_id = None;
            }
        };
        let (done, finish) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let (path, body) = read_http(&mut socket);
            assert_eq!(path, VIEW_JSON_PATH);
            let read: SignedReadRequest<ReadWorldViewRequest> =
                serde_json::from_value(body).unwrap();
            verify_read_request(VIEW_JSON_PATH, &read).unwrap();
            let commit = CommitRef {
                world: world(),
                binding: ExecutionBinding {
                    provider_world_id: "world".into(),
                    branch_id: "main".into(),
                    finality_ref: "fixture".into(),
                    reorg_generation: 3,
                    governing_manifest_ref: "manifest".into(),
                    authority_generation: 1,
                    permission_generation: 1,
                },
                position: 7,
                execution_block_hash: "block".into(),
                state_root_ref: "root".into(),
            };
            let view = ReadWorldViewResponse {
                contract_version: 1,
                formal_profile: None,
                formal_proof: None,
                execution_evidence_scope: None,
                version: ProjectionVersion {
                    commit: commit.clone(),
                    projection_revision: "test".into(),
                    visibility_scope: "agent:agent-a".into(),
                },
                logical_tick: 7,
                continuation: EventCursor {
                    stream_id: "stream".into(),
                    scope_id: "agent:agent-a".into(),
                    era: 0,
                    sequence: 0,
                    commit,
                },
                view: json!({"game_task_control":current}),
            };
            let signed = sign_service_response(
                VIEW_JSON_PATH,
                request_digest(VIEW_JSON_PATH, &read).unwrap(),
                view,
                &hex::encode([94; 32]),
            )
            .unwrap();
            let bytes = serde_json::to_vec(&signed).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            socket.write_all(&bytes).unwrap();
            drop(socket);
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut forwarded = false;
            while Instant::now() < deadline {
                if let Ok((mut extra, _)) = listener.accept() {
                    forwarded = true;
                    let _ = read_http(&mut extra);
                    let _=extra.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
                }
                if finish.try_recv().is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            assert!(
                !forwarded,
                "mismatched canonical {change} forwarded executor operation"
            );
        });
        let result = f
            .service
            .entry_executor_route(
                "POST",
                "/entry-executor/control",
                Some(&f.token),
                &serde_json::to_value(original).unwrap(),
                super::super::now_ms(),
            )
            .unwrap();
        done.send(()).unwrap();
        server.join().unwrap();
        rejected(result, "executor_context_mismatch");
    }
}
