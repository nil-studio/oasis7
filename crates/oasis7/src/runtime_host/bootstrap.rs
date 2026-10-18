//! Headless world bootstrap shared by native application hosts.
use super::gameplay_actions::{
    FACTORY_ASSEMBLER_MK1, FACTORY_SMELTER_MK1, STARTER_INDUSTRIAL_ELECTRICITY,
    formal_release_default_seed_model,
};
use crate::geometry::GeoPos;
use crate::runtime::{
    Action as RuntimeAction, AgentLocationAuthorityV1, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1,
    MaterialLedgerId, World as RuntimeWorld,
};
use crate::simulator::*;
pub use oasis7_client_api::bootstrap::FORMAL_RELEASE_DEFAULT_WORLD_ID;
use oasis7_simulator::{ChunkRuntimeConfig, Location, WorldModel};
use std::collections::HashSet;
use std::path::Path;
pub const FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID: &str = "starter-agent-0";
pub fn bootstrap_runtime_world_with_chunk_runtime(
    scenario: WorldScenario,
) -> Result<(RuntimeWorld, WorldConfig, ChunkRuntimeConfig), String> {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(scenario, &config);
    let (model, _) = build_world_model(&config, &init)
        .map_err(|err| format!("runtime live bootstrap build_world_model failed: {err:?}"))?;
    let chunk_runtime = ChunkRuntimeConfig {
        world_seed: init.seed,
        asteroid_fragment_enabled: init.asteroid_fragment.enabled,
        asteroid_fragment_seed_offset: init.asteroid_fragment.seed_offset,
        min_fragment_spacing_cm: init.asteroid_fragment.min_fragment_spacing_cm,
    };
    let (world, config) =
        bootstrap_runtime_world_from_model(config, &model, "runtime live bootstrap")?;
    Ok((world, config, chunk_runtime))
}

pub fn bootstrap_generated_sidecar_runtime_world(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel), String> {
    let (world, config, model, _) =
        bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(generated_world_dir)?;
    Ok((world, config, model))
}

pub fn bootstrap_generated_sidecar_runtime_world_with_chunk_runtime(
    generated_world_dir: &Path,
) -> Result<(RuntimeWorld, WorldConfig, WorldModel, ChunkRuntimeConfig), String> {
    let sidecar_dir = generated_world_dir.join("generated-scenario-world");
    let provenance_path = generated_world_dir.join("world-generation-provenance.json");
    if !provenance_path.is_file() {
        return Err(format!(
            "generated world provenance missing: {}",
            provenance_path.display()
        ));
    }
    let kernel = WorldKernel::load_from_dir(&sidecar_dir).map_err(|err| {
        format!(
            "runtime live generated sidecar load failed dir={} err={err:?}",
            sidecar_dir.display()
        )
    })?;
    let snapshot = kernel.snapshot();
    let chunk_runtime = snapshot.chunk_runtime.clone();
    let config = snapshot.config;
    let model = snapshot.model;
    let (world, config) =
        bootstrap_runtime_world_from_model(config, &model, "runtime live generated sidecar")?;
    Ok((world, config, model, chunk_runtime))
}

