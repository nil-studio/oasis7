//! LIVE-3 RED tests for durable cognition state on the real World store.
//!
//! These tests deliberately enter through `World::save_to_dir` and
//! `World::load_from_dir`. The fixture is written into the persisted snapshot
//! after an ordinary save so the test does not depend on an in-memory recovery
//! oracle or a private World field. The JSON fallback is forced by removing
//! the distfs sidecar; production persistence must preserve the cognition
//! projection in both the snapshot and its subsequent save.

use super::super::*;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const COGNITION_SCHEMA: &str = "cognition-persistence.v1";
const JOURNAL_SCHEMA: &str = "cognition-journal.v1";
const WORLD_ID: &str = "world-live-persistence";
const BRANCH_ID: &str = "main";
const ENVELOPE_KEY: &str = "blake3:envelope-idempotency-live-1";
const ENVELOPE_DIGEST: &str = "blake3:envelope-live-1";
const RECEIPT_ID: &str = "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const RECEIPT_DIGEST: &str =
    "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const R_PARENT: &str = "blake3:root-parent-live-1";
const R_NEXT: &str = "blake3:root-next-live-1";

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-live-cognition-{prefix}-{unique}"))
}

fn read_snapshot(dir: &Path) -> Value {
    serde_json::from_slice(
        &fs::read(dir.join("snapshot.json")).expect("read persisted snapshot.json"),
    )
    .expect("decode persisted snapshot.json")
}

fn write_snapshot(dir: &Path, snapshot: &Value) {
    fs::write(
        dir.join("snapshot.json"),
        serde_json::to_vec_pretty(snapshot).expect("encode cognition snapshot fixture"),
    )
    .expect("write cognition snapshot fixture");
}

fn force_json_fallback(dir: &Path) {
    fs::remove_dir_all(dir.join(".distfs-state")).expect("remove distfs sidecar");
}

fn seed_world_snapshot(prefix: &str) -> (PathBuf, Value) {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-live-persistence".to_string(),
        pos: super::pos(0, 0),
    });
    world.step().expect("seed world state");

    let dir = temp_dir(prefix);
    world.save_to_dir(&dir).expect("save baseline world");
    let mut snapshot = read_snapshot(&dir);
    let cognition = cognition_projection(prefix);
    snapshot["cognition"] = cognition.clone();
    write_snapshot(&dir, &snapshot);
    force_json_fallback(&dir);
    (dir, cognition)
}

fn seed_untrusted_recovery_fixture(prefix: &str) -> PathBuf {
    let (dir, _) = seed_world_snapshot(prefix);
    let mut snapshot = read_snapshot(&dir);
    let recovery = snapshot["cognition"]["recovery"]
        .as_object_mut()
        .expect("recovery projection object");
    // These are deliberately forged diagnostic counters. Recovery must derive
    // its result from the durable commit marker, receipt and journal records,
    // rather than replaying a self-asserted report from the fixture.
    recovery.insert("provider_invocation_count".to_string(), json!(99));
    recovery.insert("kernel_invocation_count".to_string(), json!(99));
    recovery.insert("effect_count".to_string(), json!(99));
    recovery.insert("debit_count".to_string(), json!(99));
    recovery.insert("receipt_count".to_string(), json!(99));
    recovery.insert("world_receipt_linked_count".to_string(), json!(99));
    write_snapshot(&dir, &snapshot);
    dir
}

fn canonical_scheduler_state() -> Value {
    let policy: SchedulerPolicyV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-policy.v1",
        "max_total_wakes_per_tick": 8,
        "max_wakes_per_agent_per_tick": 1,
        "aging_after_ticks": 2,
        "max_starvation_ticks": 4,
        "initial_priority": 0,
        "comparator": SchedulerPolicyV1::COMPARATOR,
        "service_order": SchedulerPolicyV1::SERVICE_ORDER
    }))
    .expect("decode canonical persistence scheduler policy");
    let wake: SchedulerWakeV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-live-1",
        "continuation_id": "continuation-live-1",
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:runtime-manifest-live-1",
        "agent_id": "agent-live-persistence",
        "agent_session_id": "session-live-1",
        "agent_turn_id": "turn-live-1",
        "decision_request_id": "request-live-1",
        "next_wake_tick": 2,
        "eligible_since_tick": 1,
        "starvation_deadline_tick": 5,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "receipt_linked"
    }))
    .expect("decode canonical persistence scheduler wake");
    let mut scheduler =
        CognitionScheduler::try_new(policy, 1).expect("construct canonical persistence scheduler");
    scheduler.advance_logical_tick(1);
    scheduler
        .try_enqueue(wake)
        .expect("enqueue canonical persistence wake");
    scheduler.snapshot_json()
}

