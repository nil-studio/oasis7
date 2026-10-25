//! Force the App to consume a newly generated wake before the clock's next poll.
//! The rendezvous changes thread ordering only; it never commits or edits a request.
use super::*;
use std::path::Path;

fn enabled() -> bool {
    std::env::var("PRE2_CONFORMANCE_FINAL_BUDGET_CLOCK_HANDOFF").as_deref() == Ok("1")
}

fn await_marker(root: &Path, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join(name).exists() {
        assert!(Instant::now() < deadline, "clock handoff boundary absent: {name}");
        thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn hold_resumed_reserve(
    gate: &http_fixture::WorldGate,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
    if !enabled() {
        return;
    }
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return;
    };
    let SchedulerOperationV1::ReserveLease(lease) = &signed.request.operation else {
        return;
    };
    if !lease.agent_session_id.contains("-resume-") {
        return;
    }
    let root = gate.root.lock().unwrap().clone().expect("clock test root");
    application_hosted_wait::publish_marker(&root.join("clock-resumed-reserve-arrived"), b"prepared").unwrap();
    await_marker(&root, "clock-rescan-complete");
}

#[derive(Default)]
pub(super) struct Handoff {
    started: bool,
    awaiting_rescan: bool,
}

impl Handoff {
    // Called with no driver lock held, after each whole clock poll.
    pub(super) fn after_iteration(&mut self, root: &Path, origin_wake_created: bool) {
        if !enabled() {
            return;
        }
        if self.awaiting_rescan {
            application_hosted_wait::publish_marker(&root.join("clock-rescan-complete"), b"rescanned").unwrap();
            self.awaiting_rescan = false;
            println!("PRE2_FINAL_BUDGET_CLOCK_HANDOFF original_wake_consumed_before_rescan=true original_request_unchanged=true");
        } else if origin_wake_created && !self.started {
            self.started = true;
            await_marker(root, "clock-resumed-reserve-arrived");
            self.awaiting_rescan = true;
        }
    }
}

#[test]
fn real_tcp_hosted_final_budget_clock_rescan_preserves_resumed_reserve() {
    let root = temp_dir("qa-final-budget-clock-handoff");
    fs::create_dir_all(&root).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::application_hosted_final_budget_write_failure::real_tcp_hosted_final_budget_cleanup_write_failure_restores_runtime_and_sidecar",
        "--nocapture",
    ]);
    command.env("PRE2_CONFORMANCE_FINAL_BUDGET_CLOCK_HANDOFF", "1");
    let output = application_completion::run_child(&mut command, &root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    fs::remove_dir_all(root).unwrap();
    assert!(output.status.success(), "clock rescan must preserve the original final-budget oracle");
    assert!(stdout.contains("PRE2_FINAL_BUDGET_CLOCK_HANDOFF"));
    assert!(stdout.contains("PRE2_FINAL_BUDGET_RUNTIME_AND_SIDECAR_ROLLBACK_PASSED"));
}
