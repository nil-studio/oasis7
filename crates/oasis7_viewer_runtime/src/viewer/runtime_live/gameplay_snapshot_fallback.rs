use crate::runtime_host::archive::{
    PlayerGameplayAction, PlayerGameplayFallbackTradeoffOption, PlayerGameplayRecentFeedback,
    PlayerGameplaySnapshot, PlayerGameplayWaitResolutionQuote,
};

pub(super) fn player_gameplay_wait_resolution_quote(
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> Option<PlayerGameplayWaitResolutionQuote> {
    let feedback = recent_feedback?;
    if !matches!(feedback.stage.as_str(), "queued" | "submitted") {
        return None;
    }

    Some(PlayerGameplayWaitResolutionQuote {
        safe_to_wait: false,
        resolution_trigger: format!(
            "committed feedback for {} is pending runtime application",
            feedback.action
        ),
        recheck_tick_or_event: format!(
            "recheck after the next committed runtime event for {}",
            feedback.action
        ),
        expected_change: feedback.effect.clone(),
        unresolved_risk: feedback.reason.clone().unwrap_or_else(|| {
            "the committed action may still be blocked or leave world state unchanged".to_string()
        }),
        alternative_unlock_condition: feedback.hint.clone().unwrap_or_else(|| {
            "request a fresh gameplay snapshot and choose an enabled action".to_string()
        }),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FallbackTradeoffDecision {
    pub recommended_value_class: Option<&'static str>,
    pub no_safe_fallback_reason: Option<&'static str>,
    pub required_next_decision_action_id: Option<&'static str>,
    pub required_next_decision_class: Option<&'static str>,
}

pub(super) fn fallback_tradeoff_decision(
    repair_available: bool,
    reroute_available: bool,
) -> FallbackTradeoffDecision {
    let recommended_value_class = if repair_available {
        Some("repair_now")
    } else if reroute_available {
        Some("reroute_now")
    } else {
        None
    };
    let no_safe_fallback = recommended_value_class.is_none();
    FallbackTradeoffDecision {
        recommended_value_class,
        no_safe_fallback_reason: no_safe_fallback.then_some(
            "Neither domain repair nor goal reroute is currently available; return to goal selection to choose a new actionable intent.",
        ),
        required_next_decision_action_id: no_safe_fallback.then_some("return_to_goal_selection"),
        required_next_decision_class: no_safe_fallback.then_some("return_to_goal_selection"),
    }
}

pub(super) fn fallback_tradeoff_decision_for_gameplay(
    gameplay: &PlayerGameplaySnapshot,
) -> FallbackTradeoffDecision {
    let action_available = |action_id: &str| {
        gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == action_id && action.disabled_reason.is_none())
    };
    fallback_tradeoff_decision(
        gameplay.repair_available == Some(true),
        action_available("reprioritize"),
    )
}

pub(super) fn player_gameplay_fallback_action(
    gameplay: &PlayerGameplaySnapshot,
    response_window_class: Option<&str>,
) -> Option<(String, String)> {
    let enabled_actions: Vec<&PlayerGameplayAction> = gameplay
        .available_actions
        .iter()
        .filter(|action| action.disabled_reason.is_none())
        .collect();
    if enabled_actions.is_empty() {
        return None;
    }

    let request_snapshot = enabled_actions
        .iter()
        .find(|action| action.protocol_action == "request_snapshot")
        .copied();
    let advance_step = enabled_actions
        .iter()
        .find(|action| action.action_id == "advance_step")
        .copied();
    let resume_play = enabled_actions
        .iter()
        .find(|action| action.action_id == "resume_play")
        .copied();

    let preferred = match response_window_class {
        Some("waiting_for_committed_progress") => advance_step.or(resume_play).or(request_snapshot),
        Some("stalled_needs_escalation") | Some("blocked_needs_repair") => {
            request_snapshot.or(advance_step).or(resume_play)
        }
        Some("request_rejected") => request_snapshot.or(advance_step).or(resume_play),
        _ => None,
    }
    .or_else(|| enabled_actions.first().copied())?;

    Some((preferred.action_id.clone(), preferred.label.clone()))
}

pub(super) fn player_gameplay_fallback_tradeoff_preview(
    gameplay: &PlayerGameplaySnapshot,
    response_window_class: Option<&str>,
) -> Option<Vec<PlayerGameplayFallbackTradeoffOption>> {
    if !matches!(
        response_window_class,
        Some("stalled_needs_escalation" | "blocked_needs_repair")
    ) {
        return None;
    }

    let action_available = |action_id: &str| {
        gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == action_id && action.disabled_reason.is_none())
    };
    let repair_available = gameplay.repair_available == Some(true);
    let reroute_available = action_available("reprioritize");
    let decision = fallback_tradeoff_decision_for_gameplay(gameplay);

    Some(vec![
        PlayerGameplayFallbackTradeoffOption {
            value_class: "safe_wait".to_string(),
            available: false,
            cost: "No bounded wait trigger is currently available.".to_string(),
            progress_kept: "Keeps the current intent unchanged.".to_string(),
            opportunity_cost: "Waiting cannot verify or repair the blocker.".to_string(),
            reason: "The runtime has no canonical tick or event trigger that bounds a safe wait."
                .to_string(),
            recommended: decision.recommended_value_class == Some("safe_wait"),
        },
        PlayerGameplayFallbackTradeoffOption {
            value_class: "repair_now".to_string(),
            available: repair_available,
            cost: "A verified domain repair action is required. Snapshot refresh only observes."
                .to_string(),
            progress_kept: "Keeps the current intent while checking recovery state.".to_string(),
            opportunity_cost: "Uses the next decision on diagnosis instead of a new goal."
                .to_string(),
            reason: if repair_available {
                "A current canonical repair assessment supports this action."
            } else {
                "No current canonical repair action is available."
            }
            .to_string(),
            recommended: decision.recommended_value_class == Some("repair_now"),
        },
        PlayerGameplayFallbackTradeoffOption {
            value_class: "reroute_now".to_string(),
            available: reroute_available,
            cost: "Replace the current Agent short-term goal.".to_string(),
            progress_kept: "Preserves the recorded intent for comparison, not execution progress."
                .to_string(),
            opportunity_cost: "Moves attention from repairing the current blocked intent."
                .to_string(),
            reason: if reroute_available {
                "An enabled reprioritize action can redirect the current goal."
            } else {
                "No enabled reprioritize action is currently available."
            }
            .to_string(),
            recommended: decision.recommended_value_class == Some("reroute_now"),
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_decision_requires_goal_selection_without_repair_or_reroute() {
        let decision = fallback_tradeoff_decision(false, false);

        assert_eq!(decision.recommended_value_class, None);
        assert_eq!(
            decision.no_safe_fallback_reason,
            Some(
                "Neither domain repair nor goal reroute is currently available; return to goal selection to choose a new actionable intent."
            )
        );
        assert_eq!(
            decision.required_next_decision_action_id,
            Some("return_to_goal_selection")
        );
        assert_eq!(
            decision.required_next_decision_class,
            Some("return_to_goal_selection")
        );
    }

    #[test]
    fn fallback_decision_recommends_exactly_one_available_action() {
        for (repair_available, reroute_available, expected) in [
            (true, false, "repair_now"),
            (false, true, "reroute_now"),
            (true, true, "repair_now"),
        ] {
            let decision = fallback_tradeoff_decision(repair_available, reroute_available);

            assert_eq!(decision.recommended_value_class, Some(expected));
            assert_eq!(decision.no_safe_fallback_reason, None);
            assert_eq!(decision.required_next_decision_action_id, None);
            assert_eq!(decision.required_next_decision_class, None);
        }
    }
}
