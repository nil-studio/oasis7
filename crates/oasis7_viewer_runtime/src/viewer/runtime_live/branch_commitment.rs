use crate::runtime::IndustryStage;
use crate::runtime_host::archive::{
    PlayerGameplayAction, PlayerGameplayBranchCommitment, PlayerGameplayFirstDeliveryPreview,
    PlayerGameplayStageStatus,
};
use crate::viewer::gameplay_actions::first_delivery_preview_inputs;

fn first_delivery_preview(action_id: &str) -> PlayerGameplayFirstDeliveryPreview {
    let (local_need, expected_output, value_timing, leverage_class_unlocked, return_visit_hook) =
        match action_id {
            "schedule_recipe_smelter_alloy_plate" => (
                "Regional fabricators need a dependable alloy input for advanced components.",
                "Two alloy plates from the first smelter batch.",
                "After one smelter run completes.",
                "regional_material_supplier",
                "Return with the alloy plates to choose an assembler route or sustain local throughput.",
            ),
            "build_factory_assembler_mk1" => (
                "Nearby operators need a second production capability for finished components.",
                "A functioning assembler line ready for its first component recipe.",
                "After the assembler construction completes.",
                "regional_component_builder",
                "Return to choose and run the assembler's first component recipe.",
            ),
            "schedule_recipe_assembler_module_rack" => (
                "Regional operators need durable module hardware for the next industrial capability.",
                "One module rack from the first assembler batch.",
                "After one assembler run completes.",
                "industrial_module_supplier",
                "Return with the module rack to prepare a factory-core production run.",
            ),
            "schedule_recipe_assembler_factory_core" => (
                "Regional operators need a factory core to add another industrial capability.",
                "One factory core from the first assembler batch.",
                "After one assembler run completes.",
                "regional_capacity_builder",
                "Return with the factory core to decide where the added capacity creates leverage.",
            ),
            _ => (
                "Regional fabricators need a dependable alloy input for advanced components.",
                "Two alloy plates from the first smelter batch.",
                "After one smelter run completes.",
                "regional_material_supplier",
                "Return with the alloy plates to choose the next advanced recipe.",
            ),
        };
    PlayerGameplayFirstDeliveryPreview {
        local_need: local_need.to_string(),
        expected_output: expected_output.to_string(),
        required_inputs: first_delivery_preview_inputs(action_id)
            .unwrap_or_default()
            .into_iter()
            .map(|input| format!("{} × {}", input.kind, input.amount))
            .collect(),
        value_timing: value_timing.to_string(),
        leverage_class_unlocked: leverage_class_unlocked.to_string(),
        return_visit_hook: return_visit_hook.to_string(),
    }
}

pub(super) fn branch_recommendations(
    industry_stage: IndustryStage,
    stage_status: PlayerGameplayStageStatus,
    available_actions: &[PlayerGameplayAction],
) -> Vec<PlayerGameplayBranchCommitment> {
    if stage_status != PlayerGameplayStageStatus::BranchReady {
        return Vec::new();
    }

    let enabled = |action_id: &str| {
        available_actions
            .iter()
            .any(|action| action.action_id == action_id && action.disabled_reason.is_none())
    };
    let commitment =
        |action_id: &str,
         route_label: &str,
         immediate_gain: &str,
         future_beat_changed: &str,
         future_beats: [&str; 2],
         risk_or_lockin: &str,
         next_session_hook: &str| PlayerGameplayBranchCommitment {
            action_id: action_id.to_string(),
            route_label: route_label.to_string(),
            immediate_gain: immediate_gain.to_string(),
            future_beat_changed: future_beat_changed.to_string(),
            future_beats: future_beats.into_iter().map(str::to_string).collect(),
            risk_or_lockin: risk_or_lockin.to_string(),
            next_session_hook: next_session_hook.to_string(),
            first_delivery_preview: Some(first_delivery_preview(action_id)),
        };

    let scale_out_candidates = [
        commitment(
            "schedule_recipe_smelter_alloy_plate",
            "Deepen the smelter line",
            "Convert the stable line into higher-tier alloy output.",
            "The next beat shifts from basic throughput to advanced inputs.",
            [
                "Invest the stable smelter time and inputs in alloy production.",
                "Manage the new advanced-input supply pressure as alloy throughput grows.",
            ],
            "Consumes line time that could have supplied basic materials.",
            "Return to turn the alloy output into the next production upgrade.",
        ),
        commitment(
            "build_factory_assembler_mk1",
            "Open a second production line",
            "Add the assembler capability and widen the real recipe surface.",
            "The next beat becomes coordinating two complementary factories.",
            [
                "Commit materials to install the assembler for the established production line.",
                "Coordinate the smelter and assembler queues to sustain complementary recipes.",
            ],
            "Commits construction materials before the new line produces value.",
            "Return to choose and run the assembler's first recipe.",
        ),
    ];
    let governance_candidates = [
        commitment(
            "schedule_recipe_assembler_module_rack",
            "Produce a module rack",
            "Create a durable component for the local specialist production route.",
            "The next beat shifts toward assembling durable industrial modules.",
            [
                "Allocate advanced assembler inputs to complete a durable module rack.",
                "Use the finished rack to prepare the next factory-core production run.",
            ],
            "Uses advanced assembler inputs and occupies the current recipe slot.",
            "Return to connect the module rack to a factory-core production run.",
        ),
        commitment(
            "schedule_recipe_assembler_factory_core",
            "Produce a factory core",
            "Create infrastructure output from the mature assembler line.",
            "The next beat shifts toward deploying another industrial capability.",
            [
                "Consume the module rack and alloy plate to finish the factory core.",
                "Inspect the completed core and choose whether to sustain this advanced production route or pursue another available route.",
            ],
            "Consumes a module rack and alloy plate before yielding the core.",
            "Return to decide where the new factory core creates the most leverage.",
        ),
        commitment(
            "schedule_recipe_smelter_alloy_plate",
            "Sustain alloy throughput",
            "Produce another alloy plate through the mature smelter line.",
            "The next beat keeps advanced assembler inputs supplied while other routes unlock.",
            [
                "Reserve the smelter queue and iron inputs for another alloy plate.",
                "Use the renewed alloy supply to choose among the next advanced recipes.",
            ],
            "Uses the smelter queue and iron inputs before the next advanced recipe can run.",
            "Return to inspect the alloy output and choose the next advanced recipe.",
        ),
    ];

    let candidates = match industry_stage {
        IndustryStage::ScaleOut => scale_out_candidates.as_slice(),
        IndustryStage::Governance => governance_candidates.as_slice(),
        IndustryStage::Bootstrap => return Vec::new(),
    };
    candidates
        .iter()
        .filter(|candidate| enabled(candidate.action_id.as_str()))
        .take(3)
        .cloned()
        .collect()
}

