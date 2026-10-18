//! Focused regressions for the runtime cognition lifecycle P1 review.

use super::super::*;
use super::agent_cognition_runtime_hardening::{
    AGENT_ID, WORLD_ID, bind_test_turn, digest, envelope, policy, proposal, read_snapshot,
    response_artifact_for_envelope, temp_dir, write_snapshot,
};
use crate::cognition_recovery::cognition_digest_v1;
use serde_json::{Value, json};
use std::fs;

#[test]
fn non_verified_finality_hash_is_optional_and_scheduler_preserves_absence() {
    for status in ["pending", "reorged", "suspended"] {
        let mut world = World::new();
        world
            .bind_cognition_runtime(WORLD_ID, "main", 0, Some(digest(30)), status, 0)
            .expect("non-verified finality may carry a canonical block hash");
    }

    let wake: SchedulerWakeV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-optional-finality",
        "continuation_id": "continuation-optional-finality",
        "world_id": WORLD_ID,
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": digest(31),
        "agent_id": AGENT_ID,
        "agent_session_id": "session.optional-finality",
        "agent_turn_id": "turn.optional-finality",
        "decision_request_id": "request.optional-finality",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 4,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("scheduler wake uses explicit optional absence");
    wake.validate().expect("pending wake with no block hash");
    assert!(
        serde_json::to_value(wake)
            .expect("serialize wake")
            .get("finality_block_hash")
            .is_some_and(Value::is_null)
    );
}

#[test]
fn reorg_invalidates_old_work_then_allows_one_new_runtime_binding() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepared turn");
    let admitted = world
        .admit_cognition_continuation(proposal(&world))
        .expect("continuation");

    world
        .invalidate_cognition_for_reorg(1)
        .expect("invalidate old parent");
    assert_eq!(
        world
            .current_cognition_runtime_binding()
            .expect("old binding remains historical until replacement")
            .reorg_epoch,
        0
    );
    assert_eq!(world.cognition_continuations()[0]["status"], "invalidated");
    assert_eq!(world.cognition()["commit_records"][0]["status"], "aborted");
    assert_eq!(
        world.cognition()["commit_records"][0]["abort_reason"],
        "reorg_invalidated"
    );
    assert_eq!(prepared.status, "prepared");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );

    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 1)
        .expect("new binding accepted after explicit reorg invalidation");
    assert_eq!(
        world
            .current_cognition_runtime_binding()
            .expect("new binding")
            .reorg_epoch,
        1
    );
    assert_eq!(
        world.cognition_continuations()[0]["continuation_id"],
        admitted.continuation_id
    );
}

