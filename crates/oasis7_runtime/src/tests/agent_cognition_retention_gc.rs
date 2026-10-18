//! P1 RED fixtures for terminal retention, checkpoint/GC pins and expiry.

use crate::{
    CognitionRetentionStore, RetentionExecutionProbe, RetentionGcReport, RetentionRecordV1,
    RetentionReplayRequestV1,
};
use serde_json::{Value, json};

const WORLD_ID: &str = "world-retention-fixture";
const ENVELOPE_KEY: &str = "key:retention-1";
const ENVELOPE_DIGEST: &str = "digest:retention-1";
const REPLAY_KEY: &str = "key:retention-1-replay";
const REPLAY_DIGEST: &str = "digest:retention-1-replay";
const RECEIPT_ID: &str = "receipt:retention-1";
const RESPONSE_ARTIFACT: &str = "artifact:response-1";

fn record(status: &str, suffix: &str) -> RetentionRecordV1 {
    serde_json::from_value(json!({
        "schema_version": "cognition-retention-record.v1",
        "world_id": WORLD_ID,
        "envelope_idempotency_key": format!("{ENVELOPE_KEY}-{suffix}"),
        "envelope_digest": format!("{ENVELOPE_DIGEST}-{suffix}"),
        "agent_session_id": format!("session-{suffix}"),
        "agent_turn_id": format!("turn-{suffix}"),
        "decision_request_id": format!("request-{suffix}"),
        "status": status,
        "base_tick": 10,
        "issued_at_tick": 10,
        "terminal_disposition": if status == "committed" { Value::Null } else { json!(status) },
        "receipt_id": if status == "committed" { json!(RECEIPT_ID) } else { Value::Null },
        "receipt_digest": if status == "committed" { json!("digest:receipt-1") } else { Value::Null },
        "response_artifact_id": RESPONSE_ARTIFACT,
        "continuation_id": "continuation:retention-1",
        "commit_record_id": if status == "committed" { json!("commit:retention-1") } else { Value::Null }
    }))
    .expect("decode retention record")
}

fn report_value(report: RetentionGcReport) -> Value {
    serde_json::to_value(report).expect("encode GC report")
}

#[test]
fn every_terminal_disposition_retains_key_receipt_or_response_tombstone_until_horizon() {
    let mut store = CognitionRetentionStore::with_horizon(100);
    for (index, status) in ["committed", "rejected", "failed", "cancelled"]
        .into_iter()
        .enumerate()
    {
        store.insert(record(status, index.to_string().as_str()));
    }

    let report = report_value(store.gc(50, 50).expect("GC before retention horizon"));
    assert_eq!(report["deleted_count"], 0);
    assert_eq!(report["retained_terminal_count"], 4);
    for index in 0..4 {
        assert!(store.contains_key(&format!("{ENVELOPE_KEY}-{index}")));
        assert!(store.contains_response_artifact(RESPONSE_ARTIFACT));
    }
}

#[test]
fn active_pending_retry_commit_and_continuation_references_pin_artifacts() {
    let mut store = CognitionRetentionStore::with_horizon(100);
    store.insert(record("failed", "pinned"));
    for pin in [
        "active_turn",
        "pending_wake",
        "retry_lineage",
        "unfinalized_commit",
        "continuation",
    ] {
        store.pin_reference("key:retention-1-pinned", pin);
    }

    let report = report_value(store.gc(1_000, 1_000).expect("GC with active pins"));
    assert_eq!(report["deleted_count"], 0);
    assert_eq!(report["pinned_reference_count"], 5);
    assert!(store.contains_key("key:retention-1-pinned"));
    assert!(store.contains_response_artifact(RESPONSE_ARTIFACT));
}

#[test]
fn complete_v1_below_gc_floor_is_expired_but_legacy_without_proof_is_legacy_rejection() {
    let store = CognitionRetentionStore::with_horizon(100);
    let complete = RetentionReplayRequestV1::from_json(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": WORLD_ID,
        "agent_session_id": "session-1",
        "agent_turn_id": "turn-1",
        "decision_request_id": "request-1",
        "envelope_idempotency_key": ENVELOPE_KEY,
        "envelope_digest": ENVELOPE_DIGEST,
        "base_tick": 1,
        "issued_at_tick": 1,
        "gc_floor_tick": 2
    }))
    .expect("complete v1 replay request");
    let expired = store
        .classify_replay(complete)
        .expect_err("expired v1 request");
    assert_eq!(expired.code(), "expired_idempotency");

    let legacy = RetentionReplayRequestV1::from_json(json!({
        "world_id": WORLD_ID,
        "queued_action": {"action_id": 7, "success": true, "summary": "legacy success"}
    }))
    .expect("legacy compatibility request");
    let legacy_error = store.classify_replay(legacy).expect_err("legacy request");
    assert_eq!(legacy_error.code(), "legacy_no_cognition_proof");
}

