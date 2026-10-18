#![cfg(all(feature = "wasmtime", feature = "test_tier_full"))]

use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack};
use oasis7_wasm_executor::WasmExecutor;

fn has_active(world: &World, module_id: &str) -> bool {
    world.module_registry().active.contains_key(module_id)
}

fn sandbox() -> WasmExecutor {
    WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor")
}

fn apply_module_changes(world: &mut World, actor: &str, changes: ModuleChangeSet) {
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).expect("serialize module change set"),
    );
    let manifest = Manifest {
        version: world.manifest().version.saturating_add(1),
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world
        .propose_manifest_update(manifest, actor.to_string())
        .expect("propose changes");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, actor.to_string(), ProposalDecision::Approve)
        .expect("approve proposal");
    world.apply_proposal(proposal_id).expect("apply proposal");
}

fn factory_spec(
    factory_id: &str,
    display_name: &str,
    tier: u8,
    tags: &[&str],
    build_cost: &[(&str, i64)],
) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: display_name.to_string(),
        tier,
        tags: tags.iter().map(|item| item.to_string()).collect(),
        build_cost: build_cost
            .iter()
            .map(|(kind, amount)| MaterialStack::new(*kind, *amount))
            .collect(),
        build_time_ticks: 3,
        base_power_draw: 20,
        recipe_slots: 2,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

fn step_twice(world: &mut World, sandbox: &mut WasmExecutor) {
    let journal_start = world.journal().events.len();
    world
        .step_with_modules(sandbox)
        .expect("start module-backed action");
    world
        .step_with_modules(sandbox)
        .expect("settle module-backed action");
    assert!(
        !world.journal().events[journal_start..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )),
        "factory fixture action rejected: {:?}",
        &world.journal().events[journal_start..]
    );
}

fn start_and_settle_recipe(world: &mut World, sandbox: &mut WasmExecutor) {
    let site_ledgers: Vec<_> = world
        .state()
        .factories
        .values()
        .map(|factory| factory.input_ledger.clone())
        .collect();
    let action = world.journal().events.len();
    world
        .step_with_modules(sandbox)
        .expect("start module-backed recipe action");
    for _ in 0..8 {
        if world.pending_recipe_jobs_len() == 0 {
            break;
        }
        world
            .step_with_modules(sandbox)
            .expect("settle module-backed recipe action");
    }
    assert!(
        !world.journal().events[action..]
            .iter()
            .any(|event| matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )),
        "recipe fixture action rejected: {:?}",
        &world.journal().events[action..]
    );
    for ledger in site_ledgers {
        move_fixture_materials(world, &ledger, &MaterialLedgerId::world());
    }
}

fn move_fixture_materials(world: &mut World, from: &MaterialLedgerId, to: &MaterialLedgerId) {
    for stack in world.ledger_material_stacks(from) {
        if stack.amount > 0 {
            world
                .transfer_material_between_ledgers(from, to, &stack.kind, stack.amount)
                .expect("move fixture materials without duplicating them");
        }
    }
}

fn prepare_recipe_materials(world: &mut World, factory_id: &str) {
    let ledger = world.state().factories[factory_id].input_ledger.clone();
    move_fixture_materials(world, &MaterialLedgerId::world(), &ledger);
}

fn prepare_module_factory_site(world: &mut World, site_id: &str) {
    let location_id = world.state().agent_location_authorities["builder-a"]
        .location_id
        .clone();
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: "builder-a".to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .expect("register module factory site");
    move_fixture_materials(
        world,
        &MaterialLedgerId::world(),
        &MaterialLedgerId::agent("builder-a"),
    );
}

