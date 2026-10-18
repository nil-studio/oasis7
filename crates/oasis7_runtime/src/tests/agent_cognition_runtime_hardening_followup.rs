//! Follow-up recovery, replay, and production lease regressions.

use super::super::*;
use super::agent_cognition_runtime_hardening::{
    AGENT_ID, WORLD_ID, bind_test_turn, continuous_commit_request, continuous_response_artifact,
    digest, envelope, policy, proposal, read_snapshot, response_artifact_for_envelope, temp_dir,
    test_continuation, test_continuation_wake, write_snapshot,
};
use crate::cognition_recovery::cognition_digest_v1;
use serde_json::json;
use std::fs;

#[test]
fn recovery_repairs_a_missing_committed_turn_completion() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("missing-committed-completion");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events")
        .pop();
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let restored = World::load_from_dir(&dir).expect("repair missing completion");
    let events = restored.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("events after repair");
    assert_eq!(
        events.last().expect("completion")["kind"],
        "CognitionTurnCompleted"
    );
    assert_eq!(events.last().expect("completion")["status"], "committed");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn runtime_binding_rejects_noncanonical_verified_finality_hash() {
    let mut world = World::new();
    let error = world
        .bind_cognition_runtime(
            WORLD_ID,
            "main",
            1,
            Some("hash:not-a-digest".to_string()),
            "verified",
            0,
        )
        .expect_err("verified finality must carry a canonical digest");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { ref reason }
            if reason.contains("runtime_binding")
                || reason.contains("runtime_cognition_base_binding_invalid")
    ));
}

#[test]
fn recovery_quarantines_visible_root_mismatch_but_preserves_trusted_head() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let trusted_state_root = world.current_state_root_hash().expect("trusted root");
    let dir = temp_dir("visible-root-mismatch");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["recovery"]["world_root"] = json!({
        "schema_version": "world-root-view.v1",
        "world_id": WORLD_ID,
        "branch_id": "main",
        "logical_tick": world.state().time,
        "state_root": digest(88),
        "head_status": "canonical",
        "commit_id": committed.commit_id,
        "quarantine_id": null
    });
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let restored = World::load_from_dir(&dir).expect("restore mismatch");
    let recovery = &restored.cognition()["recovery"];
    assert_eq!(recovery["disposition"], "recovery_pending");
    assert_eq!(recovery["reject_reason"], "world_root_mismatch");
    assert_eq!(recovery["world_root"]["state_root"], trusted_state_root);
    assert_eq!(recovery["world_root"]["head_status"], "canonical");
    assert_eq!(recovery["candidate_root"], committed.staged_state_root);
    assert_eq!(
        recovery["candidate_receipt"]["receipt_id"],
        committed.receipt_id
    );
    assert!(recovery["quarantine_id"].as_str().is_some());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn recovery_repairs_a_committed_marker_with_missing_projection() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("missing-committed-projection");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["receipt_registry"] = json!([]);
    snapshot["cognition"]["idempotency_index"] = json!({});
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    let restored = World::load_from_dir(&dir).expect("restore missing projection");
    assert_eq!(
        restored.cognition()["receipt_registry"][0]["receipt_id"],
        committed.receipt_id
    );
    assert_eq!(
        restored.cognition()["idempotency_index"][&committed.envelope_idempotency_key]["receipt_id"],
        committed.receipt_id
    );
    let repaired = restored.cognition()["recovery"]["repaired_projection_ids"]
        .as_array()
        .expect("repair markers");
    assert!(repaired.iter().any(|id| id == "receipt_registry"));
    assert!(repaired.iter().any(|id| id == "idempotency_index"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn typed_context_digest_handoff_rejects_alternate_goal_or_policy() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted_proposal = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(admitted_proposal.clone())
        .expect("admit continuation");
    let context = world
        .validate_cognition_context_digests(&admitted.continuation_id, &admitted_proposal)
        .expect("admitted context remains valid");
    assert_eq!(context.goal_digest, admitted_proposal.goal_digest);

    let mut alternate = admitted_proposal;
    alternate.goal_digest = digest(90);
    alternate.proposal_digest = alternate.proposal_digest();
    let error = world
        .validate_cognition_context_digests(&admitted.continuation_id, &alternate)
        .expect_err("a changed goal must not cross the Runtime wake boundary");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { ref reason }
            if reason.contains("cognition_context_mismatch")
    ));
}