#[test]
fn cognition_journal_is_dense_chained_and_hashed_sparse_entries_are_rejected() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");

    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("journal events");
    assert!(
        events.len() >= 2,
        "commit lifecycle must have a journal chain"
    );
    for event in events {
        for field in [
            "schema_version",
            "journal_seq",
            "parent_event_digest",
            "event_kind",
            "world_id",
            "branch_id",
            "finality_epoch",
            "finality_status",
            "reorg_epoch",
            "logical_tick",
            "retry_seq",
            "transport_attempt",
            "status",
            "payload_digest",
            "causal_refs",
            "event_digest",
        ] {
            assert!(event.get(field).is_some(), "dense field missing: {field}");
        }
    }
    for pair in events.windows(2) {
        assert_eq!(
            pair[1]["parent_event_digest"], pair[0]["event_digest"],
            "journal parent chain must be exact"
        );
    }

    let dir = temp_dir("dense-journal-malformed-entry");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    let events = snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("mutable events");
    events[1]
        .as_object_mut()
        .expect("event object")
        .remove("payload_digest");
    for event in events.iter_mut() {
        let object = event.as_object_mut().expect("event object");
        let event_digest = object
            .get("event_digest")
            .and_then(Value::as_str)
            .expect("event digest")
            .to_string();
        let _ = event_digest;
        object.remove("event_digest");
        let unsigned = Value::Object(object.clone());
        object.insert(
            "event_digest".to_string(),
            json!(cognition_digest_v1("oasis7.cognition.event.v1", &unsigned)),
        );
    }
    let journal = &mut snapshot["cognition"]["cognition_journal"];
    journal["head_digest"] = json!(cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": journal["head_seq"], "events": journal["events"]})
    ));
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(
        World::load_from_dir(&dir).is_err(),
        "a correctly rehashed but malformed dense event must fail closed"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_receipt_re_evaluates_queued_wake_without_waiting_for_tick() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare receipt identity");

    let mut continuation_proposal = proposal(&world);
    continuation_proposal.wake_conditions = vec![WakeConditionV1 {
        schema_version: "wake-condition.v1".to_string(),
        kind: "receipt_linked".to_string(),
        logical_tick: None,
        event_digest: None,
        receipt_id: Some(prepared.receipt_id.clone()),
        subject: None,
        path_or_rule: None,
        operator: None,
        expected_value_bytes: None,
    }];
    continuation_proposal.next_wake_tick = None;
    continuation_proposal.proposal_digest = continuation_proposal.proposal_digest();
    let admitted = world
        .admit_cognition_continuation(continuation_proposal)
        .expect("receipt wake");
    assert!(
        world
            .select_ready_cognition_wakes(0)
            .expect("initial selection")
            .is_empty()
    );

    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("commit receipt");
    let in_flight = world.cognition_in_flight_wakes().expect("in-flight wake");
    assert!(
        in_flight
            .iter()
            .any(|wake| wake.continuation_id == admitted.continuation_id),
        "committed evidence must wake the queued continuation immediately"
    );
}

#[test]
fn alternate_action_after_prepare_conflicts_without_creating_a_second_marker() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let first = envelope(&world);
    let request = continuous_request_for_envelope(&world, &first);
    let prepared = world
        .prepare_cognition_envelope(first.clone(), Some(response_artifact_for_envelope(&first)))
        .expect("prepare first action");
    let marker_count = world.cognition()["commit_records"]
        .as_array()
        .expect("markers")
        .len();
    let alternate: Action = serde_json::from_value(json!({
        "type": "RegisterAgent",
        "data": {
            "agent_id": AGENT_ID,
            "pos": {"x_cm": 1, "y_cm": 0, "z_cm": 0}
        }
    }))
    .expect("alternate action");
    let response = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
            .to_string(),
        context_version: RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
        response_digest: digest(17),
        artifact_digest: String::new(),
    };
    let mut response = response;
    response.refresh_artifact_digest();
    let error = world
        .commit_cognition_action(request, alternate, response)
        .expect_err("alternate action must conflict with prepared identity");
    assert!(format!("{error:?}").contains("idempotency_conflict"));
    assert_eq!(
        world.cognition()["commit_records"]
            .as_array()
            .expect("markers")
            .len(),
        marker_count
    );
    assert_eq!(prepared.status, "prepared");
}

