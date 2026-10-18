//! LIVE-4 RED tests for World-owned cognition orchestration.
//!
//! The scheduler, wake, continuation and retention modules each have focused
//! in-memory fixtures.  Those fixtures do not prove that a real `World` owns
//! one durable lifecycle, restores its cursor and pins, or derives wake
//! decisions from the committed head.  This file is intentionally the next
//! integration seam: until the World APIs below exist, the focused filter must
//! fail for the missing production integration methods rather than silently
//! passing against a fixture-only implementation.

use super::super::*;
use crate::{
    AgentContinuation, CognitionContinuationProposalV1, CognitionScheduler, RetentionRecordV1,
    RetentionReplayRequestV1, SchedulerPolicyV1, SchedulerWakeV1, WakeConditionV1,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const WORLD_ID: &str = "world-live-scheduler";
const BRANCH_ID: &str = "main";
const AGENT_A: &str = "agent-live-a";
const AGENT_B: &str = "agent-live-b";

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-live-scheduler-{label}-{nonce}"))
}

fn policy() -> SchedulerPolicyV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-policy.v1",
        "max_total_wakes_per_tick": 8,
        "max_wakes_per_agent_per_tick": 1,
        "aging_after_ticks": 2,
        "max_starvation_ticks": 4,
        "initial_priority": 0,
        "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
        "service_order": "stable_round_robin"
    }))
    .expect("decode live scheduler policy")
}

fn wake(
    agent_id: &str,
    wake_id: &str,
    continuation_id: &str,
    next_wake_tick: u64,
) -> SchedulerWakeV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": wake_id,
        "continuation_id": continuation_id,
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:runtime-manifest-live-7",
        "agent_id": agent_id,
        "agent_session_id": format!("session.{agent_id}"),
        "agent_turn_id": format!("turn.{agent_id}"),
        "decision_request_id": format!("request.{agent_id}"),
        "next_wake_tick": next_wake_tick,
        "eligible_since_tick": 1,
        "starvation_deadline_tick": 5,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("decode live scheduler wake")
}

fn proposal(world: &World) -> CognitionContinuationProposalV1 {
    let mut proposal: CognitionContinuationProposalV1 = serde_json::from_value(json!({
        "schema_version": 1,
        "continuation_proposal_id": "proposal-live-reorg-1",
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": world.current_manifest_hash().expect("manifest hash"),
        "agent_id": AGENT_A,
        "agent_session_id": "session.agent-live-a",
        "agent_turn_id": "turn.agent-live-a",
        "decision_request_id": "request.agent-live-a",
        "origin_turn_id": "turn.agent-live-a",
        "origin_request_digest": "blake3:origin-request-live-1",
        "action_or_plan_kind": "wait",
        "action_or_envelope_digest": null,
        "baseline_observation_digest": "blake3:baseline-live-1",
        "goal_digest": "blake3:goal-live-1",
        "policy_digest": "blake3:policy-live-1",
        "policy_revision": 1,
        "precondition_summary": "ready",
        "wake_conditions": [{
            "schema_version": "wake-condition.v1",
            "kind": "at_or_after_tick",
            "logical_tick": 1
        }],
        "next_wake_tick": 1,
        "remaining_budget": {"unit": "steps", "value": 2},
        "valid_until_tick": 100,
        "precondition_digest": "blake3:precondition-live-1",
        "source": "runtime-test",
        "proposal_digest": ""
    }))
    .expect("decode live proposal");
    proposal.proposal_digest = proposal.proposal_digest();
    proposal
}

fn world_with_scheduler() -> World {
    world_with_scheduler_configured(policy(), 1)
}

fn world_with_scheduler_configured(policy: SchedulerPolicyV1, capacity: usize) -> World {
    let mut world = World::new().with_cognition_scheduler(policy, capacity);
    world
        .bind_cognition_runtime(
            WORLD_ID,
            BRANCH_ID,
            7,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            3,
        )
        .expect("bind live cognition authority");
    world
        .start_cognition_turn(
            AGENT_A,
            "session.agent-live-a",
            "turn.agent-live-a",
            "request.agent-live-a",
            "blake3:origin-request-live-1",
        )
        .expect("register live cognition turn");
    world
}