#[test]
fn typed_budget_consumption_is_monotonic_and_completes_on_exhaustion() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted = world
        .admit_cognition_continuation(proposal(&world))
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    let _ = world
        .select_ready_cognition_wakes(world.state().time)
        .expect("select wake");
    let consumed = world
        .consume_cognition_continuation_budget(&admitted.continuation_id, 2)
        .expect("consume exact remaining budget");
    assert_eq!(consumed.remaining_budget.value, 0);
    assert_eq!(consumed.status, ContinuationStatusV1::Completed);
    assert_eq!(world.cognition_scheduler_snapshot()["in_flight"], json!({}));
    assert_eq!(
        world.cognition_continuations()[0]["terminal_disposition"],
        "budget_exhausted"
    );
    let error = world
        .consume_cognition_continuation_budget(&admitted.continuation_id, 1)
        .expect_err("completed continuations cannot spend more budget");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { ref reason }
            if reason.contains("wake_not_in_flight")
    ));
}

#[test]
fn typed_wake_handoff_requires_terminal_or_validated_replan() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted = world
        .admit_cognition_continuation(proposal(&world))
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    let selected = world
        .select_ready_cognition_wakes(world.state().time)
        .expect("select wake");
    let wake_id = selected
        .first()
        .map(|wake| wake.wake_id.clone())
        .or_else(|| {
            world
                .cognition_in_flight_wakes()
                .expect("in-flight")
                .first()
                .map(|wake| wake.wake_id.clone())
        })
        .expect("wake lease");
    let result = world
        .handoff_cognition_wake(
            &wake_id,
            CognitionWakeDispositionV1::Terminal {
                status: ContinuationStatusV1::Completed,
                reason: "provider_terminal".to_string(),
            },
        )
        .expect("terminal disposition handoff");
    assert_eq!(
        result.continuation.continuation_id,
        admitted.continuation_id
    );
    assert_eq!(result.continuation.status, ContinuationStatusV1::Completed);
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
}

#[test]
fn runtime_resume_admits_new_request_digest_with_stable_origin_lineage() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let original = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(original.clone())
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    let wake_id = world
        .cognition_in_flight_wakes()
        .expect("in-flight wake")
        .first()
        .expect("selected wake")
        .wake_id
        .clone();

    let mut next = original.clone();
    next.continuation_proposal_id = "proposal-runtime-resume-next".to_string();
    next.agent_session_id = "session.runtime-resume-next".to_string();
    next.agent_turn_id = "turn.runtime-resume-next".to_string();
    next.decision_request_id = "request.runtime-resume-next".to_string();
    // `budget_spent` is charged by Runtime before the next proposal is
    // admitted.  The proposal therefore carries the residual budget (2 - 1),
    // rather than replaying the original budget of two steps.
    next.remaining_budget.value = 1;
    next.proposal_digest = next.proposal_digest();
    let request_digest = digest(81);
    let result = world
        .resume_cognition_wake(
            &wake_id,
            next.clone(),
            1,
            CognitionContinuationResumeRequestV1 {
                agent_session_id: next.agent_session_id.clone(),
                agent_turn_id: next.agent_turn_id.clone(),
                decision_request_id: next.decision_request_id.clone(),
                request_digest: request_digest.clone(),
                context_digest: digest(82),
            },
        )
        .expect("Runtime should atomically consume and resume the wake");
    let resumed = result
        .replanned_continuation
        .expect("resume should allocate the next continuation");
    assert_eq!(resumed.agent_session_id, next.agent_session_id);
    assert_eq!(resumed.decision_request_id, next.decision_request_id);
    assert_eq!(
        resumed.origin_request_digest,
        original.origin_request_digest
    );
    assert_eq!(resumed.remaining_budget.value, 1);

    let events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("journal events");
    assert!(events.iter().any(|event| {
        event["event_kind"] == "TurnStarted"
            && event["decision_request_id"] == next.decision_request_id
            && event["request_digest"] == request_digest
            && event["origin_request_digest"] == original.origin_request_digest
            && event["continuation_id"] == admitted.continuation_id
            && event["wake_id"] == wake_id
    }));
    let replay = world
        .resume_cognition_wake(
            &wake_id,
            next,
            1,
            CognitionContinuationResumeRequestV1 {
                agent_session_id: "session.runtime-resume-next".to_string(),
                agent_turn_id: "turn.runtime-resume-next".to_string(),
                decision_request_id: "request.runtime-resume-next".to_string(),
                request_digest,
                context_digest: digest(82),
            },
        )
        .expect("an exact resume replay returns the durable result");
    assert_eq!(
        replay
            .replanned_continuation
            .expect("replayed continuation")
            .continuation_id,
        resumed.continuation_id
    );
}

