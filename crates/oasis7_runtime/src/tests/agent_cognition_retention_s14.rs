//! S14 regression coverage for canonical cognition retention and persistence.

use super::super::*;
use super::agent_cognition_runtime_hardening::{
    bind_test_turn, digest, envelope, policy, proposal, response_artifact_for_envelope, temp_dir,
};
use crate::{RetentionRecordV1, WakeConditionV1};
use serde_json::{Value, json};
use std::fs;

fn terminal_record() -> RetentionRecordV1 {
    serde_json::from_value(json!({
        "schema_version": "cognition-retention-record.v1",
        "world_id": "world-s14-retention",
        "envelope_idempotency_key": "key:s14-retention",
        "envelope_digest": digest(1),
        "agent_session_id": "session.s14-retention",
        "agent_turn_id": "turn.s14-retention",
        "decision_request_id": "request.s14-retention",
        "status": "committed",
        "base_tick": 10,
        "issued_at_tick": 10,
        "terminal_disposition": null,
        "receipt_id": "receipt:s14-retention",
        "receipt_digest": digest(2),
        "response_artifact_id": "artifact:s14-retention",
        "continuation_id": "continuation:s14-retention",
        "commit_record_id": "commit:s14-retention"
    }))
    .expect("decode S14 terminal record")
}

#[test]
fn gc_compacts_canonical_commit_receipt_response_and_journal_projections() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare canonical cognition commit");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize canonical cognition commit");

    let report = world
        .gc_cognition(1_000, 1_000)
        .expect("GC should compact expired canonical cognition");
    assert_eq!(report.deleted_count, 1);
    assert_eq!(
        report.deleted_keys,
        vec![prepared.envelope_idempotency_key.clone()]
    );
    assert!(
        world.cognition()["commit_records"]
            .as_array()
            .expect("commit records")
            .is_empty()
    );
    assert!(
        world.cognition()["responses"]
            .as_array()
            .expect("responses")
            .is_empty()
    );
    assert!(
        world.cognition()["receipt_registry"]
            .as_array()
            .expect("receipt registry")
            .is_empty()
    );
    assert!(
        world.cognition()["receipt_lineage_registry"]
            .as_array()
            .expect("receipt lineage registry")
            .is_empty()
    );
    assert!(
        world.cognition()["idempotency_index"]
            .as_object()
            .expect("idempotency index")
            .is_empty()
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("cognition journal")
            .is_empty()
    );
}

#[test]
fn attached_world_persists_record_pin_and_gc_mutations_before_publish() {
    let mut world = World::new();
    let dir = temp_dir("s14-retention-transaction");
    world
        .save_to_dir(&dir)
        .expect("attach persistence directory");

    world
        .record_cognition_terminal(terminal_record())
        .expect("persist terminal retention record");
    world
        .pin_cognition_reference("key:s14-retention", "replay-manifest")
        .expect("persist retention pin");
    world
        .gc_cognition(100, 11)
        .expect("persist retention GC floor");

    let restored = World::load_from_dir(&dir).expect("restore attached world");
    let retention = &restored.cognition()["retention_state"];
    assert_eq!(retention["gc_floor_tick"], 11);
    assert!(
        retention["records"]
            .as_object()
            .expect("retention records")
            .contains_key("key:s14-retention")
    );
    assert_eq!(
        retention["pins"]["key:s14-retention"][0],
        Value::String("replay-manifest".to_string())
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn gc_backfills_legacy_canonical_markers_without_retention_state() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare legacy canonical commit");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize legacy canonical commit");

    let mut encoded = serde_json::to_value(&world).expect("serialize world");
    encoded["cognition"]
        .as_object_mut()
        .expect("cognition projection")
        .remove("retention_state");
    let mut legacy: World = serde_json::from_value(encoded).expect("restore legacy world");
    let report = legacy
        .gc_cognition(1_000, 1_000)
        .expect("GC migrates legacy markers");

    assert_eq!(report.deleted_keys, vec![prepared.envelope_idempotency_key]);
    assert!(
        legacy.cognition()["commit_records"]
            .as_array()
            .expect("commit records")
            .is_empty()
    );
}

#[test]
fn gc_preserves_dense_journal_while_active_wake_references_an_event() {
    let mut world = World::new().with_cognition_scheduler(policy(), 1);
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare canonical commit");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize canonical commit");
    let event_digest = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .and_then(|events| events.last())
        .and_then(|event| event.get("event_digest"))
        .and_then(Value::as_str)
        .expect("journal event digest")
        .to_string();

    let mut continuation = proposal(&world);
    continuation.wake_conditions = vec![WakeConditionV1 {
        schema_version: "wake-condition.v1".to_string(),
        kind: "world_event_committed".to_string(),
        logical_tick: None,
        event_digest: Some(event_digest.clone()),
        receipt_id: None,
        subject: None,
        path_or_rule: None,
        operator: None,
        expected_value_bytes: None,
    }];
    continuation.next_wake_tick = None;
    continuation.proposal_digest = continuation.proposal_digest();
    world
        .admit_cognition_continuation(continuation)
        .expect("admit typed event wake");

    let mut encoded = serde_json::to_value(&world).expect("serialize world");
    let cognition = encoded["cognition"]
        .as_object_mut()
        .expect("cognition projection");
    let mut suffix_marker = cognition["commit_records"]
        .as_array()
        .and_then(|records| records.first())
        .cloned()
        .expect("prefix commit marker");
    suffix_marker["commit_id"] = json!("commit:s14-suffix");
    suffix_marker["envelope_idempotency_key"] = json!("key:s14-suffix");
    suffix_marker["envelope_digest"] = json!(digest(14));
    suffix_marker["agent_session_id"] = json!("session.s14-suffix");
    suffix_marker["agent_turn_id"] = json!("turn.s14-suffix");
    suffix_marker["decision_request_id"] = json!("request.s14-suffix");
    cognition["commit_records"]
        .as_array_mut()
        .expect("commit records")
        .push(suffix_marker);
    let events = cognition["cognition_journal"]["events"]
        .as_array_mut()
        .expect("journal events");
    let mut suffix_event = events.last().cloned().expect("journal event");
    suffix_event["journal_seq"] = json!(events.len() + 1);
    suffix_event["envelope_idempotency_key"] = json!("key:s14-suffix");
    suffix_event["event_digest"] = json!(digest(15));
    suffix_event["agent_session_id"] = json!("session.s14-suffix");
    suffix_event["agent_turn_id"] = json!("turn.s14-suffix");
    suffix_event["decision_request_id"] = json!("request.s14-suffix");
    events.push(suffix_event);
    let mut with_pending_wake: World =
        serde_json::from_value(encoded).expect("restore pending wake world");
    let report = with_pending_wake
        .gc_cognition(1_000, 1_000)
        .expect("GC respects active event reference");

    assert_eq!(report.deleted_keys, vec!["key:s14-suffix".to_string()]);
    assert!(
        with_pending_wake.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("retained journal events")
            .iter()
            .any(|event| event.get("event_digest").and_then(Value::as_str)
                == Some(event_digest.as_str()))
    );

    let mut encoded = serde_json::to_value(&with_pending_wake).expect("serialize pinned world");
    encoded["cognition"]["continuations"] = json!([]);
    let mut wake_completed: World =
        serde_json::from_value(encoded).expect("restore world without pending wake");
    let report = wake_completed
        .gc_cognition(1_001, 1_000)
        .expect("GC releases automatic event pins");
    assert_eq!(report.deleted_count, 1);
}
