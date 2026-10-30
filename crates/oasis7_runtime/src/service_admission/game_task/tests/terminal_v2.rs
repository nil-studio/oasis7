use super::*;
use oasis7_agent_api::game::*;
fn hash<T: serde::Serialize>(value: &T) -> String {
    oasis7_wasm_abi::canonical_hash(value).unwrap()
}
struct EmptyNodes;
impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for EmptyNodes
{
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
fn setup() -> (World, String, GameTaskControlV1) {
    let (mut world, owner, delegate, control) =
        boot_with_fixture(fixture_with_formal_history(true));
    let bound = execute(
        &world,
        executor_request(&owner, &control, "bind", bind(&delegate, 1, "executor")),
        &[],
        3,
    );
    world.record_world_service_result(bound).unwrap();
    let control = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(control.task_status, GameTaskStatusV1::Active);
    (world, owner, control)
}
fn begin(world: &mut World) -> crate::RuntimeCommittedTickContext {
    let root = world.state().retained_history_root.clone().unwrap();
    let bytes = oasis7_world_state::codec::snapshot_cbor(&world.snapshot()).unwrap();
    // Bind the explicit local parent token to these exact snapshot bytes and
    // their actual logical time, matching the formal successor fixture contract.
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot_at_tick(
        root,
        &bytes,
        world.state().time,
    );
    world
        .attach_verified_formal_history(parent, std::sync::Arc::new(EmptyNodes))
        .unwrap();
    let last_task_commit = crate::service_admission::retained_queries::hot_results(world)
        .unwrap()
        .values()
        .map(|result| result.committed_height)
        .max()
        .unwrap_or(0);
    let context = crate::RuntimeCommittedTickContext {
        height: world
            .state()
            .time
            .max(last_task_commit)
            .checked_add(1)
            .unwrap(),
        slot: 0,
        epoch: 0,
        node_block_hash: "b".repeat(64),
        action_root: "c".repeat(64),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 1000,
    };
    world.begin_formal_execution_tick(&context).unwrap();
    context
}
fn close_for_conditions(
    world: &World,
    owner: &str,
    initial: &GameTaskControlV1,
    own_stop_intent: Option<GameOwnStopIntentV2>,
) -> SignedReadRequest<GameTaskRevokeAndCloseV2> {
    let delegation = &world.capability_revocation_state().agent_signer_delegations["agent"];
    let payload = GameTaskRevokeAndCloseV2 {
        api_revision: 2,
        client_operation_id: "close".into(),
        custody_account_id: "account".into(),
        termination_plan_digest: "44".repeat(32),
        permitted_branches: GameTaskCloseConditionsV2 {
            exact_initial_active: GameTaskFenceV1::from_control(initial),
            own_stop_intent,
        },
        signer_public_key: delegation.delegate_public_key.clone(),
        signer_generation: DecimalU64(delegation.generation),
        signer_nonce: DecimalU64(delegation.nonce),
        revoke_generation: DecimalU64(delegation.generation.checked_add(1).unwrap()),
        revoke_nonce: DecimalU64(delegation.nonce.checked_add(1).unwrap()),
        cleanup_until_ms: DecimalU64(5000),
    };
    sign_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, payload, owner).unwrap()
}
fn originals(
    world: &World,
    owner: &str,
    initial: &GameTaskControlV1,
) -> (
    SubmitIntentRequest<WorldServicePayloadV1>,
    SignedReadRequest<GameTaskRevokeAndCloseV2>,
) {
    let stop = executor_request(owner, initial, "own-stop", GameExecutorOperationV1::Stop);
    let WorldServicePayloadV1::GameExecutorControl(signed) = &stop.signed_payload else {
        unreachable!()
    };
    let intent = GameOwnStopIntentV2 {
        operation_id: "own-stop".into(),
        intent_digest: hash(&(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            &signed.subject_public_key,
            &signed.request,
        )),
    };
    let close = close_for_conditions(world, owner, initial, Some(intent));
    (stop, close)
}
#[test]
fn terminal_v2_stop_unknown_active_closes_and_legacy_cannot_resume() {
    let (mut world, owner, initial) = setup();
    assert!(active_publish(&world, "agent").unwrap().is_some());
    // No Stop request, nonce, signature or receipt is constructed here.
    let close = close_for_conditions(&world, &owner, &initial, None);
    assert!(close.request.permitted_branches.own_stop_intent.is_none());
    let context = begin(&mut world);
    let record = world.apply_task_revoke_and_close_v2(&close).unwrap();
    assert_eq!(
        record.closed.selected_predicate,
        GameTaskClosePredicateV2::InitialActive
    );
    assert!(world.capability_revocation_state().agent_signer_delegations["agent"].revoked);
    assert!(current_control(&world, "agent").is_err());
    assert!(active_publish(&world, "agent").is_err());
    assert!(matches!(
        world.current_game_task_state_v2("agent").unwrap(),
        Some(GameTaskStateV2::Closed(_))
    ));
    assert!(
        prepare_executor_control(
            &world,
            &executor_request(&owner, &initial, "late-stop", GameExecutorOperationV1::Stop),
            []
        )
        .is_err()
    );
    world.finish_formal_administrative_tick(&context).unwrap();
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert!(current_control(&restored, "agent").is_err());
    assert_eq!(
        restored
            .capability_revocation_state()
            .task_terminals_v2
            .as_ref()
            .unwrap()
            .records[&initial.task_id],
        record
    );
    let mut missing = world.snapshot();
    missing.capability_revocation_state.task_terminals_v2 = None;
    assert!(World::from_snapshot(missing, world.journal().clone()).is_err());
    let mut changed = world.snapshot();
    changed
        .capability_revocation_state
        .task_terminals_v2
        .as_mut()
        .unwrap()
        .records
        .get_mut(&initial.task_id)
        .unwrap()
        .closed
        .permanent_nonce_high_water
        .0 += 1;
    assert!(World::from_snapshot(changed, world.journal().clone()).is_err());
}
#[test]
fn terminal_v2_lost_stop_result_is_resolved_and_full_witness_is_permanent() {
    let (mut world, owner, initial) = setup();
    let (stop, close) = originals(&world, &owner, &initial);
    let committed = execute(&world, stop, &[], 4);
    world.record_world_service_result(committed).unwrap();
    // Caller gets no receipt; the pre-existing close request has only original identity.
    assert!(
        serde_json::to_value(&close.request).unwrap()["permitted_branches"]["own_stop_intent"]
            .get("canonical_result_digest")
            .is_none()
    );
    let context = begin(&mut world);
    let record = world.apply_task_revoke_and_close_v2(&close).unwrap();
    assert!(matches!(
        record.closed.selected_predicate,
        GameTaskClosePredicateV2::OwnStopSuspended(_)
    ));
    assert!(record.stop_predecessor_canonical_cbor.is_some());
    world.finish_formal_administrative_tick(&context).unwrap();
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert!(current_control(&restored, "agent").is_err());
}
#[test]
fn terminal_v2_failure_after_projection_preserves_whole_snapshot() {
    let (mut world, owner, initial) = setup();
    let (_, close) = originals(&world, &owner, &initial);
    let _context = begin(&mut world);
    let before = world.snapshot();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(world.apply_task_revoke_and_close_v2(&close).is_err());
    assert_eq!(world.snapshot(), before);
    assert!(!world.capability_revocation_state().agent_signer_delegations["agent"].revoked);
    assert!(
        world
            .capability_revocation_state()
            .task_terminals_v2
            .is_none()
    );
}

