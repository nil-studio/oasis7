use super::*;

pub(super) fn should_emit_runtime_advance_snapshot(
    session: &mut RuntimeLiveSession,
    action: &str,
    emit_while_paused: bool,
) -> bool {
    let is_background_play = action == "play" && session.playing && !emit_while_paused;
    !is_background_play
        || session.should_emit_background_snapshot(BACKGROUND_PLAY_SNAPSHOT_INTERVAL)
}

pub(super) fn control_mode_label(mode: &ViewerControl) -> &'static str {
    match mode {
        ViewerControl::Pause => "pause",
        ViewerControl::Play => "play",
        ViewerControl::Step { .. } => "step",
        ViewerControl::Seek { .. } => "seek",
    }
}

pub(super) fn control_mode_for_action(action: &str, step_count: usize) -> ViewerControl {
    match action {
        "pause" => ViewerControl::Pause,
        "play" => ViewerControl::Play,
        "seek" => ViewerControl::Seek { tick: 0 },
        "step" => ViewerControl::Step {
            count: step_count.max(1),
        },
        _ => ViewerControl::Step {
            count: step_count.max(1),
        },
    }
}

pub(super) fn runtime_control_error_details(
    error: &ViewerRuntimeLiveServerError,
) -> (String, String, String) {
    match error {
        ViewerRuntimeLiveServerError::Io(err) => (
            "runtime_io_failed".to_string(),
            format!("runtime I/O failed: {err}"),
            RUNTIME_CONTROL_REQUIRED_HINT.to_string(),
        ),
        ViewerRuntimeLiveServerError::Serde(message) => (
            "runtime_sync_decode_failed".to_string(),
            format!("runtime state sync failed: {message}"),
            RUNTIME_CONTROL_REQUIRED_HINT.to_string(),
        ),
        ViewerRuntimeLiveServerError::Init(message) => (
            "runtime_init_failed".to_string(),
            format!("runtime initialization failed: {message}"),
            RUNTIME_CONTROL_REQUIRED_HINT.to_string(),
        ),
        ViewerRuntimeLiveServerError::Runtime(err) => runtime_world_error_details(err),
    }
}

fn runtime_world_error_details(error: &RuntimeWorldError) -> (String, String, String) {
    match error {
        RuntimeWorldError::AgentNotFound { agent_id } => (
            "agent_not_found".to_string(),
            format!("runtime world step failed: agent not found: {agent_id}"),
            "restore the missing agent or repair the stale player-agent binding before retrying the control"
                .to_string(),
        ),
        _ => (
            "runtime_step_failed".to_string(),
            format!("runtime world step failed: {error:?}"),
            RUNTIME_CONTROL_REQUIRED_HINT.to_string(),
        ),
    }
}
