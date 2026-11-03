//! Real Native SQLite/session/HTTP routing; upstream403 is not a World success.
use super::*;

fn prepare() -> EntryExecutorControlRequestV1 {
    let fence = GameTaskFenceV1::from_control(&control());
    let original = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            schema_revision: 2,
            fence: fence.clone(),
            client_operation_id: "attested-prepare".into(),
            client_nonce: "aa".repeat(32),
        },
        &hex::encode([92; 32]),
    )
    .unwrap();
    EntryExecutorControlRequestV1 {
        access: access(fence, 92),
        operation: EntryExecutorOperationV1::PrepareContextAttestedV2(Box::new(
            TaskContextPrepareAttestationRequestV2 {
                original,
                observation_nonce: "bb".repeat(32),
            },
        )),
    }
}
#[test]
fn context_projection_native_bad_nonce_is_rejected_before_actual_upstream() {
    let mut f = IdentityService::model_executor_test_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    f.service.participant_entry = Some(entry(
        format!("http://{}", listener.local_addr().unwrap()),
        true,
    ));
    let mut request = prepare();
    let EntryExecutorOperationV1::PrepareContextAttestedV2(q) = &mut request.operation else {
        unreachable!()
    };
    q.observation_nonce = "BB".repeat(32);
    rejected(
        f.service
            .entry_executor_route(
                "POST",
                "/entry-executor/control",
                Some(&f.token),
                &serde_json::to_value(request).unwrap(),
                super::super::super::now_ms(),
            )
            .unwrap(),
        "authentication_failed",
    );
    assert!(matches!(listener.accept(), Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock));
}
fn serve_view(socket: &mut std::net::TcpStream) {
    let (path, body) = read_http(socket);
    assert_eq!(path, VIEW_JSON_PATH);
    let read: SignedReadRequest<ReadWorldViewRequest> = serde_json::from_value(body).unwrap();
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
        view: json!({"game_task_control":control()}),
    };
    let reply = sign_service_response(
        VIEW_JSON_PATH,
        request_digest(VIEW_JSON_PATH, &read).unwrap(),
        view,
        &hex::encode([94; 32]),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&reply).unwrap();
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    )
    .unwrap();
    socket.write_all(&bytes).unwrap();
}
#[test]
fn context_projection_native_exact_prepare_uses_one_view_then_one_fixed_route() {
    let mut f = IdentityService::model_executor_test_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    f.service.participant_entry = Some(entry(
        format!("http://{}", listener.local_addr().unwrap()),
        true,
    ));
    let request = prepare();
    let EntryExecutorOperationV1::PrepareContextAttestedV2(expected) = request.operation.clone()
    else {
        unreachable!()
    };
    let server = thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        serve_view(&mut first);
        drop(first);
        let (mut second, _) = listener.accept().unwrap();
        let (path, body) = read_http(&mut second);
        assert_eq!(path, CONTEXT_PREPARE_ATTESTED_PATH_V2);
        let actual: TaskContextPrepareAttestationRequestV2 = serde_json::from_value(body).unwrap();
        validate_prepare_attestation_request_v2(&actual).unwrap();
        assert!(actual == *expected);
        second
            .write_all(
                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .unwrap();
        2
    });
    let result = f
        .service
        .entry_executor_route(
            "POST",
            "/entry-executor/control",
            Some(&f.token),
            &serde_json::to_value(request).unwrap(),
            super::super::super::now_ms(),
        )
        .unwrap();
    assert!(matches!(result, Err(StoreError::OutcomeUnknown)));
    assert_eq!(server.join().unwrap(), 2);
}
