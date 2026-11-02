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
#[derive(Clone, Copy)]
enum CloseReplyCase {
    Bare,
    Qualified,
}
#[test]
fn formal_http_guarded_stop_then_distinct_close_use_real_queue_and_qualified_commit() {
    mutation_case(CloseReplyCase::Bare)
}
#[test]
fn formal_http_qualified_mutation_reply_requires_the_actual_new_profile() {
    mutation_case(CloseReplyCase::Qualified)
}
fn mutation_case(reply_case: CloseReplyCase) {
    let (mut initial, owner) = actual_task_fixture::world_before_task();
    let delegate = sign_read_request("key", (), &"22".repeat(32))
        .unwrap()
        .subject_public_key;
    initial
        .apply_agent_signer_delegation(
            &sign_read_request(
                "delegation",
                oasis7_runtime::service_admission::AgentSignerDelegationChangeV1 {
                    world_id: "formal-fixture-world".into(),
                    branch_id: "main".into(),
                    agent_id: "agent".into(),
                    owner_binding: "owner".into(),
                    agent_identity_generation: 1,
                    generation: 1,
                    delegate_public_key: delegate.clone(),
                    revoked: false,
                    nonce: 1,
                },
                &owner,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(
        initial
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "guarded-mutation-node-{}-{}",
        std::process::id(),
        now_unix_ms()
    ));
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
    let service = Arc::new(FormalLiveService::new(
        node_id,
        world_id.clone(),
        signer.clone(),
        node.clone(),
        publication.clone(),
        trust,
        resolver,
        state,
        gate,
    ));
    let initial_control =
        install_task_through_actual_http(&service, &publication, &owner, &delegate, &world_id);
    let deadline = now_unix_ms() + 30_000;
    let mut close_body = GameTaskRevokeAndCloseV2 {
        api_revision: 2,
        client_operation_id: "actual-http-close".into(),
        custody_account_id: "account".into(),
        termination_plan_digest: "44".repeat(32),
        permitted_branches: GameTaskCloseConditionsV2 {
            exact_initial_active: GameTaskFenceV1::from_control(&initial_control),
            own_stop_intent: None,
        },
        signer_public_key: delegate,
        signer_generation: DecimalU64(1),
        signer_nonce: DecimalU64(1),
        revoke_generation: DecimalU64(2),
        revoke_nonce: DecimalU64(2),
        cleanup_until_ms: DecimalU64(deadline),
    };
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
    diagnose_not_queued("stop", status, &raw);
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
    let mut qualified_effect = None;
    let close_ack = match reply_case {
        CloseReplyCase::Bare => {
            let (status, raw) = exchange(
                &service,
                TASK_CLEANUP_SUBMIT_V2,
                &serde_json::to_vec(&close).unwrap(),
            );
            diagnose_not_queued("close", status, &raw);
            assert_eq!(status, 200);
            serde_json::from_slice::<SignedServiceResponse<TaskMutationAdmissionV2>>(&raw).unwrap()
        }
        CloseReplyCase::Qualified => {
            use oasis7_client_api::world_service::task_mutation_qualified_v2::*;
            close.verify_owner_signature().unwrap();
            let wrapper = TaskMutationWaitSubmitV2 {
                api_revision: 2,
                reply_profile: TaskMutationReplyProfileV2::QualifiedEffectV2,
                submitted: TaskMutationSubmittedV2::RevokeAndClose(Box::new(close.clone())),
                delivery_nonce: "cd".repeat(32),
                delivery_until_ms: DecimalU64(
                    now_unix_ms()
                        .checked_add(5000)
                        .unwrap()
                        .min(close.original.request.cleanup_until_ms.0),
                ),
            };
            wrapper.validate().unwrap();
            let raw = serde_json::to_vec(&wrapper).unwrap();
            let cloned = Arc::clone(&service);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let worker = std::thread::spawn(move || {
                let reply = exchange(&cloned, TASK_CLEANUP_SUBMIT_V2, &raw);
                tx.send(reply).unwrap();
            });
            if let Ok((status, raw)) = rx.recv_timeout(Duration::from_millis(200)) {
                control.release().unwrap();
                worker.join().unwrap();
                if status != 200 {
                    let error: SignedServiceResponse<FormalServiceErrorV1> =
                        serde_json::from_slice(&raw).unwrap();
                    assert_eq!(
                        error.payload.code, "invalid_request",
                        "unexpected prerequisite refusal, not the old-profile target"
                    );
                }
                assert_eq!(
                    status, 200,
                    "new qualified-effect wrapper must be supported after the genuine qualified Stop prefix"
                );
                panic!("qualified reply returned before the actual paused Close commit");
            }
            control.wait(wait_until).unwrap();
            assert!(
                service.admission_gate.try_lock().is_ok(),
                "waiting HTTP retained admission gate"
            );
            assert!(
                node.try_lock().is_ok(),
                "waiting HTTP retained Node runtime mutex"
            );
            assert!(
                publication.current.try_lock().is_ok(),
                "waiting HTTP retained publication mutex"
            );
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
                    .is_none()
            );
            control.release().unwrap();
            let (status, raw) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            worker.join().unwrap();
            assert_eq!(status, 200);
            let reply: TaskMutationSubmitReplyV2 = serde_json::from_slice(&raw).unwrap();
            let TaskMutationSubmitReplyV2::Qualified { admission, effect } = reply else {
                panic!("actual qualified Close fact required")
            };
            verify_service_response(
                CLOSE_QUALIFIED_EFFECT_DOMAIN_V2,
                &wrapper.digest().unwrap(),
                &effect,
                &hex::encode(signer.verifying_key().to_bytes()),
            )
            .unwrap();
            let bytes = oasis7_client_api::world_service::signing_bytes(
                CLOSE_QUALIFIED_EFFECT_DOMAIN_V2,
                &(&effect.request_digest, &effect.payload),
            )
            .unwrap();
            let signature: [u8; 64] = hex::decode(&effect.signature_hex)
                .unwrap()
                .try_into()
                .unwrap();
            signer
                .verifying_key()
                .verify_strict(&bytes, &ed25519_dalek::Signature::from_bytes(&signature))
                .unwrap();
            assert_eq!(effect.payload.kind, TaskMutationKindV2::RevokeAndClose);
            assert_eq!(effect.payload.path, TASK_CLEANUP_SUBMIT_V2);
            assert_eq!(effect.payload.world, world_id);
            assert_eq!(effect.payload.delivery_nonce, wrapper.delivery_nonce);
            assert_eq!(
                effect.payload.original_digest,
                close.original_digest().unwrap()
            );
            let action_id = mutation_action_id_v2(
                TaskMutationKindV2::RevokeAndClose,
                &world_id,
                &close.original_digest().unwrap(),
            )
            .unwrap();
            let action = oasis7_node::NodeConsensusAction::from_payload(
                action_id,
                node.lock().unwrap().config().player_id.clone(),
                correlation::encode_cleanup_consensus_v2(&close).unwrap(),
            )
            .unwrap();
            assert_eq!(effect.payload.action_id.0, action_id);
            assert_eq!(effect.payload.canonical_catalog_digest, action.payload_hash);
            assert_eq!(
                effect.payload.execution.action_root,
                oasis7_node::compute_consensus_action_root(&[action]).unwrap()
            );
            qualified_effect = Some(effect.payload);
            admission
        }
    };
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
    if matches!(reply_case, CloseReplyCase::Bare) {
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
    }
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
    if let Some(effect) = qualified_effect {
        use oasis7_client_api::world_service::task_mutation_qualified_v2::TaskMutationQualifiedResultV2;
        assert_eq!(
            effect.chain_id,
            closed.world.chain_resource_manifest().chain_id
        );
        let tick = closed
            .world
            .tick_consensus_records()
            .iter()
            .find(|tick| tick.block.header.tick == record.committed_height)
            .expect("actual resident committed Tick required");
        assert_eq!(
            tick.block.header.chain_height,
            Some(effect.execution.height.0)
        );
        assert_eq!(tick.block.header.chain_slot, Some(effect.execution.slot.0));
        assert_eq!(
            tick.block.header.chain_epoch,
            Some(effect.execution.epoch.0)
        );
        assert_eq!(
            tick.block.header.node_block_hash.as_deref(),
            Some(effect.execution.node_block_hash.as_str())
        );
        assert_eq!(
            tick.block.header.action_root.as_deref(),
            Some(effect.execution.action_root.as_str())
        );
        assert_eq!(effect.execution.height.0, record.committed_height);
        assert_eq!(
            effect.execution.committed_at_unix_ms.0,
            record.committed_at_unix_ms
        );
        effect.witnessed_in_commit.validate().unwrap();
        assert_eq!(effect.witnessed_in_commit.world, world_id);
        let TaskMutationQualifiedResultV2::RevokeAndClose {
            closed: fact,
            stop_predecessor,
        } = effect.result
        else {
            panic!("wrong effect kind")
        };
        assert_eq!(*fact, record.closed);
        let predecessor = stop_predecessor.expect("actual own-Stop predecessor required");
        assert_eq!(
            *predecessor,
            stopped
                .world
                .guarded_task_stop_v2(&stop.original)
                .unwrap()
                .unwrap()
        );
    }
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

fn setup_envelope(
    world: &WorldIdentity,
    payload: WorldServicePayloadV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation::derive_correlation(world.clone(), &payload).unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    }
}
fn submit_setup(service: &FormalLiveService, q: &SubmitIntentRequest<WorldServicePayloadV1>) {
    let (status, raw) = exchange(service, SUBMIT_PATH, &serde_json::to_vec(q).unwrap());
    assert_eq!(status, 200);
    let response: SignedServiceResponse<IntentResponse<serde_json::Value>> =
        serde_json::from_slice(&raw).unwrap();
    verify_service_response(
        SUBMIT_PATH,
        &authority::request_digest(SUBMIT_PATH, q).unwrap(),
        &response,
        &hex::encode(service.signer.verifying_key().to_bytes()),
    )
    .unwrap();
}
fn actual_task(world: &World) -> Option<GameTaskControlV1> {
    oasis7_runtime::service_admission::game_task::current_control(world, "agent").unwrap()
}
fn install_task_through_actual_http(
    service: &FormalLiveService,
    publication: &PublishedReadState,
    owner: &str,
    delegate: &str,
    world: &WorldIdentity,
) -> GameTaskControlV1 {
    let create = setup_envelope(
        world,
        WorldServicePayloadV1::GameTaskCreate(
            sign_read_request(
                GAME_TASK_CREATE_DOMAIN,
                GameTaskCreateV1 {
                    api_rev: 1,
                    client_operation_id: "actual-create".into(),
                    world_id: world.world_id.clone(),
                    genesis_digest: world.genesis_digest.clone(),
                    branch_id: "main".into(),
                    reorg_epoch: DecimalU64(0),
                    agent_id: "agent".into(),
                    owner_binding: "owner".into(),
                    owner_generation: DecimalU64(1),
                },
                owner,
            )
            .unwrap(),
        ),
    );
    submit_setup(service, &create);
    let created = wait_cut(publication, |world| actual_task(world).is_some());
    let control = actual_task(&created.world).unwrap();
    let goal = GameGoalPublishV1 {
        api_rev: 1,
        world_id: control.world_id.clone(),
        genesis_digest: control.genesis_digest.clone(),
        task_id: control.task_id.clone(),
        agent_id: control.agent_id.clone(),
        owner_public_key: control.owner_public_key.clone(),
        owner_binding: control.owner_binding.clone(),
        owner_generation: control.owner_generation,
        branch_id: control.branch_id.clone(),
        reorg_epoch: control.reorg_epoch,
        expected_task_revision: control.task_revision,
        expected_latest_goal_revision: control.latest_goal_revision,
        goal_revision: DecimalU64(1),
        client_operation_id: "actual-goal".into(),
        prepare_operation_id: "prepare".into(),
        goal_ref: "33".repeat(32),
    };
    submit_setup(
        service,
        &setup_envelope(
            world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, goal, owner).unwrap(),
            ),
        ),
    );
    let published = wait_cut(publication, |world| {
        actual_task(world).is_some_and(|c| c.active_goal_revision.0 == 1)
    });
    let control = actual_task(&published.world).unwrap();
    let bind = GameExecutorControlV1 {
        api_rev: 1,
        client_operation_id: "actual-bind".into(),
        fence: GameTaskFenceV1::from_control(&control),
        operation: GameExecutorOperationV1::Bind {
            executor_id: "executor".into(),
            executor_public_key: delegate.into(),
            delegation_generation: DecimalU64(1),
        },
    };
    submit_setup(
        service,
        &setup_envelope(
            world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(GAME_EXECUTOR_CONTROL_DOMAIN, bind, owner).unwrap(),
            ),
        ),
    );
    let bound = wait_cut(publication, |world| {
        actual_task(world).is_some_and(|c| c.executor_binding.is_some())
    });
    actual_task(&bound.world).unwrap()
}