fn cognition_projection(prefix: &str) -> Value {
    let committed = matches!(prefix, "committed" | "committed_missing_projection");
    let conflict = prefix == "conflict";
    let disposition = if committed {
        "committed"
    } else {
        "recovery_pending"
    };
    let head_status = if committed {
        "canonical"
    } else {
        "recovery_pending"
    };
    let state_root = if committed { R_NEXT } else { R_PARENT };
    let receipt = if committed {
        json!({"receipt_id": RECEIPT_ID, "receipt_digest": RECEIPT_DIGEST})
    } else {
        Value::Null
    };
    let receipt_projection_present = committed && prefix != "committed_missing_projection";
    let idempotency_projection_present = committed && prefix != "committed_missing_projection";
    let effect_count = u64::from(committed && prefix == "committed");
    let debit_count = effect_count;
    let receipt_count = u64::from(committed);
    let world_receipt_linked_count = u64::from(committed && prefix == "committed");

    json!({
        "schema_version": COGNITION_SCHEMA,
        "cognition_journal": {
            "schema_version": JOURNAL_SCHEMA,
            "head_seq": 8,
            "head_digest": "blake3:cognition-journal-head-live-1",
            "events": [
                {"journal_seq": 1, "kind": "TurnStarted", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 2, "kind": "ContextCaptured", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 3, "kind": "RequestDispatched", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 4, "kind": "ResponseRecorded", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 5, "kind": "DecisionEnvelopeSubmitted", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 6, "kind": "DecisionValidated", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 7, "kind": "WorldReceiptLinked", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 8, "kind": "CognitionTurnCompleted", "status": disposition, "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY}
            ]
        },
        "responses": [{
            "response_digest": "blake3:response-live-1",
            "response_artifact": {"decision": {"type": "move_agent", "agent_id": "agent-live-persistence"}},
            "envelope_digest": ENVELOPE_DIGEST,
            "journal_head": "blake3:cognition-journal-head-live-1"
        }],
        "commit_records": [{
            "schema_version": "world-commit-record.v1",
            "commit_id": "commit-live-1",
            "envelope_idempotency_key": ENVELOPE_KEY,
            "envelope_digest": ENVELOPE_DIGEST,
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 7,
            "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            "finality_status": "verified",
            "finality_binding_digest": "blake3:finality-binding-live-1",
            "runtime_manifest_hash": "blake3:runtime-manifest-live-1",
            "action_id": "action-live-1",
            "parent_tick": 1,
            "parent_world_hash": R_PARENT,
            "staged_event_root": "blake3:events-next-live-1",
            "staged_state_root": R_NEXT,
            "receipt_id": RECEIPT_ID,
            "receipt_digest": RECEIPT_DIGEST,
            "reorg_epoch": 3,
            "cognition_journal_seq": 6,
            "status": if committed {"committed"} else {"prepared"},
            "abort_reason": Value::Null
        }],
        "receipt_registry": if receipt_projection_present {json!([receipt])} else {json!([])},
        "idempotency_index": if idempotency_projection_present {
            json!({ENVELOPE_KEY: {"envelope_digest": ENVELOPE_DIGEST, "disposition": "committed", "receipt_id": RECEIPT_ID}})
        } else {
            json!({})
        },
        "scheduler_state": canonical_scheduler_state(),
        "continuations": [{
            "schema_version": "agent-continuation.v1",
            "continuation_id": "continuation-live-1",
            "wake_id": "wake-live-1",
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 7,
            "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            "finality_status": "verified",
            "reorg_epoch": 3,
            "runtime_manifest_hash": "blake3:runtime-manifest-live-1",
            "agent_id": "agent-live-persistence",
            "agent_session_id": "session-live-1",
            "agent_turn_id": "turn-live-1",
            "decision_request_id": "request-live-1",
            "origin_turn_id": "turn-live-1",
            "origin_request_digest": "blake3:request-live-1",
            "continuation_proposal_id": "proposal-live-1",
            "proposal_digest": "blake3:proposal-live-1",
            "remaining_budget": {"unit": "steps", "value": 3},
            "precondition_digest": "blake3:precondition-live-1",
            "wake_conditions": [{"schema_version": "wake-condition.v1", "kind": "receipt_linked", "receipt_id": RECEIPT_ID}],
            "wake_seq": 1,
            "status": "scheduled",
            "continuation_digest": "blake3:continuation-live-1"
        }],
        "recovery": {
            "crash_prefix": prefix,
            "disposition": disposition,
            "world_root": {
                "schema_version": "world-root-view.v1",
                "world_id": WORLD_ID,
                "branch_id": BRANCH_ID,
                "logical_tick": if committed {2} else {1},
                "state_root": state_root,
                "head_status": head_status,
                "commit_id": if committed {json!("commit-live-1")} else {Value::Null},
                "quarantine_id": if conflict {json!("quarantine-live-1")} else {Value::Null}
            },
            "candidate_root": if conflict {json!(R_NEXT)} else {Value::Null},
            "candidate_receipt": if conflict {json!(receipt)} else {Value::Null},
            "receipt": receipt,
            "provider_invocation_count": 0,
            "kernel_invocation_count": 0,
            "effect_count": effect_count,
            "debit_count": debit_count,
            "receipt_count": receipt_count,
            "world_receipt_linked_count": world_receipt_linked_count,
            "response_replayed": committed
        }
    })
}

#[test]
fn real_world_save_load_round_trip_retains_cognition_commit_and_schedule_state() {
    let (dir, expected_cognition) = seed_world_snapshot("committed");
    let restored = World::load_from_dir(&dir).expect("load persisted cognition world");
    assert_eq!(restored.journal().len(), 1);
    assert_eq!(restored.pending_effects_len(), 0);

    let round_trip = temp_dir("committed-round-trip");
    restored
        .save_to_dir(&round_trip)
        .expect("save restored cognition world");
    let actual = read_snapshot(&round_trip);
    assert_eq!(actual["cognition"], expected_cognition);

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&round_trip);
}