#[test]
fn terminal_v2_external_stop_and_expired_current_are_atomic_rejections() {
    let (mut world, owner, initial) = setup();
    let (_, close) = originals(&world, &owner, &initial);
    let external = executor_request(
        &owner,
        &initial,
        "external-stop",
        GameExecutorOperationV1::Stop,
    );
    let external_result = execute(&world, external, &[], 4);
    world.record_world_service_result(external_result).unwrap();
    let _context = begin(&mut world);
    let before = world.snapshot();
    assert!(world.apply_task_revoke_and_close_v2(&close).is_err());
    assert_eq!(world.snapshot(), before);
    let (mut world, owner, initial) = setup();
    let (_, original) = originals(&world, &owner, &initial);
    let mut payload = original.request;
    payload.cleanup_until_ms = DecimalU64(999);
    let expired = sign_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, payload, &owner).unwrap();
    let _context = begin(&mut world);
    let before = world.snapshot();
    assert!(world.apply_task_revoke_and_close_v2(&expired).is_err());
    assert_eq!(world.snapshot(), before);
}
#[test]
fn terminal_v2_requires_actual_begun_committed_context_and_advancing_hwm() {
    let (mut world, owner, initial) = setup();
    let (_, close) = originals(&world, &owner, &initial);
    let before = world.snapshot();
    assert!(world.apply_task_revoke_and_close_v2(&close).is_err());
    assert_eq!(world.snapshot(), before);
    let mut payload = close.request;
    payload.revoke_nonce = payload.signer_nonce;
    let stale_hwm = sign_read_request(GAME_TASK_CLEANUP_DOMAIN_V2, payload, &owner).unwrap();
    let _context = begin(&mut world);
    let before = world.snapshot();
    assert!(world.apply_task_revoke_and_close_v2(&stale_hwm).is_err());
    assert_eq!(world.snapshot(), before);
}