pub(super) fn effective_branch_stage_status(
    stage_status: PlayerGameplayStageStatus,
    recommendations: &[PlayerGameplayBranchCommitment],
    available_actions: &[PlayerGameplayAction],
) -> PlayerGameplayStageStatus {
    let published_branch_action = available_actions.iter().any(|action| {
        matches!(
            action.action_id.as_str(),
            "schedule_recipe_smelter_alloy_plate"
                | "build_factory_assembler_mk1"
                | "schedule_recipe_assembler_module_rack"
                | "schedule_recipe_assembler_factory_core"
        )
    });
    if stage_status == PlayerGameplayStageStatus::BranchReady
        && recommendations.is_empty()
        && !published_branch_action
    {
        PlayerGameplayStageStatus::Blocked
    } else {
        stage_status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regional_specialist_recommendations_keep_enabled_industrial_routes() {
        let action_ids = [
            "schedule_recipe_assembler_module_rack",
            "schedule_recipe_assembler_factory_core",
            "schedule_recipe_smelter_alloy_plate",
        ];
        let mut actions: Vec<_> = action_ids
            .iter()
            .map(|action_id| PlayerGameplayAction {
                action_id: (*action_id).to_string(),
                label: "Industrial route".to_string(),
                protocol_action: "gameplay_action.submit".to_string(),
                target_agent_id: None,
                disabled_reason: None,
            })
            .collect();
        let recommendations = branch_recommendations(
            IndustryStage::Governance,
            PlayerGameplayStageStatus::BranchReady,
            &actions,
        );
        assert_eq!(
            recommendations
                .iter()
                .map(|item| item.action_id.as_str())
                .collect::<Vec<_>>(),
            action_ids,
        );
        for recommendation in &recommendations {
            let mut copy = vec![
                recommendation.route_label.as_str(),
                recommendation.immediate_gain.as_str(),
                recommendation.future_beat_changed.as_str(),
                recommendation.risk_or_lockin.as_str(),
                recommendation.next_session_hook.as_str(),
            ];
            copy.extend(recommendation.future_beats.iter().map(String::as_str));
            let copy = copy.join(" ").to_lowercase();
            for unsupported_stage in ["governance", "conflict"] {
                assert!(
                    !copy.contains(unsupported_stage),
                    "regional-specialist branch must not imply {unsupported_stage}: {copy}",
                );
            }
            let delivery = recommendation
                .first_delivery_preview
                .as_ref()
                .expect("industrial branch must explain its local purpose");
            assert!(delivery.local_need.to_lowercase().contains("regional"));
        }

        actions[0].disabled_reason = Some("missing advanced inputs".to_string());
        let recommendations = branch_recommendations(
            IndustryStage::Governance,
            PlayerGameplayStageStatus::BranchReady,
            &actions,
        );
        assert_eq!(
            recommendations
                .iter()
                .map(|item| item.action_id.as_str())
                .collect::<Vec<_>>(),
            action_ids[1..],
        );
    }

    #[test]
    fn branch_ready_stays_discoverable_when_every_published_candidate_is_disabled() {
        let actions = vec![PlayerGameplayAction {
            action_id: "schedule_recipe_smelter_alloy_plate".to_string(),
            label: "Schedule alloy plate".to_string(),
            protocol_action: "gameplay_action.submit".to_string(),
            target_agent_id: None,
            disabled_reason: Some("missing iron ingot".to_string()),
        }];
        let recommendations = branch_recommendations(
            IndustryStage::ScaleOut,
            PlayerGameplayStageStatus::BranchReady,
            actions.as_slice(),
        );

        assert!(recommendations.is_empty());
        assert_eq!(
            effective_branch_stage_status(
                PlayerGameplayStageStatus::BranchReady,
                recommendations.as_slice(),
                actions.as_slice(),
            ),
            PlayerGameplayStageStatus::BranchReady,
        );
    }
}
