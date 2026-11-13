//! Payload-free, bounded observations. These never decide recovery success.
use serde_json::{Value, json};
use std::collections::VecDeque;

const CAPACITY: usize = 16;

#[derive(Default)]
pub(super) struct Observations {
    samples: VecDeque<Value>,
    observed: u64,
}
impl Observations {
    pub(super) fn record(
        &mut self,
        elapsed_ms: u64,
        received_ms: u64,
        summary: &Value,
        marker: bool,
        memory: Option<bool>,
    ) {
        let io = &summary["io_lifecycle"];
        let mut counts = serde_json::Map::new();
        for field in [
            "queued_count",
            "returned_count",
            "applied_count",
            "execute_error_count",
            "apply_error_count",
            "same_phase_launch_count",
            "current_age_us",
        ] {
            counts.insert(field.into(), json!(io[field].as_u64()));
        }
        let sample = json!({
            "captured_elapsed_ms": elapsed_ms, "received_elapsed_ms": received_ms,
            "phase": label(&summary["hosted_service_phase"], &["act", "act_view", "act_settled_view",
                "act_settle", "feedback", "feedback_ack", "feedback_ack_view", "wait", "resume",
                "handoff_wake", "handoff_view", "admission", "finalize"]),
            "operation": label(&io["operation"], &["submit", "lookup", "view", "feedback", "metadata", "absent"]),
            "io_state": label(&io["state"], &["queued", "executing", "returned", "consumed", "applied", "absent"]),
            "counts": counts,
            "pending_intents": summary["pending_intent_count"].as_u64(),
            "pending_actions": summary["pending_action_count"].as_u64(),
            "conditions": {
                "resumed_marker_read": marker,
                "terminal_committed": summary["terminal_states"]["agent-a"]["status"] == "committed",
                "publication_not_pending": summary["publication_pending"] == false,
                "capture_not_failed": summary["publication_capture_failed"] == false,
                "no_pending_intents": summary["pending_intent_count"] == 0,
                "no_pending_actions": summary["pending_action_count"] == 0,
                "exact_resumed_memory": memory,
            },
        });
        self.observed += 1;
        if self.samples.len() == CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    pub(super) fn report(
        &self,
        elapsed_ms: u64,
        readout_finished: bool,
        drained: bool,
        worker_ok: bool,
    ) -> Value {
        json!({"schema": 1, "deadline_ms": 12000, "observation_elapsed_ms": elapsed_ms,
            "observed_samples": self.observed, "retained_samples": self.samples,
            "dropped_samples": self.observed.saturating_sub(self.samples.len() as u64),
            "readout_finished": readout_finished, "drained": drained, "worker_ok": worker_ok})
    }
}
fn label<'a>(value: &Value, allowed: &[&'a str]) -> &'a str {
    match value.as_str() {
        None => "absent",
        Some(text) => allowed
            .iter()
            .copied()
            .find(|label| *label == text)
            .unwrap_or("unknown"),
    }
}

#[test]
fn wait_capture_diagnostics_rejects_private_and_mistyped_values() {
    let secret = "PRIVATE-memory-model-checkpoint-request-path";
    let summary = json!({"hosted_service_phase": secret, "memory_store": secret,
        "terminal_states": {"agent-a": {"status": secret, "reject_reason": secret}},
        "pending_intent_count": secret, "pending_action_count": -1,
        "publication_pending": secret, "publication_capture_failed": secret,
        "io_lifecycle": {"operation": secret, "state": secret, "queued_count": secret,
            "returned_count": {"private": secret}, "current_age_us": -1}});
    let mut observations = Observations::default();
    observations.record(12, 13, &summary, false, Some(false));
    let report = observations.report(12000, false, false, false);
    assert!(!report.to_string().contains(secret));
    let sample = &report["retained_samples"][0];
    assert_eq!(sample["phase"], "unknown");
    assert_eq!(sample["operation"], "unknown");
    assert!(sample["counts"]["queued_count"].is_null());
    assert!(sample["pending_actions"].is_null());
    assert!(
        sample["conditions"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == false)
    );
}

#[test]
fn wait_capture_diagnostics_bounds_samples_and_reports_each_condition() {
    let summary = json!({"hosted_service_phase":"act", "pending_intent_count":0,
        "pending_action_count":0,"publication_pending":false,"publication_capture_failed":false,
        "terminal_states":{"agent-a":{"status":"committed"}},
        "io_lifecycle":{"operation":"submit","state":"executing","queued_count":51}});
    let mut observations = Observations::default();
    for elapsed in 0..100 {
        observations.record(elapsed, elapsed + 1, &summary, true, Some(true));
    }
    let report = observations.report(12001, true, true, false);
    assert_eq!(
        report["retained_samples"].as_array().unwrap().len(),
        CAPACITY
    );
    assert_eq!(report["dropped_samples"], 84);
    assert_eq!(report["retained_samples"][0]["captured_elapsed_ms"], 84);
    let last = &report["retained_samples"][15];
    assert_eq!(last["captured_elapsed_ms"], 99);
    assert_eq!(last["counts"]["queued_count"], 51);
    assert!(
        last["conditions"]
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == true)
    );
    assert_eq!(report["worker_ok"], false);
    assert_eq!(report["deadline_ms"], 12000);
    observations.record(100, 101, &summary, true, None);
    assert!(observations.report(12000, false, false, false)["retained_samples"][15]
        ["conditions"]["exact_resumed_memory"].is_null());
    // No sampled state is represented as success when nothing was observed.
    assert_eq!(
        Observations::default().report(12000, false, false, false)["observed_samples"],
        0
    );
}

#[test]
fn wait_capture_diagnostics_separates_conditions() {
    let baseline = json!({"pending_intent_count":0, "pending_action_count":0,
        "publication_pending":false,"publication_capture_failed":false,
        "terminal_states":{"agent-a":{"status":"committed"}}});
    for (pointer, value, condition) in [
        ("/pending_intent_count", json!(1), "no_pending_intents"),
        ("/pending_action_count", json!(1), "no_pending_actions"),
        (
            "/publication_pending",
            json!(true),
            "publication_not_pending",
        ),
        (
            "/publication_capture_failed",
            json!(true),
            "capture_not_failed",
        ),
        (
            "/terminal_states/agent-a/status",
            json!("pending"),
            "terminal_committed",
        ),
    ] {
        let mut summary = baseline.clone();
        *summary.pointer_mut(pointer).unwrap() = value;
        let mut observations = Observations::default();
        observations.record(10, 11, &summary, true, Some(true));
        let report = observations.report(12000, true, true, true);
        let conditions = report["retained_samples"][0]["conditions"]
            .as_object()
            .unwrap();
        for (name, value) in conditions {
            assert_eq!(value, &(json!(name != condition)));
        }
        assert_eq!(report["retained_samples"][0]["received_elapsed_ms"], 11);
    }
}
