use crate::runtime_host::archive::PlayerGameplayRecentFeedback;
use crate::viewer::{ControlCompletionAck, ControlCompletionStatus, ViewerControl};

fn blocked_control_hint(error_code: Option<&str>) -> String {
    match error_code {
        Some("llm_mode_required" | "llm_init_failed") => {
            "enable --llm and configure a reachable LLM provider before retrying gameplay controls"
                .to_string()
        }
        _ => {
            "inspect the runtime failure, repair the broken world/module state, then retry the control"
                .to_string()
        }
    }
}

pub(super) fn player_gameplay_feedback_from_control_ack(
    mode: &ViewerControl,
    ack: &ControlCompletionAck,
) -> PlayerGameplayRecentFeedback {
    let (action, intent_summary) = match mode {
        ViewerControl::Pause => (
            "pause".to_string(),
            "pause live world advancement".to_string(),
        ),
        ViewerControl::Play => (
            "play".to_string(),
            "continue advancing the live world".to_string(),
        ),
        ViewerControl::Step { count } => (
            "step".to_string(),
            format!("advance the live world by {count} step(s)"),
        ),
        ViewerControl::Seek { tick } => (
            "seek".to_string(),
            format!("seek the live world to tick {tick}"),
        ),
    };
    let (stage, reason, hint) = match ack.status {
        ControlCompletionStatus::Advanced => ("completed_advanced".to_string(), None, None),
        ControlCompletionStatus::TimeoutNoProgress => (
            "completed_no_progress".to_string(),
            Some("latest live control did not create forward progress".to_string()),
            Some(
                "inspect blockers or restore energy/material flow before stepping again"
                    .to_string(),
            ),
        ),
        ControlCompletionStatus::Blocked => (
            "blocked".to_string(),
            Some(ack.error_message.clone().unwrap_or_else(|| {
                "latest live control was blocked before runtime advance".to_string()
            })),
            Some(blocked_control_hint(ack.error_code.as_deref())),
        ),
    };
    let effect = match ack.status {
        ControlCompletionStatus::Advanced => format!(
            "world advanced: logicalTime +{}, eventSeq +{}",
            ack.delta_logical_time, ack.delta_event_seq
        ),
        ControlCompletionStatus::TimeoutNoProgress => format!(
            "no visible world delta: logicalTime +{}, eventSeq +{}",
            ack.delta_logical_time, ack.delta_event_seq
        ),
        ControlCompletionStatus::Blocked => format!(
            "gameplay blocked before requested advance completed: logicalTime +{}, eventSeq +{}",
            ack.delta_logical_time, ack.delta_event_seq
        ),
    };
    PlayerGameplayRecentFeedback {
        action,
        stage,
        effect,
        intent_summary: Some(intent_summary),
        target_agent_id: None,
        reason,
        hint,
        delta_logical_time: ack.delta_logical_time,
        delta_event_seq: ack.delta_event_seq,
    }
}
