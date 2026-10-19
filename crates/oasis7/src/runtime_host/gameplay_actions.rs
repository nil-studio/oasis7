#[cfg(not(target_arch = "wasm32"))]
use crate::{
    geometry::GeoPos,
    runtime::{Action as RuntimeAction, RecipeExecutionPlan},
    simulator::{
        AgentSpawnConfig, AsteroidFragmentInitConfig, ChunkCoord, OriginLocationConfig,
        ResourceKind, ResourceStock, WorldConfig, WorldInitConfig, WorldModel, build_world_model,
    },
};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_proto::viewer::{GameplayActionError, GameplayActionRequest};
#[cfg(not(target_arch = "wasm32"))]
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack};

pub use oasis7_client_api::gameplay_actions::*;

// The formal/generated starter world must be able to pay the first canonical
// industrial bootstrap from one owner-held balance: 10 electricity for each
// starter factory, 12 iron-ingot batches at 8 electricity each, and 4 gear
// batches at 4 electricity each.
pub const STARTER_INDUSTRIAL_ELECTRICITY: i64 = 10 + (12 * 8) + 10 + (4 * 4);

#[cfg(not(target_arch = "wasm32"))]
const RECIPE_SMELTER_IRON_INGOT: &str = "recipe.smelter.iron_ingot";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_SMELTER_COPPER_WIRE: &str = "recipe.smelter.copper_wire";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_SMELTER_POLYMER_RESIN: &str = "recipe.smelter.polymer_resin";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_SMELTER_ALLOY_PLATE: &str = "recipe.smelter.alloy_plate";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_GEAR: &str = "recipe.assembler.gear";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_CONTROL_CHIP: &str = "recipe.assembler.control_chip";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_MOTOR_MK1: &str = "recipe.assembler.motor_mk1";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_LOGISTICS_DRONE: &str = "recipe.assembler.logistics_drone";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_SENSOR_PACK: &str = "recipe.assembler.sensor_pack";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_MODULE_RACK: &str = "recipe.assembler.module_rack";
#[cfg(not(target_arch = "wasm32"))]
const RECIPE_ASSEMBLER_FACTORY_CORE: &str = "recipe.assembler.factory_core";

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const FORMAL_RELEASE_DEFAULT_WORLD_SEED: u64 = 0x0A51_5700_0000_0001;
#[cfg(not(target_arch = "wasm32"))]
const FORMAL_RELEASE_DEFAULT_BOOTSTRAP_CHUNK: ChunkCoord = ChunkCoord { x: 0, y: 0, z: 0 };

