use super::*;
use crate::runtime::service_admission::{SchedulerIntentV1, SchedulerOperationV1};
use crate::world_service::projection;
use oasis7_agent_api::game::{
    GAME_EXECUTOR_CONTROL_DOMAIN, GameExecutorControlV1, GameExecutorOperationV1, GameTaskFenceV1,
    TaskContextIdentityV1,
};

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
    let initial = create_control(&create.request).unwrap();
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

fn task_provider_prefix_result(
    world: &World,
    context: &TaskContextIdentityV1,
    private: &str,
    request_id: &str,
    committed_height: u64,
    rejected: Option<&str>,
) -> CanonicalIntentResultV1 {
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: context.fence.agent_id.clone(),
                request_id: request_id.into(),
                delegation_generation: context
                    .fence
                    .executor_binding
                    .as_ref()
                    .unwrap()
                    .delegation_generation
                    .0,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::TaskProviderPrefix {
                    context: context.clone(),
                },
            },
            private,
        )
        .unwrap(),
    );
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: context.fence.world_id.clone(),
                genesis_digest: context.fence.genesis_digest.clone(),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    };
    CanonicalIntentResultV1 {
        request,
        action_id: committed_height,
        committed_height,
        receipt: serde_json::Value::Null,
        rejected: rejected.map(str::to_owned),
    }
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
#[test]
fn game_task_executor_staged_conflict_goal_change_and_revocation_are_fenced() {
    let (mut world, owner, delegate, initial) = boot();
    let first = execute(
        &world,
        executor_request(&owner, &initial, "bind", bind(&delegate, 1, "one")),
        &[],
        3,
    );
    assert!(
        prepare_executor_control(
            &world,
            &executor_request(&owner, &initial, "conflict", bind(&delegate, 1, "two")),
            [&first]
        )
        .is_err()
    );
    let active = reduce(collect(&world, [&first]).unwrap())
        .unwrap()
        .into_values()
        .next()
        .unwrap()
        .control;
    let old_fence = GameTaskFenceV1::from_control(&active);
    let public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    validate_cognition_fence_staged(&world, "agent", Some(&old_fence), &public, 1, [&first])
        .unwrap();
    let stop = execute(
        &world,
        executor_request(&owner, &active, "stop", GameExecutorOperationV1::Stop),
        std::slice::from_ref(&first),
        3,
    );
    assert!(
        validate_cognition_fence_staged(
            &world,
            "agent",
            Some(&old_fence),
            &public,
            1,
            [&first, &stop]
        )
        .is_err()
    );
    assert!(
        validate_cognition_fence_staged(&world, "agent", None, &public, 1, [&first, &stop])
            .is_err()
    );
    world.record_world_service_result(first).unwrap();
    let publication = goal_result(&world, goal_request(&owner, &active, "changed"), &[], 4);
    assert!(
        validate_cognition_fence_staged(
            &world,
            "agent",
            Some(&old_fence),
            &public,
            1,
            [&publication]
        )
        .is_err()
    );
    delegated(&mut world, &owner, 2, 2, &hex::encode([73; 32]));
    assert!(validate_task_executor(&world, &old_fence, &public).is_err());
    let revoked_history = validate_retention_history(&world).unwrap();
    assert!(
        revoked_history
            .validate_task_executor(&old_fence, &public)
            .is_err()
    );
}

#[test]
fn validated_history_selects_only_one_accepted_task_provider_prefix() {
    let (mut world, owner, delegate, initial) = boot();
    let binding = execute(
        &world,
        executor_request(
            &owner,
            &initial,
            "bind-prefix",
            bind(&delegate, 1, "executor"),
        ),
        &[],
        3,
    );
    world.record_world_service_result(binding).unwrap();
    let control = current_control(&world, "agent").unwrap().unwrap();
    let base = world.current_cognition_base_binding().unwrap();
    let context = TaskContextIdentityV1 {
        schema_revision: 1,
        fence: GameTaskFenceV1::from_control(&control),
        agent_session_id: "session-prefix".into(),
        agent_turn_id: "turn-prefix".into(),
        decision_request_id: "request-prefix".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_commitment: format!("blake3:{}", "11".repeat(32)),
        context_commitment: format!("blake3:{}", "22".repeat(32)),
        provider_invocation_commitment: format!("blake3:{}", "33".repeat(32)),
        valid_until_tick: DecimalU64(base.base_tick + 1),
    };
    let delegate_public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;

    let rejected = task_provider_prefix_result(
        &world,
        &context,
        &delegate,
        "rejected-prefix",
        4,
        Some("prefix_rejected"),
    );
    let accepted =
        task_provider_prefix_result(&world, &context, &delegate, "accepted-prefix", 5, None);
    world.record_world_service_result(rejected).unwrap();
    let rejected_history = validate_retention_history(&world).unwrap();
    assert_eq!(
        rejected_history
            .matching_task_prefix(&context, &delegate_public)
            .unwrap(),
        None,
        "a rejected matching record must not be selected as a canonical prefix"
    );

    world.record_world_service_result(accepted.clone()).unwrap();

    let history = validate_retention_history(&world).unwrap();
    assert_eq!(
        history
            .matching_task_prefix(&context, &delegate_public)
            .unwrap(),
        Some(&accepted),
        "a rejected matching record must not hide or replace the accepted prefix"
    );

    let duplicate = task_provider_prefix_result(
        &world,
        &context,
        &delegate,
        "second-accepted-prefix",
        6,
        None,
    );
    world.record_world_service_result(duplicate).unwrap();
    let duplicated_history = validate_retention_history(&world).unwrap();
    assert!(
        duplicated_history
            .matching_task_prefix(&context, &delegate_public)
            .is_err(),
        "two accepted matching prefixes must remain ambiguous"
    );
}

