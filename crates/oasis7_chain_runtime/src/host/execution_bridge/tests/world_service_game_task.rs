use super::*;
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
pub(super) fn create(
    identity: &WorldIdentity,
    owner: &str,
    id: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    request(
        identity,
        WorldServicePayloadV1::GameTaskCreate(
            sign_read_request(
                GAME_TASK_CREATE_DOMAIN,
                GameTaskCreateV1 {
                    api_rev: 1,
                    client_operation_id: id.into(),
                    world_id: identity.world_id.clone(),
                    genesis_digest: identity.genesis_digest.clone(),
                    branch_id: "main".into(),
                    reorg_epoch: DecimalU64(0),
                    agent_id: "agent-a".into(),
                    owner_binding: "owner-a".into(),
                    owner_generation: DecimalU64(1),
                },
                owner,
            )
            .unwrap(),
        ),
    )
}
pub(super) fn recorded(
    driver: &NodeRuntimeExecutionDriver,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> CanonicalIntentResultV1 {
    serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&correlation::key_digest(&request.correlation.key).unwrap()]
            .clone(),
    )
    .unwrap()
}
#[test]
fn game_task_driver_dedup_restart_and_owner_projection() {
    let (dir, mut driver, identity, owner, public) = canonical_agent_chat::driver_fixture();
    let first = create(&identity, &owner, "first");
    commit(&mut driver, 3, first.clone());
    let accepted = recorded(&driver, &first);
    assert!(accepted.rejected.is_none());
    commit(&mut driver, 4, first.clone());
    assert_eq!(recorded(&driver, &first), accepted);
    let second = create(&identity, &owner, "second");
    commit(&mut driver, 5, second.clone());
    assert!(
        recorded(&driver, &second)
            .rejected
            .unwrap()
            .contains("already exists")
    );
    let restarted = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .unwrap();
    assert_eq!(
        game_task::current_control(&restarted.execution_world, "agent-a")
            .unwrap()
            .unwrap()
            .created_operation_id,
        "first"
    );
    let projection = projection::WorldServiceProjection::from_authenticated_world(
        &restarted.execution_world,
        Some("agent-a"),
        &public,
    )
    .unwrap();
    assert!(projection.game_task_control.is_some());
    let delegated = projection::WorldServiceProjection::from_authenticated_world(
        &restarted.execution_world,
        Some("agent-a"),
        "different-key",
    )
    .unwrap();
    assert!(delegated.game_task_control.is_none());
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn game_task_driver_same_block_uniqueness_and_queued_owner_fence() {
    let (dir, mut driver, identity, owner, _) = canonical_agent_chat::driver_fixture();
    let first = create(&identity, &owner, "first");
    let second = create(&identity, &owner, "second");
    let actions = [&first, &second]
        .into_iter()
        .enumerate()
        .map(|(index, request)| {
            NodeConsensusAction::from_payload(
                10 + index as u64,
                "node-a",
                correlation::encode_consensus_intent(request).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 3,
            slot: 3,
            epoch: 0,
            node_block_hash: "node-h3".into(),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: 3000,
        })
        .unwrap();
    assert!(recorded(&driver, &first).rejected.is_none());
    assert!(
        recorded(&driver, &second)
            .rejected
            .unwrap()
            .contains("within block")
    );
    let queued = create(&identity, &owner, "queued");
    let WorldServicePayloadV1::GameTaskCreate(signed) = &queued.signed_payload else {
        unreachable!()
    };
    game_task::validate_owner(&driver.execution_world, signed).unwrap();
    driver
        .execution_world
        .install_capability_agent_identity("agent-a", "owner-a", 2)
        .unwrap();
    commit(&mut driver, 4, queued.clone());
    assert!(
        recorded(&driver, &queued)
            .rejected
            .unwrap()
            .contains("fence mismatch")
    );
    fs::remove_dir_all(dir).unwrap();
}

fn publish(
    identity: &WorldIdentity,
    owner: &str,
    control: &oasis7_agent_api::game::GameTaskControlV1,
    id: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    request(
        identity,
        WorldServicePayloadV1::GameGoalPublish(
            sign_read_request(
                oasis7_agent_api::game::GAME_GOAL_PUBLISH_DOMAIN,
                oasis7_agent_api::game::GameGoalPublishV1 {
                    api_rev: 1,
                    world_id: identity.world_id.clone(),
                    genesis_digest: identity.genesis_digest.clone(),
                    task_id: control.task_id.clone(),
                    agent_id: control.agent_id.clone(),
                    owner_public_key: control.owner_public_key.clone(),
                    owner_binding: control.owner_binding.clone(),
                    owner_generation: control.owner_generation,
                    branch_id: control.branch_id.clone(),
                    reorg_epoch: control.reorg_epoch,
                    expected_task_revision: control.task_revision,
                    expected_latest_goal_revision: control.latest_goal_revision,
                    goal_revision: DecimalU64(control.latest_goal_revision.0 + 1),
                    client_operation_id: id.into(),
                    prepare_operation_id: format!("prepared-{id}"),
                    goal_ref: "ab".repeat(32),
                },
                owner,
            )
            .unwrap(),
        ),
    )
}
#[test]
fn game_goal_publish_driver_activation_dedup_restart_and_stale_fence() {
    let (dir, mut driver, identity, owner, _) = canonical_agent_chat::driver_fixture();
    commit(&mut driver, 3, create(&identity, &owner, "task"));
    let before = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let first = publish(&identity, &owner, &before, "first");
    commit(&mut driver, 4, first.clone());
    let accepted = recorded(&driver, &first);
    assert!(accepted.rejected.is_none());
    let active = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    assert_eq!(
        active
            .active_goal_reference
            .as_ref()
            .unwrap()
            .activation_committed_height,
        DecimalU64(4)
    );
    assert_eq!(active.executor_epoch, DecimalU64(0));
    assert!(active.executor_id.is_none());
    commit(&mut driver, 5, first.clone());
    assert_eq!(recorded(&driver, &first), accepted);
    let stale = publish(&identity, &owner, &before, "stale");
    commit(&mut driver, 6, stale.clone());
    assert!(recorded(&driver, &stale).rejected.is_some());
    let restarted = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .unwrap();
    assert_eq!(
        game_task::current_control(&restarted.execution_world, "agent-a")
            .unwrap()
            .unwrap(),
        active
    );
    assert_eq!(
        game_task::active_publish(&restarted.execution_world, "agent-a")
            .unwrap()
            .unwrap(),
        accepted
    );
    fs::remove_dir_all(dir).unwrap();
}
