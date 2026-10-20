use super::private_goal_tests::{consumer, http_with_diagnostic_hook, http_with_hook};
use super::*;
use oasis7::world_service::{
    private_goal_prepare::sign_goal_prepare, private_goal_read::publication,
    private_goal_store::PrivateGoalStore, private_task_context::*,
};
use oasis7_agent_api::game::{
    DecimalU64, GAME_EXECUTOR_CONTROL_DOMAIN, GAME_GOAL_PUBLISH_DOMAIN, GameExecutorControlV1,
    GameExecutorOperationV1, GameGoalPrepareV1, GameTaskFenceV1, GoalContentV1,
    game_goal_version_digest,
};
use oasis7_client_api::world_service::task_context::TASK_CONTEXT_READ_DELIVERY_DRIFT;
use std::sync::{Arc, Mutex};

struct ActiveContextReadFixture {
    dir: std::path::PathBuf,
    driver: NodeRuntimeExecutionDriver,
    world: WorldIdentity,
    owner: String,
    signer: crate::feedback_submit_api::FeedbackSubmitSigner,
    runtime: Arc<Mutex<oasis7_node::NodeRuntime>>,
    store: PrivateGoalStore,
    get: SignedReadRequest<TaskContextGetV1>,
    fence: GameTaskFenceV1,
}

fn active_context_read_fixture() -> ActiveContextReadFixture {
    let (dir, mut driver, world, owner, owner_public) =
        super::canonical_agent_chat::driver_fixture_with_verified_authority();
    let create = game_task_tests::create(&world, &owner, "context-read-task");
    commit(&mut driver, 3, create);
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private, [7; 32]).unwrap();
    let content = GoalContentV1 {
        goal_text: "Read-only fixture objective".into(),
        soft_constraints: vec!["preserve the captured state".into()],
    };
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: control.agent_id.clone(),
            owner_public_key: owner_public.clone(),
            owner_binding: control.owner_binding.clone(),
            owner_generation: control.owner_generation,
            branch_id: control.branch_id.clone(),
            reorg_epoch: control.reorg_epoch,
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "context-read-goal".into(),
            client_nonce: "a1".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(1), &content)
                .unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let receipt = store.prepare(&goal).unwrap();
    let publish = publication(&goal, &receipt, "context-read-publish".into()).unwrap();
    commit(
        &mut driver,
        4,
        request(
            &world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, &owner).unwrap(),
            ),
        ),
    );
    let delegate = hex::encode([73; 32]);
    let delegate_public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    commit(
        &mut driver,
        5,
        request(
            &world,
            WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    AgentSignerDelegationChangeV1 {
                        world_id: world.world_id.clone(),
                        branch_id: "main".into(),
                        agent_id: "agent-a".into(),
                        owner_binding: "owner-a".into(),
                        agent_identity_generation: 1,
                        delegate_public_key: delegate_public.clone(),
                        generation: 1,
                        revoked: false,
                        nonce: 1,
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let bind_fence = GameTaskFenceV1::from_control(&control);
    commit(
        &mut driver,
        6,
        request(
            &world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "context-read-bind".into(),
                        fence: bind_fence,
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "context-read-executor".into(),
                            executor_public_key: delegate_public,
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let fence = GameTaskFenceV1::from_control(&control);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: owner_public,
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let prepare = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            schema_revision: 1,
            fence: fence.clone(),
            client_operation_id: "context-read-prepare".into(),
            client_nonce: "b2".repeat(32),
        },
        &delegate,
    )
    .unwrap();
    let prepared = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&prepare).unwrap()
    });
    let prefix = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", public_prefix(&prepared).unwrap(), &delegate).unwrap(),
        ),
    );
    commit(&mut driver, 7, prefix.clone());
    assert!(
        game_task_tests::recorded(&driver, &prefix)
            .rejected
            .is_none()
    );
    let get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: prepared.identity,
            prepare_operation_id: prepare.request.client_operation_id,
        },
        &delegate,
    )
    .unwrap();
    ActiveContextReadFixture {
        dir,
        driver,
        world,
        owner,
        signer,
        runtime,
        store,
        get,
        fence,
    }
}

fn advance_past_context_deadline(fixture: &mut ActiveContextReadFixture) {
    let context_deadline = fixture.get.request.context.valid_until_tick.0;
    let pin = super::super::world_service_read::pin(
        &fixture.dir.join("records"),
        &fixture.dir.join("store"),
        &fixture.world,
        None,
    )
    .unwrap();
    advance_driver_past_context_deadline(
        &mut fixture.driver,
        pin.commit.position,
        context_deadline,
    );
}

