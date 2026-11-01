//! Real HTTP queues and production formal writer/executor. Formal P/R storage
//! endpoints and initial authority are explicit test doubles, not deployment proof.
use super::*;
use crate::execution_bridge::formal_node_hook::{FormalNodeHook, FormalNodeWriter};
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{task_cleanup_v2::*, task_stop_v2::*};

use oasis7_client_api::world_service::{
    task_mutation_admission_v2::*, task_stop_record_control_v2::*,
};
use oasis7_node::{NodeConfig, NodeNetworkPolicy, NodeRole};
fn exchange(service: &FormalLiveService, path: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    write!(
        client,
        "POST {} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        path,
        body.len()
    )
    .unwrap();
    client.write_all(body).unwrap();
    http::serve_connection(service, &mut accepted).unwrap();
    drop(accepted);
    let mut raw = Vec::new();
    client.read_to_end(&mut raw).unwrap();
    let offset = raw.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
    let status = std::str::from_utf8(&raw[..offset])
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, raw[offset..].to_vec())
}
fn wait_cut(
    publication: &PublishedReadState,
    predicate: impl Fn(&World) -> bool,
) -> PublishedReadCut {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let cut = publication.current().unwrap();
        if predicate(&cut.world) {
            return cut;
        }
        assert!(
            Instant::now() < deadline,
            "actual Node did not publish the expected canonical effect"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn formal_http_guarded_stop_then_distinct_close_use_real_queue_and_qualified_commit() {
    let (initial, old_close, owner) = actual_task_fixture::fixture();
    // Discard the legacy fixture signature. The actual second signature below
    // is made only after this test verifies the first committed Stop witness.
    let mut close_body = old_close.request;
    close_body.client_operation_id = "actual-http-close".into();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "guarded-mutation-node-{}-{}",
        std::process::id(),
        now_unix_ms()
    ));
    // FormalWriter::open initializes its journal lazily; the arm requires an
    // already private-owned writer directory before any Node is started.
    std::fs::create_dir(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (writer, trust) =
        crate::execution_bridge::formal_writer::tests::task_mutation_writer(initial, root.clone());
    let (world, anchor) = writer.committed_read_cut().unwrap();
    let world_id = identity(&anchor);
    let node_id = world.tick_consensus_authority_source().to_owned();
    let publication = Arc::new(PublishedReadState::default());
    let gate = Arc::new(Mutex::new(()));
    let arm = Arc::new(
        crate::execution_bridge::formal_durable_set_arm::FormalDurableSetArmControlV1::new(
            root.clone(),
            false,
        )
        .unwrap(),
    );
    let hook = FormalNodeHook::new(writer)
        .unwrap()
        .with_durable_set_arm(arm, gate.clone())
        .unwrap()
        .with_publication_sink(publication.clone())
        .unwrap();
    let genesis = hook.genesis_bootstrap().unwrap();
    let bootstrap = if genesis.is_none() {
        Some(hook.bootstrap().unwrap())
    } else {
        None
    };
    let mut config = NodeConfig::new(&node_id, &world_id.world_id, NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(Duration::from_millis(100))
        .unwrap()
        .with_auto_attest_all_validators(false)
        .with_require_execution_on_commit(true)
        .with_require_peer_execution_hashes(false);
    config.network_policy = NodeNetworkPolicy::for_runtime_role(NodeRole::Sequencer);
    config.allow_local_proposals = true;
    config.gossip = None;
    config.replication = None;
    config.feedback_p2p = None;
    let mut runtime = oasis7_node::NodeRuntime::new(config);
    if let Some(genesis) = genesis {
        runtime = runtime.with_local_execution_genesis_bootstrap(genesis);
    } else {
        runtime = runtime.with_local_execution_bootstrap(bootstrap.unwrap());
    }
    runtime = runtime.with_execution_hook(hook);
    runtime.start().unwrap();
    let node = Arc::new(Mutex::new(runtime));
    let resolver = Arc::new(CurrentOnlyResolver::new(world.clone(), anchor.clone()));
    let state = FormalLiveState::default();
    state.ready.store(true, Ordering::Release);
    let signer = fixtures::key(91);
    let service = FormalLiveService::new(
        node_id,
        world_id.clone(),
        signer.clone(),
        node.clone(),
        publication.clone(),
        trust,
        resolver,
        state,
        gate,
    );
    let deadline = now_unix_ms() + 30_000;
    close_body.cleanup_until_ms = DecimalU64(deadline);
    let mut stop = GameTaskStopV2 {
        api_revision: 2,
        client_operation_id: "actual-http-stop".into(),
        custody_account_id: close_body.custody_account_id.clone(),
        termination_plan_digest: close_body.termination_plan_digest.clone(),
        initial_fence: close_body.permitted_branches.exact_initial_active.clone(),
        target_executor_public_key: close_body.signer_public_key.clone(),
        target_executor_generation: close_body.signer_generation,
        target_executor_nonce: close_body.signer_nonce,
        execute_until_ms: DecimalU64(deadline),
        observation_grant: None,
    };
    stop.observation_grant = Some(
        StopObservationGrantV2::known_control_for_exact_target(&stop, deadline, deadline, deadline)
            .unwrap(),
    );
    let signed = sign_read_request(GAME_TASK_STOP_DOMAIN_V2, stop, &owner).unwrap();
    let stop = TaskStopSubmitV2 {
        api_revision: 2,
        expected_world: world_id.clone(),
        original: SignedGameTaskStopV2 {
            request: signed.request,
            subject_public_key: signed.subject_public_key,
            signature_hex: signed.signature_hex,
        },
        observation_nonce: "11".repeat(32),
    };
    let query = StopRecordControlRequestV2 {
        api_revision: 2,
        original: stop.clone(),
        source: StopRecordSourceV2 {
            approved_input_digest: "22".repeat(32),
            session_id: "fixture-session".into(),
            invocation_id: "fixture-invocation".into(),
            source_operation_id: "source".into(),
            source_nonce: "33".repeat(32),
            world_attempt_id: "world-read".into(),
        },
    };
    let read = |q: &StopRecordControlRequestV2| {
        let (status, raw) = exchange(
            &service,
            TASK_STOP_RECORD_CONTROL_V2,
            &serde_json::to_vec(q).unwrap(),
        );
        assert_eq!(status, 200);
        let response: StopRecordControlResponseV2 = serde_json::from_slice(&raw).unwrap();
        verify_stop_record_control_v2(
            q,
            &response,
            &hex::encode(signer.verifying_key().to_bytes()),
        )
        .unwrap();
        response
    };
    let unknown = read(&query);
    assert!(matches!(
        unknown.fact.payload.fact,
        StopRecordFactV2::Unknown
    ));
    assert!(matches!(
        unknown.current.payload.current,
        StopControlDeliveryV2::Unavailable
    ));
    let control = publication.control_boundaries().unwrap();
    let frame = correlation::encode_guarded_stop_consensus_v2(&stop).unwrap();
    let wait_until = Instant::now() + Duration::from_secs(20);
    control
        .arm(
            test_control::Point::BeforeResume,
            None,
            Some(frame),
            wait_until,
        )
        .unwrap();
    let (status, raw) = exchange(
        &service,
        TASK_STOP_SUBMIT_V2,
        &serde_json::to_vec(&stop).unwrap(),
    );
    assert_eq!(status, 200);
    let ack: SignedServiceResponse<TaskMutationAdmissionV2> = serde_json::from_slice(&raw).unwrap();
    verify_service_response(
        TASK_STOP_SUBMIT_V2,
        &authority::request_digest(TASK_STOP_SUBMIT_V2, &stop).unwrap(),
        &ack,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    ack.payload
        .validate(
            TaskMutationKindV2::GuardedStop,
            &world_id,
            &stop.original_digest().unwrap(),
        )
        .unwrap();
    control.wait(wait_until).unwrap();
    assert!(
        publication
            .current()
            .unwrap()
            .world
            .guarded_task_stop_v2(&stop.original)
            .unwrap()
            .is_none(),
        "queue ACK is not a canonical Stop"
    );
    control.release().unwrap();
    let stopped = wait_cut(&publication, |world| {
        world
            .guarded_task_stop_v2(&stop.original)
            .unwrap()
            .is_some()
    });
    let result = read(&query);
    let StopRecordFactV2::Known(witness) = result.fact.payload.fact else {
        panic!("actual Stop required")
    };
    assert_eq!(
        witness.record,
        stopped
            .world
            .guarded_task_stop_v2(&stop.original)
            .unwrap()
            .unwrap()
    );
    assert!(matches!(
        result.current.payload.current,
        StopControlDeliveryV2::Exact(_)
    ));
    close_body.permitted_branches.own_stop_intent = Some(GameOwnStopIntentV2 {
        profile: GameStopIntentProfileV2::GuardedTaskStopV2,
        operation_id: stop.original.request.client_operation_id.clone(),
        intent_digest: oasis7_wasm_abi::canonical_hash(&(
            GAME_TASK_STOP_DOMAIN_V2,
            &stop.original.subject_public_key,
            &stop.original.request,
        ))
        .unwrap(),
    });
    let close = TaskCleanupSubmitV2 {
        api_revision: 2,
        expected_world: world_id.clone(),
        original: sign_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, close_body, &owner).unwrap(),
    };
    let frame = correlation::encode_cleanup_consensus_v2(&close).unwrap();
    let wait_until = Instant::now() + Duration::from_secs(20);
    control
        .arm(
            test_control::Point::BeforeResume,
            None,
            Some(frame),
            wait_until,
        )
        .unwrap();
    let (status, raw) = exchange(
        &service,
        TASK_CLEANUP_SUBMIT_V2,
        &serde_json::to_vec(&close).unwrap(),
    );
    assert_eq!(status, 200);
    let close_ack: SignedServiceResponse<TaskMutationAdmissionV2> =
        serde_json::from_slice(&raw).unwrap();
    verify_service_response(
        TASK_CLEANUP_SUBMIT_V2,
        &authority::request_digest(TASK_CLEANUP_SUBMIT_V2, &close).unwrap(),
        &close_ack,
        &hex::encode(signer.verifying_key().to_bytes()),
    )
    .unwrap();
    close_ack
        .payload
        .validate(
            TaskMutationKindV2::RevokeAndClose,
            &world_id,
            &close.original_digest().unwrap(),
        )
        .unwrap();
    assert_ne!(ack.payload.action_id, close_ack.payload.action_id);
    assert_ne!(
        ack.payload.original_digest,
        close_ack.payload.original_digest
    );
    control.wait(wait_until).unwrap();
    assert!(
        publication
            .current()
            .unwrap()
            .world
            .closed_game_task_v2(
                &close
                    .original
                    .request
                    .permitted_branches
                    .exact_initial_active
                    .task_id
            )
            .unwrap()
            .is_none(),
        "queue ACK is not Closed"
    );
    control.release().unwrap();
    let task = close
        .original
        .request
        .permitted_branches
        .exact_initial_active
        .task_id
        .clone();
    let closed = wait_cut(&publication, |world| {
        world.closed_game_task_v2(&task).unwrap().is_some()
    });
    let record = closed
        .world
        .capability_revocation_state()
        .task_terminals_v2
        .as_ref()
        .unwrap()
        .records
        .get(&task)
        .unwrap();
    assert_eq!(record.original, close.original);
    assert!(record.stop_predecessor_canonical_cbor.is_some());
    let delegation = &closed
        .world
        .capability_revocation_state()
        .agent_signer_delegations["agent"];
    assert!(delegation.revoked);
    assert_eq!(delegation.nonce, close.original.request.revoke_nonce.0);
    node.lock().unwrap().stop().unwrap();
    drop(service);
    drop(node);
    drop(control);
    drop(publication);
    std::fs::remove_dir_all(root).unwrap();
}
