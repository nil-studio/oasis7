use super::private_goal_tests::{consumer, http};
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
use std::sync::{Arc, Mutex};

#[test]
fn executor_rotation_recovers_immutable_original_with_current_authority() {
    let (dir, mut driver, world, owner, owner_public) =
        canonical_agent_chat::driver_fixture_with_verified_authority();
    let create = game_task_tests::create(&world, &owner, "context-task");
    commit(&mut driver, 3, create.clone());
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private, [7; 32]).unwrap();
    let content = GoalContentV1 {
        goal_text: format!("完整任务正文{}", "x".repeat(1800)),
        soft_constraints: vec!["actual full constraint".into()],
    };
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: c.task_id.clone(),
            agent_id: c.agent_id.clone(),
            owner_public_key: owner_public.clone(),
            owner_binding: c.owner_binding.clone(),
            owner_generation: c.owner_generation,
            branch_id: c.branch_id.clone(),
            reorg_epoch: c.reorg_epoch,
            expected_task_revision: c.task_revision,
            expected_latest_goal_revision: c.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "context-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&c.task_id, DecimalU64(1), &content).unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let receipt = store.prepare(&goal).unwrap();
    let publish = publication(&goal, &receipt, "context-publish".into()).unwrap();
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
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
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
                        client_operation_id: "bind-context".into(),
                        fence: GameTaskFenceV1::from_control(&c),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "context-executor".into(),
                            executor_public_key: delegate_public.clone(),
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let fence = GameTaskFenceV1::from_control(&c);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: owner_public.clone(),
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let proof = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            schema_revision: 1,
            fence: fence.clone(),
            client_operation_id: "context-operation".into(),
            client_nonce: "cd".repeat(32),
        },
        &delegate,
    )
    .unwrap();

    let prepared = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&proof).unwrap()
    });
    let original = WorldServicePayloadV1::Scheduler(
        sign_read_request("scheduler", public_prefix(&prepared).unwrap(), &delegate).unwrap(),
    );
    let submit = request(&world, original.clone());
    // Ignore the transport ACK, then commit the exact original prefix.
    assert_eq!(
        http(&dir, &runtime, &signer, None, SUBMIT_PATH, &submit, "").0,
        200
    );
    commit(&mut driver, 7, submit.clone());
    assert!(
        game_task_tests::recorded(&driver, &submit)
            .rejected
            .is_none()
    );
    let correlation = derive_correlation(world.clone(), &original).unwrap();
    let access = |fence: GameTaskFenceV1, key: &str| {
        oasis7_client_api::world_service::entry_executor::EntryExecutorAccessV1 {
            read: sign_read_request(
                VIEW_JSON_PATH,
                ReadWorldViewRequest {
                    observation_nonce: Some("ab".repeat(32)),
                    formal_anchor: None,
                    contract_version: 1,
                    world: world.clone(),
                    scope_id: "agent:agent-a".into(),
                    min_commit: None,
                    fixed_commit: None,
                    deadline_unix_ms: None,
                },
                key,
            )
            .unwrap(),
            fence,
        }
    };
    let lookup =
        |access| oasis7_client_api::world_service::entry_executor::EntryExecutorLookupRequestV1 {
            access,
            lookup: oasis7_client_api::world_service_wire::AuthenticatedLookup {
                request: LookupIntentRequest {
                    contract_version: 1,
                    key: correlation.key.clone(),
                },
                original: serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap(),
            },
        };
    let old_access = access(fence.clone(), &delegate);
    let stop = request(
        &world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "rotation-stop".into(),
                    fence,
                    operation: GameExecutorOperationV1::Stop,
                },
                &owner,
            )
            .unwrap(),
        ),
    );
    commit(&mut driver, 8, stop);
    let delegation = |public: String, generation, revoked, nonce| {
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
                        delegate_public_key: public,
                        generation,
                        revoked,
                        nonce,
                    },
                    &owner,
                )
                .unwrap(),
            ),
        )
    };
    commit(&mut driver, 9, delegation(delegate_public, 2, true, 2));
    let suspended = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let path = oasis7_client_api::world_service::entry_executor::EXECUTOR_LOOKUP_PATH;
    let (status, owner_body) = http(
        &dir,
        &runtime,
        &signer,
        None,
        path,
        &lookup(access(GameTaskFenceV1::from_control(&suspended), &owner)),
        "",
    );
    assert_eq!(status, 200, "owner recovery: {owner_body}");
    assert_eq!(
        game_task::current_control(&driver.execution_world, "agent-a")
            .unwrap()
            .unwrap(),
        suspended
    );
    let next = hex::encode([74; 32]);
    let next_public = sign_read_request("key", (), &next)
        .unwrap()
        .subject_public_key;
    commit(
        &mut driver,
        10,
        delegation(next_public.clone(), 3, false, 3),
    );
    let bind = request(
        &world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "rotation-bind-b".into(),
                    fence: GameTaskFenceV1::from_control(&suspended),
                    operation: GameExecutorOperationV1::Bind {
                        executor_id: "executor-b".into(),
                        executor_public_key: next_public,
                        delegation_generation: DecimalU64(3),
                    },
                },
                &owner,
            )
            .unwrap(),
        ),
    );
    commit(&mut driver, 11, bind.clone());
    assert!(
        game_task_tests::recorded(&driver, &bind).rejected.is_none(),
        "bind B: {:?}",
        game_task_tests::recorded(&driver, &bind).rejected
    );
    let current = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let current_access = access(GameTaskFenceV1::from_control(&current), &next);
    let exact = lookup(current_access.clone());
    let (status, body) = http(&dir, &runtime, &signer, None, path, &exact, "");
    assert_eq!(status, 200, "rotated recovery: {body}");
    assert_eq!(
        body["payload"]["correlation"],
        owner_body["payload"]["correlation"]
    );
    assert_eq!(body["payload"]["outcome"], owner_body["payload"]["outcome"]);
    assert_eq!(
        body["payload"]["correlation"],
        serde_json::to_value(&correlation).unwrap()
    );
    assert!(matches!(
        serde_json::from_value::<IntentOutcome<serde_json::Value>>(
            body["payload"]["outcome"].clone()
        )
        .unwrap(),
        IntentOutcome::Committed { .. }
    ));
    assert_ne!(
        http(&dir, &runtime, &signer, None, path, &lookup(old_access), "").0,
        200
    );
    let revoked_current = lookup(access(GameTaskFenceV1::from_control(&current), &delegate));
    assert_ne!(
        http(&dir, &runtime, &signer, None, path, &revoked_current, "").0,
        200
    );
    for field in 0..3 {
        let mut bad = exact.clone();
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) =
            &mut bad.lookup.original
        else {
            unreachable!()
        };
        if field == 0 {
            signed.signature_hex = "00".repeat(64);
        } else {
            let oasis7_client_api::world_service_wire::SchedulerOperationV1::TaskProviderPrefix {
                context,
            } = &mut signed.request.operation
            else {
                unreachable!()
            };
            if field == 1 {
                context.fence.task_id.push_str("-other");
            } else {
                context.fence.owner_binding.push_str("-other");
            }
            *signed = sign_read_request("scheduler", signed.request.clone(), &delegate).unwrap();
        }
        assert_ne!(http(&dir, &runtime, &signer, None, path, &bad, "").0, 200);
    }
    // A fresh B access wrapper grants receipt reads, never permission to replay A's write.
    assert_ne!(
        http(&dir, &runtime, &signer, None, SUBMIT_PATH, &submit, "").0,
        200
    );
    let WorldServicePayloadV1::Scheduler(old) = &original else {
        unreachable!()
    };
    let replay_by_b = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", old.request.clone(), &next).unwrap(),
        ),
    );
    assert_ne!(
        http(&dir, &runtime, &signer, None, SUBMIT_PATH, &replay_by_b, "").0,
        200
    );
    std::fs::remove_dir_all(dir).unwrap();
}
