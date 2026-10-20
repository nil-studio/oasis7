use crate::runtime_host::archive::{
    PlayerGameplayExecutionState, PlayerGameplayRecentFeedback, PlayerGameplaySnapshot,
    PlayerGameplayStageStatus,
};

use super::PlayerGameplayCausalityKind;

pub(super) fn player_gameplay_status_reason(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<String> {
    gameplay
        .causality_detail
        .clone()
        .or_else(|| gameplay.blocker_detail.clone())
        .or_else(|| recent_feedback.and_then(|feedback| feedback.reason.clone()))
        .or_else(|| gameplay.blocker_kind.clone())
}

pub(super) fn player_gameplay_last_world_change(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<String> {
    recent_feedback
        .filter(|feedback| feedback.delta_logical_time > 0 || feedback.delta_event_seq > 0)
        .map(|feedback| feedback.effect.clone())
        .filter(|effect| !effect.trim().is_empty())
        .or_else(|| {
            matches!(
                gameplay.causality_kind,
                Some(PlayerGameplayCausalityKind::GoalProgressed)
            )
            .then(|| gameplay.progress_detail.clone())
        })
}

pub(super) fn player_gameplay_primary_blocker(
    gameplay: &PlayerGameplaySnapshot,
    status_reason: Option<&String>,
) -> Option<String> {
    if gameplay.execution_state != PlayerGameplayExecutionState::Blocked
        && gameplay.stage_status != PlayerGameplayStageStatus::Blocked
    {
        return None;
    }
    gameplay
        .blocker_detail
        .clone()
        .or_else(|| gameplay.blocker_kind.clone())
        .or_else(|| status_reason.cloned())
}

pub(super) fn player_gameplay_response_window_class(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<String> {
    if let Some(feedback) = recent_feedback {
        return Some(match feedback.stage.as_str() {
            "accepted" | "submitted" | "queued" | "ack" => {
                "waiting_for_committed_progress".to_string()
            }
            "completed_no_progress" => "stalled_needs_escalation".to_string(),
            "blocked" => "blocked_needs_repair".to_string(),
            "rejected" => "request_rejected".to_string(),
            "completed_advanced" => "resolved".to_string(),
            _ => match gameplay.execution_state {
                PlayerGameplayExecutionState::Accepted => "waiting_for_committed_progress",
                PlayerGameplayExecutionState::Blocked => "blocked_needs_repair",
                PlayerGameplayExecutionState::Rejected => "request_rejected",
                PlayerGameplayExecutionState::Completed => "resolved",
                PlayerGameplayExecutionState::Executing => "watch_next_tick",
            }
            .to_string(),
        });
    }

    match gameplay.execution_state {
        PlayerGameplayExecutionState::Accepted => {
            Some("waiting_for_committed_progress".to_string())
        }
        PlayerGameplayExecutionState::Blocked => Some("blocked_needs_repair".to_string()),
        PlayerGameplayExecutionState::Rejected => Some("request_rejected".to_string()),
        PlayerGameplayExecutionState::Completed => Some("resolved".to_string()),
        PlayerGameplayExecutionState::Executing => None,
    }
}

pub(super) fn player_gameplay_stalled_reason(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<String> {
    recent_feedback
        .filter(|feedback| matches!(feedback.stage.as_str(), "completed_no_progress" | "blocked"))
        .and_then(|feedback| feedback.reason.clone())
        .or_else(|| {
            (gameplay.execution_state == PlayerGameplayExecutionState::Blocked)
                .then(|| {
                    gameplay
                        .blocker_detail
                        .clone()
                        .or_else(|| gameplay.blocker_kind.clone())
                })
                .flatten()
        })
}

pub(super) fn player_gameplay_escalation_hint(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<String> {
    recent_feedback
        .filter(|feedback| {
            matches!(
                feedback.stage.as_str(),
                "accepted" | "submitted" | "queued" | "ack" | "completed_no_progress" | "blocked"
            )
        })
        .and_then(|feedback| feedback.hint.clone())
        .or_else(|| {
            (gameplay.execution_state == PlayerGameplayExecutionState::Blocked
                || gameplay.execution_state == PlayerGameplayExecutionState::Accepted)
                .then(|| gameplay.next_step_hint.clone())
        })
}

pub(super) fn derive_player_gameplay_execution_state(
    stage_status: PlayerGameplayStageStatus,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> PlayerGameplayExecutionState {
    if let Some(feedback) = recent_feedback {
        match feedback.stage.as_str() {
            "accepted" | "submitted" | "queued" | "ack" => {
                return PlayerGameplayExecutionState::Accepted;
            }
            "rejected" => return PlayerGameplayExecutionState::Rejected,
            "executing" => return PlayerGameplayExecutionState::Executing,
            "blocked" | "completed_no_progress" => return PlayerGameplayExecutionState::Blocked,
            "completed_advanced" => return PlayerGameplayExecutionState::Completed,
            _ => {}
        }
    }
    match stage_status {
        PlayerGameplayStageStatus::Blocked => PlayerGameplayExecutionState::Blocked,
        PlayerGameplayStageStatus::BranchReady => PlayerGameplayExecutionState::Completed,
        PlayerGameplayStageStatus::Active => PlayerGameplayExecutionState::Executing,
    }
}
