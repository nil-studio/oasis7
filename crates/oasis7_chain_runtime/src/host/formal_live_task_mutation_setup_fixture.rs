//! Genuine HTTP task/Goal/Bind fixture setup shared by mutation cases.
use super::*;
pub(super) fn setup_envelope(
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
pub(super) fn submit_setup(
    service: &FormalLiveService,
    q: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
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
pub(super) fn actual_task(world: &World) -> Option<GameTaskControlV1> {
    oasis7_runtime::service_admission::game_task::current_control(world, "agent").unwrap()
}
pub(super) fn install_task_through_actual_http(
    service: &FormalLiveService,
    publication: &PublishedReadState,
    owner: &str,
    delegate: &str,
    world: &WorldIdentity,
) -> GameTaskControlV1 {
    install_task_with_private_goal(service, publication, owner, delegate, world, None)
}
pub(super) fn install_task_with_private_goal(
    service: &FormalLiveService,
    publication: &PublishedReadState,
    owner: &str,
    delegate: &str,
    world: &WorldIdentity,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
) -> GameTaskControlV1 {
    install_task_for_agent_with_private_goal(
        service,
        publication,
        owner,
        delegate,
        world,
        "agent",
        private,
    )
}
pub(super) fn install_task_for_agent_with_private_goal(
    service: &FormalLiveService,
    publication: &PublishedReadState,
    owner: &str,
    delegate: &str,
    world: &WorldIdentity,
    agent: &str,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
) -> GameTaskControlV1 {
    let cut = publication.current().unwrap();
    let current_owner = cut.world.current_agent_owner(agent).unwrap();
    let owner_binding = current_owner.player_id.clone();
    let owner_generation = DecimalU64(current_owner.generation);
    let task = |world: &World| {
        oasis7_runtime::service_admission::game_task::current_control(world, agent).unwrap()
    };
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
                    agent_id: agent.into(),
                    owner_binding,
                    owner_generation,
                },
                owner,
            )
            .unwrap(),
        ),
    );
    submit_setup(service, &create);
    let created = wait_cut(publication, |world| task(world).is_some());
    let control = task(&created.world).unwrap();
    let goal_ref =
        private.map(|store| coordinate_tests::prepare_actual_goal(store, &control, owner));
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
        goal_ref: goal_ref.unwrap_or_else(|| "33".repeat(32)),
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
        task(world).is_some_and(|c| c.active_goal_revision.0 == 1)
    });
    let control = task(&published.world).unwrap();
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
        task(world).is_some_and(|c| c.executor_binding.is_some())
    });
    task(&bound.world).unwrap()
}

// Fixed classifications only; never log body, signature, request IDs or raw errors.