fn install_module_factory_fixture_profiles(world: &mut World) {
    for (factory_id, module_id, tier, tags, power) in [
        (
            "factory.smelter.mk1",
            M4_FACTORY_SMELTER_MODULE_ID,
            2,
            vec!["smelter".to_string(), "thermal".to_string()],
            6,
        ),
        (
            "factory.assembler.mk1",
            M4_FACTORY_ASSEMBLER_MODULE_ID,
            3,
            vec![
                "assembler".to_string(),
                "precision".to_string(),
                "heavy".to_string(),
            ],
            8,
        ),
    ] {
        world
            .upsert_factory_profile(FactoryProfileV1 {
                factory_id: factory_id.to_string(),
                tier,
                recipe_slots: 2,
                tags,
            })
            .expect("install module factory capability profile");
        world
            .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
                factory_id: factory_id.to_string(),
                factory_kind: factory_id.to_string(),
                source_module_id: Some(module_id.to_string()),
                electricity_amount: power,
                mode: FactoryConstructionPowerMode::StartOnlySink,
                authority_revision: 1,
                active: true,
            })
            .expect("install module-bound construction power profile");
    }
}

fn establish_stable_stage_fixture(world: &mut World, sandbox: &mut WasmExecutor) {
    let mut spec = factory_spec(
        "factory.stable-line.fixture",
        "Stable Line Fixture",
        1,
        &["fixture"],
        &[],
    );
    spec.build_time_ticks = 1;
    spec.base_power_draw = 0;
    spec.recipe_slots = 1;
    spec.maintenance_per_tick = 0;

    super::economy_factory_lifecycle::install_factory_authority(
        world,
        "builder-a",
        "site-stable-line-fixture",
        &spec.factory_id,
        0,
    );
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .expect("install stable-line fixture capability profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-stable-line-fixture".to_string(),
        spec,
    });
    step_twice(world, sandbox);
    assert!(world.has_factory("factory.stable-line.fixture"));

    world
        .set_material_balance("stable_line_marker", 1)
        .expect("seed stable-line marker");
    let plan = RecipeExecutionPlan::accepted(
        1,
        vec![MaterialStack::new("stable_line_marker", 1)],
        vec![MaterialStack::new("stable_line_marker", 1)],
        Vec::new(),
        0,
        1,
    );
    for _ in 0..3 {
        prepare_recipe_materials(world, "factory.stable-line.fixture");
        world.submit_action(Action::ScheduleRecipe {
            requester_agent_id: "builder-a".to_string(),
            factory_id: "factory.stable-line.fixture".to_string(),
            recipe_id: "recipe.stable-line.fixture".to_string(),
            plan: plan.clone(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
        });
        start_and_settle_recipe(world, sandbox);
    }

    let fixture = world
        .state()
        .factories
        .get("factory.stable-line.fixture")
        .expect("stable-line fixture factory");
    assert_eq!(fixture.production.same_recipe_repeat_count, 3);
    assert_eq!(world.state().industry_progress.completed_recipe_jobs, 3);
    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::ScaleOut
    );
    assert!(world.state().gameplay_policy.electricity_tax_bps > 0);
    assert_eq!(world.material_balance("stable_line_marker"), 1);
}

#[test]
fn module_factory_fixture_admission_uses_registered_capabilities() {
    let mut world = World::new();
    let mut wasm = sandbox();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register fixture builder");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 400)
        .expect("seed construction power");
    establish_stable_stage_fixture(&mut world, &mut wasm);
    install_module_factory_fixture_profiles(&mut world);
    for (kind, amount) in [
        ("structural_frame", 12),
        ("heat_coil", 4),
        ("refractory_brick", 6),
    ] {
        world
            .set_material_balance(kind, amount)
            .expect("seed build material");
    }
    prepare_module_factory_site(&mut world, "site-smelter");
    let mut spec = factory_spec(
        "factory.smelter.mk1",
        "Smelter MK1",
        2,
        &["smelter", "thermal"],
        &[
            ("structural_frame", 12),
            ("heat_coil", 4),
            ("refractory_brick", 6),
        ],
    );
    // Actual m4_factory_smelter_mk1 decision: the source constants select one tick.
    spec.build_time_ticks = 1;
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-smelter".to_string(),
        spec,
    });
    step_twice(&mut world, &mut wasm);
    assert!(world.has_factory("factory.smelter.mk1"));
    world.set_material_balance("iron_ore", 1).expect("seed ore");
    prepare_recipe_materials(&mut world, "factory.smelter.mk1");
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.smelter.mk1".to_string(),
        recipe_id: "recipe.smelter.iron_ingot".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ore", 1)],
            vec![MaterialStack::new("iron_ingot", 10)],
            Vec::new(),
            0,
            1,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    start_and_settle_recipe(&mut world, &mut wasm);
    assert!(
        world
            .state()
            .industry_progress
            .starter_industrial_milestone
            .is_some()
    );
    world
        .set_material_balance("structural_frame", 8)
        .expect("seed frame");
    world
        .set_material_balance("copper_wire", 8)
        .expect("seed wire");
    prepare_module_factory_site(&mut world, "site-assembler");
    let mut spec = factory_spec(
        "factory.assembler.mk1",
        "Assembler MK1",
        3,
        &["assembler", "precision", "heavy"],
        &[
            ("structural_frame", 8),
            ("iron_ingot", 10),
            ("copper_wire", 8),
        ],
    );
    spec.build_time_ticks = 1;
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-assembler".to_string(),
        spec,
    });
    step_twice(&mut world, &mut wasm);
    assert!(world.has_factory("factory.assembler.mk1"));
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity after construction"),
        400 - 6 - 8,
        "both StartOnlySink construction obligations must charge the builder"
    );
}

