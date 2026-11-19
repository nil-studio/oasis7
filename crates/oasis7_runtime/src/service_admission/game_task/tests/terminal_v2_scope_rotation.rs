//! Legitimate Runtime Owner/history generation rotation, not custody MFA qualification.
use super::*;
fn create_in_current_scope(
    world: &World,
    owner: &str,
    id: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let identity = world.current_agent_identity("agent").unwrap();
    let original = request(owner, id);
    let WorldServicePayloadV1::GameTaskCreate(signed) = original.signed_payload else {
        unreachable!()
    };
    let mut payload = signed.request;
    payload.owner_binding = identity.owner_binding.clone();
    payload.owner_generation = DecimalU64(identity.generation);
    let binding = world.current_cognition_runtime_binding().unwrap();
    payload.branch_id = binding.branch_id;
    payload.reorg_epoch = DecimalU64(binding.reorg_epoch);
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(GAME_TASK_CREATE_DOMAIN, payload, owner).unwrap(),
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
#[test]
fn terminal_v2_new_owner_generation_can_create_publish_bind_new_id_without_taskless_escape() {
    let (mut world, owner, initial) = setup();
    let close = close_for_conditions(&world, &owner, &initial, None);
    let first = begin(&mut world);
    world.apply_task_revoke_and_close_v2(&close).unwrap();
    world.finish_formal_administrative_tick(&first).unwrap();
    let next = begin(&mut world);
    world
        .install_capability_agent_identity("agent", "owner", 2)
        .unwrap();
    assert_eq!(world.current_agent_identity("agent").unwrap().generation, 2);
    // This exact legitimate new scope was accepted by V1 before terminal support.
    assert!(current_control(&world, "agent").unwrap().is_none());
    assert!(
        validate_retention_history(&world)
            .unwrap()
            .current_control("agent")
            .unwrap()
            .is_none()
    );
    assert!(
        validate_cognition_fence_staged(&world, "agent", None, &initial.owner_public_key, 0, [])
            .is_err()
    );
    // Reusing the original task operation still derives the same permanent task ID.
    let revive = create_in_current_scope(&world, &owner, "create");
    let WorldServicePayloadV1::GameTaskCreate(signed) = &revive.signed_payload else {
        unreachable!()
    };
    validate_owner(&world, signed).unwrap();
    assert_eq!(create_control(&revive).unwrap().task_id, initial.task_id);
    let before = world.snapshot();
    assert!(prepare_create(&world, &revive, []).is_err());
    assert_eq!(world.snapshot(), before);
    // A genuinely new ID is an ordinary V1 Create in the new authorized scope.
    let create = create_in_current_scope(&world, &owner, "new-task-create");
    let created = create_control(&create).unwrap();
    assert_ne!(created.task_id, initial.task_id);
    let receipt = prepare_create(&world, &create, []).unwrap();
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: create,
            receipt,
            action_id: 100,
            committed_height: next.height,
            rejected: None,
        })
        .unwrap();
    let publish = goal_request(&owner, &created, "new-goal");
    let published = goal_result(&world, publish, &[], next.height);
    world.record_world_service_result(published).unwrap();
    // New canonical Owner scope installs an actual new signer above old permanent HWM.
    let delegate = hex::encode([72; 32]);
    let public = sign_read_request("key", (), &delegate)
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
                    agent_identity_generation: 2,
                    generation: 3,
                    delegate_public_key: public.clone(),
                    revoked: false,
                    nonce: 3,
                },
                &owner,
            )
            .unwrap(),
        )
        .unwrap();
    let control = current_control(&world, "agent").unwrap().unwrap();
    let binding = execute(
        &world,
        executor_request(
            &owner,
            &control,
            "new-bind",
            bind(&delegate, 3, "new-executor"),
        ),
        &[],
        next.height,
    );
    world.record_world_service_result(binding).unwrap();
    let current = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(current.task_id, created.task_id);
    assert_eq!(current.owner_generation, DecimalU64(2));
    assert_eq!(current.task_status, GameTaskStatusV1::Active);
    let fence = GameTaskFenceV1::from_control(&current);
    validate_task_reader(&world, &fence, &current.owner_public_key).unwrap();
    validate_cognition_fence_staged(&world, "agent", Some(&fence), &public, 3, []).unwrap();
    let history = validate_retention_history(&world).unwrap();
    history
        .validate_task_reader(&fence, &current.owner_public_key)
        .unwrap();
    assert_eq!(
        history.current_control("agent").unwrap().unwrap().task_id,
        created.task_id
    );
    assert!(
        validate_task_reader(
            &world,
            &GameTaskFenceV1::from_control(&initial),
            &initial.owner_public_key
        )
        .is_err()
    );
    assert!(
        validate_cognition_fence_staged(&world, "agent", None, &initial.owner_public_key, 0, [])
            .is_err()
    );
    assert!(
        world
            .closed_game_task_v2(&initial.task_id)
            .unwrap()
            .is_some()
    );
    world.finish_formal_administrative_tick(&next).unwrap();
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(
        current_control(&restored, "agent")
            .unwrap()
            .unwrap()
            .task_id,
        created.task_id
    );
    assert!(
        restored
            .closed_game_task_v2(&initial.task_id)
            .unwrap()
            .is_some()
    );
}
