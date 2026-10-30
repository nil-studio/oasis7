use super::*;
use oasis7_client_api::world_service::{
    request_digest, sign_read_request, sign_service_response,
    task_cleanup_v2::verify_cleanup_observation_v2,
};
#[path = "task_cleanup_v2_fixture.rs"]
mod fixture;
#[test]
fn cleanup_observation_actual_closed_world_and_full_delivery_drift() {
    let (mut world, request, _) = fixture::fixture();
    let context = fixture::begin(&mut world, 1000);
    world
        .apply_task_revoke_and_close_v2(&request.submitted.original)
        .unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    let historical = world.clone();
    let commit = fixture::pin(&world);
    let response = observe(&world, &commit, Some((&historical, &commit)), &request).unwrap();
    assert!(matches!(
        &response.outcome,
        TaskCleanupOutcomeV2::KnownClosed {
            delivery: TaskCleanupDeliveryV2::ExactClosed(_),
            ..
        }
    ));
    let ready = before_delivery(response.clone(), &commit).unwrap();
    assert!(matches!(
        &ready.outcome,
        TaskCleanupOutcomeV2::KnownClosed {
            delivery: TaskCleanupDeliveryV2::ExactClosed(_),
            ..
        }
    ));
    let node_private = "77".repeat(32);
    let node_public = sign_read_request("key", (), &node_private)
        .unwrap()
        .subject_public_key;
    let signed = sign_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        request_digest(TASK_CLEANUP_LOOKUP_V2, &request).unwrap(),
        ready,
        &node_private,
    )
    .unwrap();
    verify_cleanup_observation_v2(&request, &signed, &node_public).unwrap();
    // An actual second formal administrative tick changes the full current pin.
    let next = fixture::begin(&mut world, 2000);
    world.finish_formal_administrative_tick(&next).unwrap();
    let changed = fixture::pin(&world);
    assert_ne!(changed, commit);
    let drifted = before_delivery(response, &changed).unwrap();
    let TaskCleanupOutcomeV2::KnownClosed { fact, delivery } = &drifted.outcome else {
        panic!("historical fact discarded on current drift");
    };
    assert_eq!(
        *delivery,
        TaskCleanupDeliveryV2::Drift(TaskCleanupDriftV2::DeliveryCommit)
    );
    assert_eq!(fact.closed.close_operation_id, "close");
    let drift_signed = sign_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        request_digest(TASK_CLEANUP_LOOKUP_V2, &request).unwrap(),
        drifted,
        &node_private,
    )
    .unwrap();
    verify_cleanup_observation_v2(&request, &drift_signed, &node_public).unwrap();
    let missing = observe(&world, &changed, None, &request).unwrap();
    assert_eq!(missing.outcome, TaskCleanupOutcomeV2::HistoryUnavailable);
}
#[test]
fn cleanup_observation_actual_current_signer_change_is_not_cached_authority() {
    let (mut world, request, owner) = fixture::fixture();
    let context = fixture::begin(&mut world, 1000);
    world
        .apply_task_revoke_and_close_v2(&request.submitted.original)
        .unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    let historical = world.clone();
    let old_commit = fixture::pin(&historical);
    let next = fixture::begin(&mut world, 2000);
    let mut changed = world.capability_revocation_state().agent_signer_delegations["agent"].clone();
    changed.generation = 3;
    changed.nonce = 3;
    changed.revoked = false;
    world
        .apply_agent_signer_delegation(&sign_read_request("delegation", changed, &owner).unwrap())
        .unwrap();
    world.finish_formal_administrative_tick(&next).unwrap();
    let response = observe(
        &world,
        &fixture::pin(&world),
        Some((&historical, &old_commit)),
        &request,
    )
    .unwrap();
    let TaskCleanupOutcomeV2::KnownClosed { fact, delivery } = response.outcome else {
        panic!("historical fact discarded");
    };
    assert_eq!(
        delivery,
        TaskCleanupDeliveryV2::Drift(TaskCleanupDriftV2::SignerOrHighWater)
    );
    assert_eq!(fact.closed.close_operation_id, "close");
}