// Fixed classifications only; never log body, signature, request IDs or raw errors.
fn diagnose_not_queued(kind: &'static str, status: u16, raw: &[u8]) {
    if status == 200 {
        return;
    }
    let parsed = serde_json::from_slice::<SignedServiceResponse<FormalServiceErrorV1>>(raw).ok();
    let code = match parsed.as_ref().map(|v| v.payload.code.as_str()) {
        Some("mutation_not_queued") => "mutation_not_queued",
        Some("invalid_request") => "invalid_request",
        _ => "other",
    };
    let message = parsed
        .as_ref()
        .map(|v| v.payload.message.as_str())
        .unwrap_or("");
    eprintln!(
        "mutation refusal: kind={kind} status={status} code={code} busy={} target={} expiry={} history={} profile={} clock={} owner_sig={} qualified={} task_missing={} binding={}",
        message.contains("busy"),
        message.contains("target drift"),
        message.contains("expired"),
        message.contains("history") || message.contains("predecessor"),
        message.contains("profile"),
        message.contains("clock"),
        message.contains("signature"),
        message.contains("qualified"),
        message.contains("task unavailable"),
        message.contains("binding")
    );
    eprintln!(
        "mutation refusal detail: consensus={} auth={} payload={} cbor={} local={} read={} support={} lock={} fence={} signer={} nonce={} revoke={} control={} task={} canonical={} root={} scope={} utc={} uncertain={} missing_file={} record_io={}",
        message.contains("consensus"),
        message.contains("auth"),
        message.contains("payload"),
        message.contains("CBOR") || message.contains("cbor"),
        message.contains("local"),
        message.contains("read"),
        message.contains("support"),
        message.contains("lock"),
        message.contains("fence"),
        message.contains("signer"),
        message.contains("nonce"),
        message.contains("revoke"),
        message.contains("control"),
        message.contains("task"),
        message.contains("canonical"),
        message.contains("root"),
        message.contains("scope"),
        message.contains("UTC"),
        message.contains("uncertain") || message.contains("regressed"),
        message.contains("No such file or directory"),
        message.contains("execution bridge record")
    );
}