fn enqueue_fixture_wake(world: &mut World, wake: SchedulerWakeV1) {
    let conditions = json!([{
        "schema_version": "wake-condition.v1",
        "kind": "at_or_after_tick",
        "logical_tick": wake.next_wake_tick
    }]);
    enqueue_fixture_wake_with_conditions(world, wake, conditions);
}

fn enqueue_fixture_wake_with_conditions(
    world: &mut World,
    wake: SchedulerWakeV1,
    conditions: Value,
) {
    enqueue_fixture_wake_with_conditions_until(world, wake, conditions, 100);
}

fn enqueue_fixture_wake_with_conditions_until(
    world: &mut World,
    wake: SchedulerWakeV1,
    conditions: Value,
    valid_until_tick: u64,
) {
    let mut wake = wake;
    wake.runtime_manifest_hash = world.cognition()["runtime_binding"]["runtime_manifest_hash"]
        .as_str()
        .expect("bound runtime manifest")
        .to_string();
    let mut continuation: AgentContinuation = serde_json::from_value(json!({
        "schema_version": "agent-continuation.v1",
        "continuation_id": wake.continuation_id,
        "wake_id": wake.wake_id,
        "world_id": wake.world_id,
        "branch_id": wake.branch_id,
        "finality_epoch": wake.finality_epoch,
        "finality_block_hash": wake.finality_block_hash,
        "finality_status": wake.finality_status,
        "reorg_epoch": wake.reorg_epoch,
        "runtime_manifest_hash": wake.runtime_manifest_hash,
        "agent_id": wake.agent_id,
        "agent_session_id": wake.agent_session_id,
        "agent_turn_id": wake.agent_turn_id,
        "decision_request_id": wake.decision_request_id,
        "origin_turn_id": wake.agent_turn_id,
        "origin_request_digest": "blake3:fixture-origin-request-0000000000000000000000000000000000000000000000000000000000000000",
        "continuation_proposal_id": "proposal.fixture-wake",
        "proposal_digest": "blake3:fixture-proposal-0000000000000000000000000000000000000000000000000000000000000000",
        "action_or_envelope_digest": null,
        "wake_conditions": conditions,
        "next_wake_tick": wake.next_wake_tick,
        "remaining_budget": {"unit": "steps", "value": 2},
        "valid_until_tick": valid_until_tick,
        "precondition_digest": "blake3:fixture-precondition-0000000000000000000000000000000000000000000000000000000000000000",
        "wake_seq": wake.wake_seq,
        "logical_tick": 0,
        "status": "scheduled",
        "terminal_disposition": null
    }))
    .expect("decode fixture continuation");
    continuation.refresh_status_digest();
    world
        .install_cognition_continuation_for_test(continuation)
        .expect("install fixture continuation");
    world
        .enqueue_cognition_wake_for_test(wake)
        .expect("enqueue fixture wake");
}

#[test]
fn world_step_runs_the_production_scheduler_and_releases_exact_wake_identity() {
    let mut world = world_with_scheduler();
    enqueue_fixture_wake(
        &mut world,
        wake(AGENT_A, "wake-production", "continuation-production", 1),
    );
    enqueue_fixture_wake(
        &mut world,
        wake(
            AGENT_B,
            "wake-production-pending",
            "continuation-production-pending",
            1,
        ),
    );

    world
        .step()
        .expect("production World step services scheduler");
    assert_eq!(
        world.cognition_scheduler_snapshot()["in_flight"]["wake-production"]["wake_id"],
        "wake-production"
    );
    assert_eq!(
        world
            .cognition_in_flight_wakes()
            .expect("read production scheduler leases")
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-production"]
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("cognition journal events")
            .iter()
            .any(|event| event["kind"] == "ContinuationWoken")
    );

    let released = world
        .release_cognition_wake("wake-production")
        .expect("release exact production wake");
    assert_eq!(released.wake_id, "wake-production");
    assert!(
        world.cognition_scheduler_snapshot()["in_flight"]
            .get("wake-production")
            .is_none()
    );
    assert_eq!(
        world.cognition_scheduler_snapshot()["active"][0]["wake_id"],
        "wake-production-pending",
        "exact release must immediately promote the due backpressure wake"
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("cognition journal events")
            .iter()
            .any(|event| event["kind"] == "SchedulerWakeReleased")
    );
    let before = world.cognition_scheduler_snapshot();
    assert!(world.release_cognition_wake("wake-missing").is_err());
    assert_eq!(world.cognition_scheduler_snapshot(), before);
}

