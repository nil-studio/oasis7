use super::*;
use crate::runtime::RuntimeFeedbackOutboxRecordV1;
use serde_json::json;

fn feedback(agent: &str, session: &str, seq: u64, feedback_id: &str) -> FeedbackEnvelopeV1 {
    FeedbackEnvelopeV1 {
        feedback_id: feedback_id.to_string(),
        feedback_seq: seq,
        agent_subject: agent.to_string(),
        agent_session_id: session.to_string(),
        agent_turn_id: format!("turn-{seq}"),
        decision_request_id: format!("request-{seq}"),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "rejected".to_string(),
        request_digest: Digest32(format!("blake3:{}", "a".repeat(64))),
        reject_reason: Some("stale_base".to_string()),
        provenance: "runtime_authoritative".to_string(),
    }
}

fn acked_record(feedback: &FeedbackEnvelopeV1) -> RuntimeFeedbackOutboxRecordV1 {
    let mut record = crate::runtime_host::world::new()
        .enqueue_runtime_feedback(crate::simulator::agent_api_compat::feedback_to_api(
            feedback,
        ))
        .expect("feedback fixture forms a canonical Runtime outbox record");
    record.state = "acked".to_string();
    record.validate().expect("acked outbox fixture validates");
    record
}

fn turn(agent: &str, session: &str) -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: agent.to_string(),
        agent_session_id: session.to_string(),
        agent_turn_id: "new-turn".to_string(),
        decision_request_id: "new-request".to_string(),
        request_digest: Digest32(format!("blake3:{}", "b".repeat(64))),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    }
}

#[test]
fn acknowledged_history_restores_only_bounded_replay_and_collision_metadata() {
    let records = (1..=10)
        .map(|seq| {
            acked_record(&feedback(
                "agent-restore",
                "session-restore",
                seq,
                &format!("feedback-{seq}"),
            ))
        })
        .collect::<Vec<_>>();
    let mut store = AgentCognitionStore::default();

    store
        .restore_runtime_feedback_outbox(&records)
        .expect("complete acknowledged history restores");
    store
        .restore_runtime_feedback_outbox(&records)
        .expect("repeated restore is idempotent");

    assert!(!store.contains_feedback("agent-restore", "session-restore", "feedback-1"));
    assert!(store.contains_feedback("agent-restore", "session-restore", "feedback-3"));
    assert!(store.contains_feedback("agent-restore", "session-restore", "feedback-10"));
    store
        .accept_feedback(feedback(
            "agent-restore",
            "session-restore",
            10,
            "feedback-10",
        ))
        .expect("exact replay is recognized without an active request");

    let mut reused_id = feedback("agent-restore", "session-restore", 10, "feedback-10");
    reused_id.reject_reason = Some("expired".to_string());
    assert_eq!(
        store
            .accept_feedback(reused_id)
            .expect_err("changed envelope with the same ID is rejected")
            .code(),
        "feedback_id_conflict"
    );
    assert_eq!(
        store
            .accept_feedback(feedback(
                "agent-restore",
                "session-restore",
                10,
                "different-id-at-seq-10",
            ))
            .expect_err("different ID cannot reuse a restored sequence")
            .code(),
        "feedback_identity_collision"
    );

    let next_feedback = feedback("agent-restore", "session-restore", 11, "feedback-11");
    let mut next_turn = turn("agent-restore", "session-restore");
    next_turn.agent_turn_id = next_feedback.agent_turn_id.clone();
    next_turn.decision_request_id = next_feedback.decision_request_id.clone();
    next_turn.request_digest = next_feedback.request_digest.clone();
    store
        .begin_turn(&next_turn)
        .expect("restored sequence permits the next request");
    store
        .accept_feedback(next_feedback)
        .expect("next contiguous envelope advances restored sequence");
}