#[test]
fn stable_stage_fixture_uses_site_bound_recipe_materials() {
    let mut world = World::new();
    let mut wasm = sandbox();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register fixture builder");
    establish_stable_stage_fixture(&mut world, &mut wasm);
}

#[test]
fn m4_builtin_module_ids_manifest_matches_runtime_constants() {
    let expected = vec![
        M4_FACTORY_MINER_MODULE_ID,
        M4_FACTORY_SMELTER_MODULE_ID,
        M4_FACTORY_ASSEMBLER_MODULE_ID,
        M4_RECIPE_SMELT_IRON_MODULE_ID,
        M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID,
        M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID,
        M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID,
        M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID,
        M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID,
        M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID,
        M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID,
        M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID,
        M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID,
        M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID,
        M4_PRODUCT_IRON_INGOT_MODULE_ID,
        M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
        M4_PRODUCT_CONTROL_CHIP_MODULE_ID,
        M4_PRODUCT_MOTOR_MODULE_ID,
        M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID,
        M4_PRODUCT_SENSOR_PACK_MODULE_ID,
        M4_PRODUCT_MODULE_RACK_MODULE_ID,
        M4_PRODUCT_FACTORY_CORE_MODULE_ID,
    ];
    assert_eq!(m4_bootstrap_module_ids(), expected);
    assert_eq!(m4_builtin_module_ids_manifest(), expected);
}

#[test]
fn install_m4_economy_bootstrap_modules_registers_and_activates() {
    let mut world = World::new();
    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("install m4 economy modules");

    for module_id in m4_builtin_module_ids_manifest() {
        assert!(has_active(&world, module_id));
        let key = ModuleRegistry::record_key(module_id, M4_ECONOMY_MODULE_VERSION);
        assert!(world.module_registry().records.contains_key(&key));
    }
}

#[test]
fn install_m4_economy_bootstrap_modules_injects_layered_profiles() {
    let mut world = World::new();
    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("install m4 economy modules");

    let iron_ore = world
        .state()
        .material_profiles
        .get("iron_ore")
        .expect("material profile iron_ore");
    assert_eq!(iron_ore.tier, 1);
    assert_eq!(iron_ore.category, "ore");

    let factory_core = world
        .state()
        .material_profiles
        .get("factory_core")
        .expect("material profile factory_core");
    assert_eq!(factory_core.tier, 5);
    assert_eq!(factory_core.category, "infrastructure");

    let module_rack = world
        .state()
        .product_profiles
        .get("module_rack")
        .expect("product profile module_rack");
    assert_eq!(module_rack.role_tag, "governance");
    assert_eq!(module_rack.unlock_stage, "governance");

    let drone_recipe = world
        .state()
        .recipe_profiles
        .get("recipe.assembler.logistics_drone")
        .expect("recipe profile logistics_drone");
    assert_eq!(drone_recipe.stage_gate, "bootstrap");
    assert!(
        drone_recipe
            .preferred_factory_tags
            .iter()
            .any(|tag| tag == "assembler")
    );
}