fn bootstrap_runtime_world_from_model(
    config: WorldConfig,
    model: &WorldModel,
    label: &str,
) -> Result<(RuntimeWorld, WorldConfig), String> {
    let mut world =
        crate::runtime_host::world::new_production_hardened_with_initial_module_visuals(
            &model.module_visual_entities,
        )
        .map_err(|err| format!("{label} initialize module visuals failed: {err:?}"))?;
    world.set_resource_balance(ResourceKind::Electricity, 400);
    for (material, amount) in [
        ("structural_frame", 40),
        ("circuit_board", 4),
        ("servo_motor", 2),
        ("heat_coil", 6),
        ("refractory_brick", 8),
        ("iron_ore", 60),
        ("carbon_fuel", 20),
        ("copper_ore", 60),
        ("silicate_ore", 20),
        ("hardware_part", 40),
    ] {
        world
            .set_material_balance(material, amount)
            .map_err(|err| {
                format!(
                    "{label} set material balance failed material={} err={err:?}",
                    material
                )
            })?;
    }
    let mut seed_agents: Vec<(String, GeoPos, i64, i64)> = model
        .agents
        .iter()
        .map(|(agent_id, agent)| {
            (
                agent_id.clone(),
                agent.pos,
                agent.resources.get(ResourceKind::Electricity),
                agent.resources.get(ResourceKind::Data),
            )
        })
        .collect();
    seed_agents.sort_by(|left, right| left.0.cmp(&right.0));

    if seed_agents.is_empty() {
        seed_agents.push(("runtime-agent-0".to_string(), GeoPos::new(0, 0, 0), 32, 8));
        seed_agents.push(("runtime-agent-1".to_string(), GeoPos::new(0, 0, 0), 32, 8));
    }

    for (agent_id, pos, _, _) in &seed_agents {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent_id.clone(),
            pos: *pos,
        });
    }

    if world.pending_actions_len() > 0 {
        world
            .step()
            .map_err(|err| format!("{label} register step failed: {err:?}"))?;
    }

    for (agent_id, electricity, data) in world
        .state()
        .agents
        .keys()
        .cloned()
        .map(|agent_id| {
            let maybe_seed = seed_agents
                .iter()
                .find(|entry| entry.0 == agent_id)
                .cloned();
            match maybe_seed {
                Some((_, _, electricity, data)) => (agent_id, electricity.max(32), data.max(8)),
                None => (agent_id, 32, 8),
            }
        })
        .collect::<Vec<_>>()
    {
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Electricity, electricity)
            .map_err(|err| {
                format!(
                    "{label} set electricity failed agent={} err={err:?}",
                    agent_id
                )
            })?;
        world
            .set_agent_resource_balance(agent_id.as_str(), ResourceKind::Data, data)
            .map_err(|err| format!("{label} set data failed agent={} err={err:?}", agent_id))?;
    }
    let authority_agents = world
        .state()
        .agents
        .keys()
        .map(|agent_id| {
            let location_id = model
                .agents
                .get(agent_id)
                .map(|agent| agent.location_id.clone())
                .unwrap_or_else(|| "runtime-bootstrap-location".to_string());
            (agent_id.clone(), location_id)
        })
        .collect::<Vec<_>>();
    install_starter_factory_authorities(&mut world, authority_agents.as_slice(), label)?;
    world
        .step()
        .map_err(|err| format!("{label} resource seed consensus step failed: {err:?}"))?;

    Ok((world, config))
}

