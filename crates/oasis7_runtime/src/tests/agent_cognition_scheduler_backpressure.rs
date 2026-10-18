//! P1 RED fixtures for bounded scheduler enqueue and capacity recovery.
//!
//! These tests freeze the runtime boundary from issue #3602: a full queue is
//! durable pending state, never a blocking World worker or a synchronous
//! provider call.  Capacity recovery must reuse the original wake identity
//! and produce exactly one wake.

use crate::{
    CognitionScheduler, SchedulerEnqueueOutcome, SchedulerExecutionMetrics, SchedulerPolicyV1,
    SchedulerWakeV1,
};
use serde_json::{Value, json};

const WORLD_ID: &str = "world-scheduler-fixture";
const BRANCH_ID: &str = "main";

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
    .expect("decode v1 scheduler policy")
}

fn wake(agent_id: &str, continuation_id: &str, wake_id: &str, wake_seq: u64) -> SchedulerWakeV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": wake_id,
        "continuation_id": continuation_id,
        "world_id": WORLD_ID,
        "branch_id": BRANCH_ID,
        "finality_epoch": 7,
        "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "agent_id": agent_id,
        "agent_session_id": format!("session.{agent_id}"),
        "agent_turn_id": format!("turn.{agent_id}"),
        "decision_request_id": format!("request.{agent_id}"),
        "next_wake_tick": 10,
        "eligible_since_tick": 10,
        "starvation_deadline_tick": 14,
        "initial_priority": 0,
        "wake_seq": wake_seq,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("decode scheduler wake fixture")
}

fn outcome_value(outcome: SchedulerEnqueueOutcome) -> Value {
    serde_json::to_value(outcome).expect("encode scheduler enqueue outcome")
}

#[test]
fn queue_full_is_nonblocking_durable_pending_without_world_or_provider_effect() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    let accepted = outcome_value(
        scheduler
            .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
            .expect("first wake accepted"),
    );
    let full = outcome_value(
        scheduler
            .try_enqueue(wake("agent-b", "cont-b", "wake-b", 1))
            .expect("queue-full is a disposition, not a blocking/error path"),
    );

    assert!(accepted["disposition"].is_string());
    assert_eq!(full["disposition"], "pending");
    assert_eq!(full["reason"], "scheduler_backpressure");
    assert_eq!(full["provider_invocation_count"], 0);
    assert_eq!(full["world_event_count"], 0);
    assert_eq!(full["effect_count"], 0);
    assert_eq!(full["debit_count"], 0);
    assert_eq!(full["receipt_count"], 0);
    assert_eq!(full["world_receipt_linked_count"], 0);
    assert_eq!(scheduler.pending_backpressure_count(), 1);
}

#[test]
fn releasing_capacity_recovers_the_original_wake_once_in_canonical_order() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
        .expect("first wake accepted");
    scheduler
        .try_enqueue(wake("agent-b", "cont-b", "wake-b", 1))
        .expect("second wake becomes durable pending");

    let first = scheduler.select_ready(10);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].wake_id, "wake-a");

    scheduler.release_capacity();
    let recovered = scheduler.recover_capacity(10);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].wake_id, "wake-b");
    assert_eq!(recovered[0].continuation_id, "cont-b");

    let duplicate = scheduler.recover_capacity(10);
    assert!(duplicate.is_empty(), "capacity wake was delivered twice");
    let metrics: SchedulerExecutionMetrics = scheduler.metrics();
    let metrics = serde_json::to_value(metrics).expect("encode scheduler metrics");
    assert_eq!(metrics["recovery_wake_count"], 1);
    assert_eq!(metrics["provider_invocation_count"], 0);
    assert_eq!(metrics["effect_count"], 0);
    assert_eq!(metrics["debit_count"], 0);
}

