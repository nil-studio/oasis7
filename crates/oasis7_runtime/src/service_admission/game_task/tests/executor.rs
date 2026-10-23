use super::*;
use crate::service_admission::{SchedulerIntentV1, SchedulerOperationV1};
use oasis7_agent_api::game::{
    GAME_EXECUTOR_CONTROL_DOMAIN, GameExecutorControlV1, GameExecutorOperationV1, GameTaskFenceV1,
    TaskContextIdentityV1,
};

#[path = "prefix_lease.rs"]
mod prefix_lease;

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
    boot_with_fixture(fixture())
}
fn boot_with_fixture(
    (mut world, owner): (World, String),
) -> (World, String, String, GameTaskControlV1) {
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
                task_prefix_binding_revision: 0,
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
        authority_mode: None,
        native_authorization_root: None,
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
    let request = crate::RuntimeCognitionCommitRequestV1 {
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
    let mut artifact = crate::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: crate::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
            .into(),
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
        to: oasis7_agent_api::world_action::GeoPos::new(1, 0, 0),
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
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
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

#[test]
fn native_task_authority_needs_real_executor_but_no_provider_context() {
    let (mut world, owner, delegate, initial) = boot();
    assert!(world.capability_invocation_contexts().is_empty());
    let before = world.snapshot();
    assert!(
        world
            .canonical_native_task_authority(&GameTaskFenceV1::from_control(&initial))
            .is_err()
    );
    assert_eq!(world.snapshot(), before);
    let binding = execute(
        &world,
        executor_request(
            &owner,
            &initial,
            "native-bind",
            bind(&delegate, 1, "native-executor"),
        ),
        &[],
        3,
    );
    world.record_world_service_result(binding).unwrap();
    let current = current_control(&world, "agent").unwrap().unwrap();
    let fence = GameTaskFenceV1::from_control(&current);
    let authority = world.canonical_native_task_authority(&fence).unwrap();
    assert!(world.canonical_agent_service_context("agent").is_err());
    authority
        .validate_for(&fence, &world.current_cognition_base_binding().unwrap())
        .unwrap();
    assert_eq!(authority.valid_until_tick, world.state().time + 16);
    assert!(world.capability_invocation_contexts().is_empty());
    // A bare native World has no provider-bound global capability lane. The
    // task lane must nevertheless compare the actual action authority hashes.
    let context = TaskContextIdentityV1 {
        authority_mode: Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1),
        native_authorization_root: Some(authority.native_authorization_root.clone()),
        schema_revision: 2,
        fence: fence.clone(),
        agent_session_id: "native-session".into(),
        agent_turn_id: "native-turn".into(),
        decision_request_id: "native-decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_commitment: format!("blake3:{}", "11".repeat(32)),
        context_commitment: format!("blake3:{}", "22".repeat(32)),
        provider_invocation_commitment: format!("blake3:{}", "33".repeat(32)),
        valid_until_tick: DecimalU64(authority.valid_until_tick),
    };
    let mut prefix_world = world.clone();
    prefix_world
        .record_world_service_result(task_provider_prefix_result(
            &world,
            &context,
            &delegate,
            "native-prefix",
            4,
            None,
        ))
        .unwrap();
    let mut intent =
        oasis7_client_api::world_service::task_context::task_cognition_intent_from_public_context(
            &context,
            world.current_cognition_base_binding().unwrap(),
            oasis7_client_api::world_service::task_context::TaskAuthorityRef::Native(&authority),
            &serde_json::to_value(world.state().agents.get("agent").unwrap()).unwrap(),
            Action::MoveAgent {
                agent_id: "agent".into(),
                to: oasis7_agent_api::world_action::GeoPos::new(1, 0, 0),
            },
        )
        .unwrap();
    intent.request.capability_snapshot_hash = format!("blake3:{}", "99".repeat(32));
    let error = validate_task_commit_context(
        &prefix_world,
        &intent.request,
        &fence,
        &authority.signer_delegation.delegate_public_key,
        &[],
    )
    .unwrap_err();
    assert_eq!(error, "native task action authority root changed");
    let mut changed_world = world.clone();
    let source = changed_world.current_agent_owner("agent").unwrap();
    changed_world
        .install_test_native_delegation_grant(
            oasis7_agent_api::world_quote::AgentDelegationGrantV1 {
                grant_id: "native-task-budget".into(),
                source_id: source.source_id,
                issuer_id: source.player_id.clone(),
                owner_id: source.player_id,
                organization_id: None,
                agent_id: "agent".into(),
                object_id: "agent".into(),
                action_kinds: vec!["MoveAgent".into()],
                revision: 1,
                period_id: "native-context".into(),
                valid_from_tick: 0,
                valid_until_tick: 1000,
                limit_units: 10,
                resource_kind: "electricity".into(),
                revoked: false,
            },
        )
        .unwrap();
    assert_eq!(
        validate_task_context_identity(
            &changed_world,
            &context,
            &authority.signer_delegation.delegate_public_key,
            1,
            &[]
        )
        .unwrap_err(),
        "native task authority definition changed"
    );
    let mut damaged = authority.clone();
    damaged.signer_delegation.revoked = true;
    assert!(
        damaged
            .validate_for(&fence, &world.current_cognition_base_binding().unwrap())
            .is_err()
    );
    // A provider for a different Agent must not select a provider for this executor.
    world.submit_action(Action::RegisterAgent {
        agent_id: "other-agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .install_test_provider_capability_fixture_without_cognition_balance("other-agent")
        .unwrap();
    assert!(!world.capability_invocation_contexts().is_empty());
    assert!(world.canonical_agent_service_context("agent").is_err());
    // A finalized authority paired with the original pending Runtime binding is
    // a contradictory World, even when that provider belongs to another Agent.
    assert_eq!(
        world.canonical_native_task_authority(&fence).unwrap_err(),
        "reorg_invalidated"
    );
    let stopped = execute(
        &world,
        executor_request(
            &owner,
            &current,
            "native-stop",
            GameExecutorOperationV1::Stop,
        ),
        &[],
        4,
    );
    world.record_world_service_result(stopped).unwrap();
    assert!(world.canonical_native_task_authority(&fence).is_err());
}

#[test]
fn native_task_authority_accepts_consistent_finality_with_unrelated_provider() {
    let (mut world, owner, delegate, initial) = boot_with_fixture(fixture_with_initial_finality(
        false,
        Some(format!(
            "blake3:{}",
            blake3::hash(b"native-unrelated-provider-finality")
        )),
    ));
    let binding = execute(
        &world,
        executor_request(
            &owner,
            &initial,
            "native-coexist-bind",
            bind(&delegate, 1, "native-coexist-executor"),
        ),
        &[],
        3,
    );
    world.record_world_service_result(binding).unwrap();
    let fence = GameTaskFenceV1::from_control(&current_control(&world, "agent").unwrap().unwrap());
    assert!(world.canonical_agent_service_context("agent").is_err());
    assert!(world.canonical_agent_service_context("other-agent").is_ok());
    world.canonical_native_task_authority(&fence).unwrap();
}
