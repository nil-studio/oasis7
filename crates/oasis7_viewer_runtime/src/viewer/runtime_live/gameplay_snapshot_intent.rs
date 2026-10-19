use crate::runtime_host::archive::PlayerGameplayRecentFeedback;

pub(super) fn player_gameplay_intent_scope(action: &str) -> Option<&'static str> {
    if action.starts_with("gameplay_action:") {
        Some("gameplay_action")
    } else if action.starts_with("prompt_control.") {
        Some("prompt_control")
    } else if action == "agent_chat" {
        Some("agent_chat")
    } else if matches!(action, "play" | "pause" | "step" | "seek") {
        Some("world_control")
    } else if action == "chain_sync" {
        Some("world_sync")
    } else {
        None
    }
}

pub(super) fn player_gameplay_intent_summary(
    feedback: &PlayerGameplayRecentFeedback,
) -> Option<String> {
    feedback
        .intent_summary
        .clone()
        .or_else(|| (!feedback.action.is_empty()).then(|| feedback.action.replace('_', " ")))
}
