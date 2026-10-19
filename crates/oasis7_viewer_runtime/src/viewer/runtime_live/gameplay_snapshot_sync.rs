use crate::runtime_host::archive::PlayerGameplayRecentFeedback;

pub(super) fn first_session_runtime_sync_blocker(
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<(String, String, String)> {
    let feedback = recent_feedback?;
    if feedback.action != "chain_sync"
        || !matches!(feedback.stage.as_str(), "blocked" | "completed_no_progress")
    {
        return None;
    }
    let detail = feedback.reason.clone().unwrap_or_else(|| {
        "committed runtime sync did not expose a usable world snapshot".to_string()
    });
    let kind = if detail.contains("execution world is not ready") {
        "execution_world_not_ready".to_string()
    } else {
        "runtime_sync_unavailable".to_string()
    };
    let hint = feedback.hint.clone().unwrap_or_else(|| {
        "repair the runtime sync path, then refresh gameplay to confirm the committed world is available"
            .to_string()
    });
    Some((kind, detail, hint))
}