fn advance_driver_past_context_deadline(
    driver: &mut NodeRuntimeExecutionDriver,
    starting_height: u64,
    context_deadline: u64,
) {
    let mut height = starting_height;
    while driver.execution_world.state().time <= context_deadline {
        height += 1;
        assert!(
            height - starting_height <= 128,
            "test fixture should have a bounded task-context lifetime"
        );
        let actions = vec![];
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".into(),
                node_id: "node-a".into(),
                proposer_id: "node-a".into(),
                height,
                slot: height,
                epoch: 0,
                node_block_hash: format!("node-h{height}"),
                action_root: compute_consensus_action_root(&actions).unwrap(),
                committed_actions: actions,
                committed_at_unix_ms: height as i64 * 1000,
            })
            .unwrap();
    }
    assert!(driver.execution_world.state().time > context_deadline);
}

#[test]
fn private_task_context_read_retries_same_get_after_benign_head_advance_but_rejects_stop() {
    let mut fixture = active_context_read_fixture();
    let original_get_bytes = serde_json::to_vec(&fixture.get).unwrap();
    let pin = || {
        super::super::world_service_read::pin(
            &fixture.dir.join("records"),
            &fixture.dir.join("store"),
            &fixture.world,
            None,
        )
        .unwrap()
    };
    let before = pin();
    let before_tick = before.world.state().time;
    let (status, body, stage) = http_with_diagnostic_hook(
        &fixture.dir,
        &fixture.runtime,
        &fixture.signer,
        Some(&fixture.store),
        TASK_CONTEXT_READ_PATH,
        &fixture.get,
        "",
        || {
            let height = before.commit.position + 1;
            fixture
                .driver
                .on_commit(NodeExecutionCommitContext {
                    world_id: "w1".into(),
                    node_id: "node-a".into(),
                    proposer_id: "node-a".into(),
                    height,
                    slot: height,
                    epoch: 0,
                    node_block_hash: format!("node-h{height}"),
                    action_root: compute_consensus_action_root(&[]).unwrap(),
                    committed_actions: vec![],
                    committed_at_unix_ms: height as i64 * 1000,
                })
                .unwrap();
        },
    );
    assert_eq!(status, 409);
    assert_eq!(body["error"], TASK_CONTEXT_READ_DELIVERY_DRIFT);
    assert!(body.get("payload").is_none());
    assert_eq!(
        stage, None,
        "a same-binding commit advance should reach the typed delivery-drift fence"
    );
    let after = pin();
    assert_ne!(after.commit, before.commit);
    assert_eq!(after.commit.position, before.commit.position + 1);
    assert_eq!(after.commit.binding, before.commit.binding);
    assert!(
        after.world.state().time > before_tick,
        "empty runtime commit must advance simulation time"
    );
    assert_ne!(after.world.state(), before.world.state());

    assert!(
        serde_json::to_vec(&fixture.get).unwrap() == original_get_bytes,
        "retry must preserve the exact original signed GET"
    );
    let (status, body) = http_with_hook(
        &fixture.dir,
        &fixture.runtime,
        &fixture.signer,
        Some(&fixture.store),
        TASK_CONTEXT_READ_PATH,
        &fixture.get,
        "",
        || {},
    );
    assert_eq!(status, 200);
    assert!(
        serde_json::to_vec(&fixture.get).unwrap() == original_get_bytes,
        "successful retry must use the unchanged original signed GET"
    );
    let response: SignedServiceResponse<CapturedTaskContextV1> =
        serde_json::from_value(body).unwrap();
    verify_private_reply(
        TASK_CONTEXT_READ_PATH,
        &fixture.get,
        &response,
        &fixture.signer.public_key_hex,
    )
    .unwrap();
    response.payload.validate_against(&fixture.get).unwrap();
    assert_eq!(
        response.payload.current_public_context.read_commit, after.commit,
        "the retry response must be signed for its current pinned world"
    );

    let stop = request(
        &fixture.world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "context-read-stop".into(),
                    fence: fixture.fence.clone(),
                    operation: GameExecutorOperationV1::Stop,
                },
                &fixture.owner,
            )
            .unwrap(),
        ),
    );
    let (status, body) = http_with_hook(
        &fixture.dir,
        &fixture.runtime,
        &fixture.signer,
        Some(&fixture.store),
        TASK_CONTEXT_READ_PATH,
        &fixture.get,
        "",
        || commit(&mut fixture.driver, after.commit.position + 1, stop.clone()),
    );
    assert_eq!(status, 403);
    assert!(body.get("payload").is_none());
    assert!(
        game_task_tests::recorded(&fixture.driver, &stop)
            .rejected
            .is_none()
    );
}

