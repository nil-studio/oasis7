//! Real Owner-signed Some grant remains inside Runtime root/history/Close witness.
use super::*;
struct NoArchivedNodes;
impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for NoArchivedNodes
{
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
#[test]
fn guarded_stop_v2_observation_grant_is_permanent_and_close_witness_bound() {
    let (base, owner, initial) = setup();
    let old = guarded_stop(&base, &owner, &initial, "grant-stop", 5000);
    let mut p = old.request.clone();
    p.observation_grant =
        Some(StopObservationGrantV2::known_control_for_exact_target(&p, 5000, 4500, 4000).unwrap());
    let signed = sign_read_request(GAME_TASK_STOP_DOMAIN_V2, p, &owner).unwrap();
    let stop = SignedGameTaskStopV2 {
        request: signed.request,
        subject_public_key: signed.subject_public_key,
        signature_hex: signed.signature_hex,
    };
    let mut ordinary = base.clone();
    let ctx = begin(&mut ordinary);
    ordinary.apply_guarded_task_stop_v2(&old).unwrap();
    ordinary.finish_formal_administrative_tick(&ctx).unwrap();
    let mut world = base.clone();
    let ctx = begin(&mut world);
    let record = world.apply_guarded_task_stop_v2(&stop).unwrap();
    world.finish_formal_administrative_tick(&ctx).unwrap();
    assert_eq!(
        record.original.request.observation_grant,
        stop.request.observation_grant
    );
    assert_ne!(
        world.snapshot().capability_authorization_root,
        ordinary.snapshot().capability_authorization_root
    );
    assert_eq!(
        current_control(&world, "agent").unwrap(),
        current_control(&ordinary, "agent").unwrap()
    );
    let intent = GameOwnStopIntentV2 {
        profile: GameStopIntentProfileV2::GuardedTaskStopV2,
        operation_id: stop.request.client_operation_id.clone(),
        intent_digest: oasis7_wasm_abi::canonical_hash(&(
            GAME_TASK_STOP_DOMAIN_V2,
            &stop.subject_public_key,
            &stop.request,
        ))
        .unwrap(),
    };
    let close = close_for_conditions(&world, &owner, &initial, Some(intent));
    let ctx = begin(&mut world);
    let terminal = world.apply_task_revoke_and_close_v2(&close).unwrap();
    world.finish_formal_administrative_tick(&ctx).unwrap();
    let bytes = terminal.stop_predecessor_canonical_cbor.as_ref().unwrap();
    let archived: GameTaskStopRecordV2 = serde_cbor::from_slice(bytes).unwrap();
    assert_eq!(archived, record);
    assert_eq!(
        archived.original.request.observation_grant,
        stop.request.observation_grant
    );
    let mut stripped = archived.original.clone();
    stripped.request.observation_grant = None;
    assert!(
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_TASK_STOP_DOMAIN_V2,
            &stripped.request,
            &stripped.subject_public_key,
            &stripped.signature_hex
        )
        .is_err()
    );
    assert_eq!(
        world.guarded_task_stop_v2(&stop).unwrap(),
        Some(record.clone())
    );
    let snapshot = world.snapshot();
    let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        snapshot.state.retained_history_root.clone().unwrap(),
        &bytes,
    );
    let restored = World::from_verified_formal_snapshot(
        snapshot.clone(),
        world.journal().clone(),
        parent,
        std::sync::Arc::new(NoArchivedNodes),
    )
    .unwrap();
    assert_eq!(restored.guarded_task_stop_v2(&stop).unwrap(), Some(record));
    assert_eq!(
        restored.snapshot().capability_authorization_root,
        snapshot.capability_authorization_root
    );
    assert_eq!(
        restored
            .closed_game_task_v2(&initial.task_id)
            .unwrap()
            .unwrap()
            .selected_predicate,
        terminal.closed.selected_predicate
    );
}