#[test]
fn real_world_load_recovers_every_crash_prefix_without_provider_or_duplicate_effect() {
    for prefix in [
        "before_prepared",
        "prepared_only",
        "committed",
        "committed_missing_projection",
        "conflict",
    ] {
        let (dir, expected_cognition) = seed_world_snapshot(prefix);
        let restored = World::load_from_dir(&dir)
            .unwrap_or_else(|error| panic!("load {prefix} fixture: {error:?}"));
        let round_trip = temp_dir(&format!("{prefix}-round-trip"));
        restored
            .save_to_dir(&round_trip)
            .unwrap_or_else(|error| panic!("save {prefix} recovery: {error:?}"));
        let actual = read_snapshot(&round_trip);
        if prefix == "committed_missing_projection" {
            assert_eq!(actual["cognition"]["recovery"]["disposition"], "committed");
            assert_eq!(
                actual["cognition"]["recovery"]["repaired_projection_ids"]
                    .as_array()
                    .map(Vec::len),
                Some(2),
                "restore must durably record the repaired receipt projections"
            );
        } else if matches!(prefix, "before_prepared" | "prepared_only") {
            assert_eq!(
                actual["cognition"]["recovery"]["disposition"],
                "recovery_pending"
            );
            assert_eq!(
                actual["cognition"]["recovery"]["reject_reason"],
                Value::Null
            );
            assert_eq!(
                actual["cognition"]["recovery"]["provider_invocation_count"],
                0
            );
            assert_eq!(actual["cognition"]["recovery"]["effect_count"], 0);
            assert_eq!(actual["cognition"]["recovery"]["debit_count"], 0);
        } else if prefix == "conflict" {
            assert_eq!(
                actual["cognition"]["recovery"]["disposition"],
                "recovery_pending"
            );
            assert_eq!(
                actual["cognition"]["recovery"]["reject_reason"],
                "commit_conflict"
            );
            assert!(actual["cognition"]["recovery"]["quarantine_id"].is_string());
        } else {
            assert_eq!(
                actual["cognition"]["recovery"], expected_cognition["recovery"],
                "recovery changed durable crash-prefix oracle for {prefix}"
            );
        }
        assert_eq!(
            actual["cognition"]["cognition_journal"], expected_cognition["cognition_journal"],
            "recovery changed cognition journal for {prefix}"
        );
        assert_eq!(
            actual["cognition"]["scheduler_state"], expected_cognition["scheduler_state"],
            "recovery changed scheduler state for {prefix}"
        );
        assert_eq!(
            actual["cognition"]["continuations"], expected_cognition["continuations"],
            "recovery changed continuation state for {prefix}"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&round_trip);
    }
}