#[test]
fn aborted_commit_marker_is_durable_and_has_no_receipt_or_world_effect() {
    let mut world = World::new();
    let decision = envelope(&world);
    let _prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    let parent_root = world.current_state_root_hash().expect("parent root");
    let dir = temp_dir("aborted-cognition-marker");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    let marker = snapshot["cognition"]["commit_records"]
        .as_array_mut()
        .expect("markers")
        .first_mut()
        .expect("prepared marker");
    marker["status"] = json!("aborted");
    marker["abort_reason"] = json!("cancelled");
    snapshot["cognition"]["staged_actions"] = json!({});
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let restored = World::load_from_dir(&dir).expect("aborted marker recovery");
    assert_eq!(restored.state().time, world.state().time);
    assert_eq!(
        restored.current_state_root_hash().expect("root"),
        parent_root
    );
    assert_eq!(
        restored.cognition()["commit_records"][0]["status"],
        "aborted"
    );
    assert_eq!(
        restored.cognition()["commit_records"][0]["abort_reason"],
        "cancelled"
    );
    assert!(
        restored.cognition()["receipt_registry"]
            .as_array()
            .expect("receipts")
            .is_empty()
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn aborting_a_prepared_commit_writes_terminal_cancel_events_without_effect() {
    let mut world = World::new();
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    let parent_root = world.current_state_root_hash().expect("parent root");
    let aborted = world
        .abort_cognition_commit(&prepared.commit_id, "cancelled")
        .expect("abort prepared commit");
    assert_eq!(aborted.status, "aborted");
    assert_eq!(aborted.abort_reason.as_deref(), Some("cancelled"));
    assert_eq!(world.current_state_root_hash().expect("root"), parent_root);
    assert!(
        world.cognition()["receipt_registry"]
            .as_array()
            .expect("receipts")
            .is_empty()
    );
    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("events");
    assert!(events.iter().any(|event| {
        event["event_kind"] == "CognitionTurnCancelled"
            && event["status"] == "rejected"
            && event["reject_reason"] == "cancelled"
    }));
    assert!(events.iter().any(|event| {
        event["event_kind"] == "CognitionTurnCompleted" && event["status"] == "rejected"
    }));

    world
        .abort_cognition_commit(&prepared.commit_id, "late_response_after_cancel")
        .expect("late response must be durably rejected after cancellation");
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| {
                event["event_kind"] == "LateResponseRejected"
                    && event["reject_reason"] == "late_response_after_cancel"
            })
    );
}

#[test]
fn provider_failure_before_response_is_durable_terminal_without_receipt_or_effect() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let root_before = world.current_state_root_hash().expect("parent root");
    world
        .capture_cognition_context(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &digest(5),
            &digest(4),
        )
        .expect("capture context");
    world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &digest(5),
            &digest(6),
            1,
            1,
        )
        .expect("persist provider dispatch");
    world
        .fail_cognition_turn(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &digest(5),
            "provider_failure",
            1,
            1,
        )
        .expect("persist provider failure");

    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("journal events");
    let turn_events: Vec<&str> = events
        .iter()
        .filter(|event| event["agent_turn_id"] == "turn.runtime-hardening")
        .filter_map(|event| event["event_kind"].as_str())
        .collect();
    assert_eq!(
        turn_events,
        vec![
            "TurnStarted",
            "ContextCaptured",
            "RequestDispatched",
            "CognitionTurnFailed",
            "CognitionTurnCompleted"
        ]
    );
    for event in events.iter().filter(|event| {
        event["agent_turn_id"] == "turn.runtime-hardening"
            && matches!(
                event["event_kind"].as_str(),
                Some("CognitionTurnFailed") | Some("CognitionTurnCompleted")
            )
    }) {
        assert_eq!(event["status"], "failed");
        assert_eq!(event["failure_reason"], "provider_failure");
        assert_eq!(event["retry_seq"], 1);
        assert_eq!(event["transport_attempt"], 1);
    }
    assert!(
        events.iter().all(|event| {
            event["agent_turn_id"] != "turn.runtime-hardening"
                || !matches!(
                    event["event_kind"].as_str(),
                    Some("ResponseRecorded")
                        | Some("DecisionEnvelopeSubmitted")
                        | Some("WorldReceiptLinked")
                )
        }),
        "pre-response failure must not publish response or receipt events"
    );
    assert!(
        world.cognition()["responses"]
            .as_array()
            .expect("responses")
            .is_empty()
    );
    assert!(
        world.cognition()["commit_records"]
            .as_array()
            .expect("commit records")
            .is_empty()
    );
    assert_eq!(
        world.current_state_root_hash().expect("root after failure"),
        root_before
    );
    let event_count = events.len();
    world
        .fail_cognition_turn(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &digest(5),
            "provider_failure",
            1,
            1,
        )
        .expect("same failure replay is idempotent");
    assert_eq!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("journal")
            .len(),
        event_count
    );

    let dir = temp_dir("provider-failure-terminal");
    world.save_to_dir(&dir).expect("persist failed turn");
    let restored = World::load_from_dir(&dir).expect("restore failed turn");
    assert!(
        restored.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("restored journal")
            .iter()
            .any(|event| {
                event["event_kind"] == "CognitionTurnCompleted"
                    && event["status"] == "failed"
                    && event["failure_reason"] == "provider_failure"
            })
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_cognition_history_survives_later_world_progression_and_restore() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let commit_tick = world.state().time;
    world.step().expect("ordinary world progression");
    assert!(world.state().time > commit_tick);

    let dir = temp_dir("committed-history-after-progress");
    world.save_to_dir(&dir).expect("save progressed world");
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let restored = World::load_from_dir(&dir).expect("historical committed receipt remains valid");
    assert_eq!(restored.cognition()["recovery"]["disposition"], "committed");
    assert_eq!(
        restored.cognition()["recovery"]["receipt"]["receipt_id"],
        committed.receipt_id
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn same_key_transport_retry_is_monotonic_and_exact_duplicates_are_idempotent() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let request_digest = digest(5);
    let provider_key = digest(6);
    world
        .capture_cognition_context(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &digest(4),
        )
        .expect("capture context");
    world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &provider_key,
            1,
            1,
        )
        .expect("first dispatch");
    world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &provider_key,
            1,
            2,
        )
        .expect("second transport attempt");
    world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &provider_key,
            1,
            2,
        )
        .expect("duplicate attempt is idempotent");
    let stale_replay = world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &provider_key,
            1,
            1,
        )
        .expect_err("an older transport attempt cannot replay after a newer one");
    assert!(format!("{stale_replay:?}").contains("cognition_dispatch_conflict"));
    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("events");
    let attempts: Vec<u64> = events
        .iter()
        .filter(|event| event["event_kind"] == "RequestDispatched")
        .filter_map(|event| event["transport_attempt"].as_u64())
        .collect();
    assert_eq!(attempts, vec![1, 2]);
    let collision = world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &digest(7),
            1,
            3,
        )
        .expect_err("changed provider invocation key must conflict");
    assert!(format!("{collision:?}").contains("cognition_dispatch_conflict"));
}

