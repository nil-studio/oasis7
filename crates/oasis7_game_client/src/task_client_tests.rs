use super::*;
use crate::WorldServiceClientError;
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn body(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    body
}
#[test]
fn lost_task_submit_is_unknown_and_only_explicit_lookup_recovers_same_signed_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = WorldServiceTaskClient::new(crate::tests::config(format!(
        "http://{}",
        listener.local_addr().unwrap()
    )))
    .unwrap();
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "create".into(),
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "a".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
            },
            &hex::encode([7; 32]),
        )
        .unwrap(),
    );
    let correlation = derive_correlation(client.config().expected_world.clone(), &payload).unwrap();
    let expected_payload = payload.clone();
    let expected_correlation = correlation.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let submitted: SubmitIntentRequest<WorldServicePayloadV1> =
            serde_json::from_slice(&body(&mut stream)).unwrap();
        assert_eq!(submitted.signed_payload, expected_payload);
        assert_eq!(submitted.correlation, expected_correlation);
        drop(stream);
        let (mut stream, _) = listener.accept().unwrap();
        let lookup: AuthenticatedLookup = serde_json::from_slice(&body(&mut stream)).unwrap();
        assert_eq!(lookup.original, expected_payload);
        assert_eq!(lookup.request.key, expected_correlation.key);
        let digest = request_digest(LOOKUP_PATH, &lookup).unwrap();
        let response = sign_service_response(
            LOOKUP_PATH,
            digest,
            IntentResponse::<Value> {
                formal_profile: None,
                execution_evidence_scope: None,
                contract_version: 1,
                correlation: expected_correlation,
                outcome: IntentOutcome::Pending,
            },
            &hex::encode([9; 32]),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&response).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        stream.write_all(&bytes).unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    });
    let response = client
        .submit(SubmitIntentRequest {
            contract_version: 1,
            correlation: correlation.clone(),
            deadline_unix_ms: None,
            signed_payload: payload.clone(),
        })
        .unwrap();
    assert!(
        matches!(response, SubmitObservation::OutcomeUnknown(ref value) if value == &correlation)
    );
    assert!(matches!(
        client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: correlation.key
                },
                payload
            )
            .unwrap()
            .outcome,
        IntentOutcome::Pending
    ));
    server.join().unwrap();
}

fn assert_snapshot_reader_rejects_wrong_mode(native_reply: bool, legacy_reader: bool) {
    use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextAuthorityModeV1};
    use oasis7_client_api::world_service::task_context::*;
    let (snapshot_bytes, request_bytes): (&[u8], &[u8]) = if native_reply {
        (
            include_bytes!("../testdata/task-context/native-snapshot.json"),
            include_bytes!("../testdata/task-context/native-snapshot-request.json"),
        )
    } else {
        (
            include_bytes!("../testdata/task-context/provider-snapshot.json"),
            include_bytes!("../testdata/task-context/provider-snapshot-request.json"),
        )
    };
    let snapshot: GoalExecutionSnapshotV1 = serde_json::from_slice(snapshot_bytes).unwrap();
    let original: SignedReadRequest<TaskGoalSnapshotRequestV1> =
        serde_json::from_slice(request_bytes).unwrap();
    snapshot.validate().unwrap();
    verify_snapshot_request(&original).unwrap();
    assert!(GameTaskFenceV1::from_control(&snapshot.control) == original.request.fence);
    assert_eq!(
        snapshot.authority_mode().unwrap(),
        original.request.authority_mode
    );
    assert_eq!(
        original.subject_public_key,
        original
            .request
            .fence
            .executor_binding
            .as_ref()
            .unwrap()
            .executor_public_key
    );
    let mut opposite = original.request.clone();
    opposite.authority_mode = if native_reply {
        None
    } else {
        Some(TaskContextAuthorityModeV1::NativeExecutorV1)
    };
    opposite.schema_revision = TaskContextAuthorityModeV1::schema_revision(opposite.authority_mode);
    let opposite = sign_read_request(TASK_SNAPSHOT_PATH, opposite, &hex::encode([73; 32])).unwrap();
    verify_snapshot_request(&opposite).unwrap();
    assert_eq!(opposite.subject_public_key, original.subject_public_key);
    for mismatch in [false, true] {
        let selected = if mismatch { &opposite } else { &original };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut config = crate::tests::config(format!("http://{}", listener.local_addr().unwrap()));
        config.expected_world = snapshot.read_commit.world.clone();
        config.scope_id = format!("agent:{}", original.request.fence.agent_id);
        config.max_response_bytes = MAX_PRIVATE_CONTEXT_BYTES;
        config.timeout = std::time::Duration::from_secs(2);
        let expected = selected.clone();
        let snapshot = snapshot.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let received: SignedReadRequest<TaskGoalSnapshotRequestV1> =
                serde_json::from_slice(&body(&mut stream)).unwrap();
            assert!(received == expected);
            let response = sign_service_response(
                TASK_SNAPSHOT_PATH,
                request_digest(TASK_SNAPSHOT_PATH, &received).unwrap(),
                snapshot,
                &hex::encode([9; 32]),
            )
            .unwrap();
            let bytes = serde_json::to_vec(&response).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            stream.write_all(&bytes).unwrap();
        });
        let result = if legacy_reader {
            crate::legacy_profile::LegacyWorldServiceClient::new(
                crate::remote_world_service::WorldServiceClientConfig {
                    formal_read_trust: None,
                    endpoint: config.endpoint,
                    trusted_service_public_key: config.trusted_service_public_key,
                    expected_world: config.expected_world,
                    scope_id: config.scope_id,
                    read_private_key_hex: config.read_private_key_hex,
                    timeout: config.timeout,
                    max_response_bytes: config.max_response_bytes,
                },
            )
            .unwrap()
            .read_task_goal_snapshot(selected)
        } else {
            WorldServiceTaskClient::new(config)
                .unwrap()
                .read_task_goal_snapshot(selected)
        };
        if mismatch {
            match result {
                Err(WorldServiceClientError::Assurance(message)) => assert_eq!(
                    message,
                    if legacy_reader {
                        "private task response not verified"
                    } else {
                        "snapshot authority mode differs"
                    }
                ),
                Err(error) => panic!("unexpected snapshot failure: {error}"),
                Ok(_) => {
                    panic!("valid signed snapshot from the opposite authority mode was accepted")
                }
            }
        } else {
            assert!(
                result.is_ok(),
                "matching signed snapshot must pass all reader checks"
            );
        }
        server.join().unwrap();
    }
}
#[test]
fn task_snapshot_reader_rejects_signed_provider_reply_for_native_request() {
    assert_snapshot_reader_rejects_wrong_mode(false, false);
}
#[test]
fn task_snapshot_reader_rejects_signed_native_reply_for_provider_request() {
    assert_snapshot_reader_rejects_wrong_mode(true, false);
}
#[test]
fn legacy_snapshot_reader_rejects_signed_provider_reply_for_native_request() {
    assert_snapshot_reader_rejects_wrong_mode(false, true);
}
#[test]
fn legacy_snapshot_reader_rejects_signed_native_reply_for_provider_request() {
    assert_snapshot_reader_rejects_wrong_mode(true, true);
}