#[test]
fn terminal_v2_validated_history_owner_reader_and_cognition_cannot_bypass_closed() {
    let (mut world, owner, initial) = setup();
    let (_, close) = originals(&world, &owner, &initial);
    let fence = GameTaskFenceV1::from_control(&initial);
    let executor = fence
        .executor_binding
        .as_ref()
        .unwrap()
        .executor_public_key
        .clone();
    {
        let history = validate_retention_history(&world).unwrap();
        history
            .validate_task_reader(&fence, &fence.owner_public_key)
            .unwrap();
        history
            .validate_cognition_fence("agent", Some(&fence), &executor, 1)
            .unwrap();
        assert!(history.current_control("agent").unwrap().is_some());
    }
    let _context = begin(&mut world);
    world.apply_task_revoke_and_close_v2(&close).unwrap();
    let history = validate_retention_history(&world).unwrap();
    assert!(
        history
            .validate_task_reader(&fence, &fence.owner_public_key)
            .is_err()
    );
    assert!(
        history
            .validate_cognition_fence("agent", Some(&fence), &executor, 1)
            .is_err()
    );
    assert!(history.validate_task_executor(&fence, &executor).is_err());
    assert!(history.current_control("agent").is_err());
    assert!(history.active_publish("agent").is_err());
}

#[test]
fn terminal_v2_native_context_identity_has_real_positive_then_closed_rejection() {
    let (mut world, owner, initial) = setup();
    let (_, close) = originals(&world, &owner, &initial);
    let fence = GameTaskFenceV1::from_control(&initial);
    let authority = world.canonical_native_task_authority(&fence).unwrap();
    let executor = fence
        .executor_binding
        .as_ref()
        .unwrap()
        .executor_public_key
        .clone();
    let context = TaskContextIdentityV1 {
        authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
        native_authorization_root: Some(authority.native_authorization_root),
        schema_revision: 2,
        fence,
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
    validate_task_context_identity(&world, &context, &executor, 1, &[]).unwrap();
    let _committed = begin(&mut world);
    world.apply_task_revoke_and_close_v2(&close).unwrap();
    assert!(validate_task_context_identity(&world, &context, &executor, 1, &[]).is_err());
}

#[path = "terminal_v2_retention.rs"]
mod retention;

#[test]
fn terminal_v2_allocated_unsigned_stop_intent_does_not_require_stop_signature() {
    let (mut world, owner, initial) = setup();
    let unsigned = GameExecutorControlV1 {
        api_rev: 1,
        client_operation_id: "unsigned-stop".into(),
        fence: GameTaskFenceV1::from_control(&initial),
        operation: GameExecutorOperationV1::Stop,
    };
    let intent = GameOwnStopIntentV2 {
        operation_id: unsigned.client_operation_id.clone(),
        intent_digest: hash(&(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            &initial.owner_public_key,
            &unsigned,
        )),
    };
    // Only the independent close original is signed; Stop has no signature.
    let close = close_for_conditions(&world, &owner, &initial, Some(intent));
    let _context = begin(&mut world);
    let result = world.apply_task_revoke_and_close_v2(&close).unwrap();
    assert_eq!(
        result.closed.selected_predicate,
        GameTaskClosePredicateV2::InitialActive
    );
    assert!(result.stop_predecessor_canonical_cbor.is_none());
}

#[test]
fn terminal_v2_same_qualified_context_reexecution_is_deterministic() {
    let (mut first, owner, initial) = setup();
    let close = close_for_conditions(&first, &owner, &initial, None);
    let mut replay = first.clone();
    let first_context = begin(&mut first);
    let replay_context = begin(&mut replay);
    assert_eq!(first_context, replay_context);
    let first_result = first.apply_task_revoke_and_close_v2(&close).unwrap();
    let replay_result = replay.apply_task_revoke_and_close_v2(&close).unwrap();
    assert_eq!(first_result, replay_result);
    first
        .finish_formal_administrative_tick(&first_context)
        .unwrap();
    replay
        .finish_formal_administrative_tick(&replay_context)
        .unwrap();
    assert_eq!(first.snapshot(), replay.snapshot());
    assert_eq!(
        serde_cbor::to_vec(first.journal()).unwrap(),
        serde_cbor::to_vec(replay.journal()).unwrap()
    );
}

#[path = "terminal_v2_scope_rotation.rs"]
mod scope_rotation;