#[test]
fn recovery_rejects_a_tampered_trailing_cognition_event() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("tampered-trailing-cognition-event");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events")
        .push(json!({"journal_seq": 99, "kind": "ForgedTrailingEvent"}));
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(World::load_from_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn recovery_rejects_a_missing_middle_cognition_event() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let dir = temp_dir("missing-middle-cognition-event");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events")
        .remove(1);
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON restore");
    assert!(World::load_from_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unknown_continuation_wake_is_rejected() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let binding = world.current_cognition_runtime_binding().expect("binding");
    let wake: SchedulerWakeV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-unknown-continuation",
        "continuation_id": "continuation-does-not-exist",
        "world_id": WORLD_ID,
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": binding.runtime_manifest_hash.to_string(),
        "agent_id": AGENT_ID,
        "agent_session_id": "session.unknown",
        "agent_turn_id": "turn.unknown",
        "decision_request_id": "request.unknown",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 4,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake");
    assert!(world.enqueue_cognition_wake(wake).is_err());
}

#[test]
fn scheduler_rejects_duplicate_wake_while_original_lease_is_in_flight() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let continuation = world
        .admit_cognition_continuation(proposal(&world))
        .expect("continuation");
    world.step().expect("service scheduler");
    let wake: SchedulerWakeV1 = serde_json::from_value(
        world.cognition_scheduler_snapshot()["in_flight"][&continuation.wake_id].clone(),
    )
    .expect("in-flight wake");
    assert!(world.enqueue_cognition_wake(wake).is_err());
}

#[test]
fn bound_world_rejects_continuation_without_a_registered_turn() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    assert!(
        world
            .admit_cognition_continuation(proposal(&world))
            .is_err()
    );
}

#[test]
fn failed_first_proposal_does_not_install_runtime_binding() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    let before = world.cognition().clone();
    let mut candidate = proposal(&world);
    candidate.proposal_digest = digest(99);
    assert!(world.admit_cognition_continuation(candidate).is_err());
    assert_eq!(world.cognition(), &before);
}

#[test]
fn committed_replay_rejects_an_alternate_action_for_the_same_request() {
    let mut world = World::new();
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 0)
        .expect("bind World authority");
    let envelope = envelope(&world);
    let first_action: Action = serde_json::from_value(envelope.action.clone()).expect("action");
    world
        .commit_cognition_action(
            continuous_commit_request(&world),
            first_action,
            continuous_response_artifact(&continuous_commit_request(&world)),
        )
        .expect("first commit");
    let mut alternate = envelope.action.clone();
    alternate["data"]["pos"]["x_cm"] = json!(1);
    let alternate_action: Action = serde_json::from_value(alternate).expect("alternate action");
    let request = continuous_commit_request(&world);
    let error = world
        .commit_cognition_action(
            request.clone(),
            alternate_action,
            continuous_response_artifact(&request),
        )
        .expect_err("alternate action must not replay the original receipt");
    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { ref reason }
            if reason.contains("envelope_idempotency") || reason.contains("replay")
    ));
}

#[test]
fn direct_response_replay_rejects_a_tampered_trailing_event() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let mut snapshot = world.snapshot();
    snapshot.cognition["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events")
        .push(json!({"journal_seq": 99, "kind": "forged"}));
    let tampered = World::from_snapshot(snapshot, world.journal().clone()).expect("snapshot");
    assert!(
        tampered
            .replay_cognition_response(&committed.envelope_digest)
            .is_err(),
        "replay must verify the current journal before accepting a historical head"
    );
}

