use super::*;
use crate::{STARTER_SMELTER_FACTORY_ID, STARTER_SMELTER_RECIPE_ID};

#[test]
fn committed_recipe_origin_module_due_completion_preserves_pending_and_legacy() {
    for has_origin in [false, true] {
        let mut world = logistics_drone_module_recipe_world("factory.test");
        let mut origin = crate::events::recipe_origin_tests::origin();
        origin.submission.recipe_id = "recipe.test".into();
        let action = Action::ScheduleRecipe {
            requester_agent_id: "builder-a".into(),
            factory_id: "factory.test".into(),
            recipe_id: "recipe.test".into(),
            plan: RecipeExecutionPlan::accepted(
                1,
                vec![],
                vec![MaterialStack::new("iron_ingot", 3)],
                vec![],
                1,
                1,
            ),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        let id = if has_origin {
            world
                .submit_recipe_action_with_origin(action, origin.clone())
                .unwrap()
        } else {
            world.submit_action(action)
        };
        let expected = has_origin.then_some(origin);
        let mut sandbox = CaptureContextSandbox::with_outputs(vec![]);
        world
            .step_with_modules(&mut sandbox)
            .expect("start with modules");
        assert_eq!(
            world.state().pending_recipe_jobs[&id].committed_recipe_origin,
            expected
        );
        world
            .step_with_modules(&mut sandbox)
            .expect("module due completion");
        assert_eq!(
            world.state().recipe_completion_receipts[&id].committed_recipe_origin,
            expected
        );
        let completed = world
            .journal()
            .events
            .iter()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                    job_id,
                    committed_recipe_origin,
                    ..
                }) if *job_id == id => Some(committed_recipe_origin.clone()),
                _ => None,
            })
            .expect("actual module completion event");
        assert_eq!(completed, expected);
        let settled = world.state().material_ledgers.clone();
        world
            .step_with_modules(&mut sandbox)
            .expect("later module tick");
        assert_eq!(
            world.state().material_ledgers,
            settled,
            "completion is not repeated"
        );
        assert_eq!(world.state().industry_progress.completed_recipe_jobs, 1);
    }
}

#[test]
fn starter_settlement_summary_preserves_actual_commitment_and_legacy() {
    for has_origin in [false, true] {
        let mut world = logistics_drone_module_recipe_world(STARTER_SMELTER_FACTORY_ID);
        world.set_material_balance("iron_ore", 48).unwrap();
        world
            .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ore", 48)
            .unwrap();
        let mut origin = crate::events::recipe_origin_tests::origin();
        origin.submission.recipe_id = STARTER_SMELTER_RECIPE_ID.into();
        origin.submission.factory_id = STARTER_SMELTER_FACTORY_ID.into();
        let action = Action::ScheduleRecipe {
            requester_agent_id: "builder-a".into(),
            factory_id: STARTER_SMELTER_FACTORY_ID.into(),
            recipe_id: STARTER_SMELTER_RECIPE_ID.into(),
            plan: RecipeExecutionPlan::accepted(
                12,
                vec![MaterialStack::new("iron_ore", 48)],
                vec![MaterialStack::new("iron_ingot", 36)],
                vec![],
                24,
                1,
            ),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        let id = if has_origin {
            world
                .submit_recipe_action_with_origin(action, origin.clone())
                .unwrap()
        } else {
            world.submit_action(action)
        };
        let expected = has_origin.then_some(origin);
        let mut sandbox = CaptureContextSandbox::with_outputs(vec![]);
        world
            .step_with_modules(&mut sandbox)
            .expect("start with modules");
        assert_eq!(
            world.state().pending_recipe_jobs[&id].committed_recipe_origin,
            expected
        );
        world
            .step_with_modules(&mut sandbox)
            .expect("module due completion");
        assert_eq!(
            world.state().recipe_completion_receipts[&id].committed_recipe_origin,
            expected
        );
        let completed = world
            .journal()
            .events
            .iter()
            .find_map(|event| match &event.body {
                WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                    job_id,
                    committed_recipe_origin,
                    ..
                }) if *job_id == id => Some(committed_recipe_origin.clone()),
                _ => None,
            })
            .expect("actual module completion event");
        assert_eq!(completed, expected);
        let milestone = world
            .state()
            .industry_progress
            .starter_industrial_milestone
            .as_ref()
            .expect("first settled milestone");
        if has_origin {
            let summary = milestone
                .settlement_summary
                .as_ref()
                .expect("new v2 summary");
            assert_eq!(summary.requester_agent_id, "builder-a");
            assert_eq!(summary.accepted_batches, 12);
            assert_eq!(summary.produce, vec![MaterialStack::new("iron_ingot", 36)]);
            assert_eq!(summary.consume, vec![MaterialStack::new("iron_ore", 48)]);
            assert_eq!(summary.power_required, 24);
        } else {
            assert!(milestone.settlement_summary.is_none());
            assert!(
                serde_json::to_value(milestone)
                    .unwrap()
                    .get("settlement_summary")
                    .is_none()
            );
        }
        let settled = world.state().material_ledgers.clone();
        world
            .step_with_modules(&mut sandbox)
            .expect("later module tick");
        assert_eq!(
            world.state().material_ledgers,
            settled,
            "completion is not repeated"
        );
        assert_eq!(world.state().industry_progress.completed_recipe_jobs, 1);
    }
}