#[test]
fn install_m4_economy_bootstrap_modules_is_idempotent() {
    let mut world = World::new();
    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("first install");
    let event_len = world.journal().len();

    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("second install");

    assert_eq!(world.journal().len(), event_len);
}

#[test]
fn install_m4_economy_bootstrap_modules_reactivates_registered_version() {
    let mut world = World::new();
    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("initial install");

    let registered_count = world.module_registry().records.len();

    apply_module_changes(
        &mut world,
        "bootstrap",
        ModuleChangeSet {
            deactivate: vec![ModuleDeactivation {
                module_id: M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID.to_string(),
                reason: "test deactivate".to_string(),
            }],
            ..ModuleChangeSet::default()
        },
    );
    assert!(!has_active(&world, M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID));

    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("reactivate install");

    assert!(has_active(&world, M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID));
    assert_eq!(world.module_registry().records.len(), registered_count);
}

#[test]
fn m4_economy_modules_drive_resource_to_product_chain() {
    let mut world = World::new();
    world
        .install_m4_economy_bootstrap_modules("bootstrap")
        .expect("install m4 economy package");

    let mut wasm = sandbox();
    world.submit_action(Action::RegisterAgent {
        agent_id: "builder-a".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut wasm)
        .expect("register builder");

    world.set_resource_balance(ResourceKind::Electricity, 400);
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 400)
        .expect("seed builder electricity");
    world
        .set_material_balance("structural_frame", 40)
        .expect("seed structural frames");
    world
        .set_material_balance("circuit_board", 4)
        .expect("seed circuit boards");
    world
        .set_material_balance("servo_motor", 2)
        .expect("seed servo motors");
    world
        .set_material_balance("heat_coil", 6)
        .expect("seed heat coils");
    world
        .set_material_balance("refractory_brick", 8)
        .expect("seed refractory bricks");
    world
        .set_material_balance("iron_ore", 60)
        .expect("seed iron ore");
    world
        .set_material_balance("carbon_fuel", 20)
        .expect("seed carbon fuel");
    world
        .set_material_balance("copper_ore", 60)
        .expect("seed copper ore");
    world
        .set_material_balance("silicate_ore", 20)
        .expect("seed silicate ore");
    world
        .set_material_balance("hardware_part", 40)
        .expect("seed hardware parts");

    establish_stable_stage_fixture(&mut world, &mut wasm);

    install_module_factory_fixture_profiles(&mut world);
    prepare_module_factory_site(&mut world, "site-smelter");
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-smelter".to_string(),
        module_id: M4_FACTORY_SMELTER_MODULE_ID.to_string(),
        spec: factory_spec(
            "factory.smelter.mk1",
            "Smelter MK1",
            2,
            &["smelter", "thermal"],
            &[
                ("structural_frame", 12),
                ("heat_coil", 4),
                ("refractory_brick", 6),
            ],
        ),
    });
    step_twice(&mut world, &mut wasm);
    assert!(world.has_factory("factory.smelter.mk1"));
    move_fixture_materials(
        &mut world,
        &MaterialLedgerId::agent("builder-a"),
        &MaterialLedgerId::world(),
    );

    prepare_recipe_materials(&mut world, "factory.smelter.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.smelter.mk1".to_string(),
        recipe_id: "recipe.smelter.iron_ingot".to_string(),
        module_id: M4_RECIPE_SMELT_IRON_MODULE_ID.to_string(),
        desired_batches: 12,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.smelter.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.smelter.mk1".to_string(),
        recipe_id: "recipe.smelter.copper_wire".to_string(),
        module_id: M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID.to_string(),
        desired_batches: 12,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.smelter.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.smelter.mk1".to_string(),
        recipe_id: "recipe.smelter.polymer_resin".to_string(),
        module_id: M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID.to_string(),
        desired_batches: 4,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_module_factory_site(&mut world, "site-assembler");
    world.submit_action(Action::BuildFactoryWithModule {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-assembler".to_string(),
        module_id: M4_FACTORY_ASSEMBLER_MODULE_ID.to_string(),
        spec: factory_spec(
            "factory.assembler.mk1",
            "Assembler MK1",
            3,
            &["assembler", "precision", "heavy"],
            &[
                ("structural_frame", 8),
                ("iron_ingot", 10),
                ("copper_wire", 8),
            ],
        ),
    });
    step_twice(&mut world, &mut wasm);
    assert!(world.has_factory("factory.assembler.mk1"));
    move_fixture_materials(
        &mut world,
        &MaterialLedgerId::agent("builder-a"),
        &MaterialLedgerId::world(),
    );

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.gear".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID.to_string(),
        desired_batches: 4,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.control_chip".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID.to_string(),
        desired_batches: 4,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.motor_mk1".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID.to_string(),
        desired_batches: 2,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.logistics_drone".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID.to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    assert_eq!(
        world.state().industry_progress.stage,
        IndustryStage::Governance
    );

    prepare_recipe_materials(&mut world, "factory.smelter.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.smelter.mk1".to_string(),
        recipe_id: "recipe.smelter.alloy_plate".to_string(),
        module_id: M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID.to_string(),
        desired_batches: 3,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.sensor_pack".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID.to_string(),
        desired_batches: 2,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.module_rack".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID.to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    prepare_recipe_materials(&mut world, "factory.assembler.mk1");
    world.submit_action(Action::ScheduleRecipeWithModule {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.assembler.mk1".to_string(),
        recipe_id: "recipe.assembler.factory_core".to_string(),
        module_id: M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID.to_string(),
        desired_batches: 1,
        deterministic_seed: 20260214,
    });
    start_and_settle_recipe(&mut world, &mut wasm);

    assert_eq!(world.material_balance("factory_core"), 1);
    assert_eq!(world.material_balance("module_rack"), 0);
    assert_eq!(world.material_balance("sensor_pack"), 0);
    assert_eq!(world.material_balance("logistics_drone"), 1);
    assert_eq!(world.material_balance("motor_mk1"), 0);
    assert_eq!(world.material_balance("control_chip"), 0);
    assert_eq!(world.material_balance("gear"), 0);
    assert_eq!(world.material_balance("alloy_plate"), 2);
    assert_eq!(world.material_balance("hardware_part"), 24);
    assert_eq!(world.material_balance("iron_ingot"), 10);
    assert_eq!(world.material_balance("copper_wire"), 8);
    assert_eq!(world.material_balance("slag"), 15);
    assert_eq!(world.material_balance("waste_resin"), 8);
    assert_eq!(world.material_balance("assembly_scrap"), 1);
    assert_eq!(world.material_balance("calibration_scrap"), 2);
    assert_eq!(world.material_balance("precision_scrap"), 1);
    assert_eq!(world.material_balance("structural_waste"), 1);
    let mut recipe_power = 0;
    let mut construction_power = 0;
    for event in &world.journal().events {
        match &event.body {
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                power_required,
                power_owner_agent_id,
                ..
            }) => {
                assert_eq!(power_owner_agent_id.as_deref(), Some("builder-a"));
                recipe_power += power_required;
            }
            WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
                construction_power_obligation: Some(obligation),
                ..
            }) => {
                assert_eq!(obligation.payer_agent_id, "builder-a");
                construction_power += obligation.electricity_amount;
            }
            _ => {}
        }
    }
    // The eleven builtin recipes charge per batch: 12*8 + 12*6 + 4*7 +
    // 4*4 + 4*6 + 2*7 + 1*12 + 3*9 + 2*8 + 1*10 + 1*14 = 329.
    assert_eq!(recipe_power, 329);
    assert_eq!(construction_power, 6 + 8);
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity"),
        400 - recipe_power - construction_power
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 400);
    assert_eq!(world.material_balance("stable_line_marker"), 1);

    let rejected_events = world
        .journal()
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )
        })
        .count();
    assert_eq!(rejected_events, 0);
}
