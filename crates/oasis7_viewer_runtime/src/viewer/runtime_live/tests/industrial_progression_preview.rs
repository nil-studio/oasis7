use super::industrial_progression::{
    expect_player_gameplay, setup_industrial_gameplay_with_completed_jobs,
};
use super::*;
use crate::simulator::PlayerGameplayGoalKind;
use oasis7_agent_api::world_action::ResourceKind;

#[test]
fn runtime_gameplay_snapshot_binds_a_deterministic_first_delivery_preview_to_each_midloop_branch() {
    let _guard = lock_test_llm_env();
    let mut server = setup_industrial_gameplay_with_completed_jobs(53, 6);
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("mid-loop agent")
        .clone();
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, i64::MAX)
        .expect("fund mid-loop agent electricity");
    server
        .world
        .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, i64::MAX)
        .expect("fund mid-loop agent data");
    let smelter_site_ledger = crate::runtime::MaterialLedgerId::site(
        server
            .world
            .state()
            .factories
            .get(crate::viewer::FACTORY_SMELTER_MK1)
            .expect("mid-loop smelter")
            .site_id
            .as_str(),
    );
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger.clone(), "iron_ingot", 200)
        .expect("seed mid-loop iron ingot");
    server
        .world
        .set_ledger_material_balance(smelter_site_ledger, "copper_wire", 200)
        .expect("seed mid-loop copper wire");
    server
        .world
        .set_resource_balance(ResourceKind::Electricity, 2_000);

    let gameplay = expect_player_gameplay(&mut server, "mid-loop specialization snapshot");
    assert_eq!(
        gameplay.goal_kind,
        PlayerGameplayGoalKind::ChooseMidLoopPath,
        "first-delivery previews are required at the specialization choice"
    );
    assert!(!gameplay.branch_recommendations.is_empty());

    let serialized = serde_json::to_value(server.compat_snapshot(Some("player-a")))
        .expect("compat snapshot serializes");
    let branches = serialized
        .pointer("/player_gameplay/branch_recommendations")
        .and_then(serde_json::Value::as_array)
        .expect("mid-loop snapshot publishes branch recommendations");
    assert!(!branches.is_empty());
    let expected_inputs = |action_id: &str| match action_id {
        "schedule_recipe_smelter_alloy_plate" => vec!["iron_ingot × 2", "copper_wire × 2"],
        "build_factory_assembler_mk1" => {
            vec!["structural_frame × 8", "iron_ingot × 10", "copper_wire × 8"]
        }
        "schedule_recipe_assembler_module_rack" => {
            vec!["sensor_pack × 2", "control_chip × 1"]
        }
        "schedule_recipe_assembler_factory_core" => {
            vec!["module_rack × 1", "alloy_plate × 3"]
        }
        other => panic!("unsupported mid-loop branch action {other}"),
    };
    for branch in branches {
        let action_id = branch
            .get("action_id")
            .and_then(serde_json::Value::as_str)
            .expect("branch recommendation action identity");
        let preview = branch
            .get("first_delivery_preview")
            .and_then(serde_json::Value::as_object)
            .unwrap_or_else(|| panic!("branch {action_id} must publish first-delivery preview"));

        for field in [
            "local_need",
            "expected_output",
            "value_timing",
            "leverage_class_unlocked",
            "return_visit_hook",
        ] {
            assert!(
                preview
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty()),
                "branch {action_id} first-delivery preview must publish non-empty {field}"
            );
        }
        let required_inputs = preview
            .get("required_inputs")
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("branch {action_id} must publish required inputs"));
        assert!(!required_inputs.is_empty());
        assert!(
            required_inputs
                .iter()
                .all(|input| input.as_str().is_some_and(|value| !value.trim().is_empty())),
            "branch {action_id} first-delivery preview inputs must be player-readable"
        );
        let actual_inputs = required_inputs
            .iter()
            .map(|input| {
                input
                    .as_str()
                    .unwrap_or_else(|| panic!("branch {action_id} input must be a string"))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual_inputs,
            expected_inputs(action_id),
            "branch {action_id} first-delivery preview inputs must match its canonical recipe/build amounts"
        );
    }

    let repeated = serde_json::to_value(server.compat_snapshot(Some("player-a")))
        .expect("repeat compat snapshot serializes");
    assert_eq!(
        serialized.pointer("/player_gameplay/branch_recommendations"),
        repeated.pointer("/player_gameplay/branch_recommendations"),
        "first-delivery previews must be deterministic for an unchanged world"
    );
}
