use crate::runtime_host::archive::{
    PlayerGameplayFineGrainActionTranslation, PlayerGameplayRecentFeedback, PlayerGameplaySnapshot,
};
use crate::viewer::ACTION_BUILD_SMELTER_MK1;

pub(super) fn player_gameplay_fine_grain_action_translation(
    gameplay: &PlayerGameplaySnapshot,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<PlayerGameplayFineGrainActionTranslation> {
    let feedback = recent_feedback?;
    if !is_rejected_unknown_gameplay_action(feedback) {
        return None;
    }
    let action_id = feedback.action.strip_prefix("gameplay_action:")?;

    let translation = match action_id {
        "mine_resource" => PlayerGameplayFineGrainActionTranslation {
            requested_granularity: "mine a resource directly".to_string(),
            why_fine_action_deferred:
                "direct resource mining is not an available world action".to_string(),
            canonical_replacement_action: None,
            closest_playable_goal: "establish a supported resource capability".to_string(),
            player_next_step_hint:
                "Choose a published capability action; no current action safely replaces direct mining."
                    .to_string(),
            replacement_value_class: "no_safe_replacement".to_string(),
        },
        "place_block_at_smelter_site" => {
            let smelter_action = gameplay.available_actions.iter().find(|action| {
                action.action_id == ACTION_BUILD_SMELTER_MK1 && action.disabled_reason.is_none()
            });
            PlayerGameplayFineGrainActionTranslation {
                requested_granularity: "place a block at the smelter site".to_string(),
                why_fine_action_deferred:
                    "block editing is not an available world action".to_string(),
                canonical_replacement_action: smelter_action.map(|action| action.action_id.clone()),
                closest_playable_goal: "establish the first smelter capability".to_string(),
                player_next_step_hint: smelter_action
                    .map(|action| format!("Use {} to create the supported facility.", action.label))
                    .unwrap_or_else(|| {
                        "Choose a published recovery or goal action; no enabled smelter action safely replaces block editing."
                            .to_string()
                    }),
                replacement_value_class: if smelter_action.is_some() {
                    "replacement_available".to_string()
                } else {
                    "no_safe_replacement".to_string()
                },
            }
        }
        "jump" => PlayerGameplayFineGrainActionTranslation {
            requested_granularity: "jump".to_string(),
            why_fine_action_deferred:
                "embodied movement is only a future candidate, not a current runtime capability"
                    .to_string(),
            canonical_replacement_action: None,
            closest_playable_goal: "inspect and advance the supported world plan".to_string(),
            player_next_step_hint:
                "Use the published goal actions now; revisit embodied movement only when that capability is opened."
                    .to_string(),
            replacement_value_class: "future_embodied_candidate".to_string(),
        },
        "local_physics" => PlayerGameplayFineGrainActionTranslation {
            requested_granularity: "use local physics directly".to_string(),
            why_fine_action_deferred:
                "local physics is only a future candidate, not a current runtime capability"
                    .to_string(),
            canonical_replacement_action: None,
            closest_playable_goal: "inspect and advance the supported world plan".to_string(),
            player_next_step_hint:
                "Use the published goal actions now; revisit local physics only when that capability is opened."
                    .to_string(),
            replacement_value_class: "future_embodied_candidate".to_string(),
        },
        "attack" => PlayerGameplayFineGrainActionTranslation {
            requested_granularity: "attack directly".to_string(),
            why_fine_action_deferred:
                "direct combat is not an available world action".to_string(),
            canonical_replacement_action: None,
            closest_playable_goal: "choose a supported recovery or a new goal".to_string(),
            player_next_step_hint:
                "Reprioritize before committing another action; no published action safely replaces direct combat."
                    .to_string(),
            replacement_value_class: "no_safe_replacement".to_string(),
        },
        _ => return None,
    };

    Some(translation)
}

pub(super) fn is_rejected_unknown_gameplay_action(feedback: &PlayerGameplayRecentFeedback) -> bool {
    feedback.stage == "rejected"
        && feedback.reason.as_deref().is_some_and(|reason| {
            reason == "unknown_gameplay_action" || reason.starts_with("unknown_gameplay_action:")
        })
}

pub(super) fn player_gameplay_player_facing_feedback(
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<PlayerGameplayRecentFeedback> {
    let mut feedback = recent_feedback?.clone();
    if is_rejected_unknown_gameplay_action(&feedback) {
        feedback.action = "unsupported_gameplay_action".to_string();
        feedback.intent_summary = Some("An unsupported gameplay action was rejected.".to_string());
        feedback.effect = "The requested gameplay action is not available.".to_string();
        feedback.reason = Some("unsupported_gameplay_action".to_string());
        feedback.hint = Some("Choose a published gameplay action before retrying.".to_string());
    }
    Some(feedback)
}