#[test]
fn saved_world_persists_cognition_commit_before_the_api_returns() {
    let mut world = World::new();
    let envelope = envelope(&world);
    let dir = temp_dir("durable-cognition-commit");
    world.save_to_dir(&dir).expect("save baseline");
    let prepared = world
        .prepare_cognition_envelope(
            envelope.clone(),
            Some(response_artifact_for_envelope(&envelope)),
        )
        .expect("prepare");
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("finalize");
    let restored = World::load_from_dir(&dir).expect("restore durable commit");
    assert_eq!(
        restored.cognition()["commit_records"][0]["receipt_id"],
        committed.receipt_id
    );
    assert_eq!(restored.state().time, world.state().time);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn unbound_world_rejects_unknown_continuation_wakes() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    let wake: SchedulerWakeV1 = serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-unbound-foreign",
        "continuation_id": "continuation-unbound-foreign",
        "world_id": WORLD_ID,
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": world.current_manifest_hash().expect("manifest"),
        "agent_id": AGENT_ID,
        "agent_session_id": "session.foreign",
        "agent_turn_id": "turn.foreign",
        "decision_request_id": "request.foreign",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 4,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake");
    assert!(world.enqueue_cognition_wake(wake).is_err());
}

#[test]
fn unbound_continuation_admission_requires_a_registered_turn() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    assert!(
        world
            .admit_cognition_continuation(proposal(&world))
            .is_err()
    );
}

#[test]
fn production_constructor_installs_a_world_owned_cognition_binding() {
    let world = World::new_production_hardened_with_cognition_binding(
        WORLD_ID, "main", 0, None, "pending", 0,
    )
    .expect("production runtime binding");
    let binding = world
        .current_cognition_runtime_binding()
        .expect("current production binding");
    assert_eq!(binding.world_id, WORLD_ID);
    assert!(binding.base_world_hash.is_canonical_blake3());
}

#[test]
fn production_lease_consumer_releases_only_the_dispatched_wake() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    let wake = test_continuation_wake(&world, "wake-consumer", "continuation-consumer");
    world
        .install_cognition_continuation_for_test(test_continuation(&wake))
        .expect("install continuation");
    world.enqueue_cognition_wake(wake).expect("enqueue");
    world.step().expect("select lease");
    let consumed = world
        .consume_cognition_wake("wake-consumer", |lease| {
            assert_eq!(lease.wake_id, "wake-consumer");
            Ok(CognitionWakeDispositionV1::Terminal {
                status: ContinuationStatusV1::Completed,
                reason: "provider_terminal".to_string(),
            })
        })
        .expect("consume lease");
    assert_eq!(consumed.wake_id, "wake-consumer");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
}

#[test]
fn continuation_validity_window_expires_and_releases_wake_before_delivery() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let mut expired_proposal = proposal(&world);
    expired_proposal.valid_until_tick = Some(world.state().time);
    expired_proposal.proposal_digest = expired_proposal.proposal_digest();
    let admitted = world
        .admit_cognition_continuation(expired_proposal)
        .expect("admit expiring continuation");

    world.step().expect("advance beyond validity window");
    assert_eq!(world.cognition()["continuations"][0]["status"], "expired");
    assert_eq!(
        world.cognition()["continuations"][0]["terminal_disposition"],
        "expired"
    );
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["event_kind"] == "ContinuationExpired"
                && event["continuation_id"] == admitted.continuation_id)
    );
}

#[test]
fn stale_replan_context_releases_old_continuation_without_budget_spend() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted_proposal = proposal(&world);
    world
        .admit_cognition_continuation(admitted_proposal.clone())
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    let wake_id = world
        .cognition_in_flight_wakes()
        .expect("in-flight wake")
        .first()
        .expect("leased wake")
        .wake_id
        .clone();
    let mut stale = admitted_proposal;
    stale.goal_digest = digest(90);
    stale.proposal_digest = stale.proposal_digest();
    let error = world
        .handoff_cognition_wake(
            &wake_id,
            CognitionWakeDispositionV1::Replan {
                proposal: stale,
                budget_spent: 1,
            },
        )
        .expect_err("stale context must not be replanned");
    assert!(format!("{error:?}").contains("cognition_context_mismatch"));
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
    assert_eq!(world.cognition()["continuations"][0]["status"], "rejected");
    assert_eq!(
        world.cognition()["continuations"][0]["remaining_budget"]["value"],
        2
    );
}

