use super::{
    ACTION_SCHEDULE_SMELTER_ALLOY_PLATE, ACTION_SCHEDULE_SMELTER_COPPER_WIRE,
    ACTION_SCHEDULE_SMELTER_IRON_INGOT, ACTION_SCHEDULE_SMELTER_POLYMER_RESIN,
    GAMEPLAY_ACTION_PROTOCOL, IndustryStage, PlayerGameplayAction, WorldState,
    schedule_recipe_data_advisory, schedule_recipe_disabled_reason, stage_gate_disabled_reason,
};

pub(super) fn extend_smelter_actions(
    state: &WorldState,
    agent_id: &str,
    industry_stage: IndustryStage,
    actions: &mut Vec<PlayerGameplayAction>,
) {
    for (action_id, label) in [
        (ACTION_SCHEDULE_SMELTER_IRON_INGOT, "Queue iron ingot run"),
        (ACTION_SCHEDULE_SMELTER_COPPER_WIRE, "Queue copper wire run"),
        (
            ACTION_SCHEDULE_SMELTER_POLYMER_RESIN,
            "Queue polymer resin run",
        ),
    ] {
        actions.push(PlayerGameplayAction {
            action_id: action_id.to_string(),
            label: schedule_recipe_data_advisory(agent_id, action_id)
                .map(|advisory| format!("{label} ({advisory})"))
                .unwrap_or_else(|| label.to_string()),
            protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
            target_agent_id: Some(agent_id.to_string()),
            disabled_reason: schedule_recipe_disabled_reason(state, agent_id, action_id),
        });
    }
    actions.push(PlayerGameplayAction {
        action_id: ACTION_SCHEDULE_SMELTER_ALLOY_PLATE.to_string(),
        label: schedule_recipe_data_advisory(agent_id, ACTION_SCHEDULE_SMELTER_ALLOY_PLATE)
            .map(|advisory| format!("Queue alloy plate run ({advisory})"))
            .unwrap_or_else(|| "Queue alloy plate run".to_string()),
        protocol_action: GAMEPLAY_ACTION_PROTOCOL.to_string(),
        target_agent_id: Some(agent_id.to_string()),
        disabled_reason: stage_gate_disabled_reason(industry_stage, IndustryStage::ScaleOut)
            .or_else(|| {
                schedule_recipe_disabled_reason(
                    state,
                    agent_id,
                    ACTION_SCHEDULE_SMELTER_ALLOY_PLATE,
                )
            }),
    });
}
