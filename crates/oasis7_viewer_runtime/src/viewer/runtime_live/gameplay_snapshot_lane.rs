use crate::runtime_host::archive::{PlayerGameplayGoalKind, PlayerGameplaySnapshot};

/// Labels describe the presentation lane; absent domain evidence stays unknown.
/// Display text and goal labels never establish sponsorship or executable recovery.
pub(super) fn apply_small_player_lane_truth(gameplay: &mut PlayerGameplaySnapshot) {
    gameplay.small_player_lane_id.get_or_insert_with(|| {
        match gameplay.goal_kind {
            PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unclassified",
            PlayerGameplayGoalKind::ChooseMidLoopPath => "regional_specialist",
            _ => "local_operator",
        }
        .into()
    });
    gameplay.leverage_class.get_or_insert_with(|| {
        match gameplay.goal_kind {
            PlayerGameplayGoalKind::RecoverCapability => "repair_elasticity",
            PlayerGameplayGoalKind::ChooseFirstExpansionTradeoff => {
                "regional_specialization_option"
            }
            PlayerGameplayGoalKind::ChooseMidLoopPath => "local_bargaining_position",
            PlayerGameplayGoalKind::CreateFirstWorldFeedback => "unclassified",
            _ => "throughput_only",
        }
        .into()
    });
    gameplay
        .major_power_dependency_status
        .get_or_insert_with(|| "unverified".into());
    gameplay
        .requires_major_power_sponsorship
        .get_or_insert_with(|| "unverified".into());
    gameplay
        .recovery_path_kind
        .get_or_insert_with(|| "unverified".into());
    gameplay.recovery_path_detail.get_or_insert_with(||
        "Recovery requires current canonical action and authority evidence; unsupported options remain unknown.".into());
    gameplay.grind_only_flag = gameplay.same_loop_repeat_count >= 3
        && matches!(
            gameplay.leverage_class.as_deref(),
            Some("throughput_only" | "unclassified")
        );
}