#[test]
fn committed_state_evidence_promotes_untimed_backpressure_after_restore_and_release() {
    let mut world = world_with_scheduler();
    enqueue_fixture_wake(
        &mut world,
        wake(
            AGENT_A,
            "wake-evidence-owner",
            "continuation-evidence-owner",
            1,
        ),
    );
    let untimed = wake(
        AGENT_B,
        "wake-evidence-pending",
        "continuation-evidence-pending",
        u64::MAX,
    );
    enqueue_fixture_wake_with_conditions(
        &mut world,
        untimed,
        json!([{
            "schema_version": "wake-condition.v1",
            "kind": "state_predicate",
            "subject": {"kind": "world", "id": WORLD_ID},
            "path_or_rule": "world.logical_tick",
            "operator": "gte",
            "expected_value_bytes": serde_cbor::to_vec(&1_u64).expect("encode tick")
        }]),
    );
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        1
    );

    world.step().expect("lease the capacity owner");
    let dir = temp_dir("untimed-backpressure");
    world.save_to_dir(&dir).expect("persist backpressure state");
    let mut restored = World::load_from_dir(&dir).expect("restore backpressure state");
    assert_eq!(
        restored.cognition_scheduler_snapshot()["backpressure_count"],
        1
    );

    restored
        .release_cognition_wake("wake-evidence-owner")
        .expect("release exact owner and promote committed evidence wake");
    let scheduler = restored.cognition_scheduler_snapshot();
    assert_eq!(scheduler["backpressure_count"], 0);
    assert_eq!(
        scheduler["in_flight"]["wake-evidence-pending"]["wake_id"],
        "wake-evidence-pending"
    );
    assert_eq!(
        scheduler["in_flight"]["wake-evidence-pending"]["next_wake_tick"],
        u64::MAX
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn untimed_backpressure_expires_before_starvation_without_committed_evidence() {
    let mut world = world_with_scheduler();
    enqueue_fixture_wake(
        &mut world,
        wake(AGENT_A, "wake-expiry-owner", "continuation-expiry-owner", 1),
    );
    let untimed = wake(
        AGENT_B,
        "wake-expiry-pending",
        "continuation-expiry-pending",
        u64::MAX,
    );
    enqueue_fixture_wake_with_conditions_until(
        &mut world,
        untimed,
        json!([{
            "schema_version": "wake-condition.v1",
            "kind": "state_predicate",
            "subject": {"kind": "world", "id": WORLD_ID},
            "path_or_rule": "world.logical_tick",
            "operator": "gte",
            "expected_value_bytes": serde_cbor::to_vec(&99_u64).expect("encode tick")
        }]),
        2,
    );

    world.step().expect("lease the capacity owner");
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        1
    );
    world
        .service_cognition_scheduler_tick(3)
        .expect("expire an untimed wake before starvation");
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        0
    );
    assert!(
        world
            .cognition_continuations()
            .as_array()
            .is_some_and(|continuations| {
                continuations.iter().any(|continuation| {
                    continuation["continuation_id"] == "continuation-expiry-pending"
                        && continuation["status"] == "expired"
                })
            })
    );
}

