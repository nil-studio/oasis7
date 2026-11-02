//! Real signatures and strict wire data only, not a qualified Node producer.
use super::*;
use crate::world_service::task_mutation_admission_v2::*;
use crate::world_service::task_mutation_qualified_v2::*;
fn request() -> TaskMutationWaitSubmitV2 {
    let (q, _, _) = fixture();
    let mut p = q.submitted.original.request.clone();
    p.observation_grant =
        Some(StopObservationGrantV2::known_control_for_exact_target(&p, 5000, 5000, 4000).unwrap());
    let signed = sign_read_request(GAME_TASK_STOP_DOMAIN_V2, p, &"11".repeat(32)).unwrap();
    let submitted = TaskStopSubmitV2 {
        api_revision: 2,
        expected_world: q.submitted.expected_world,
        original: SignedGameTaskStopV2 {
            request: signed.request,
            subject_public_key: signed.subject_public_key,
            signature_hex: signed.signature_hex,
        },
        observation_nonce: "aa".repeat(32),
    };
    TaskMutationWaitSubmitV2 {
        api_revision: 2,
        reply_profile: TaskMutationReplyProfileV2::QualifiedEffectV2,
        submitted: TaskMutationSubmittedV2::GuardedStop(Box::new(submitted)),
        delivery_nonce: "bb".repeat(32),
        delivery_until_ms: DecimalU64(4500),
    }
}
#[test]
fn qualified_mutation_wire_requires_actual_signed_grant_and_preserves_bare_disjointness() {
    let q = request();
    q.validate().unwrap();
    let raw = serde_json::to_vec(&q).unwrap();
    assert_eq!(
        serde_json::from_slice::<TaskMutationWaitSubmitV2>(&raw).unwrap(),
        q
    );
    assert!(serde_json::from_slice::<TaskStopSubmitV2>(&raw).is_err());
    let (old, _, _) = fixture();
    old.submitted.verify_owner_signature().unwrap();
    let mut no_grant = q.clone();
    no_grant.submitted = TaskMutationSubmittedV2::GuardedStop(Box::new(old.submitted));
    assert!(no_grant.validate().is_err());
    let mut changed = q.clone();
    let TaskMutationSubmittedV2::GuardedStop(inner) = &mut changed.submitted else {
        unreachable!()
    };
    inner.original.request.execute_until_ms.0 -= 1;
    assert!(changed.validate().is_err());
    let mut late = q.clone();
    late.delivery_until_ms = DecimalU64(5001);
    assert!(late.validate().is_err());
    let mut value = serde_json::to_value(&q).unwrap();
    value["reply_profile"] = serde_json::json!("unknown");
    assert!(serde_json::from_value::<TaskMutationWaitSubmitV2>(value).is_err());
    let mut value = serde_json::to_value(&q).unwrap();
    value["paid"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TaskMutationWaitSubmitV2>(value).is_err());
}
#[test]
fn qualified_mutation_wire_never_reinterprets_a_real_signed_queued_ack_as_effect() {
    let q = request();
    let kind = q.submitted.kind();
    let world = q.submitted.world();
    let original = q.submitted.original_digest().unwrap();
    let ack = TaskMutationAdmissionV2 {
        api_revision: 2,
        kind,
        world: world.clone(),
        original_digest: original.clone(),
        action_id: DecimalU64(mutation_action_id_v2(kind, world, &original).unwrap()),
        admitted_at_ms: DecimalU64(3000),
        outcome: TaskMutationQueuedOutcomeV2::QueuedVolatile,
    };
    ack.validate(kind, world, &original).unwrap();
    let TaskMutationSubmittedV2::GuardedStop(inner) = &q.submitted else {
        unreachable!()
    };
    let bare_digest = request_digest(kind.path(), inner).unwrap();
    let signed =
        sign_service_response(kind.path(), bare_digest.clone(), ack, &"77".repeat(32)).unwrap();
    let node = sign_read_request("key", (), &"77".repeat(32))
        .unwrap()
        .subject_public_key;
    crate::world_service::verify_service_response(kind.path(), &bare_digest, &signed, &node)
        .unwrap();
    let raw = serde_json::to_vec(&signed).unwrap();
    assert!(
        serde_json::from_slice::<SignedServiceResponse<TaskMutationQualifiedEffectV2>>(&raw)
            .is_err()
    );
    assert_ne!(
        STOP_QUALIFIED_EFFECT_DOMAIN_V2,
        CLOSE_QUALIFIED_EFFECT_DOMAIN_V2
    );
    assert_ne!(STOP_QUALIFIED_EFFECT_DOMAIN_V2, kind.path());
}

#[path = "task_mutation_qualified_strict_tests.rs"]
mod strict;
