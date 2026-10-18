use super::economy::activate_pure_module;
use super::pos;
use crate::{Action, MaterialLedgerId, World};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{MaterialStack, ModuleEmit, ModuleOutput, RecipeExecutionPlan};
use oasis7_wasm_executor::FixedSandbox;

use super::economy::{authorize_factory_build, factory_spec};

#[test]
fn schedule_recipe_with_module_uses_module_plan() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");

    world
        .set_material_balance("steel_plate", 10)
        .expect("seed steel");
    world
        .set_material_balance("circuit_board", 2)
        .expect("seed circuits");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 10)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 2)
        .expect("seed builder circuits");
    authorize_factory_build(&mut world, "builder-a", "site-1", "factory.recipe.module");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.recipe.module", 1, 1),
    });
    world.step().expect("start build");
    world.step().expect("build complete");

    world
        .set_material_balance("iron_ingot", 7)
        .expect("seed ingot");
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 7)
        .expect("seed site ingot");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 30)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 30);
    activate_pure_module(&mut world, "m4.recipe.motor", b"recipe-module");

    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.recipe.module".to_string(),
        recipe_id: "recipe.motor.mk1".to_string(),
        module_id: "m4.recipe.motor".to_string(),
        desired_batches: 2,
        deterministic_seed: 42,
    });

    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "economy.recipe_execution_plan".to_string(),
            payload: serde_json::to_value(RecipeExecutionPlan::accepted(
                2,
                vec![MaterialStack::new("iron_ingot", 6)],
                vec![MaterialStack::new("motor_mk1", 2)],
                vec![MaterialStack::new("metal_scrap", 1)],
                9,
                1,
            ))
            .expect("serialize recipe execution plan"),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    };
    let mut sandbox = FixedSandbox::succeed(output);
    world
        .step_with_modules(&mut sandbox)
        .expect("start recipe with module");

    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        1
    );
    assert_eq!(world.material_balance("iron_ingot"), 7);
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity"),
        21
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 30);
    assert_eq!(world.pending_recipe_jobs_len(), 1);

    for _ in 0..4 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(&mut sandbox)
            .expect("advance module recipe toward completion");
    }
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "motor_mk1"),
        2
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "metal_scrap"),
        1
    );
    assert_eq!(world.material_balance("motor_mk1"), 0);
    assert_eq!(world.material_balance("metal_scrap"), 0);
}