#[test]
fn game_task_executor_invalid_history_tail_and_weak_or_foreign_bind_fail_closed() {
    let (mut world, owner, delegate, initial) = boot();
    let first_request = executor_request(&owner, &initial, "bind", bind(&delegate, 1, "one"));
    let first = execute(&world, first_request, &[], 3);
    world.record_world_service_result(first).unwrap();
    let checkpoint = goal_snapshot(&world);
    let conflict = executor_request(&owner, &initial, "conflict", bind(&delegate, 1, "two"));
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            receipt: serde_json::to_value(executor::executor_receipt(&conflict).unwrap()).unwrap(),
            request: conflict,
            action_id: 4,
            committed_height: 4,
            rejected: None,
        })
        .unwrap();
    let error = match World::from_snapshot(checkpoint, world.journal().clone()) {
        Ok(_) => panic!("conflicting executor tail must fail restore"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("executor predecessor"));
    let (world, owner, _, initial) = boot();
    let foreign = executor_request(
        &owner,
        &initial,
        "foreign",
        bind(&hex::encode([74; 32]), 1, "foreign"),
    );
    assert!(prepare_executor_control(&world, &foreign, []).is_err());
    let proof = sign_read_request(
        GAME_EXECUTOR_CONTROL_DOMAIN,
        GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: "weak".into(),
            fence: GameTaskFenceV1::from_control(&initial),
            operation: GameExecutorOperationV1::Bind {
                executor_id: "weak".into(),
                executor_public_key: format!("01{}", "00".repeat(31)),
                delegation_generation: DecimalU64(1),
            },
        },
        &owner,
    )
    .unwrap();
    assert!(verify_executor_control(&proof).is_err());
}

#[test]
fn game_task_executor_core_commit_rejects_unfenced_and_uncaptured_actions_without_mutation() {
    let (mut world, owner, delegate, initial) = boot();
    let accepted = execute(
        &world,
        executor_request(&owner, &initial, "bind", bind(&delegate, 1, "one")),
        &[],
        3,
    );
    world.record_world_service_result(accepted).unwrap();
    let fence = GameTaskFenceV1::from_control(&current_control(&world, "agent").unwrap().unwrap());
    let public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    let request = crate::runtime::RuntimeCognitionCommitRequestV1 {
        agent_id: "agent".into(),
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: format!("blake3:{}", "11".repeat(32)),
        observation_digest: format!("blake3:{}", "22".repeat(32)),
        context_digest: format!("blake3:{}", "33".repeat(32)),
        capability_snapshot_hash: format!("blake3:{}", "44".repeat(32)),
        authority_context_hash: format!("blake3:{}", "55".repeat(32)),
        captured_base_binding: world.current_cognition_base_binding().unwrap(),
    };
    let mut artifact = crate::runtime::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator:
            crate::runtime::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: request.request_digest.clone(),
        response_digest: format!("blake3:{}", "66".repeat(32)),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let before = world.snapshot();
    let action = Action::MoveAgent {
        agent_id: "agent".into(),
        to: crate::GeoPos::new(1, 0, 0),
    };
    let error = world
        .commit_cognition_action(request.clone(), action.clone(), artifact.clone())
        .unwrap_err();
    assert!(format!("{error:?}").contains("cognition_task_fenced"));
    let error = world
        .commit_task_cognition_action(request, action, artifact, (&fence, &public, 1), &[])
        .unwrap_err();
    assert!(format!("{error:?}").contains("cognition_task_fenced"));
    assert_eq!(world.snapshot(), before);
}

#[path = "legacy_retirement.rs"]
mod legacy_retirement;

#[test]
fn game_executor_cold_revision_chain_accepts_next_control() {
    let (mut world, owner) = fixture_with_formal_history(true);
    let create = result(&world, request(&owner, "retention-create"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let publish = goal_result(
        &world,
        goal_request(&owner, &initial, "retention-goal"),
        &[],
        2,
    );
    world.record_world_service_result(publish).unwrap();
    let delegate = hex::encode([72; 32]);
    delegated(&mut world, &owner, 1, 1, &delegate);
    for revision in 0..40 {
        let control = current_control(&world, "agent").unwrap().unwrap();
        let accepted = execute(
            &world,
            executor_request(
                &owner,
                &control,
                &format!("retain-bind-{revision}"),
                bind(&delegate, 1, &format!("executor-{revision}")),
            ),
            &[],
            revision + 3,
        );
        world.record_world_service_result(accepted).unwrap();
    }
    let expected = current_control(&world, "agent").unwrap().unwrap();
    let nodes = archive_task_history(&mut world);
    let parent = crate::runtime::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state().retained_history_root.clone().unwrap(),
        &serde_cbor::to_vec(&world.snapshot()).unwrap(),
    );
    let mut world = World::from_verified_formal_snapshot(
        world.snapshot(),
        world.journal().clone(),
        parent,
        std::sync::Arc::new(nodes),
    )
    .unwrap();
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .len()
            < 42
    );
    assert_eq!(current_control(&world, "agent").unwrap().unwrap(), expected);
    let next = execute(
        &world,
        executor_request(
            &owner,
            &expected,
            "after-cold-bind",
            bind(&delegate, 1, "after-cold-executor"),
        ),
        &[],
        44,
    );
    world.record_world_service_result(next).unwrap();
    assert_eq!(
        current_control(&world, "agent")
            .unwrap()
            .unwrap()
            .executor_epoch
            .0,
        41
    );
}
