use crate::runtime_host::archive::{
    PlayerGameplayGoalKind, PlayerGameplayRecoveryOption, PlayerGameplaySnapshot,
};

fn recovery_option(
    kind: &str,
    estimated_time_class: &str,
    estimated_resource_class: &str,
    risk_class: &str,
    retained_benefit: &str,
    recommendation_reason: &str,
) -> PlayerGameplayRecoveryOption {
    PlayerGameplayRecoveryOption {
        kind: kind.to_string(),
        estimated_time_class: estimated_time_class.to_string(),
        estimated_resource_class: estimated_resource_class.to_string(),
        risk_class: risk_class.to_string(),
        retained_benefit: retained_benefit.to_string(),
        recommendation_reason: recommendation_reason.to_string(),
    }
}

fn player_gameplay_forces_major_power_dependency(gameplay: &PlayerGameplaySnapshot) -> bool {
    let mut haystack = String::new();
    for part in [
        gameplay.blocker_kind.as_deref(),
        gameplay.blocker_detail.as_deref(),
        gameplay.status_reason.as_deref(),
        gameplay.primary_blocker.as_deref(),
        gameplay.stalled_reason.as_deref(),
        gameplay.escalation_hint.as_deref(),
        gameplay.causality_detail.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !haystack.is_empty() {
            haystack.push(' ');
        }
        haystack.push_str(part);
    }
    let haystack = haystack.to_ascii_lowercase();
    let dependency_signal = haystack.contains("major power")
        || haystack.contains("major-power")
        || haystack.contains("sponsor")
        || haystack.contains("sponsorship")
        || haystack.contains("alliance")
        || haystack.contains("patron");
    let forced_signal = haystack.contains("only")
        || haystack.contains("forced")
        || haystack.contains("required")
        || haystack.contains("requires")
        || haystack.contains("must")
        || haystack.contains("no independent path")
        || haystack.contains("cannot proceed without");
    dependency_signal && forced_signal
}

pub(super) fn apply_small_player_lane_truth(gameplay: &mut PlayerGameplaySnapshot) {
    let forced_major_power_dependency = player_gameplay_forces_major_power_dependency(gameplay);

    if gameplay.small_player_lane_id.is_none() {
        gameplay.small_player_lane_id = Some(
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unclassified",
                PlayerGameplayGoalKind::ChooseMidLoopPath => "regional_specialist",
                _ => "local_operator",
            }
            .to_string(),
        );
    }

    if gameplay.leverage_class.is_none() {
        gameplay.leverage_class = Some(
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::RecoverCapability => "repair_elasticity",
                PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff => {
                    "regional_specialization_option"
                }
                PlayerGameplayGoalKind::ChooseMidLoopPath => "local_bargaining_position",
                PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unclassified",
                _ => "throughput_only",
            }
            .to_string(),
        );
    }

    if gameplay.major_power_dependency_status.is_none() {
        gameplay.major_power_dependency_status = Some(
            if forced_major_power_dependency {
                "forced"
            } else {
                match gameplay.goal_kind {
                    PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unverified",
                    PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                    | PlayerGameplayGoalKind::ChooseMidLoopPath => "independent_path_available",
                    PlayerGameplayGoalKind::RecoverCapability => "sponsor_helpful_not_required",
                    _ => "unverified",
                }
            }
            .to_string(),
        );
    }

    if gameplay.recovery_path_kind.is_none() {
        gameplay.recovery_path_kind = Some(
            if forced_major_power_dependency {
                "none"
            } else {
                match gameplay.goal_kind {
                    PlayerGameplayGoalKind::RecoverCapability
                    | PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                    | PlayerGameplayGoalKind::ChooseMidLoopPath => "repair_rebuild_or_pivot",
                    _ => "unverified",
                }
            }
            .to_string(),
        );
    }

    if gameplay.recovery_path_detail.is_none() {
        gameplay.recovery_path_detail = if forced_major_power_dependency {
            Some(
                "The current blocker exposes no repair, rebuild, or pivot path before securing major-power sponsorship."
                    .to_string(),
            )
        } else {
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::RecoverCapability => Some(
                    "Repair the blocked line, rebuild the local site, or pivot to a lower-pressure specialization without requiring major-power sponsorship."
                        .to_string(),
                ),
                PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff => Some(
                    "The next branch can repair upstream inputs, rebuild for more resilient throughput, or pivot toward logistics reach."
                        .to_string(),
                ),
                PlayerGameplayGoalKind::ChooseMidLoopPath => Some(
                    "Regional branches remain available without making global governance or major-power membership the entry requirement."
                        .to_string(),
                ),
                _ => None,
            }
        };
    }

    if gameplay.requires_major_power_sponsorship.is_none() {
        gameplay.requires_major_power_sponsorship = Some(
            if forced_major_power_dependency {
                "yes"
            } else {
                match gameplay.goal_kind {
                    PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unverified",
                    PlayerGameplayGoalKind::RecoverCapability
                    | PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                    | PlayerGameplayGoalKind::ChooseMidLoopPath => "no",
                    _ => "unverified",
                }
            }
            .to_string(),
        );
    }

    if gameplay.repair_available.is_none() {
        gameplay.repair_available = if forced_major_power_dependency {
            Some(false)
        } else {
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::RecoverCapability
                | PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                | PlayerGameplayGoalKind::ChooseMidLoopPath => Some(true),
                _ => None,
            }
        };
    }
    if gameplay.rebuild_available.is_none() {
        gameplay.rebuild_available = if forced_major_power_dependency {
            Some(false)
        } else {
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::RecoverCapability
                | PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                | PlayerGameplayGoalKind::ChooseMidLoopPath => Some(true),
                _ => None,
            }
        };
    }
    if gameplay.pivot_available.is_none() {
        gameplay.pivot_available = if forced_major_power_dependency {
            Some(false)
        } else {
            match gameplay.goal_kind {
                PlayerGameplayGoalKind::RecoverCapability
                | PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff
                | PlayerGameplayGoalKind::ChooseMidLoopPath => Some(true),
                _ => None,
            }
        };
    }

    if gameplay.recovery_options.is_empty()
        && gameplay.recovery_path_kind.as_deref() == Some("repair_rebuild_or_pivot")
        && gameplay.repair_available == Some(true)
        && gameplay.rebuild_available == Some(true)
        && gameplay.pivot_available == Some(true)
    {
        gameplay.recovery_options = vec![
            recovery_option(
                "repair",
                "short",
                "focused_local_input",
                "low",
                "Retains the current local line and its accumulated operating context.",
                "Use repair when the current blocker is localized and restoring the existing line is the least disruptive response.",
            ),
            recovery_option(
                "rebuild",
                "medium",
                "broader_local_reinvestment",
                "moderate",
                "Retains local production ownership while replacing the fragile operating arrangement.",
                "Use rebuild when the current arrangement cannot reliably absorb the blocker and a resilient local line is needed.",
            ),
            recovery_option(
                "pivot",
                "medium",
                "redirected_local_commitment",
                "tradeoff",
                "Retains independent progress by redirecting the next specialization instead of requiring sponsorship.",
                "Use pivot when changing the local specialization creates a clearer path around the current pressure.",
            ),
        ];
    }

    let leverage_class = gameplay.leverage_class.as_deref().unwrap_or("unclassified");
    gameplay.grind_only_flag = gameplay.same_loop_repeat_count >= 3
        && matches!(leverage_class, "throughput_only" | "unclassified");
}
