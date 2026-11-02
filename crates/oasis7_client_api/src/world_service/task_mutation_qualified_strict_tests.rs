//! Codec-only genuine signatures; these literals do not qualify a Node commit.
use super::*;
use crate::world_service::verify_service_response;
fn decode_reply(raw: &[u8]) -> Result<TaskMutationSubmitReplyV2, String> {
    TaskMutationSubmitReplyV2::decode_strict(raw)
}
fn decode_request(raw: &[u8]) -> Result<TaskMutationWaitSubmitV2, String> {
    TaskMutationWaitSubmitV2::decode_strict(raw)
}
fn replies() -> (TaskMutationWaitSubmitV2, Vec<TaskMutationSubmitReplyV2>) {
    let q = request();
    let kind = q.submitted.kind();
    let original = q.submitted.original_digest().unwrap();
    let action_id =
        DecimalU64(mutation_action_id_v2(kind, q.submitted.world(), &original).unwrap());
    let ack = TaskMutationAdmissionV2 {
        api_revision: 2,
        kind,
        world: q.submitted.world().clone(),
        original_digest: original.clone(),
        action_id,
        admitted_at_ms: DecimalU64(3000),
        outcome: TaskMutationQueuedOutcomeV2::QueuedVolatile,
    };
    let TaskMutationSubmittedV2::GuardedStop(inner) = &q.submitted else {
        unreachable!()
    };
    let bare_digest = request_digest(kind.path(), inner).unwrap();
    let admission =
        sign_service_response(kind.path(), bare_digest.clone(), ack, &"77".repeat(32)).unwrap();
    let (_, old, _) = fixture();
    let TaskStopOutcomeV2::KnownStopped { fact, .. } = old.payload.outcome else {
        unreachable!()
    };
    let mut record = fact.record;
    record.original = inner.original.clone();
    record.after_control = record
        .original
        .request
        .derive_post_control(&record.before_control)
        .unwrap();
    record.result_core_digest = record.compute_result_core_digest().unwrap();
    let payload = TaskMutationQualifiedEffectV2 {
        api_revision: 2,
        kind,
        path: kind.path().into(),
        world: q.submitted.world().clone(),
        chain_id: "codec-fixture".into(),
        delivery_nonce: q.delivery_nonce.clone(),
        original_digest: original,
        action_id,
        canonical_catalog_digest: "aa".repeat(32),
        execution: TaskMutationExecutionV2 {
            height: DecimalU64(4),
            slot: DecimalU64(4),
            epoch: DecimalU64(1),
            node_block_hash: "fixture-block".into(),
            action_root: "bb".repeat(32),
            committed_at_unix_ms: DecimalU64(1000),
        },
        witnessed_in_commit: fact.stopped_at_commit,
        result: TaskMutationQualifiedResultV2::GuardedStop {
            record: Box::new(record),
        },
    };
    let effect = sign_service_response(
        STOP_QUALIFIED_EFFECT_DOMAIN_V2,
        q.digest().unwrap(),
        payload,
        &"77".repeat(32),
    )
    .unwrap();
    let node = sign_read_request("key", (), &"77".repeat(32))
        .unwrap()
        .subject_public_key;
    verify_service_response(kind.path(), &bare_digest, &admission, &node).unwrap();
    verify_service_response(
        STOP_QUALIFIED_EFFECT_DOMAIN_V2,
        &q.digest().unwrap(),
        &effect,
        &node,
    )
    .unwrap();
    (
        q,
        vec![
            TaskMutationSubmitReplyV2::Queued {
                admission: admission.clone(),
                wait_outcome: TaskMutationWaitOutcomeV2::Unavailable,
            },
            TaskMutationSubmitReplyV2::Qualified {
                admission,
                effect: Box::new(effect),
            },
        ],
    )
}
#[test]
fn qualified_mutation_wire_strict_nested_unknown_requires_full_tree_decode() {
    let (q, replies) = replies();
    assert_eq!(decode_request(&serde_json::to_vec(&q).unwrap()).unwrap(), q);
    for reply in &replies {
        let raw = serde_json::to_vec(reply).unwrap();
        assert_eq!(decode_reply(&raw).unwrap(), *reply);
        let mut value = serde_json::to_value(reply).unwrap();
        value["admission"]["unapproved"] = true.into();
        assert!(
            decode_reply(&serde_json::to_vec(&value).unwrap()).is_err(),
            "nested admission unknown must be rejected after genuine signed positive"
        );
    }
    let mut value = serde_json::to_value(&replies[1]).unwrap();
    value["effect"]["unapproved"] = true.into();
    assert!(decode_reply(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value = serde_json::to_value(&replies[1]).unwrap();
    value["effect"]["payload"]["witnessed_in_commit"]["unapproved"] = true.into();
    assert!(decode_reply(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value = serde_json::to_value(&replies[1]).unwrap();
    value["effect"]["payload"]["witnessed_in_commit"]["binding"]["unapproved"] = true.into();
    assert!(decode_reply(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut value = serde_json::to_value(&q).unwrap();
    value["submitted"]["request"]["expected_world"]["unapproved"] = true.into();
    assert!(decode_request(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn qualified_mutation_wire_strict_known_duplicates_do_not_get_value_erased() {
    let (q, replies) = replies();
    let raw = serde_json::to_string(&replies[1]).unwrap();
    assert!(decode_reply(raw.as_bytes()).is_ok());
    for (needle, replacement) in [
        (
            "\"admission\":{",
            "\"admission\":{\"request_digest\":\"duplicate\",",
        ),
        (
            "\"effect\":{",
            "\"effect\":{\"request_digest\":\"duplicate\",",
        ),
        (
            "\"witnessed_in_commit\":{",
            "\"witnessed_in_commit\":{\"position\":4,",
        ),
    ] {
        assert!(raw.contains(needle));
        let changed = raw.replacen(needle, replacement, 1);
        assert!(decode_reply(changed.as_bytes()).is_err());
    }
    let raw = serde_json::to_string(&q).unwrap();
    let changed = raw.replacen(
        "\"expected_world\":{",
        "\"expected_world\":{\"world_id\":\"duplicate\",",
        1,
    );
    assert_ne!(raw, changed);
    assert!(decode_request(changed.as_bytes()).is_err());
}