#[test]
fn current_context_is_required_before_budget_and_partial_budget_requeues_wake() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted_proposal = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(admitted_proposal.clone())
        .expect("admit continuation");
    world.step().expect("advance to wake tick");

    let mut stale_context = CognitionContextDigestsV1::from_proposal(&admitted_proposal);
    stale_context.goal_digest = digest(99);
    let error = world
        .consume_cognition_continuation_budget_with_context(
            &admitted.continuation_id,
            1,
            stale_context,
        )
        .expect_err("stale current context must fail closed before debit");
    assert!(format!("{error:?}").contains("cognition_context_mismatch"));
    assert_eq!(
        world.cognition_continuations()[0]["remaining_budget"]["value"],
        2
    );

    let mut partial_world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut partial_world);
    let partial_proposal = proposal(&partial_world);
    let partial_admitted = partial_world
        .admit_cognition_continuation(partial_proposal.clone())
        .expect("admit continuation");
    partial_world.step().expect("select wake");
    let consumed = partial_world
        .consume_cognition_continuation_budget_with_context(
            &partial_admitted.continuation_id,
            1,
            CognitionContextDigestsV1::from_proposal(&partial_proposal),
        )
        .expect("partial budget consumption");
    assert_eq!(consumed.remaining_budget.value, 1);
    assert_eq!(consumed.status, ContinuationStatusV1::Scheduled);
    assert_eq!(
        partial_world.cognition_scheduler_snapshot()["in_flight"],
        json!({})
    );
    assert!(
        partial_world.cognition_scheduler_snapshot()["active"]
            .as_array()
            .expect("active wakes")
            .iter()
            .any(|wake| wake["wake_id"] == partial_admitted.wake_id)
    );
    partial_world.step().expect("reselect partial wake");
    let completed = partial_world
        .consume_cognition_continuation_budget_with_context(
            &partial_admitted.continuation_id,
            1,
            CognitionContextDigestsV1::from_proposal(&partial_proposal),
        )
        .expect("final budget consumption");
    assert_eq!(completed.status, ContinuationStatusV1::Completed);
    assert!(
        partial_world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
}

#[test]
fn scheduler_backpressure_is_a_real_pending_status_and_recovers_to_scheduled() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let first = world
        .admit_cognition_continuation(proposal(&world))
        .expect("first continuation");
    world
        .start_cognition_turn(
            AGENT_ID,
            "session.runtime-hardening-second",
            "turn.runtime-hardening-second",
            "request.runtime-hardening-second",
            &digest(5),
        )
        .expect("register second turn");
    let mut second_proposal = proposal(&world);
    second_proposal.continuation_proposal_id = "proposal.runtime-hardening-second".to_string();
    second_proposal.agent_session_id = "session.runtime-hardening-second".to_string();
    second_proposal.agent_turn_id = "turn.runtime-hardening-second".to_string();
    second_proposal.decision_request_id = "request.runtime-hardening-second".to_string();
    second_proposal.proposal_digest = second_proposal.proposal_digest();
    let second = world
        .admit_cognition_continuation(second_proposal)
        .expect("backpressured continuation is admitted durably");
    assert_eq!(second.status, ContinuationStatusV1::Pending);
    assert_eq!(world.cognition()["continuations"][1]["status"], "pending");
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        1
    );

    world.step().expect("select first wake");
    world
        .handoff_cognition_wake(
            &first.wake_id,
            CognitionWakeDispositionV1::Terminal {
                status: ContinuationStatusV1::Completed,
                reason: "provider_terminal".to_string(),
            },
        )
        .expect("release first wake");
    world
        .recover_cognition_scheduler(world.state().time)
        .expect("recover backpressure after capacity release");
    assert_eq!(world.cognition()["continuations"][1]["status"], "scheduled");
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        0
    );
}

