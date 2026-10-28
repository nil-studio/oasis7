//! Separate bounded clock-production and final-consumption observation phases.
//! These deadlines never replace the genuine canonical completion oracle.
use super::*;
use std::path::Path;

const PRODUCER_BUDGET: Duration = Duration::from_secs(25);
const CONSUMER_BUDGET: Duration = Duration::from_secs(14);
const LATE_CLOCK_ENV: &str = "PRE2_FINAL_BUDGET_LATE_CLOCK";
const ORIGINAL_WINDOW_MARKER: &str = "final-budget-original-window-elapsed";

fn late_clock() -> bool {
    std::env::var(LATE_CLOCK_ENV).as_deref() == Ok("1")
}

// This control defers only the fixture's second clock. It does not fabricate a
// selected wake, change a signed request, or wait while holding the driver lock.
pub(super) fn clock_ready(root: &Path, file: &str) -> bool {
    if file != "hosted-resumed-model.json" || !late_clock() {
        return true;
    }
    if !root.join(ORIGINAL_WINDOW_MARKER).exists() {
        return false;
    }
    println!("PRE2_FINAL_BUDGET_LATE_CLOCK_RELEASED");
    true
}

pub(super) struct Window {
    producer_deadline: Instant,
    consumer_deadline: Option<Instant>,
    outer_deadline: Instant,
    original_boundary: Instant,
    original_marker_sent: bool,
}
impl Window {
    pub(super) fn new(outer_deadline: Instant) -> Self {
        let now = Instant::now();
        Self {
            producer_deadline: (now + PRODUCER_BUDGET).min(outer_deadline),
            consumer_deadline: None,
            outer_deadline,
            original_boundary: now + CONSUMER_BUDGET,
            original_marker_sent: false,
        }
    }
    pub(super) fn keep_waiting(&mut self, root: &Path, started: Instant) -> bool {
        let now = Instant::now();
        if now >= self.outer_deadline || self.deadline_reached() {
            return false;
        }
        if late_clock() && !self.original_marker_sent && now >= self.original_boundary {
            application_hosted_wait::publish_marker(&root.join(ORIGINAL_WINDOW_MARKER), b"elapsed")
                .unwrap();
            self.original_marker_sent = true;
            timing(
                "child_original_window_elapsed",
                started,
                serde_json::json!({}),
            );
        }
        // This marker is a fixture progress barrier, never completion evidence.
        // The unchanged report still verifies actual model/lease/Consume counts
        // and the authenticated Completed receipt with zero remaining budget.
        if self.consumer_deadline.is_none()
            && root.join("hosted-final-budget-selected.json").exists()
        {
            self.consumer_deadline = Some((now + CONSUMER_BUDGET).min(self.outer_deadline));
            timing(
                "child_final_selected_observed",
                started,
                serde_json::json!({"final_consumer_budget_ms":14000}),
            );
        }
        true
    }
    pub(super) fn deadline_reached(&self) -> bool {
        Instant::now() >= self.consumer_deadline.unwrap_or(self.producer_deadline)
    }
    pub(super) fn phase(&self) -> &'static str {
        if self.consumer_deadline.is_some() {
            "final_consume"
        } else {
            "clock_production"
        }
    }
}

#[test]
fn real_tcp_final_clock_after_original_window_still_consumes_once() {
    let root = temp_dir("qa-final-budget-late-clock");
    fs::create_dir_all(&root).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::application_hosted_final_budget::real_tcp_hosted_final_wait_budget_consumes_once_without_new_model_or_lease",
        "--nocapture",
    ]).env(LATE_CLOCK_ENV, "1");
    let output = application_completion::run_child(&mut command, &root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(root).unwrap();
    assert!(
        stdout.contains("PRE2_FINAL_BUDGET_LATE_CLOCK_RELEASED"),
        "the genuine final clock must cross the original completion boundary"
    );
    assert!(
        output.status.success(),
        "late clock must preserve the original canonical oracle"
    );
    assert!(stdout.contains("PRE2_HOSTED_FINAL_BUDGET_PASSED"));
}
