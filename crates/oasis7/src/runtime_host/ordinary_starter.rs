//! Explicit fresh-world finite public inventory; no participant receives assets here.
use oasis7_agent_api::world_action::{
    Action, GeoPos, MaterialLedgerId, ORDINARY_STARTER_FACTORY_KIND, ORDINARY_STARTER_PROFILE_ID,
    OrdinaryStarterSlotInputV1, ResourceKind,
};
use oasis7_agent_api::world_quote::builtin_factory_spec;
use oasis7_runtime::{
    FactoryConstructionPowerMode, FactoryConstructionPowerProfileV1, FactoryProfileV1,
    FactorySiteAuthorityV1, LocationAnchorV1, World,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
mod legacy_claim_tests;

pub const SUPPLIER_ID: &str = "ordinary-starter-supplier-v1";
pub const LOCATION_ID: &str = "ordinary-starter-yard-v1";
pub const SLOT_COUNT: usize = 8;
const CONSTRUCTION_POWER: i64 = 10;

/// The initializer commits to this implementation through its owning crate,
/// without reading another package's source tree directly.
pub fn initializer_source_bytes() -> &'static [u8] {
    include_bytes!("ordinary_starter.rs")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryStarterPoolV1 {
    pub profile_id: String,
    pub supplier_agent_id: String,
    pub location_id: String,
    pub slots: Vec<OrdinaryStarterSlotInputV1>,
    pub supplier_materials: BTreeMap<String, i64>,
    pub supplier_electricity: i64,
}

/// Called only while assembling a new genesis artifact. Repeated initialization
/// fails; replenishment is not a side effect of joining, reading, or claiming.
pub fn initialize(world: &mut World) -> Result<OrdinaryStarterPoolV1, String> {
    if world.state().agents.contains_key(SUPPLIER_ID)
        || world.state().location_anchors.contains_key(LOCATION_ID)
        || !world
            .capability_revocation_state()
            .participants
            .bindings
            .is_empty()
    {
        return Err("ordinary starter requires a fresh unclaimed public pool".into());
    }
    world.submit_action(Action::RegisterAgent {
        agent_id: SUPPLIER_ID.into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world
        .step()
        .map_err(|error| format!("starter supplier registration: {error:?}"))?;
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: LOCATION_ID.into(),
            active: true,
            authority_revision: 1,
            effective_at: world.state().time,
        })
        .map_err(|error| format!("starter public location: {error:?}"))?;
    let mut slots = Vec::with_capacity(SLOT_COUNT);
    for index in 1..=SLOT_COUNT {
        let input = OrdinaryStarterSlotInputV1 {
            slot_id: format!("ordinary-starter-slot-{index}"),
            site_id: format!("ordinary-starter-site-{index}"),
            factory_id: format!("ordinary-starter-smelter-{index}"),
        };
        world
            .set_factory_site_authority(FactorySiteAuthorityV1 {
                site_id: input.site_id.clone(),
                location_id: LOCATION_ID.into(),
                owner_agent_id: SUPPLIER_ID.into(),
                authorized_agent_ids: vec![SUPPLIER_ID.into()],
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: world.state().time,
            })
            .map_err(|error| format!("starter public site: {error:?}"))?;
        let spec = builtin_factory_spec(ORDINARY_STARTER_FACTORY_KIND, &input.factory_id)
            .ok_or("starter canonical factory unavailable")?;
        world
            .upsert_factory_profile(FactoryProfileV1 {
                factory_id: input.factory_id.clone(),
                tier: spec.tier,
                recipe_slots: spec.recipe_slots,
                tags: spec.tags,
            })
            .map_err(|error| format!("starter factory profile: {error:?}"))?;
        world
            .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
                factory_id: input.factory_id.clone(),
                factory_kind: ORDINARY_STARTER_FACTORY_KIND.into(),
                source_module_id: None,
                electricity_amount: CONSTRUCTION_POWER,
                mode: FactoryConstructionPowerMode::StartOnlySink,
                authority_revision: 1,
                active: true,
            })
            .map_err(|error| format!("starter construction power: {error:?}"))?;
        slots.push(input);
    }
    let (materials, recipe_electricity, _) = world
        .state()
        .ordinary_starter_package()
        .map_err(|error| format!("starter complete recipe inputs: {error:?}"))?;
    let count = i64::try_from(SLOT_COUNT).map_err(|_| "starter slot count overflow")?;
    let mut supplier_materials = BTreeMap::new();
    for (material, per_claim) in materials {
        let total = per_claim
            .checked_mul(count)
            .ok_or("starter material total overflow")?;
        world
            .set_ledger_material_balance(MaterialLedgerId::agent(SUPPLIER_ID), &material, total)
            .map_err(|error| format!("starter supplier material: {error:?}"))?;
        supplier_materials.insert(material, total);
    }
    let supplier_electricity = recipe_electricity
        .checked_add(CONSTRUCTION_POWER)
        .and_then(|per_claim| per_claim.checked_mul(count))
        .ok_or("starter electricity total overflow")?;
    world
        .set_agent_resource_balance(SUPPLIER_ID, ResourceKind::Electricity, supplier_electricity)
        .map_err(|error| format!("starter supplier electricity: {error:?}"))?;
    world
        .step()
        .map_err(|error| format!("starter inventory boundary: {error:?}"))?;
    Ok(OrdinaryStarterPoolV1 {
        profile_id: ORDINARY_STARTER_PROFILE_ID.into(),
        supplier_agent_id: SUPPLIER_ID.into(),
        location_id: LOCATION_ID.into(),
        slots,
        supplier_materials,
        supplier_electricity,
    })
}

/// Publication follows genesis identity derivation, avoiding a self-referential
/// genesis hash. Both the finite inventory and this fixed rule are committed by
/// the explicit initializer before any participant can enter.
pub fn publish(world: &mut World, pool: &OrdinaryStarterPoolV1) -> Result<(), String> {
    if pool.profile_id != ORDINARY_STARTER_PROFILE_ID
        || pool.supplier_agent_id != SUPPLIER_ID
        || pool.location_id != LOCATION_ID
        || pool.slots.len() != SLOT_COUNT
    {
        return Err("ordinary starter initialization profile mismatch".into());
    }
    let binding = world
        .ordinary_starter_status(None)
        .world_binding
        .ok_or("ordinary starter requires a bound world genesis")?;
    let offer = world
        .state()
        .ordinary_starter_offer(
            ORDINARY_STARTER_PROFILE_ID,
            SUPPLIER_ID,
            &binding,
            &pool.slots,
            world.state().time,
        )
        .map_err(|error| format!("starter offer preparation: {error:?}"))?;
    world
        .publish_ordinary_starter_offer(offer)
        .map_err(|error| format!("starter offer publication: {error:?}"))?;
    world
        .step()
        .map_err(|error| format!("starter publication boundary: {error:?}"))?;
    Ok(())
}