#[test]
fn world_scheduler_is_nonblocking_fair_and_restores_cursor_and_backpressure() {
    let mut world = world_with_scheduler();
    enqueue_fixture_wake(&mut world, wake(AGENT_A, "wake-a", "continuation-a", 1));
    assert_eq!(
        world.cognition_scheduler_snapshot()["active"][0]["wake_id"],
        "wake-a"
    );

    enqueue_fixture_wake(&mut world, wake(AGENT_B, "wake-b", "continuation-b", 1));
    assert_eq!(
        world.cognition_scheduler_snapshot()["backpressure_count"],
        1
    );
    assert_eq!(
        world.cognition_scheduler_snapshot()["cursor"]["cursor_seq"],
        1,
        "queue-full service attempt must advance the durable cursor"
    );

    let selected = world
        .select_ready_cognition_wakes(1)
        .expect("World should select ready wakes without provider I/O");
    assert_eq!(
        selected
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-a"]
    );

    let before = world.cognition_scheduler_snapshot();
    let dir = temp_dir("scheduler-round-trip");
    world.save_to_dir(&dir).expect("save live scheduler state");
    let mut restored = World::load_from_dir(&dir).expect("restore live scheduler state");
    assert_eq!(restored.cognition_scheduler_snapshot(), before);

    let recovered = restored
        .recover_cognition_scheduler(2)
        .expect("recovery should preserve the selected wake identity and age");
    assert_eq!(
        recovered
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-a"]
    );
    let after = restored.cognition_scheduler_snapshot();
    assert_eq!(after["policy"], before["policy"]);
    assert_eq!(after["cursor"], before["cursor"]);
    assert_eq!(after["backpressure_count"], 1);
    assert_eq!(after["active"][0]["wake_id"], "wake-a");
    assert_eq!(after["active"][0]["eligible_since_tick"], 1);
    assert_eq!(after["active"][0]["retry_seq"], 0);
    assert_eq!(
        restored.cognition_execution_metrics()["provider_invocation_count"],
        0
    );
    assert_eq!(restored.cognition_execution_metrics()["effect_count"], 0);
    assert_eq!(restored.cognition_execution_metrics()["debit_count"], 0);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn selecting_another_wake_preserves_evaluation_for_the_existing_lease() {
    let mut configured = policy();
    configured.max_total_wakes_per_tick = 1;
    let mut world = world_with_scheduler_configured(configured, 2);
    enqueue_fixture_wake(
        &mut world,
        wake(AGENT_A, "wake-evaluation-a", "continuation-evaluation-a", 1),
    );
    enqueue_fixture_wake(
        &mut world,
        wake(AGENT_B, "wake-evaluation-b", "continuation-evaluation-b", 1),
    );

    assert_eq!(
        world
            .select_ready_cognition_wakes(1)
            .expect("select first ready wake")
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-evaluation-a"]
    );
    let first_evaluation =
        world.cognition()["continuation_evaluations"]["continuation-evaluation-a"].clone();

    assert_eq!(
        world
            .select_ready_cognition_wakes(1)
            .expect("select second ready wake")
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-evaluation-b"]
    );
    assert_eq!(
        world.cognition()["continuation_evaluations"]["continuation-evaluation-a"],
        first_evaluation,
        "evaluation for the existing in-flight wake must survive a later lease"
    );
}

#[test]
fn scheduler_restore_rejects_active_and_inflight_capacity_overflow() {
    let world = world_with_scheduler();
    let mut state = world.cognition_scheduler_snapshot();
    state["active"] = json!([
        wake(AGENT_A, "wake-overflow-a", "continuation-overflow-a", 1),
        wake(AGENT_B, "wake-overflow-b", "continuation-overflow-b", 1)
    ]);
    assert!(
        CognitionScheduler::from_snapshot_json(state).is_err(),
        "restore must reject active plus in-flight entries over capacity"
    );
}

#[test]
fn committed_projection_wake_rejects_unbacked_event_and_receipt_evidence() {
    let world = world_with_scheduler();
    let conditions: Vec<WakeConditionV1> = serde_json::from_value(json!([
        {
            "schema_version": "wake-condition.v1",
            "kind": "world_event_committed",
            "event_digest": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        {
            "schema_version": "wake-condition.v1",
            "kind": "receipt_linked",
            "receipt_id": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }
    ]))
    .expect("decode caller evidence conditions");
    let evaluation = world
        .evaluate_cognition_wake_from_committed_projection(
            &conditions,
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .expect("unbacked evidence should be a deterministic pending result");
    assert_eq!(evaluation.status, "pending");
    assert_eq!(evaluation.reason, "condition_not_met");
}

#[test]
fn world_wake_evaluation_rejects_caller_evidence_without_projection() {
    let world = world_with_scheduler();
    let conditions: Vec<WakeConditionV1> = serde_json::from_value(json!([
        {
            "schema_version": "wake-condition.v1",
            "kind": "world_event_committed",
            "event_digest": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        },
        {
            "schema_version": "wake-condition.v1",
            "kind": "receipt_linked",
            "receipt_id": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
        },
        {
            "schema_version": "wake-condition.v1",
            "kind": "state_predicate",
            "subject": {"kind": "world", "id": WORLD_ID},
            "path_or_rule": "world.logical_tick",
            "operator": "gte",
            "expected_value_bytes": [0]
        }
    ]))
    .expect("decode live wake conditions");

    let evaluation = world
        .evaluate_cognition_wake(&conditions)
        .expect("World should evaluate wake conditions from its committed head");
    assert_eq!(evaluation.status, "pending");
    assert_eq!(evaluation.reason, "condition_not_met");

    let committed = world
        .evaluate_cognition_wake_from_committed_projection(
            &conditions,
            "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        )
        .expect("committed event/receipt projection should produce a deterministic wake");
    assert_eq!(committed.status, "pending");
    assert_eq!(committed.reason, "condition_not_met");
}

#[test]
fn world_continuation_reorg_invalidates_durable_schedule_without_reexecution() {
    let mut world = world_with_scheduler();
    world
        .admit_cognition_continuation(proposal(&world))
        .expect("admit continuation proposal through World ownership");

    let report = world
        .invalidate_cognition_for_reorg(4)
        .expect("reorg should invalidate the pending continuation");
    assert_eq!(report["terminal_disposition"], "reorg_invalidated");
    assert_eq!(report["provider_invocation_count"], 0);
    assert_eq!(report["effect_count"], 0);
    assert_eq!(report["receipt_count"], 0);

    let state = world.cognition_continuations();
    assert_eq!(state[0]["status"], "invalidated");
    assert_eq!(state[0]["terminal_disposition"], "reorg_invalidated");
}

#[test]
fn world_retention_gc_keeps_terminal_pins_and_replay_is_read_only_after_restore() {
    let mut world = world_with_scheduler();
    world
        .record_cognition_terminal(terminal_record())
        .expect("record terminal cognition outcome through World ownership");
    world
        .pin_cognition_reference("key:live-1", "pending_wake")
        .expect("persist cognition pin");

    let before = world.cognition_execution_metrics();
    let gc = world
        .gc_cognition(1_000, 1_000)
        .expect("GC should honor the active continuation pin");
    assert_eq!(gc.deleted_count, 0);
    assert!(gc.pinned_reference_count >= 1);

    let dir = temp_dir("retention-replay");
    world.save_to_dir(&dir).expect("save pinned terminal state");
    let restored = World::load_from_dir(&dir).expect("restore pinned terminal state");
    let replay = restored
        .replay_cognition_terminal(
            RetentionReplayRequestV1::from_json(json!({
                "schema_version": "agent-decision-envelope.v1",
                "world_id": WORLD_ID,
                "agent_session_id": "session.live-1",
                "agent_turn_id": "turn.live-1",
                "decision_request_id": "request.live-1",
                "envelope_idempotency_key": "key:live-1",
                "envelope_digest": "blake3:envelope-live-1",
                "base_tick": 1_001,
                "issued_at_tick": 1_001,
                "gc_floor_tick": 1_000
            }))
            .expect("decode v1 replay request"),
        )
        .expect("terminal replay should read its canonical record");
    assert_eq!(replay["provider_invocation_count"], 0);
    assert_eq!(replay["effect_delta"], 0);
    assert_eq!(replay["world_receipt_linked_delta"], 0);
    assert_eq!(restored.cognition_execution_metrics(), before);

    for (field, value) in [
        ("agent_session_id", "session.live-other"),
        ("agent_turn_id", "turn.live-other"),
        ("decision_request_id", "request.live-other"),
    ] {
        let mut mismatched = json!({
            "schema_version": "agent-decision-envelope.v1",
            "world_id": WORLD_ID,
            "agent_session_id": "session.live-1",
            "agent_turn_id": "turn.live-1",
            "decision_request_id": "request.live-1",
            "envelope_idempotency_key": "key:live-1",
            "envelope_digest": "blake3:envelope-live-1",
            "base_tick": 1_001,
            "issued_at_tick": 1_001,
            "gc_floor_tick": 1_000
        });
        mismatched[field] = json!(value);
        let error = restored
            .replay_cognition_terminal(
                RetentionReplayRequestV1::from_json(mismatched)
                    .expect("decode World replay mismatch fixture"),
            )
            .expect_err("World replay must reject mismatched lineage");
        assert!(format!("{error:?}").contains("idempotency_conflict"));
        assert_eq!(restored.cognition_execution_metrics(), before);
    }

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn world_terminal_replay_rejects_legacy_and_expired_proof_without_mutating_metrics() {
    let mut world = world_with_scheduler();
    world
        .record_cognition_terminal(terminal_record())
        .expect("record terminal cognition outcome through World ownership");
    world
        .pin_cognition_reference("key:live-1", "pending_wake")
        .expect("persist cognition pin");
    world
        .gc_cognition(2_000, 1_500)
        .expect("advance durable replay GC floor while pinned");
    let dir = temp_dir("retention-proof-rejection");
    world.save_to_dir(&dir).expect("save replay proof state");
    let restored = World::load_from_dir(&dir).expect("restore replay proof state");
    let before = restored.cognition_execution_metrics();

    let legacy = RetentionReplayRequestV1::from_json(json!({
        "world_id": WORLD_ID,
        "envelope_idempotency_key": "key:live-1",
        "envelope_digest": "blake3:envelope-live-1"
    }))
    .expect("decode legacy replay request");
    let legacy_error = restored
        .replay_cognition_terminal(legacy)
        .expect_err("legacy replay must require complete v1 proof");
    assert!(format!("{legacy_error:?}").contains("legacy_no_cognition_proof"));

    let expired = RetentionReplayRequestV1::from_json(json!({
        "schema_version": "agent-decision-envelope.v1",
        "world_id": WORLD_ID,
        "agent_session_id": "session.live-1",
        "agent_turn_id": "turn.live-1",
        "decision_request_id": "request.live-1",
        "envelope_idempotency_key": "key:live-1",
        "envelope_digest": "blake3:envelope-live-1",
        "base_tick": 1_000,
        "issued_at_tick": 1_000,
        "gc_floor_tick": 1_500
    }))
    .expect("decode expired replay request");
    let expired_error = restored
        .replay_cognition_terminal(expired)
        .expect_err("replay below the persisted GC floor must expire");
    assert!(format!("{expired_error:?}").contains("expired_idempotency"));
    assert_eq!(restored.cognition_execution_metrics(), before);

    let _ = fs::remove_dir_all(dir);
}

fn terminal_record() -> RetentionRecordV1 {
    serde_json::from_value(json!({
        "schema_version": "cognition-retention-record.v1",
        "world_id": WORLD_ID,
        "envelope_idempotency_key": "key:live-1",
        "envelope_digest": "blake3:envelope-live-1",
        "agent_session_id": "session.live-1",
        "agent_turn_id": "turn.live-1",
        "decision_request_id": "request.live-1",
        "status": "committed",
        "base_tick": 1_001,
        "issued_at_tick": 1_001,
        "terminal_disposition": null,
        "receipt_id": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "receipt_digest": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "response_artifact_id": "artifact:live-1",
        "continuation_id": "continuation-live-1",
        "commit_record_id": "commit:live-1"
    }))
    .expect("decode terminal cognition record")
}

#[test]
fn world_scheduler_state_is_durable_json_and_contains_no_provider_effect_or_debit_replay() {
    let mut world = world_with_scheduler();
    enqueue_fixture_wake(
        &mut world,
        wake(AGENT_A, "wake-json", "continuation-json", 1),
    );
    let state: Value = world.cognition_scheduler_snapshot();
    assert_eq!(state["policy"]["schema_version"], "scheduler-policy.v1");
    assert_eq!(state["cursor"]["schema_version"], "scheduler-cursor.v1");
    assert_eq!(state["metrics"]["provider_invocation_count"], 0);
    assert_eq!(state["metrics"]["effect_count"], 0);
    assert_eq!(state["metrics"]["debit_count"], 0);
}
