//! Bounded exact-owner shutdown for shipping process fixtures.
use super::*;

const OWNER_STOP_PENDING: &str = "formal Node stop is still pending; runtime remains owned";

fn stop_owner_snapshot_is_terminal(
    expected: &formal_live_runtime::FormalRuntimeInstanceV1,
    snapshot: &formal_live_runtime::FormalRuntimeOwnerSnapshotV1,
) -> Result<bool, String> {
    let state: String = snapshot.state.chars().take(64).collect();
    let reason = snapshot.last_error.as_deref().map(|reason| {
        let text: String = reason.chars().take(512).collect();
        (text, reason.chars().count() > 512)
    });
    if snapshot.instance != *expected {
        return Err(format!(
            "successor stop changed exact owner instance; state={state:?} last_error={reason:?}"
        ));
    }
    if snapshot.state == "stopped" {
        return Ok(true);
    }
    // Owner IPC currently maps runtime.stop's bounded pending Err to unknown.
    // Retry only that exact documented outcome, never arbitrary unknown errors.
    if matches!(snapshot.state.as_str(), "unknown" | "stopping")
        && snapshot.last_error.as_deref() == Some(OWNER_STOP_PENDING)
    {
        eprintln!("formal_healthy_owner_stop event=pending state={state:?} last_error={reason:?}");
        return Ok(false);
    }
    Err(format!(
        "successor stop failed; state={state:?} last_error={reason:?}"
    ))
}

pub(super) fn stop_exact_successor_owner(
    config: &Path,
    expected: &formal_live_runtime::FormalRuntimeInstanceV1,
    deadline: ScenarioDeadline,
) -> Result<(), String> {
    loop {
        deadline.report_stage("stopping exact successor owner")?;
        let remaining = deadline
            .expires_at
            .saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("exact successor stop remained pending at scenario deadline".into());
        }
        let snapshot = formal_live_runtime::stop_operation_owner(
            config,
            expected,
            remaining.min(Duration::from_secs(10)),
        )
        .map_err(|error| format!("exact successor stop IPC failed: {error}"))?;
        if stop_owner_snapshot_is_terminal(expected, &snapshot)? {
            return Ok(());
        }
        thread::sleep(
            Duration::from_millis(25).min(
                deadline
                    .expires_at
                    .saturating_duration_since(Instant::now()),
            ),
        );
    }
}

#[path = "formal_live_healthy_operation_stop_tests.rs"]
mod tests;
