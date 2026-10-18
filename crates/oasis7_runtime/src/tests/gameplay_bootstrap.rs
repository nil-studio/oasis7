#![cfg(all(feature = "wasmtime", feature = "test_tier_full"))]

use super::super::*;

fn has_active(world: &World, module_id: &str) -> bool {
    world.module_registry().active.contains_key(module_id)
}

#[test]
fn m5_builtin_module_ids_manifest_matches_runtime_constants() {
    let expected = vec![
        M5_GAMEPLAY_WAR_MODULE_ID,
        M5_GAMEPLAY_GOVERNANCE_MODULE_ID,
        M5_GAMEPLAY_CRISIS_MODULE_ID,
        M5_GAMEPLAY_ECONOMIC_MODULE_ID,
        M5_GAMEPLAY_META_MODULE_ID,
    ];
    assert_eq!(m5_builtin_module_ids_manifest(), expected);
}

#[test]
fn install_m5_gameplay_bootstrap_modules_registers_and_activates() {
    let mut world = World::new();
    world
        .install_m5_gameplay_bootstrap_modules("bootstrap")
        .expect("install m5 gameplay modules");

    for module_id in m5_builtin_module_ids_manifest() {
        assert!(has_active(&world, module_id));
        let key = ModuleRegistry::record_key(module_id, M5_GAMEPLAY_MODULE_VERSION);
        let record = world
            .module_registry()
            .records
            .get(&key)
            .expect("gameplay module record");
        assert_eq!(record.manifest.role, ModuleRole::Gameplay);
        assert!(record.manifest.abi_contract.gameplay.is_some());
    }

    let readiness = world.gameplay_mode_readiness("sandbox");
    assert!(readiness.is_ready());
    assert!(readiness.missing_kinds.is_empty());
}

#[test]
fn install_gameplay_scenario_bootstrap_modules_stacks_base_and_gameplay() {
    let mut world = World::new();
    world
        .install_gameplay_scenario_bootstrap_modules(
            "bootstrap",
            M1ScenarioBootstrapConfig::default(),
        )
        .expect("install gameplay scenario bootstrap");

    assert!(has_active(&world, M1_RADIATION_POWER_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_POWER_MODULE_ID));
    assert!(has_active(&world, M1_SENSOR_MODULE_ID));
    assert!(has_active(&world, M4_FACTORY_MINER_MODULE_ID));

    for module_id in m5_builtin_module_ids_manifest() {
        assert!(has_active(&world, module_id));
    }

    let readiness = world.gameplay_mode_readiness("sandbox");
    assert!(readiness.is_ready());
}
