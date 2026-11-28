//! Test-executable-only scheduling at the real empty continuation and original
//! freeze boundaries. No production build reads these variables or pauses here.
use crate::{
    formal_live_original_pending_producer::OriginalPendingPinV1,
    formal_live_service::{
        PublishedReadState,
        test_control::{BoundaryControl, Point},
    },
    node_lifecycle::local_io,
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub(crate) const PHASE: &str = "OASIS7_ORIGINAL_PENDING_TEST_PHASE";
pub(crate) const MARKER: &str = "OASIS7_ORIGINAL_PENDING_TEST_MARKER";
static CONTROL: OnceLock<(String, PathBuf, Arc<BoundaryControl>)> = OnceLock::new();

pub(crate) fn install(published: &PublishedReadState) -> Result<(), String> {
    let Ok(phase) = std::env::var(PHASE) else {
        return Ok(());
    };
    if !matches!(phase.as_str(), "prepared" | "frozen") {
        return Err("unknown original pending process-test phase".into());
    }
    let marker = PathBuf::from(std::env::var_os(MARKER).ok_or("missing process-test marker")?);
    if !marker.is_absolute() {
        return Err("process-test marker must be absolute".into());
    }
    let control = Arc::new(published.control_boundaries()?);
    let deadline = Instant::now() + Duration::from_secs(60);
    control.arm(Point::BeforeResume, None, None, deadline)?;
    CONTROL
        .set((phase, marker.clone(), Arc::clone(&control)))
        .map_err(|_| "original pending process-test control already installed")?;
    thread::Builder::new()
        .name("original-pending-boundary".into())
        .spawn(move || {
            let observed = match control.wait(deadline) {
                Ok(observed) => serde_json::json!({
                    "point": "before_resume", "action_count": observed.action_count,
                    "position": observed.position,
                }),
                Err(error) => serde_json::json!({"error": error}),
            };
            local_io::write(&marker.with_extension("node.json"), &observed)
                .expect("persist test-only Node boundary observation");
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn stop_at(
    phase: &str,
    pin: &OriginalPendingPinV1,
    node_running: bool,
) -> Result<(), String> {
    let Some((selected, marker, control)) = CONTROL.get() else {
        return Ok(());
    };
    if phase == "prepared" && selected == "frozen" {
        // Only the real begin_freeze has persisted the original here. Let the
        // pre-existing empty Tick complete so the real Node stop can join it.
        return control.release();
    }
    if selected != phase {
        return Ok(());
    }
    local_io::write(
        marker,
        &serde_json::json!({
            "phase": phase, "pin": pin, "node_running": node_running,
        }),
    )?;
    // The parent kills its retained Child handle before any freeze reply. A
    // missing parent is bounded and cannot hang the ordinary test executable.
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    Err("original pending process-test SIGKILL deadline expired".into())
}