pub fn bootstrap_formal_release_runtime_world() -> Result<(RuntimeWorld, WorldConfig), String> {
    let (config, seed_model) = formal_release_default_seed_model()?;
    let starter_agent = seed_model
        .agents
        .get(FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID)
        .ok_or_else(|| {
            format!(
                "formal release seed model missing bootstrap agent {}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    let mut world = crate::runtime_host::world::new_production_hardened();
    world.set_resource_balance(ResourceKind::Electricity, 400);
    for (material, amount) in [
        ("structural_frame", 40),
        ("circuit_board", 4),
        ("servo_motor", 2),
        ("heat_coil", 6),
        ("refractory_brick", 8),
        ("iron_ore", 60),
        ("carbon_fuel", 20),
        ("copper_ore", 60),
        ("silicate_ore", 20),
        ("hardware_part", 40),
    ] {
        world
            .set_material_balance(material, amount)
            .map_err(|err| {
                format!(
                    "formal release bootstrap set material balance failed material={} err={err:?}",
                    material
                )
            })?;
    }
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID.to_string(),
        pos: starter_agent.pos,
    });
    world
        .step()
        .map_err(|err| format!("formal release bootstrap register step failed: {err:?}"))?;
    world
        .set_agent_resource_balance(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID,
            ResourceKind::Electricity,
            starter_agent
                .resources
                .get(ResourceKind::Electricity)
                .max(STARTER_INDUSTRIAL_ELECTRICITY),
        )
        .map_err(|err| {
            format!(
                "formal release bootstrap set electricity failed agent={} err={err:?}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    world
        .set_agent_resource_balance(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID,
            ResourceKind::Data,
            starter_agent.resources.get(ResourceKind::Data).max(8),
        )
        .map_err(|err| {
            format!(
                "formal release bootstrap set data failed agent={} err={err:?}",
                FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID
            )
        })?;
    install_starter_factory_authorities(
        &mut world,
        &[(
            FORMAL_RELEASE_DEFAULT_BOOTSTRAP_AGENT_ID.to_string(),
            starter_agent.location_id.clone(),
        )],
        "formal release bootstrap",
    )?;
    world.step().map_err(|err| {
        format!("formal release bootstrap resource consensus step failed: {err:?}")
    })?;
    Ok((world, config))
}

fn install_starter_factory_authorities(
    world: &mut RuntimeWorld,
    agents: &[(String, String)],
    label: &str,
) -> Result<(), String> {
    let Some((owner_agent_id, starter_location_id)) = agents.first() else {
        return Ok(());
    };
    let owner_power = world
        .agent_resource_balance(owner_agent_id.as_str(), ResourceKind::Electricity)
        .map_err(|err| format!("{label} read starter electricity failed: {err:?}"))?;
    if owner_power < STARTER_INDUSTRIAL_ELECTRICITY {
        world
            .set_agent_resource_balance(
                owner_agent_id.as_str(),
                ResourceKind::Electricity,
                STARTER_INDUSTRIAL_ELECTRICITY,
            )
            .map_err(|err| format!("{label} seed starter electricity failed: {err:?}"))?;
    }
    let mut anchored_locations = HashSet::new();
    for (_, location_id) in agents {
        if anchored_locations.insert(location_id.clone()) {
            world
                .set_location_anchor(LocationAnchorV1 {
                    location_id: location_id.clone(),
                    active: true,
                    authority_revision: 1,
                    effective_at: 0,
                })
                .map_err(|err| format!("{label} install location anchor failed: {err:?}"))?;
        }
    }
    for (agent_id, location_id) in agents {
        world
            .set_agent_location_authority(AgentLocationAuthorityV1 {
                agent_id: agent_id.clone(),
                location_id: location_id.clone(),
                active: true,
                authority_revision: 1,
                effective_at: world.state().time,
            })
            .map_err(|err| format!("{label} install agent location authority failed: {err:?}"))?;
    }
    let authorized_agent_ids = agents
        .iter()
        .skip(1)
        .map(|(agent_id, _)| agent_id.clone())
        .collect::<Vec<_>>();
    for site_id in ["site-smelter", "site-assembler"] {
        world
            .set_factory_site_authority(FactorySiteAuthorityV1 {
                site_id: site_id.to_string(),
                location_id: starter_location_id.clone(),
                owner_agent_id: owner_agent_id.clone(),
                authorized_agent_ids: authorized_agent_ids.clone(),
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: world.state().time,
            })
            .map_err(|err| format!("{label} install factory site authority failed: {err:?}"))?;
    }
    for factory_id in [FACTORY_SMELTER_MK1, FACTORY_ASSEMBLER_MK1] {
        world
            .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
                factory_id: factory_id.to_string(),
                factory_kind: factory_id.to_string(),
                source_module_id: None,
                electricity_amount: 10,
                mode: FactoryConstructionPowerMode::StartOnlySink,
                authority_revision: 1,
                active: true,
            })
            .map_err(|err| {
                format!("{label} install factory construction power profile failed: {err:?}")
            })?;
    }
    for (factory_id, tier, tags) in [
        (
            FACTORY_SMELTER_MK1,
            2,
            vec!["smelter".to_string(), "thermal".to_string()],
        ),
        (
            FACTORY_ASSEMBLER_MK1,
            3,
            vec!["assembler".to_string(), "precision".to_string()],
        ),
    ] {
        world
            .upsert_factory_profile(FactoryProfileV1 {
                factory_id: factory_id.to_string(),
                tier,
                recipe_slots: 2,
                tags,
            })
            .map_err(|err| format!("{label} install factory profile failed: {err:?}"))?;
    }
    let owner_ledger = MaterialLedgerId::agent(owner_agent_id);
    for (material, amount) in [
        // BuildFactory consumes from the owner ledger, and no executable
        // material-transfer action currently connects the smelter site ledger
        // back to that builder ledger. Keep both starter construction beats
        // attainable by seeding the exact authoritative obligations here.
        ("structural_frame", 20),
        ("heat_coil", 4),
        ("refractory_brick", 6),
        ("iron_ingot", 10),
        ("copper_wire", 8),
    ] {
        world
            .set_ledger_material_balance(owner_ledger.clone(), material, amount)
            .map_err(|err| format!("{label} seed starter build material failed: {err:?}"))?;
    }
    let smelter_ledger = MaterialLedgerId::site("site-smelter");
    for (material, amount) in [("iron_ore", 60), ("carbon_fuel", 20)] {
        world
            .set_ledger_material_balance(smelter_ledger.clone(), material, amount)
            .map_err(|err| format!("{label} seed starter recipe material failed: {err:?}"))?;
    }
    // The first canonical assembler recipe consumes two iron ingots per batch
    // and the gameplay action schedules four batches. Keep its inputs on the
    // assembler's authoritative site ledger; recipe admission no longer
    // falls back to the world ledger and the Viewer exposes no transfer
    // action that could bridge this bootstrap gap.
    let assembler_ledger = MaterialLedgerId::site("site-assembler");
    world
        .set_ledger_material_balance(assembler_ledger, "iron_ingot", 8)
        .map_err(|err| format!("{label} seed starter assembler recipe material failed: {err:?}"))?;
    Ok(())
}

pub fn formal_release_default_seed_location_for_pos(pos: GeoPos) -> Option<Location> {
    let (_, model) = formal_release_default_seed_model().ok()?;
    let location_id = model
        .agents
        .values()
        .find(|agent| agent.pos == pos)
        .map(|agent| agent.location_id.clone())?;
    model.locations.get(&location_id).cloned()
}
