//! P1 RED fixtures for the deterministic scheduler comparator, budgets and
//! durable cursor recovery.

use super::super::*;
use crate::{
    SchedulerCrashPoint, SchedulerCursorRecoveryFixture, SchedulerPolicyV1, SchedulerWakeV1,
};
use serde_json::{Value, json};

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
    .expect("decode scheduler policy")
}

fn wake(
    agent_id: &str,
    continuation_id: &str,
    wake_id: &str,
    next_wake_tick: u64,
    eligible_since_tick: u64,
    starvation_deadline_tick: u64,
    wake_seq: u64,
) -> SchedulerWakeV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": wake_id,
        "continuation_id": continuation_id,
        "world_id": "world-scheduler-fixture",
        "branch_id": "main",
        "finality_epoch": 7,
        "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "finality_status": "verified",
        "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "agent_id": agent_id,
        "agent_session_id": format!("session.{agent_id}"),
        "agent_turn_id": format!("turn.{agent_id}"),
        "decision_request_id": format!("request.{agent_id}"),
        "next_wake_tick": next_wake_tick,
        "eligible_since_tick": eligible_since_tick,
        "starvation_deadline_tick": starvation_deadline_tick,
        "initial_priority": 0,
        "wake_seq": wake_seq,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("decode scheduler wake")
}

fn ids(wakes: &[SchedulerWakeV1]) -> Vec<String> {
    wakes.iter().map(|wake| wake.wake_id.clone()).collect()
}

#[test]
fn v1_policy_freezes_budgets_aging_and_total_comparator() {
    let policy = policy();
    assert_eq!(policy.max_total_wakes_per_tick, 8);
    assert_eq!(policy.max_wakes_per_agent_per_tick, 1);
    assert_eq!(policy.aging_after_ticks, 2);
    assert_eq!(policy.max_starvation_ticks, 4);
    assert_eq!(
        policy.comparator,
        "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc"
    );
    assert_eq!(policy.service_order, "stable_round_robin");
    let policy_digest = policy.policy_config_digest();
    assert!(!policy_digest.is_empty());
    assert_eq!(policy_digest, policy.policy_config_digest());
}

#[test]
fn deadline_due_is_compared_before_tick_priority_and_all_tie_breakers() {
    let mut scheduler = CognitionScheduler::new(policy(), 16);
    scheduler.enqueue_for_test(wake("agent-z", "cont-z", "wake-due", 100, 10, 14, 9));
    scheduler.enqueue_for_test(wake("agent-a", "cont-a", "wake-ordinary", 1, 10, 100, 1));
    scheduler.enqueue_for_test(wake("agent-b", "cont-b", "wake-tick", 2, 10, 100, 2));

    let selected = scheduler.select_ready(14);
    assert_eq!(
        ids(&selected),
        vec![
            "wake-due".to_string(),
            "wake-ordinary".to_string(),
            "wake-tick".to_string(),
        ]
    );
}

#[test]
fn capacity_recovery_keeps_the_same_ascending_wake_tick_order() {
    let mut scheduler = CognitionScheduler::new(policy(), 1);
    scheduler
        .try_enqueue(wake("agent-a", "cont-a", "wake-active", 12, 10, 20, 1))
        .expect("first wake uses the only slot");
    scheduler
        .try_enqueue(wake("agent-b", "cont-b", "wake-late", 13, 10, 20, 2))
        .expect("second wake is durably backpressured");
    scheduler
        .try_enqueue(wake("agent-c", "cont-c", "wake-early", 11, 10, 20, 3))
        .expect("third wake is durably backpressured");

    let selected = scheduler.select_ready(12);
    assert_eq!(ids(&selected), vec!["wake-active".to_string()]);
    scheduler.release_capacity();

    let recovered = scheduler.recover_capacity(12);
    assert_eq!(ids(&recovered), vec!["wake-early".to_string()]);
    assert_eq!(scheduler.pending_backpressure_count(), 1);
}

