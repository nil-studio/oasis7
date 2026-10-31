use super::*;
#[derive(Clone)]
struct ArchivedNodes(std::collections::BTreeMap<String, Vec<u8>>);
impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for ArchivedNodes
{
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}
#[test]
fn terminal_v2_actual_retention_prunes_stop_but_cold_restore_keeps_closed_and_floor() {
    let (mut world, owner, initial) = setup();
    assert!(active_publish(&world, "agent").unwrap().is_some());
    let (stop, close) = originals(&world, &owner, &initial);
    let stop_key =
        crate::service_admission::correlation::key_digest(&stop.correlation.key).unwrap();
    let committed = execute(&world, stop, &[], 4);
    world.record_world_service_result(committed).unwrap();
    // Genuine signed requests with actual deterministic admission failures,
    // not fabricated successful operations, push the actual retention limit.
    for i in 0..40 {
        let request = request(&owner, &format!("rejected-create-{i}"));
        let reason = prepare_create(&world, &request, []).unwrap_err();
        let rejected = CanonicalIntentResultV1 {
            request,
            action_id: 100 + i,
            committed_height: 4,
            receipt: serde_json::Value::Null,
            rejected: Some(reason),
        };
        world.record_world_service_result(rejected).unwrap();
    }
    let context = begin(&mut world);
    let closed = world.apply_task_revoke_and_close_v2(&close).unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    let index_before = world
        .capability_revocation_state()
        .task_terminals_v2
        .clone();
    let authority_before = world.snapshot().capability_authorization_root;
    let plan = world.prepare_formal_retention_candidate().unwrap();
    assert!(plan.world_service_result_keys_to_prune.contains(&stop_key));
    let nodes = ArchivedNodes(
        plan.delta
            .as_ref()
            .unwrap()
            .node_writes
            .iter()
            .map(|write| (write.digest.clone(), write.bytes.clone()))
            .collect(),
    );
    world.apply_formal_retention_plan(&plan).unwrap();
    assert!(
        !world
            .capability_revocation_state()
            .world_service_results
            .contains_key(&stop_key)
    );
    assert_eq!(
        world.capability_revocation_state().task_terminals_v2,
        index_before
    );
    assert_eq!(
        world.snapshot().capability_authorization_root,
        authority_before
    );
    let snapshot = world.snapshot();
    let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        snapshot.state.retained_history_root.clone().unwrap(),
        &bytes,
    );
    let restored = World::from_verified_formal_snapshot(
        snapshot,
        world.journal().clone(),
        parent,
        std::sync::Arc::new(nodes),
    )
    .unwrap();
    assert_eq!(
        restored.closed_game_task_v2(&initial.task_id).unwrap(),
        Some(closed.closed)
    );
    assert!(current_control(&restored, "agent").is_err());
    assert!(active_publish(&restored, "agent").is_err());
    assert!(
        prepare_executor_control(
            &restored,
            &executor_request(
                &owner,
                &initial,
                "cold-late-stop",
                GameExecutorOperationV1::Stop
            ),
            []
        )
        .is_err()
    );
    assert!(prepare_create(&restored, &request(&owner, "create"), []).is_err());
    let history = validate_retention_history(&restored).unwrap();
    let fence = GameTaskFenceV1::from_control(&initial);
    assert!(
        history
            .validate_task_reader(&fence, &fence.owner_public_key)
            .is_err()
    );
    assert!(history.current_control("agent").is_err());
}

#[test]
fn terminal_v2_real_max_signer_inventory_has_no_wrapping_close() {
    let (mut world, owner, initial) = setup();
    let (_, template) = originals(&world, &owner, &initial);
    let delegate = hex::encode([72; 32]);
    delegated(&mut world, &owner, u64::MAX, u64::MAX, &delegate);
    let rebound = execute(
        &world,
        executor_request(
            &owner,
            &initial,
            "bind-max",
            bind(&delegate, u64::MAX, "executor-max"),
        ),
        &[],
        4,
    );
    world.record_world_service_result(rebound).unwrap();
    let max_control = current_control(&world, "agent").unwrap().unwrap();
    let actual = &world.capability_revocation_state().agent_signer_delegations["agent"];
    assert_eq!(actual.generation, u64::MAX);
    assert_eq!(actual.nonce, u64::MAX);
    assert!(actual.generation.checked_add(1).is_none());
    assert!(actual.nonce.checked_add(1).is_none());
    let stop = executor_request(
        &owner,
        &max_control,
        "own-stop-max",
        GameExecutorOperationV1::Stop,
    );
    let WorldServicePayloadV1::GameExecutorControl(stop) = stop.signed_payload else {
        unreachable!()
    };
    let mut payload = template.request;
    payload.permitted_branches.exact_initial_active = GameTaskFenceV1::from_control(&max_control);
    payload.permitted_branches.own_stop_intent = Some(GameOwnStopIntentV2 {
        operation_id: "own-stop-max".into(),
        intent_digest: hash(&(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            &stop.subject_public_key,
            &stop.request,
        )),
    });
    payload.signer_generation = DecimalU64(u64::MAX);
    payload.signer_nonce = DecimalU64(u64::MAX);
    payload.revoke_generation = DecimalU64(u64::MAX);
    payload.revoke_nonce = DecimalU64(u64::MAX);
    let close = sign_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, payload, &owner).unwrap();
    let _context = begin(&mut world);
    let before = world.snapshot();
    assert!(world.apply_task_revoke_and_close_v2(&close).is_err());
    assert_eq!(world.snapshot(), before);
}
