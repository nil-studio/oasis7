use crate::{
    AgentLocationAuthorityV1, FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1,
    FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1, World,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack};

pub(crate) fn factory_spec(
    factory_id: &str,
    build_time_ticks: u32,
    recipe_slots: u16,
) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Test Factory".to_string(),
        tier: 1,
        tags: vec!["assembly".to_string()],
        build_cost: vec![
            MaterialStack::new("steel_plate", 10),
            MaterialStack::new("circuit_board", 2),
        ],
        build_time_ticks,
        base_power_draw: 5,
        recipe_slots,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

pub(crate) fn authorize_factory_build(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    factory_id: &str,
) {
    let location_id = format!("location-{site_id}");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install location anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: builder_agent_id.to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install builder location authority");
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: builder_agent_id.to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .expect("install factory site authority");
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: factory_id.to_string(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .expect("install construction power profile");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: factory_id.to_string(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["assembly".to_string()],
        })
        .expect("install factory capability profile");
    world
        .set_agent_resource_balance(builder_agent_id, ResourceKind::Electricity, 10)
        .expect("seed construction power");
}

pub(crate) fn bind_factory_build_module(world: &mut World, factory_id: &str, module_id: &str) {
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: factory_id.to_string(),
            factory_kind: "test".to_string(),
            source_module_id: Some(module_id.to_string()),
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 2,
            active: true,
        })
        .expect("bind construction power profile to module");
}

pub(crate) fn prepare_module_test_factory_build(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    spec: &FactoryModuleSpec,
) {
    let location_id = format!("location-{site_id}");
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install module-test location anchor");
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: builder_agent_id.to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .expect("install module-test agent location authority");
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: builder_agent_id.to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .expect("install module-test factory site authority");
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: spec.factory_id.clone(),
            factory_kind: "test".to_string(),
            source_module_id: None,
            electricity_amount: 10,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .expect("install module-test construction power profile");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .expect("install module-test factory capability profile");
    let builder_ledger = crate::MaterialLedgerId::agent(builder_agent_id);
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed module-test construction material");
    }
    let existing_power = world
        .agent_resource_balance(builder_agent_id, ResourceKind::Electricity)
        .expect("read module-test construction power");
    world
        .set_agent_resource_balance(
            builder_agent_id,
            ResourceKind::Electricity,
            existing_power.max(10),
        )
        .expect("seed module-test construction power");
}