#[test]
fn private_task_context_read_rejects_goal_revision_advanced_before_delivery() {
    let mut fixture = active_context_read_fixture();
    let before = super::super::world_service_read::pin(
        &fixture.dir.join("records"),
        &fixture.dir.join("store"),
        &fixture.world,
        None,
    )
    .unwrap();
    let control = game_task::current_control(&fixture.driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let content = GoalContentV1 {
        goal_text: "Replacement objective published during context delivery".into(),
        soft_constraints: vec!["the captured revision must be fenced".into()],
    };
    let replacement = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: fixture.world.world_id.clone(),
            genesis_digest: fixture.world.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: control.agent_id.clone(),
            owner_public_key: fixture.signer.public_key_hex.clone(),
            owner_binding: control.owner_binding.clone(),
            owner_generation: control.owner_generation,
            branch_id: control.branch_id.clone(),
            reorg_epoch: control.reorg_epoch,
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(2),
            client_operation_id: "context-read-replacement-goal".into(),
            client_nonce: "c3".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(2), &content)
                .unwrap(),
            content,
        },
        &fixture.owner,
    )
    .unwrap();
    let receipt = fixture.store.prepare(&replacement).unwrap();
    let publish = request(
        &fixture.world,
        WorldServicePayloadV1::GameGoalPublish(
            sign_read_request(
                GAME_GOAL_PUBLISH_DOMAIN,
                publication(
                    &replacement,
                    &receipt,
                    "context-read-replacement-publish".into(),
                )
                .unwrap(),
                &fixture.owner,
            )
            .unwrap(),
        ),
    );
    let (status, body) = http_with_hook(
        &fixture.dir,
        &fixture.runtime,
        &fixture.signer,
        Some(&fixture.store),
        TASK_CONTEXT_READ_PATH,
        &fixture.get,
        "",
        || {
            commit(
                &mut fixture.driver,
                before.commit.position + 1,
                publish.clone(),
            )
        },
    );
    assert!(status == 403 || status == 409);
    assert!(body.get("payload").is_none());
    assert!(
        game_task_tests::recorded(&fixture.driver, &publish)
            .rejected
            .is_none(),
        "replacement goal must be a committed revision change"
    );
    let after = super::super::world_service_read::pin(
        &fixture.dir.join("records"),
        &fixture.dir.join("store"),
        &fixture.world,
        None,
    )
    .unwrap();
    assert!(after.commit.position > before.commit.position);
    assert_eq!(
        game_task::current_control(&after.world, "agent-a")
            .unwrap()
            .unwrap()
            .latest_goal_revision,
        DecimalU64(2)
    );
}

#[test]
fn private_task_context_read_reports_expiry_at_capture_and_delivery() {
    let mut initial = active_context_read_fixture();
    advance_past_context_deadline(&mut initial);
    let (status, body, stage) = http_with_diagnostic_hook(
        &initial.dir,
        &initial.runtime,
        &initial.signer,
        Some(&initial.store),
        TASK_CONTEXT_READ_PATH,
        &initial.get,
        "",
        || {},
    );
    assert_eq!(status, 409);
    assert_eq!(body["error"], "private_goal_conflict");
    assert!(body.get("payload").is_none());
    assert_eq!(stage, Some("initial_capture_expired"));

    let mut delivery = active_context_read_fixture();
    let starting_height = super::super::world_service_read::pin(
        &delivery.dir.join("records"),
        &delivery.dir.join("store"),
        &delivery.world,
        None,
    )
    .unwrap()
    .commit
    .position;
    let context_deadline = delivery.get.request.context.valid_until_tick.0;
    let ActiveContextReadFixture {
        dir,
        driver,
        signer,
        runtime,
        store,
        get,
        ..
    } = &mut delivery;
    let (status, body, stage) = http_with_diagnostic_hook(
        dir,
        runtime,
        signer,
        Some(store),
        TASK_CONTEXT_READ_PATH,
        get,
        "",
        || advance_driver_past_context_deadline(driver, starting_height, context_deadline),
    );
    assert_eq!(status, 409);
    assert_eq!(body["error"], "private_goal_conflict");
    assert!(body.get("payload").is_none());
    assert_eq!(stage, Some("delivery_capture_expired"));
}

#[test]
fn private_task_context_http_service_emits_stage_without_tracing_subscriber() {
    let mut fixture = active_context_read_fixture();
    advance_past_context_deadline(&mut fixture);
    let get = fixture.get.clone();
    let result = consumer(
        &fixture.dir,
        &fixture.runtime,
        &fixture.signer,
        &fixture.store,
        &fixture.world,
        |client| client.read_task_context(&get),
    );
    assert!(matches!(
        result,
        Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
            status: 409,
            message,
        }) if message == "private goal request rejected"
    ));
}