#[test]
fn global_and_per_agent_budgets_bound_service_and_aging_does_not_starve() {
    let mut scheduler = CognitionScheduler::new(policy(), 32);
    for index in 0..9 {
        let agent = format!("agent-{index:02}");
        scheduler.enqueue_for_test(wake(
            agent.as_str(),
            format!("cont-{index:02}").as_str(),
            format!("wake-{index:02}").as_str(),
            10,
            10,
            14,
            index as u64,
        ));
    }
    scheduler.enqueue_for_test(wake(
        "agent-00",
        "cont-00-second",
        "wake-00-second",
        10,
        10,
        14,
        99,
    ));

    let selected = scheduler.select_ready(14);
    assert_eq!(selected.len(), 8, "global wake budget must be eight");
    assert_eq!(
        selected
            .iter()
            .map(|wake| wake.agent_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        8,
        "one agent may receive at most one wake per tick"
    );
    assert!(
        selected.iter().any(|wake| wake.wake_id == "wake-00"),
        "an item at the four-tick starvation deadline must be served"
    );
}

#[test]
fn cursor_crash_before_commit_keeps_old_cursor_and_after_commit_resumes_once() {
    let wakes = vec![
        wake("agent-a", "cont-a", "wake-a", 10, 10, 14, 1),
        wake("agent-b", "cont-b", "wake-b", 10, 10, 14, 2),
    ];

    let mut before = SchedulerCursorRecoveryFixture::new(policy(), wakes.clone());
    let before_report = before
        .run_tick_with_crash(10, SchedulerCrashPoint::BeforeCursorCommit)
        .expect("crash-before-cursor fixture");
    let before_report: Value = serde_json::to_value(before_report).expect("encode report");
    assert_eq!(before_report["cursor"]["cursor_seq"], 0);
    assert_eq!(before_report["cursor"]["last_served_agent_id"], Value::Null);
    assert_eq!(before_report["delivered_wake_count"], 0);
    assert_eq!(before_report["provider_invocation_count"], 0);

    let mut after = SchedulerCursorRecoveryFixture::new(policy(), wakes);
    let after_report = after
        .run_tick_with_crash(10, SchedulerCrashPoint::AfterCursorCommitBeforeDelivery)
        .expect("crash-after-cursor fixture");
    let after_report: Value = serde_json::to_value(after_report).expect("encode report");
    assert_eq!(after_report["cursor"]["cursor_seq"], 1);
    assert_eq!(after_report["cursor"]["last_served_agent_id"], "agent-a");
    assert_eq!(after_report["delivered_wake_count"], 0);

    let resumed = after
        .recover_and_deliver(10)
        .expect("resume after cursor commit");
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].wake_id, "wake-a");
    assert!(
        after
            .recover_and_deliver(10)
            .expect("idempotent recovery")
            .is_empty(),
        "cursor recovery must not redeliver the same wake"
    );
}

#[test]
fn round_robin_starts_after_the_last_served_agent() {
    let mut configured = policy();
    configured.max_total_wakes_per_tick = 1;
    let mut scheduler = CognitionScheduler::new(configured, 8);
    scheduler.enqueue_for_test(wake("agent-a", "cont-a", "wake-a", 1, 0, 100, 1));
    scheduler.enqueue_for_test(wake("agent-b", "cont-b", "wake-b", 1, 0, 100, 2));

    assert_eq!(ids(&scheduler.select_ready(1)), vec!["wake-a"]);
    scheduler
        .release_in_flight("wake-a")
        .expect("release first served wake");
    scheduler.enqueue_for_test(wake("agent-a", "cont-a-2", "wake-a-2", 1, 0, 100, 3));

    assert_eq!(
        ids(&scheduler.select_ready(1)),
        vec!["wake-b"],
        "the next round must start with the agent after the cursor"
    );
}

#[test]
fn configured_per_agent_budget_allows_exactly_that_many_ready_wakes() {
    let mut configured = policy();
    configured.max_wakes_per_agent_per_tick = 2;
    let mut scheduler = CognitionScheduler::new(configured, 16);
    for index in 0..3 {
        scheduler.enqueue_for_test(wake(
            "agent-a",
            format!("cont-a-{index}").as_str(),
            format!("wake-a-{index}").as_str(),
            1,
            0,
            100,
            index,
        ));
    }
    let selected = scheduler.select_ready(1);
    assert_eq!(
        selected.len(),
        2,
        "runtime must honor configured per-agent budget"
    );
}

#[test]
fn scheduler_runtime_owns_priority_and_rejects_invalid_restore_state() {
    let mut scheduler = CognitionScheduler::new(policy(), 4);
    let mut caller_owned = wake("agent-a", "cont-a", "wake-a", 1, 0, 100, 1);
    caller_owned.initial_priority = 7;
    scheduler
        .try_enqueue(caller_owned)
        .expect("caller wake should be accepted after runtime normalization");
    let selected = scheduler.select_ready(1);
    assert_eq!(selected[0].initial_priority, 0);

    let mut snapshot = scheduler.snapshot_json();
    snapshot["cursor"]["policy_config_digest"] = json!("blake3:caller-forged");
    assert_eq!(
        CognitionScheduler::from_snapshot_json(snapshot)
            .expect_err("forged cursor policy binding must fail closed")
            .code(),
        "scheduler_policy_digest_mismatch"
    );
}
