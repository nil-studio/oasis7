//! Legacy scheduler wake request-digest compatibility regressions.

use super::super::*;
use super::agent_cognition_runtime_hardening::{
    bind_test_turn, digest, policy, test_continuation, test_continuation_wake,
};

#[test]
fn direct_enqueue_hydrates_legacy_scheduler_wake_request_digest_from_validated_continuation() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let wake = test_continuation_wake(&world, "wake.legacy-enqueue", "continuation.legacy-enqueue");
    let continuation = test_continuation(&wake);
    world
        .install_cognition_continuation_for_test(continuation.clone())
        .expect("continuation projection");

    world
        .enqueue_cognition_wake(wake.clone())
        .expect("legacy wake enqueue should hydrate its digest");

    let snapshot = world.cognition_scheduler_snapshot();
    let persisted = snapshot["active"]
        .as_array()
        .and_then(|wakes| wakes.iter().find(|value| value["wake_id"] == wake.wake_id))
        .expect("persisted legacy wake");
    assert_eq!(
        persisted["request_digest"],
        continuation.origin_request_digest
    );
}

#[test]
fn direct_readback_hydrates_legacy_scheduler_wake_request_digest_from_validated_continuation() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let wake = test_continuation_wake(
        &world,
        "wake.legacy-readback",
        "continuation.legacy-readback",
    );
    let mut continuation = test_continuation(&wake);
    continuation.continuation_status_digest = None;
    world
        .install_cognition_continuation_for_test(continuation.clone())
        .expect("continuation projection");
    world
        .enqueue_cognition_wake_for_test(wake.clone())
        .expect("legacy wake projection");

    let readback = world
        .cognition_wake_readback(&wake.wake_id)
        .expect("legacy wake readback")
        .expect("legacy wake present");
    assert_eq!(readback.request_digest, continuation.origin_request_digest);
}

#[test]
fn sparse_legacy_continuation_normalization_supports_direct_wake_ingress() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let wake = test_continuation_wake(&world, "wake.sparse-legacy", "continuation.sparse-legacy");
    let mut continuation = test_continuation(&wake);
    continuation.continuation_status_digest = None;
    world
        .install_cognition_continuation_for_test(continuation.clone())
        .expect("sparse continuation projection");

    world
        .enqueue_cognition_wake(wake.clone())
        .expect("sparse legacy continuation should normalize for wake ingress");

    let readback = world
        .cognition_wake_readback(&wake.wake_id)
        .expect("sparse legacy wake readback")
        .expect("sparse legacy wake present");
    assert_eq!(readback.request_digest, continuation.origin_request_digest);
    assert!(
        world.cognition_continuations()[0]
            .get("continuation_status_digest")
            .is_none()
    );
}

#[test]
fn direct_enqueue_rejects_a_contradictory_scheduler_wake_request_digest() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let mut wake = test_continuation_wake(
        &world,
        "wake.contradictory-digest",
        "continuation.contradictory-digest",
    );
    let mut continuation = test_continuation(&wake);
    continuation.continuation_status_digest = None;
    wake.request_digest = digest(99);
    world
        .install_cognition_continuation_for_test(continuation)
        .expect("continuation projection");

    let error = world
        .enqueue_cognition_wake(wake)
        .expect_err("contradictory explicit digest must remain foreign");
    assert!(format!("{error:?}").contains("foreign_scheduler_wake"));
}
