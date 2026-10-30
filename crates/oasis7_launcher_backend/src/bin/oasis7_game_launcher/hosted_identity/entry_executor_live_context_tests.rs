//! Native transport boundary tests; the upstream denial is not a World success fixture.
use super::*;

fn live_request() -> EntryExecutorControlRequestV1 {
    let fence = GameTaskFenceV1::from_control(&control());
    let WorldServicePayloadV1::Scheduler(prefix) = scheduler(fence.clone()) else {
        unreachable!()
    };
    let SchedulerOperationV1::TaskProviderPrefix { context } = prefix.request.operation else {
        unreachable!()
    };
    let original = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context,
            prepare_operation_id: "original-prepare".into(),
        },
        &hex::encode([92; 32]),
    )
    .unwrap();
    verify_get(&original).unwrap();
    EntryExecutorControlRequestV1 {
        access: access(fence, 92),
        operation: EntryExecutorOperationV1::ReadLiveContext(TaskContextLiveReadV1 {
            original,
            observation_nonce: "ab".repeat(32),
        }),
    }
}

#[test]
fn live_context_invalid_nonce_is_local_and_old_variant_wire_is_unchanged() {
    let mut f = IdentityService::model_executor_test_fixture();
    f.service.participant_entry = Some(entry("http://127.0.0.1:1".into(), true));
    let mut request = live_request();
    let EntryExecutorOperationV1::ReadLiveContext(live) = &mut request.operation else {
        unreachable!()
    };
    live.observation_nonce = "AB".repeat(32);
    let get = live.original.clone();
    rejected(
        f.service
            .entry_executor_route(
                "POST",
                "/entry-executor/control",
                Some(&f.token),
                &serde_json::to_value(&request).unwrap(),
                super::super::super::now_ms(),
            )
            .unwrap(),
        "authentication_failed",
    );
    // A legacy mirror is serialized by the same codecs, not a hand-written hash expectation.
    #[derive(serde::Serialize)]
    #[serde(tag = "kind", content = "request", rename_all = "snake_case")]
    enum LegacyOperation {
        ReadContext(SignedReadRequest<TaskContextGetV1>),
    }
    let old = LegacyOperation::ReadContext(get.clone());
    let current = EntryExecutorOperationV1::ReadContext(get);
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&current).unwrap()
    );
    assert_eq!(
        signing_bytes("legacy-wire-fixture", &old).unwrap(),
        signing_bytes("legacy-wire-fixture", &current).unwrap()
    );
}

#[test]
fn live_context_native_forwards_only_fixed_wrapper_after_fresh_access() {
    let mut f = IdentityService::model_executor_test_fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    f.service.participant_entry = Some(entry(
        format!("http://{}", listener.local_addr().unwrap()),
        true,
    ));
    let original = live_request();
    let EntryExecutorOperationV1::ReadLiveContext(expected) = original.operation.clone() else {
        unreachable!()
    };
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let (path, body) = read_http(&mut socket);
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
        let (mut socket, _) = listener.accept().unwrap();
        let (path, body) = read_http(&mut socket);
        assert_eq!(path, TASK_CONTEXT_LIVE_READ_PATH);
        let actual: TaskContextLiveReadV1 = serde_json::from_value(body).unwrap();
        verify_live_context_read(&actual).unwrap();
        assert!(actual == expected);
        socket
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
            &serde_json::to_value(original).unwrap(),
            super::super::super::now_ms(),
        )
        .unwrap();
    assert!(result.is_err(), "actual upstream denial must be retained");
    assert_eq!(server.join().unwrap(), 2);
}