#[test]
fn in_flight_expiry_is_serviced_even_when_no_ready_queue_entry_remains() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let mut expiring = proposal(&world);
    expiring.valid_until_tick = Some(1);
    expiring.proposal_digest = expiring.proposal_digest();
    let admitted = world
        .admit_cognition_continuation(expiring)
        .expect("admit expiring continuation");
    world.step().expect("lease wake");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("in-flight")
            .iter()
            .any(|wake| wake.wake_id == admitted.wake_id)
    );
    world
        .service_cognition_scheduler_tick(2)
        .expect("service expiry for in-flight wake");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
    assert_eq!(world.cognition()["continuations"][0]["status"], "expired");
}

#[test]
fn recovery_rejects_a_rehashed_out_of_order_lifecycle_event() {
    let mut world = World::new();
    let decision = envelope(&world);
    let prepared = world
        .prepare_cognition_envelope(
            decision.clone(),
            Some(response_artifact_for_envelope(&decision)),
        )
        .expect("prepare");
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("commit");
    let dir = temp_dir("out-of-order-lifecycle");
    world.save_to_dir(&dir).expect("save");
    let mut snapshot = read_snapshot(&dir);
    let events = snapshot["cognition"]["cognition_journal"]["events"]
        .as_array_mut()
        .expect("events");
    let submitted = events
        .iter_mut()
        .find(|event| event["event_kind"] == "DecisionEnvelopeSubmitted")
        .expect("submitted event");
    submitted["event_kind"] = json!("WorldReceiptLinked");
    submitted["kind"] = json!("WorldReceiptLinked");
    let mut parent = String::new();
    for event in events.iter_mut() {
        event["parent_event_digest"] = json!(parent);
        let mut payload = event.clone();
        let payload_object = payload.as_object_mut().expect("payload object");
        payload_object.remove("journal_seq");
        payload_object.remove("parent_event_digest");
        payload_object.remove("payload_digest");
        payload_object.remove("event_digest");
        event["payload_digest"] = json!(cognition_digest_v1(
            "oasis7.cognition.event-payload.v1",
            &payload
        ));
        let mut unsigned = event.clone();
        unsigned
            .as_object_mut()
            .expect("event object")
            .remove("event_digest");
        let digest = cognition_digest_v1("oasis7.cognition.event.v1", &unsigned);
        event["event_digest"] = json!(digest.clone());
        parent = digest;
    }
    snapshot["cognition"]["cognition_journal"]["head_digest"] = json!(cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({
            "head_seq": snapshot["cognition"]["cognition_journal"]["head_seq"],
            "events": snapshot["cognition"]["cognition_journal"]["events"]
        })
    ));
    write_snapshot(&dir, &snapshot);
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON recovery");
    assert!(World::load_from_dir(&dir).is_err());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn committed_marker_remains_recoverable_after_reorg_rebind_as_history() {
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
    let committed = world
        .finalize_cognition_commit(&prepared.commit_id)
        .expect("commit");
    world
        .invalidate_cognition_for_reorg(1)
        .expect("record reorg history");
    world
        .bind_cognition_runtime(WORLD_ID, "main", 0, None, "pending", 1)
        .expect("rebind canonical runtime");
    let dir = temp_dir("committed-history-after-rebind");
    world.save_to_dir(&dir).expect("save rebound world");
    fs::remove_dir_all(dir.join(".distfs-state")).expect("force JSON recovery");
    let restored = World::load_from_dir(&dir).expect("historical marker survives rebind");
    assert_eq!(restored.cognition()["recovery"]["disposition"], "committed");
    assert_eq!(
        restored.cognition()["recovery"]["receipt"]["receipt_id"],
        committed.receipt_id
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn direct_feedback_allocator_requires_durable_receipt_lineage() {
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
        .expect("commit");
    let lineage = world
        .read_runtime_receipt_lineage(&committed.receipt_id)
        .expect("durable receipt lineage");
    let action_id = committed
        .action_id
        .strip_prefix("action:")
        .expect("action prefix")
        .parse::<u64>()
        .expect("action id");
    let request = RuntimeFeedbackRequestV1 {
        feedback_id: Some(lineage.feedback_id.clone()),
        agent_subject: lineage.agent_id.clone(),
        agent_session_id: lineage.agent_session_id.clone(),
        agent_turn_id: lineage.agent_turn_id.clone(),
        decision_request_id: lineage.decision_request_id.clone(),
        candidate_action_id: Some(action_id),
        runtime_receipt_id: Some(lineage.receipt_id.clone()),
        status: "committed".to_string(),
        request_digest: lineage.request_digest.clone(),
        reject_reason: None,
    };
    let feedback = world
        .allocate_runtime_feedback_with_projection(
            request.clone(),
            RuntimeFeedbackProjectionV1 {
                envelope_digest: Some(lineage.envelope_digest.clone()),
                ..RuntimeFeedbackProjectionV1::default()
            },
        )
        .expect("direct allocator accepts exact durable lineage");
    assert_eq!(feedback.feedback_id, lineage.feedback_id);
    let mut mismatch = request;
    mismatch.candidate_action_id = Some(action_id.saturating_add(1));
    assert!(
        format!(
            "{:?}",
            world
                .allocate_runtime_feedback(mismatch)
                .expect_err("lineage mismatch")
        )
        .contains("feedback_receipt_lineage_mismatch")
    );
}

#[test]
fn missing_selection_evaluation_is_rejected_before_resume() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted_proposal = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(admitted_proposal.clone())
        .expect("admit continuation");
    world.step().expect("select wake");
    let mut snapshot = world.snapshot();
    snapshot.cognition["continuation_evaluations"] = json!({});
    world = World::from_snapshot(snapshot, world.journal().clone()).expect("mutated snapshot");
    let error = world
        .handoff_cognition_wake_with_context(
            &admitted.wake_id,
            CognitionWakeDispositionV1::Terminal {
                status: ContinuationStatusV1::Completed,
                reason: "provider_terminal".to_string(),
            },
            CognitionContextDigestsV1::from_proposal(&admitted_proposal),
        )
        .expect_err("a selected wake without durable evaluation must fail closed");
    assert!(format!("{error:?}").contains("cognition_wake_evaluation_missing"));
    assert_eq!(world.cognition_continuations()[0]["status"], "rejected");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
}

#[test]
fn invalid_resume_persists_terminal_cleanup_even_when_nested_handoff_fails() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let original = proposal(&world);
    let admitted = world
        .admit_cognition_continuation(original.clone())
        .expect("admit continuation");
    world.step().expect("select wake");
    let wake_id = admitted.wake_id.clone();
    let mut stale = original.clone();
    stale.goal_digest = digest(98);
    stale.agent_session_id = "session.resume-invalid".to_string();
    stale.agent_turn_id = "turn.resume-invalid".to_string();
    stale.decision_request_id = "request.resume-invalid".to_string();
    stale.proposal_digest = stale.proposal_digest();
    let error = world
        .resume_cognition_wake_with_context(
            &wake_id,
            stale,
            1,
            CognitionContinuationResumeRequestV1 {
                agent_session_id: "session.resume-invalid".to_string(),
                agent_turn_id: "turn.resume-invalid".to_string(),
                decision_request_id: "request.resume-invalid".to_string(),
                request_digest: digest(80),
                context_digest: digest(81),
            },
            CognitionContextDigestsV1::from_proposal(&original),
        )
        .expect_err("invalid nested handoff must fail");
    assert!(format!("{error:?}").contains("cognition_context_mismatch"));
    assert_eq!(world.cognition_continuations()[0]["status"], "rejected");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
}

#[test]
fn wake_handoff_revalidates_current_state_head_before_budget_debit() {
    let mut world = World::new()
        .try_with_cognition_scheduler(policy(), 1)
        .expect("scheduler");
    bind_test_turn(&mut world);
    let admitted = world
        .admit_cognition_continuation(proposal(&world))
        .expect("admit continuation");
    world.step().expect("advance to wake tick");
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-evidence-mutation".to_string(),
        pos: super::pos(0, 0),
    });
    world.step().expect("mutate world evidence head");
    let error = world
        .consume_cognition_continuation_budget(&admitted.continuation_id, 1)
        .expect_err("stale selected evidence must not spend budget");
    assert!(format!("{error:?}").contains("cognition_wake_evidence_stale"));
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("leases")
            .is_empty()
    );
    assert_eq!(
        world.cognition()["continuations"][0]["remaining_budget"]["value"],
        2
    );
}