#[test]
fn transport_attempt_gap_is_rejected_instead_of_becoming_a_forged_prefix() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let request_digest = digest(5);
    world
        .capture_cognition_context(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &digest(4),
        )
        .expect("capture context");
    let error = world
        .dispatch_cognition_request(
            AGENT_ID,
            "session.runtime-hardening",
            "turn.runtime-hardening",
            "request.runtime-hardening",
            &request_digest,
            &digest(6),
            1,
            3,
        )
        .expect_err("first persisted transport attempt must be one");
    assert!(format!("{error:?}").contains("cognition_dispatch_conflict"));
    assert!(
        !world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("journal")
            .iter()
            .any(|event| event["event_kind"] == "RequestDispatched")
    );
}

#[test]
fn exact_response_prefix_dispatch_replay_is_idempotent() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let decision = envelope(&world);
    world
        .start_cognition_turn(
            &decision.agent_id,
            &decision.agent_session_id,
            &decision.agent_turn_id,
            &decision.decision_request_id,
            &decision.request_digest,
        )
        .expect("register cognition turn");
    world
        .capture_cognition_context(
            &decision.agent_id,
            &decision.agent_session_id,
            &decision.agent_turn_id,
            &decision.decision_request_id,
            &decision.request_digest,
            &decision.context_digest,
        )
        .expect("capture context");
    world
        .dispatch_cognition_request(
            &decision.agent_id,
            &decision.agent_session_id,
            &decision.agent_turn_id,
            &decision.decision_request_id,
            &decision.request_digest,
            &decision.provider_invocation_key,
            decision.retry_seq,
            1,
        )
        .expect("record initial dispatch");

    world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("exact prefix replay must be idempotent");
    let dispatch_count = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("journal")
        .iter()
        .filter(|event| event["event_kind"] == "RequestDispatched")
        .count();
    assert_eq!(dispatch_count, 1);
}

