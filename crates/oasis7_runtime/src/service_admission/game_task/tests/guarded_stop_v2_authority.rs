use super::*;
#[test]
fn guarded_stop_v2_actual_owner_authority_and_committed_cutoff_are_atomic() {
    let (world, owner, initial) = setup();
    let original = guarded_stop(&world, &owner, &initial, "guarded-authority", 5000);
    let mut accepted = world.clone();
    let context = begin(&mut accepted);
    let record = accepted.apply_guarded_task_stop_v2(&original).unwrap();
    assert_eq!(record.original.subject_public_key, initial.owner_public_key);
    assert_eq!(
        record.after_control.task_status,
        GameTaskStatusV1::Suspended
    );
    assert_eq!(
        accepted.guarded_task_stop_v2(&original).unwrap(),
        Some(record)
    );
    accepted
        .finish_formal_administrative_tick(&context)
        .unwrap();
    // The actual delegated executor fixture seed is distinct from the Owner.
    let wrong = sign_read_request(
        GAME_TASK_STOP_DOMAIN_V2,
        original.request.clone(),
        &hex::encode([72; 32]),
    )
    .unwrap();
    assert_eq!(
        wrong.subject_public_key,
        original.request.target_executor_public_key
    );
    oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_STOP_DOMAIN_V2,
        &wrong.request,
        &wrong.subject_public_key,
        &wrong.signature_hex,
    )
    .unwrap();
    let wrong = SignedGameTaskStopV2 {
        request: wrong.request,
        subject_public_key: wrong.subject_public_key,
        signature_hex: wrong.signature_hex,
    };
    let mut denied = world.clone();
    begin(&mut denied);
    let before = denied.snapshot();
    assert!(denied.apply_guarded_task_stop_v2(&wrong).is_err());
    assert_eq!(denied.snapshot(), before);
    // Declaring the Owner subject cannot turn a genuine Executor signature into Owner authority.
    let mut forged_owner = wrong.clone();
    forged_owner.subject_public_key = original.subject_public_key.clone();
    forged_owner.validate_shape().unwrap();
    assert!(denied.apply_guarded_task_stop_v2(&forged_owner).is_err());
    assert_eq!(denied.snapshot(), before);
    // A fresh valid Owner signature cannot bless an invented Owner generation.
    let mut payload = original.request.clone();
    payload.initial_fence.owner_generation.0 += 1;
    let changed = sign_read_request(GAME_TASK_STOP_DOMAIN_V2, payload, &owner).unwrap();
    let changed = SignedGameTaskStopV2 {
        request: changed.request,
        subject_public_key: changed.subject_public_key,
        signature_hex: changed.signature_hex,
    };
    assert!(denied.apply_guarded_task_stop_v2(&changed).is_err());
    assert_eq!(denied.snapshot(), before);
    // The cutoff is evaluated at the actual begun committed UTC (1000), exclusive.
    let expired = guarded_stop(&world, &owner, &initial, "guarded-expired", 1000);
    assert!(denied.apply_guarded_task_stop_v2(&expired).is_err());
    assert_eq!(denied.snapshot(), before);
    // Reusing the same signature under the old domain is not a valid Owner control.
    assert!(
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            &original.request,
            &original.subject_public_key,
            &original.signature_hex
        )
        .is_err()
    );
}