#[test]
fn gap_duplicate_and_unacknowledged_history_fence_fresh_admission() {
    let gap = vec![
        acked_record(&feedback("agent-fence", "session-gap", 1, "gap-1")),
        acked_record(&feedback("agent-fence", "session-gap", 3, "gap-3")),
    ];
    let duplicate_sequence = vec![
        acked_record(&feedback("agent-fence", "session-duplicate", 1, "dup-a")),
        acked_record(&feedback("agent-fence", "session-duplicate", 1, "dup-b")),
    ];
    let duplicate_id = vec![
        acked_record(&feedback("agent-fence", "session-id", 1, "reused-id")),
        acked_record(&feedback("agent-fence", "session-id", 2, "reused-id")),
    ];
    let mut unacked = acked_record(&feedback("agent-fence", "session-unacked", 1, "pending-1"));
    unacked.state = "pending".to_string();

    for (records, session) in [
        (gap, "session-gap"),
        (duplicate_sequence, "session-duplicate"),
        (vec![unacked], "session-unacked"),
    ] {
        let mut store = AgentCognitionStore::default();
        store
            .restore_runtime_feedback_outbox(&records)
            .expect("a partition-local history problem must not pause unrelated Agents");
        assert_eq!(
            store
                .begin_turn(&turn("agent-fence", session))
                .expect_err("blocked history cannot admit a new request")
                .code(),
            "feedback_recovery_blocked"
        );
        store
            .begin_turn(&turn("agent-fence", "healthy-session"))
            .expect("a healthy session of the same Agent remains admissible");
    }

    let mut store = AgentCognitionStore::default();
    store
        .restore_runtime_feedback_outbox(&duplicate_id)
        .expect("the duplicate-ID history is fenced at its own partition");
    assert_eq!(
        store
            .begin_turn(&turn("agent-fence", "session-id"))
            .expect_err("duplicate identity cannot admit a new request")
            .code(),
        "feedback_recovery_blocked"
    );
    store
        .begin_turn(&turn("healthy-agent", "healthy-session"))
        .expect("an unrelated Agent remains admissible");
}

#[test]
fn feedback_id_collision_fences_only_the_colliding_partitions() {
    let records = vec![
        acked_record(&feedback("agent-a", "session-a", 1, "reused-id")),
        acked_record(&feedback("agent-b", "session-b", 1, "reused-id")),
        acked_record(&feedback("agent-c", "session-c", 1, "healthy-id")),
    ];
    let mut store = AgentCognitionStore::default();

    store
        .restore_runtime_feedback_outbox(&records)
        .expect("a cross-partition ID collision fences the affected partitions only");
    for (agent, session) in [("agent-a", "session-a"), ("agent-b", "session-b")] {
        assert_eq!(
            store
                .begin_turn(&turn(agent, session))
                .expect_err("a colliding history cannot admit a new request")
                .code(),
            "feedback_recovery_blocked"
        );
    }
    store
        .begin_turn(&turn("agent-c", "session-c"))
        .expect("a non-colliding partition restores and remains admissible");
}

#[test]
fn tampered_outbox_payload_fails_validation_before_restore() {
    let mut record = acked_record(&feedback("agent-tamper", "session-tamper", 1, "tamper-1"));
    record.payload["decision_request_id"] = json!("forged-request");
    let mut store = AgentCognitionStore::default();

    assert_eq!(
        store
            .restore_runtime_feedback_outbox(&[record])
            .expect_err("payload/identity tampering is rejected")
            .code(),
        "feedback_recovery_record_invalid"
    );
}

#[test]
fn restore_partitions_history_by_agent_and_session() {
    let records = vec![
        acked_record(&feedback("agent-isolated", "session-a", 1, "feedback-a-1")),
        acked_record(&feedback("agent-isolated", "session-b", 1, "feedback-b-1")),
        acked_record(&feedback("other-agent", "session-a", 1, "feedback-other-1")),
    ];
    let mut store = AgentCognitionStore::default();

    store
        .restore_runtime_feedback_outbox(&records)
        .expect("independent acknowledged partitions restore");
    assert!(store.contains_feedback("agent-isolated", "session-a", "feedback-a-1"));
    assert!(!store.contains_feedback("agent-isolated", "session-b", "feedback-a-1"));
    assert!(store.contains_feedback("agent-isolated", "session-b", "feedback-b-1"));
    assert!(!store.contains_feedback("agent-isolated", "session-a", "feedback-other-1"));
}