#[test]
fn full_queue_preserves_retry_sequence_and_age_until_capacity_returns() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
        .expect("first wake accepted");
    scheduler
        .try_enqueue(wake("agent-b", "cont-b", "wake-b", 9))
        .expect("second wake becomes pending");

    let before = scheduler.pending_backpressure("wake-b");
    scheduler.advance_logical_tick(12);
    scheduler.advance_logical_tick(13);
    let after = scheduler.pending_backpressure("wake-b");
    assert_eq!(before["wake_id"], after["wake_id"]);
    assert_eq!(before["continuation_id"], after["continuation_id"]);
    assert_eq!(before["retry_seq"], after["retry_seq"]);
    assert_eq!(before["eligible_since_tick"], after["eligible_since_tick"]);
    assert_eq!(before["effective_priority"], after["effective_priority"]);
    assert_eq!(after["reason"], "scheduler_backpressure");
}

#[test]
fn untimed_backpressure_requires_committed_evidence_and_preserves_cursor() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
        .expect("first wake accepted");
    let mut untimed = wake("agent-b", "cont-b", "wake-b", 1);
    untimed.next_wake_tick = u64::MAX;
    scheduler
        .try_enqueue(untimed)
        .expect("untimed wake becomes durable pending");
    let cursor_before = scheduler.cursor_seq();
    let selected = scheduler.select_ready(10);
    assert_eq!(selected.len(), 1);
    scheduler.release_capacity();

    let not_ready = scheduler.recover_capacity_if_preserving_cursor(10, |_| false);
    assert!(
        not_ready.is_empty(),
        "false evidence must not lease an untimed wake"
    );
    assert_eq!(scheduler.pending_backpressure_count(), 1);
    assert_eq!(scheduler.cursor_seq(), cursor_before + 1);

    let recovered =
        scheduler.recover_capacity_if_preserving_cursor(10, |wake| wake.wake_id == "wake-b");
    assert_eq!(
        recovered
            .iter()
            .map(|wake| wake.wake_id.as_str())
            .collect::<Vec<_>>(),
        ["wake-b"]
    );
    assert_eq!(scheduler.pending_backpressure_count(), 0);
    assert_eq!(scheduler.cursor_seq(), cursor_before + 1);
}

#[test]
fn evidence_ineligible_wake_stays_backpressured_at_starvation_boundary() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
        .expect("first wake accepted");
    let mut evidence_wake = wake("agent-b", "cont-b", "wake-b", 1);
    evidence_wake.next_wake_tick = u64::MAX;
    scheduler
        .try_enqueue(evidence_wake)
        .expect("evidence wake becomes durable pending");

    assert_eq!(scheduler.select_ready(10).len(), 1);
    scheduler.release_capacity();

    let not_ready = scheduler.recover_capacity_if_preserving_cursor(14, |_| false);
    assert!(
        not_ready.is_empty(),
        "starvation readiness must not bypass authoritative evidence eligibility"
    );
    assert!(scheduler.is_backpressured("wake-b"));
    assert_eq!(scheduler.pending_backpressure_count(), 1);
    assert_eq!(
        scheduler.snapshot_json()["active"].as_array().map(Vec::len),
        Some(0)
    );

    let recovered = scheduler.recover_capacity_if_preserving_cursor(14, |_| true);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].wake_id, "wake-b");
    assert!(
        scheduler
            .recover_capacity_if_preserving_cursor(14, |_| true)
            .is_empty()
    );
}

#[test]
fn mixed_tick_and_evidence_wake_stays_backpressured_when_evidence_is_false() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-a", 1))
        .expect("first wake accepted");
    let mixed_wake = wake("agent-b", "cont-b", "wake-b", 10);
    scheduler
        .try_enqueue(mixed_wake)
        .expect("mixed wake becomes durable pending");

    assert_eq!(scheduler.select_ready(10).len(), 1);
    scheduler.release_capacity();

    let not_ready = scheduler.recover_capacity_if_preserving_cursor(10, |_| false);
    assert!(
        not_ready.is_empty(),
        "a due tick must not bypass an unsatisfied evidence condition"
    );
    assert!(scheduler.is_backpressured("wake-b"));
    assert_eq!(scheduler.pending_backpressure_count(), 1);
    assert_eq!(
        scheduler.snapshot_json()["active"].as_array().map(Vec::len),
        Some(0)
    );

    let recovered = scheduler.recover_capacity_if_preserving_cursor(10, |_| true);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].wake_id, "wake-b");
    assert!(
        scheduler
            .recover_capacity_if_preserving_cursor(10, |_| true)
            .is_empty()
    );
}