#[test]
fn reorg_invalidates_response_artifacts_and_rejects_stale_epoch_requests() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    world
        .admit_cognition_continuation(proposal(&world))
        .expect("continuation");
    world
        .invalidate_cognition_for_reorg(1)
        .expect("invalidate old epoch");
    assert_eq!(
        world.cognition()["responses"][0]["invalidation_reason"],
        "reorg_invalidated"
    );
    assert_eq!(
        world.cognition()["responses"][0]["envelope_digest"],
        prepared.envelope_digest
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| {
                event["event_kind"] == "CognitionResponseInvalidated"
                    && event["envelope_digest"] == prepared.envelope_digest
                    && event["agent_turn_id"] == decision.agent_turn_id
                    && event["request_digest"] == decision.request_digest
                    && event["invalidation_reason"] == "reorg_invalidated"
            })
    );
    let stale = world
        .invalidate_cognition_for_reorg(0)
        .expect_err("a stale reorg epoch must not rewrite newer invalidation");
    assert!(format!("{stale:?}").contains("reorg_epoch_stale"));
}

#[test]
fn persisted_finality_hash_shape_is_strictly_optional_or_string() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("binding");
    let dir = temp_dir("malformed-finality-json-shape");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["runtime_binding"]["finality_block_hash"] = json!(42);
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let error =
        World::load_from_dir(&dir).expect_err("numeric finality hash is malformed, not absent");
    assert!(format!("{error:?}").contains("recovery_pending"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn rehashed_event_with_forged_payload_digest_is_rejected_on_restore() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("forged-event-payload-digest");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    let events = snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events");
    events[1]["payload_digest"] = json!(digest(99));
    for event in events.iter_mut() {
        let object = event.as_object_mut().expect("event object");
        object.remove("event_digest");
        let unsigned = Value::Object(object.clone());
        object.insert(
            "event_digest".to_string(),
            json!(cognition_digest_v1("oasis7.cognition.event.v1", &unsigned)),
        );
    }
    let journal = &mut snapshot["cognition"]["cognition_journal"];
    journal["head_digest"] = json!(cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": journal["head_seq"], "events": journal["events"]})
    ));
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(World::load_from_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn response_bearing_commit_emits_complete_dense_lifecycle_prefix() {
    let mut world = World::new();
    bind_test_turn(&mut world);
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let kinds: Vec<&str> = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|event| {
            event["agent_turn_id"] == decision.agent_turn_id
                && event["request_digest"] == decision.request_digest
        })
        .filter_map(|event| event["event_kind"].as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "TurnStarted",
            "ContextCaptured",
            "RequestDispatched",
            "ResponseRecorded",
            "DecisionEnvelopeSubmitted",
            "DecisionValidated",
            "WorldReceiptLinked",
            "CognitionTurnCompleted",
        ]
    );
}

fn continuous_request_for_envelope(
    world: &World,
    envelope: &AgentDecisionEnvelopeV1,
) -> RuntimeCognitionCommitRequestV1 {
    let binding = world.current_cognition_runtime_binding().expect("binding");
    RuntimeCognitionCommitRequestV1 {
        agent_id: envelope.agent_id.clone(),
        agent_session_id: envelope.agent_session_id.clone(),
        agent_turn_id: envelope.agent_turn_id.clone(),
        decision_request_id: envelope.decision_request_id.clone(),
        retry_seq: envelope.retry_seq,
        transport_attempt: 1,
        request_digest: envelope.request_digest.clone(),
        observation_digest: envelope.observation_digest.clone(),
        context_digest: envelope.context_digest.clone(),
        capability_snapshot_hash: envelope.capability_snapshot_hash.clone(),
        authority_context_hash: envelope.authority_context_hash.clone(),
        captured_base_binding: RuntimeCognitionBaseBindingV1 {
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            finality_epoch: binding.finality_epoch,
            finality_block_hash: binding.finality_block_hash.map(|hash| hash.to_string()),
            finality_status: binding.finality_status,
            base_tick: binding.base_tick,
            base_world_hash: binding.base_world_hash.to_string(),
            reorg_epoch: binding.reorg_epoch,
            runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        },
    }
}