#[cfg(not(target_arch = "wasm32"))]
pub fn build_runtime_action_from_gameplay_request(
    request: &GameplayActionRequest,
) -> Result<RuntimeAction, GameplayActionError> {
    let target_agent_id = request.target_agent_id.trim();
    if target_agent_id.is_empty() {
        return Err(GameplayActionError {
            code: "target_agent_required".to_string(),
            message: "gameplay_action requires non-empty target_agent_id".to_string(),
            action_id: Some(request.action_id.clone()),
            target_agent_id: Some(request.target_agent_id.clone()),
        });
    }

    let action = match request.action_id.as_str() {
        ACTION_CLAIM_FIRST_AGENT => Some(RuntimeAction::RegisterAgent {
            agent_id: require_first_agent_claim_target(request, target_agent_id)?,
            pos: formal_release_default_first_agent_spawn_pos().map_err(|message| {
                GameplayActionError {
                    code: "starter_spawn_location_missing".to_string(),
                    message,
                    action_id: Some(request.action_id.clone()),
                    target_agent_id: Some(request.target_agent_id.clone()),
                }
            })?,
        }),
        ACTION_CLAIM_AGENT => Some(RuntimeAction::ClaimAgent {
            claimer_agent_id: required_actor_agent_id(request)?,
            target_agent_id: target_agent_id.to_string(),
        }),
        ACTION_CLAIM_STARTER_OC => Some(RuntimeAction::ClaimStarterOc {
            agent_id: target_agent_id.to_string(),
            player_id: required_player_id(request)?,
            public_key: request
                .public_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        }),
        ACTION_RELEASE_AGENT_CLAIM => Some(RuntimeAction::ReleaseAgentClaim {
            claimer_agent_id: required_actor_agent_id(request)?,
            target_agent_id: target_agent_id.to_string(),
        }),
        ACTION_BUILD_SMELTER_MK1 => runtime_factory_build_action(
            target_agent_id,
            "site-smelter",
            FACTORY_SMELTER_MK1,
            FACTORY_SMELTER_MK1,
        ),
        ACTION_BUILD_ASSEMBLER_MK1 => runtime_factory_build_action(
            target_agent_id,
            "site-assembler",
            FACTORY_ASSEMBLER_MK1,
            FACTORY_ASSEMBLER_MK1,
        ),
        ACTION_SCHEDULE_SMELTER_IRON_INGOT => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_SMELTER_MK1,
            RECIPE_SMELTER_IRON_INGOT,
            12,
        ),
        ACTION_SCHEDULE_SMELTER_COPPER_WIRE => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_SMELTER_MK1,
            RECIPE_SMELTER_COPPER_WIRE,
            12,
        ),
        ACTION_SCHEDULE_SMELTER_POLYMER_RESIN => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_SMELTER_MK1,
            RECIPE_SMELTER_POLYMER_RESIN,
            4,
        ),
        ACTION_SCHEDULE_SMELTER_ALLOY_PLATE => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_SMELTER_MK1,
            RECIPE_SMELTER_ALLOY_PLATE,
            4,
        ),
        ACTION_SCHEDULE_ASSEMBLER_GEAR => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_GEAR,
            4,
        ),
        ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_CONTROL_CHIP,
            4,
        ),
        ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1 => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_MOTOR_MK1,
            2,
        ),
        ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_LOGISTICS_DRONE,
            1,
        ),
        ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_SENSOR_PACK,
            2,
        ),
        ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_MODULE_RACK,
            1,
        ),
        ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE => runtime_schedule_recipe_action(
            target_agent_id,
            FACTORY_ASSEMBLER_MK1,
            RECIPE_ASSEMBLER_FACTORY_CORE,
            1,
        ),
        _ => None,
    }
    .ok_or_else(|| GameplayActionError {
        code: "unknown_gameplay_action".to_string(),
        message: format!(
            "unknown gameplay action `{}` for target `{}`",
            request.action_id, request.target_agent_id
        ),
        action_id: Some(request.action_id.clone()),
        target_agent_id: Some(request.target_agent_id.clone()),
    })?;
    Ok(action)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn runtime_factory_build_action(
    builder_agent_id: &str,
    site_id: &str,
    factory_id: &str,
    factory_kind: &str,
) -> Option<RuntimeAction> {
    Some(RuntimeAction::BuildFactory {
        builder_agent_id: builder_agent_id.to_string(),
        site_id: site_id.to_string(),
        spec: factory_spec_for_kind(factory_kind, factory_id)?,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn runtime_schedule_recipe_action(
    requester_agent_id: &str,
    factory_id: &str,
    recipe_id: &str,
    accepted_batches: u32,
) -> Option<RuntimeAction> {
    Some(RuntimeAction::ScheduleRecipe {
        requester_agent_id: requester_agent_id.to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        plan: recipe_plan_for_id(recipe_id, accepted_batches)?,
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn required_actor_agent_id(request: &GameplayActionRequest) -> Result<String, GameplayActionError> {
    let actor_agent_id = request
        .actor_agent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GameplayActionError {
            code: "actor_agent_required".to_string(),
            message: format!(
                "gameplay_action `{}` requires non-empty actor_agent_id",
                request.action_id
            ),
            action_id: Some(request.action_id.clone()),
            target_agent_id: Some(request.target_agent_id.clone()),
        })?;
    Ok(actor_agent_id.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn required_player_id(request: &GameplayActionRequest) -> Result<String, GameplayActionError> {
    let player_id = request.player_id.trim();
    if player_id.is_empty() {
        return Err(GameplayActionError {
            code: "player_id_required".to_string(),
            message: format!(
                "gameplay_action `{}` requires non-empty player_id",
                request.action_id
            ),
            action_id: Some(request.action_id.clone()),
            target_agent_id: Some(request.target_agent_id.clone()),
        });
    }
    Ok(player_id.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn formal_release_default_seed_model() -> Result<(WorldConfig, WorldModel), String> {
    let config = WorldConfig::default();
    let mut starter_resources = ResourceStock::new();
    starter_resources
        .set(ResourceKind::Electricity, STARTER_INDUSTRIAL_ELECTRICITY)
        .map_err(|err| format!("formal release starter electricity stock failed: {err:?}"))?;
    starter_resources
        .set(ResourceKind::Data, 8)
        .map_err(|err| format!("formal release starter data stock failed: {err:?}"))?;

    let init = WorldInitConfig {
        seed: FORMAL_RELEASE_DEFAULT_WORLD_SEED,
        origin: OriginLocationConfig {
            enabled: false,
            ..OriginLocationConfig::default()
        },
        asteroid_fragment: AsteroidFragmentInitConfig {
            enabled: true,
            bootstrap_chunks: vec![FORMAL_RELEASE_DEFAULT_BOOTSTRAP_CHUNK],
            ..AsteroidFragmentInitConfig::default()
        },
        agents: AgentSpawnConfig {
            count: 1,
            id_prefix: "starter-agent-".to_string(),
            start_index: 0,
            resources: starter_resources,
        },
        ..WorldInitConfig::default()
    };

    let (model, _) = build_world_model(&config, &init)
        .map_err(|err| format!("formal release seed model build failed: {err:?}"))?;
    Ok((config, model))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn formal_release_default_first_agent_spawn_pos() -> Result<GeoPos, String> {
    let (_, model) = formal_release_default_seed_model()?;
    model
        .agents
        .get(FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
        .map(|agent| agent.pos)
        .ok_or_else(|| {
            format!(
                "formal release seed model missing bootstrap agent {}",
                FIRST_AGENT_CLAIM_TARGET_AGENT_ID
            )
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn require_first_agent_claim_target(
    request: &GameplayActionRequest,
    target_agent_id: &str,
) -> Result<String, GameplayActionError> {
    if target_agent_id != FIRST_AGENT_CLAIM_TARGET_AGENT_ID {
        return Err(GameplayActionError {
            code: "invalid_first_agent_claim_target".to_string(),
            message: format!(
                "gameplay_action `{}` must target {}",
                request.action_id, FIRST_AGENT_CLAIM_TARGET_AGENT_ID
            ),
            action_id: Some(request.action_id.clone()),
            target_agent_id: Some(request.target_agent_id.clone()),
        });
    }
    Ok(target_agent_id.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn factory_spec_for_kind(factory_kind: &str, factory_id: &str) -> Option<FactoryModuleSpec> {
    crate::runtime::builtin_factory_spec(factory_kind, factory_id)
}

#[cfg(not(target_arch = "wasm32"))]
fn recipe_plan_for_id(recipe_id: &str, accepted_batches: u32) -> Option<RecipeExecutionPlan> {
    crate::runtime::builtin_recipe_plan(recipe_id, accepted_batches)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn first_delivery_preview_inputs(action_id: &str) -> Option<Vec<MaterialStack>> {
    match action_id {
        ACTION_BUILD_ASSEMBLER_MK1 => {
            factory_spec_for_kind(FACTORY_ASSEMBLER_MK1, FACTORY_ASSEMBLER_MK1)
                .map(|spec| spec.build_cost)
        }
        ACTION_SCHEDULE_SMELTER_ALLOY_PLATE => {
            recipe_plan_for_id(RECIPE_SMELTER_ALLOY_PLATE, 1).map(|plan| plan.consume)
        }
        ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK => {
            recipe_plan_for_id(RECIPE_ASSEMBLER_MODULE_RACK, 1).map(|plan| plan.consume)
        }
        ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE => {
            recipe_plan_for_id(RECIPE_ASSEMBLER_FACTORY_CORE, 1).map(|plan| plan.consume)
        }
        _ => None,
    }
}