#[test]
fn real_world_recovery_api_derives_outcome_from_durable_records() {
    let dir = seed_untrusted_recovery_fixture("committed_missing_projection");
    let mut restored = World::load_from_dir(&dir).expect("load committed recovery fixture");
    let report = restored
        .recover_cognition()
        .expect("recover cognition from real World persistence");
    let report = serde_json::to_value(report).expect("encode World recovery report");

    // A committed marker is authoritative, but repairing a missing projection
    // must not re-run provider/kernel/effect/debit or create a second receipt.
    assert_eq!(report["disposition"], "committed");
    assert_eq!(report["receipt"]["receipt_id"], RECEIPT_ID);
    assert_eq!(report["receipt"]["receipt_digest"], RECEIPT_DIGEST);
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["kernel_invocation_count"], 0);
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["debit_count"], 0);
    assert_eq!(report["world_receipt_linked_count"], 1);

    let replay = restored
        .recover_cognition()
        .expect("replay recovery from the same real World");
    let replay = serde_json::to_value(replay).expect("encode replay recovery report");
    assert_eq!(replay, report, "recovery replay changed durable outcome");
    assert_eq!(restored.pending_effects_len(), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_world_snapshot_gets_explicit_read_only_cognition_defaults() {
    let world = World::new();
    let dir = temp_dir("legacy-defaults");
    world.save_to_dir(&dir).expect("save legacy baseline world");
    let mut snapshot = read_snapshot(&dir);
    snapshot
        .as_object_mut()
        .expect("snapshot object")
        .remove("cognition");
    write_snapshot(&dir, &snapshot);
    force_json_fallback(&dir);

    let restored = World::load_from_dir(&dir).expect("load legacy snapshot");
    let round_trip = temp_dir("legacy-defaults-round-trip");
    restored
        .save_to_dir(&round_trip)
        .expect("save legacy compatibility projection");
    let actual = read_snapshot(&round_trip);

    assert_eq!(actual["cognition"]["schema_version"], COGNITION_SCHEMA);
    assert_eq!(actual["cognition"]["recovery"]["disposition"], "rejected");
    assert_eq!(
        actual["cognition"]["recovery"]["reject_reason"],
        "legacy_no_cognition_proof"
    );
    assert_eq!(
        actual["cognition"]["recovery"]["provider_invocation_count"],
        0
    );
    assert_eq!(actual["cognition"]["recovery"]["effect_count"], 0);
    assert_eq!(actual["cognition"]["recovery"]["debit_count"], 0);
    assert!(actual["cognition"]["recovery"]["receipt"].is_null());

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&round_trip);
}

#[test]
fn world_rejects_caller_lineage_projection_for_sparse_legacy_marker() {
    let (dir, _) = seed_world_snapshot("committed");
    let mut restored = World::load_from_dir(&dir).expect("load committed world");
    let marker: WorldCommitRecordV1 =
        serde_json::from_value(read_snapshot(&dir)["cognition"]["commit_records"][0].clone())
            .expect("decode committed marker");
    let lineage = RuntimeReceiptLineageV1::from_commit_record(
        &marker,
        "agent-live-persistence",
        "session-live-1",
        "turn-live-1",
        "request-live-1",
        "blake3:request-live-1",
        "feedback-live-1",
    );
    assert!(restored.project_runtime_receipt_lineage(lineage).is_err());

    let mut forged = RuntimeReceiptLineageV1::from_commit_record(
        &marker,
        "agent-live-persistence",
        "session-live-1",
        "turn-live-1",
        "request-live-1",
        "blake3:request-live-1",
        "feedback-live-1",
    );
    forged.receipt_digest = "blake3:forged-receipt".to_string();
    assert!(
        restored.project_runtime_receipt_lineage(forged).is_err(),
        "caller-only receipt mutations must fail closed"
    );
    let _ = fs::remove_dir_all(&dir);
}