#[test]
fn committed_replay_never_calls_provider_or_reexecutes_effect_or_receipt() {
    let mut store = CognitionRetentionStore::with_horizon(100);
    store.insert(record("committed", "replay"));
    let mut probe = RetentionExecutionProbe::default();
    let first = store
        .replay(REPLAY_KEY, REPLAY_DIGEST, &mut probe)
        .expect("committed replay reads canonical receipt");
    let second = store
        .replay(REPLAY_KEY, REPLAY_DIGEST, &mut probe)
        .expect("duplicate committed replay");
    let first = serde_json::to_value(first).expect("encode first replay");
    let second = serde_json::to_value(second).expect("encode second replay");

    assert_eq!(first["receipt_id"], RECEIPT_ID);
    assert_eq!(first["provider_invocation_count"], 0);
    assert_eq!(first["effect_delta"], 0);
    assert_eq!(second["provider_invocation_count"], 0);
    assert_eq!(second["effect_delta"], 0);
    assert_eq!(second["world_receipt_linked_delta"], 0);
    assert_eq!(probe.provider_invocation_count, 0);
}

#[test]
fn v1_replay_binds_record_identity_and_persisted_gc_floor() {
    let mut store = CognitionRetentionStore::with_horizon(100);
    store.insert(record("committed", "replay-v1"));
    store.gc(50, 10).expect("persist replay GC floor");
    let request = RetentionReplayRequestV1::from_json(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": WORLD_ID,
        "agent_session_id": "session-replay-v1",
        "agent_turn_id": "turn-replay-v1",
        "decision_request_id": "request-replay-v1",
        "envelope_idempotency_key": format!("{REPLAY_KEY}-v1"),
        "envelope_digest": format!("{REPLAY_DIGEST}-v1"),
        "base_tick": 10,
        "issued_at_tick": 10,
        "gc_floor_tick": 10
    }))
    .expect("decode v1 replay request");
    let mut probe = RetentionExecutionProbe::default();
    let replay = store
        .replay_v1(request, &mut probe)
        .expect("complete proof should replay committed receipt");
    assert_eq!(replay.receipt_id.as_deref(), Some(RECEIPT_ID));
    assert_eq!(probe, RetentionExecutionProbe::default());

    for (field, value) in [
        ("agent_session_id", "session-other"),
        ("agent_turn_id", "turn-other"),
        ("decision_request_id", "request-other"),
    ] {
        let mut mismatched = json!({
            "schema_version": "agent-decision-envelope.v1",
            "world_id": WORLD_ID,
            "agent_session_id": "session-replay-v1",
            "agent_turn_id": "turn-replay-v1",
            "decision_request_id": "request-replay-v1",
            "envelope_idempotency_key": format!("{REPLAY_KEY}-v1"),
            "envelope_digest": format!("{REPLAY_DIGEST}-v1"),
            "base_tick": 10,
            "issued_at_tick": 10,
            "gc_floor_tick": 10
        });
        mismatched[field] = json!(value);
        let mismatched = RetentionReplayRequestV1::from_json(mismatched)
            .expect("decode mutated mismatch fixture");
        assert_eq!(
            store
                .replay_v1(mismatched, &mut probe)
                .expect_err("lineage mismatch must be rejected")
                .code(),
            "idempotency_conflict",
            "mismatched {field} must not replay"
        );
    }

    let legacy = RetentionReplayRequestV1::from_json(json!({
        "world_id": WORLD_ID,
        "envelope_idempotency_key": format!("{REPLAY_KEY}-v1"),
        "envelope_digest": format!("{REPLAY_DIGEST}-v1")
    }))
    .expect("decode legacy replay request");
    assert_eq!(
        store
            .replay_v1(legacy, &mut probe)
            .expect_err("legacy proof must be rejected")
            .code(),
        "legacy_no_cognition_proof"
    );

    let foreign = RetentionReplayRequestV1::from_json(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": "foreign-world",
        "agent_session_id": "session-1",
        "agent_turn_id": "turn-1",
        "decision_request_id": "request-1",
        "envelope_idempotency_key": format!("{REPLAY_KEY}-v1"),
        "envelope_digest": format!("{REPLAY_DIGEST}-v1"),
        "base_tick": 10,
        "issued_at_tick": 10,
        "gc_floor_tick": 10
    }))
    .expect("decode foreign replay request");
    assert_eq!(
        store
            .replay_v1(foreign, &mut probe)
            .expect_err("foreign replay proof must be rejected")
            .code(),
        "idempotency_conflict"
    );

    let wrong_floor = RetentionReplayRequestV1::from_json(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": WORLD_ID,
        "agent_session_id": "session-1",
        "agent_turn_id": "turn-1",
        "decision_request_id": "request-1",
        "envelope_idempotency_key": format!("{REPLAY_KEY}-v1"),
        "envelope_digest": format!("{REPLAY_DIGEST}-v1"),
        "base_tick": 10,
        "issued_at_tick": 10,
        "gc_floor_tick": 0
    }))
    .expect("decode weak-floor replay request");
    assert_eq!(
        store
            .replay_v1(wrong_floor, &mut probe)
            .expect_err("weak replay floor must be rejected")
            .code(),
        "expired_idempotency"
    );
}
