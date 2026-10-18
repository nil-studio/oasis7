//! Stable gameplay action identifiers consumed by clients and native hosts.
pub const ACTION_BUILD_SMELTER_MK1: &str = "build_factory_smelter_mk1";
pub const ACTION_BUILD_ASSEMBLER_MK1: &str = "build_factory_assembler_mk1";
pub const ACTION_CLAIM_AGENT: &str = "claim_agent";
pub const ACTION_CLAIM_FIRST_AGENT: &str = "claim_first_agent";
pub const ACTION_CLAIM_STARTER_OC: &str = "claim_starter_oc";
pub const ACTION_RELEASE_AGENT_CLAIM: &str = "release_agent_claim";
pub const ACTION_SCHEDULE_SMELTER_IRON_INGOT: &str = "schedule_recipe_smelter_iron_ingot";
pub const ACTION_SCHEDULE_SMELTER_COPPER_WIRE: &str = "schedule_recipe_smelter_copper_wire";
pub const ACTION_SCHEDULE_SMELTER_POLYMER_RESIN: &str = "schedule_recipe_smelter_polymer_resin";
pub const ACTION_SCHEDULE_SMELTER_ALLOY_PLATE: &str = "schedule_recipe_smelter_alloy_plate";
pub const ACTION_SCHEDULE_ASSEMBLER_GEAR: &str = "schedule_recipe_assembler_gear";
pub const ACTION_SCHEDULE_ASSEMBLER_CONTROL_CHIP: &str = "schedule_recipe_assembler_control_chip";
pub const ACTION_SCHEDULE_ASSEMBLER_MOTOR_MK1: &str = "schedule_recipe_assembler_motor_mk1";
pub const ACTION_SCHEDULE_ASSEMBLER_LOGISTICS_DRONE: &str =
    "schedule_recipe_assembler_logistics_drone";
pub const ACTION_SCHEDULE_ASSEMBLER_SENSOR_PACK: &str = "schedule_recipe_assembler_sensor_pack";
pub const ACTION_SCHEDULE_ASSEMBLER_MODULE_RACK: &str = "schedule_recipe_assembler_module_rack";
pub const ACTION_SCHEDULE_ASSEMBLER_FACTORY_CORE: &str = "schedule_recipe_assembler_factory_core";
pub const FACTORY_SMELTER_MK1: &str = "factory.smelter.mk1";
pub const FACTORY_ASSEMBLER_MK1: &str = "factory.assembler.mk1";

pub const FIRST_AGENT_CLAIM_TARGET_AGENT_ID: &str = "starter-agent-0";

pub fn gameplay_action_requires_actor_agent(action_id: &str) -> bool {
    matches!(action_id, ACTION_CLAIM_AGENT | ACTION_RELEASE_AGENT_CLAIM)
}
