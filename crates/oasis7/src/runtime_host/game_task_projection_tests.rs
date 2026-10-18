use crate::runtime::{
    Action, World, WorldState,
    service_admission::{game_task::*, *},
};
use crate::world_service::projection;
use oasis7_agent_api::game::*;
use oasis7_agent_api::game::{
    GAME_EXECUTOR_CONTROL_DOMAIN, GameExecutorControlV1, GameExecutorOperationV1, GameTaskFenceV1,
};
use oasis7_client_api::world_service::sign_read_request;

fn delegated(world: &mut World, owner: &str, generation: u64, nonce: u64, delegate: &str) {
    let public = sign_read_request("key", (), delegate)
        .unwrap()
        .subject_public_key;
    world
        .apply_agent_signer_delegation(
            &sign_read_request(
                "delegation",
                AgentSignerDelegationChangeV1 {
                    world_id: "world".into(),
                    branch_id: "main".into(),
                    agent_id: "agent".into(),
                    owner_binding: "owner".into(),
                    agent_identity_generation: 1,
                    delegate_public_key: public,
                    generation,
                    revoked: false,
                    nonce,
                },
                owner,
            )
            .unwrap(),
        )
        .unwrap();
}
fn boot() -> (World, String, String, GameTaskControlV1) {
    let (mut world, owner) = fixture();
    let create = result(&world, request(&owner, "create"));
    let initial: GameTaskControlV1 = serde_json::from_value(create.receipt.clone()).unwrap();
    world.record_world_service_result(create).unwrap();
    let publish = goal_result(&world, goal_request(&owner, &initial, "goal"), &[], 2);
    world.record_world_service_result(publish).unwrap();
    let delegate = hex::encode([72; 32]);
    delegated(&mut world, &owner, 1, 1, &delegate);
    let control = current_control(&world, "agent").unwrap().unwrap();
    (world, owner, delegate, control)
}
fn executor_request(
    owner: &str,
    control: &GameTaskControlV1,
    id: &str,
    operation: GameExecutorOperationV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let payload = WorldServicePayloadV1::GameExecutorControl(
        sign_read_request(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            GameExecutorControlV1 {
                api_rev: 1,
                client_operation_id: id.into(),
                fence: GameTaskFenceV1::from_control(control),
                operation,
            },
            owner,
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    }
}
fn bind(delegate: &str, generation: u64, id: &str) -> GameExecutorOperationV1 {
    GameExecutorOperationV1::Bind {
        executor_id: id.into(),
        executor_public_key: sign_read_request("key", (), delegate)
            .unwrap()
            .subject_public_key,
        delegation_generation: DecimalU64(generation),
    }
}
fn execute(
    world: &World,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
    staged: &[CanonicalIntentResultV1],
    height: u64,
) -> CanonicalIntentResultV1 {
    CanonicalIntentResultV1 {
        receipt: prepare_executor_control(world, &request, staged).unwrap(),
        request,
        action_id: height,
        committed_height: height,
        rejected: None,
    }
}

fn fixture() -> (World, String) {
    fixture_with_formal_history(false)
}
fn fixture_with_formal_history(_formal: bool) -> (World, String) {
    let private = hex::encode([71u8; 32]);
    let public = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default())
        .try_with_cognition_scheduler(
            crate::runtime::SchedulerPolicyV1 {
                schema_version: crate::runtime::SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: crate::runtime::SchedulerPolicyV1::COMPARATOR.into(),
                service_order: crate::runtime::SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(public),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    (world, private)
}
fn request(private: &str, id: &str) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let create = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: id.into(),
        world_id: "world".into(),
        genesis_digest: "fixture".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(GAME_TASK_CREATE_DOMAIN, create, private).unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
fn result(
    world: &World,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
) -> CanonicalIntentResultV1 {
    CanonicalIntentResultV1 {
        receipt: prepare_create(world, &request, []).unwrap(),
        request,
        action_id: 1,
        committed_height: 1,
        rejected: None,
    }
}
fn goal_request(
    private: &str,
    c: &GameTaskControlV1,
    id: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let goal = GameGoalPublishV1 {
        api_rev: 1,
        world_id: c.world_id.clone(),
        genesis_digest: c.genesis_digest.clone(),
        task_id: c.task_id.clone(),
        agent_id: c.agent_id.clone(),
        owner_public_key: c.owner_public_key.clone(),
        owner_binding: c.owner_binding.clone(),
        owner_generation: c.owner_generation,
        branch_id: c.branch_id.clone(),
        reorg_epoch: c.reorg_epoch,
        expected_task_revision: c.task_revision,
        expected_latest_goal_revision: c.latest_goal_revision,
        goal_revision: DecimalU64(c.latest_goal_revision.0 + 1),
        client_operation_id: id.into(),
        prepare_operation_id: format!("prepare-{id}"),
        goal_ref: "ab".repeat(32),
    };
    let payload = WorldServicePayloadV1::GameGoalPublish(
        sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, goal, private).unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: c.world_id.clone(),
                genesis_digest: c.genesis_digest.clone(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
fn goal_result(
    world: &World,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
    staged: &[&CanonicalIntentResultV1],
    height: u64,
) -> CanonicalIntentResultV1 {
    CanonicalIntentResultV1 {
        receipt: prepare_publish(world, &request, staged.iter().copied(), height).unwrap(),
        request,
        action_id: 2,
        committed_height: height,
        rejected: None,
    }
}
fn goal_snapshot(world: &World) -> crate::runtime::Snapshot {
    world.snapshot_with_chain_resource_context(
        crate::runtime::ChainResourceDerivationContext {
            world_id: "world",
            chain_id: "fixture-chain",
            genesis_ref: None,
            created_at_height: world.state().time,
            manifest_height: world.state().time,
            commit_block_hash: None,
            tick: world.state().time,
        },
        "fixture",
        "fixture",
    )
}

#[test]
fn game_task_executor_bind_replace_stop_restart_and_metadata_audience() {
    let (mut world, owner, delegate, initial) = boot();
    let first = execute(
        &world,
        executor_request(&owner, &initial, "bind", bind(&delegate, 1, "executor-one")),
        &[],
        3,
    );
    world.record_world_service_result(first).unwrap();
    let active = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(active.task_revision.0, 3);
    assert_eq!(active.executor_epoch.0, 1);
    let fence = GameTaskFenceV1::from_control(&active);
    let history = validate_retention_history(&world).unwrap();
    assert_eq!(
        history.current_control("agent").unwrap(),
        Some(active.clone())
    );
    assert_eq!(
        history.active_publish("agent").unwrap(),
        active_publish(&world, "agent").unwrap()
    );
    let public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    validate_task_executor(&world, &fence, &public).unwrap();
    history.validate_task_executor(&fence, &public).unwrap();
    history.validate_task_reader(&fence, &public).unwrap();
    assert!(validate_task_executor(&world, &fence, &fence.owner_public_key).is_err());
    assert!(
        projection::WorldServiceProjection::from_authenticated_world(
            &world,
            Some("agent"),
            &public
        )
        .unwrap()
        .game_task_control
        .is_some()
    );
    let replacement = execute(
        &world,
        executor_request(
            &owner,
            &active,
            "replace",
            bind(&delegate, 1, "executor-two"),
        ),
        &[],
        4,
    );
    world.record_world_service_result(replacement).unwrap();
    assert!(validate_task_executor(&world, &fence, &public).is_err());
    let replaced = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(replaced.executor_epoch.0, 2);
    let replaced_history = validate_retention_history(&world).unwrap();
    assert!(
        replaced_history
            .validate_task_executor(&fence, &public)
            .is_err()
    );
    assert_eq!(
        replaced_history.current_control("agent").unwrap(),
        Some(replaced.clone())
    );
    let checkpoint = goal_snapshot(&world);
    let stopped = execute(
        &world,
        executor_request(&owner, &replaced, "stop", GameExecutorOperationV1::Stop),
        &[],
        5,
    );
    world.record_world_service_result(stopped).unwrap();
    let stopped = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(stopped.executor_epoch.0, 3);
    assert_eq!(stopped.task_status, GameTaskStatusV1::Suspended);
    assert!(stopped.executor_binding.is_none());
    let stopped_history = validate_retention_history(&world).unwrap();
    assert!(
        stopped_history
            .validate_task_executor(&GameTaskFenceV1::from_control(&stopped), &public)
            .is_err()
    );
    assert!(
        projection::WorldServiceProjection::from_authenticated_world(
            &world,
            Some("agent"),
            &public
        )
        .unwrap()
        .game_task_control
        .is_none()
    );
    assert!(validate_cognition_fence_staged(&world, "agent", None, &public, 1, []).is_err());
    let restored = World::from_snapshot(checkpoint, world.journal().clone()).unwrap();
    assert_eq!(current_control(&restored, "agent").unwrap(), Some(stopped));
    assert!(active_publish(&restored, "agent").unwrap().is_some());
}
