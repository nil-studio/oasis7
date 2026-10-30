use super::decimal::strict_optional_map;
use super::*;
use oasis7_agent_api::world_quote as native;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack, RecipeExecutionPlan};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMaterialStackV1 {
    pub kind: String,
    pub amount: ResourceDecimalI64V1,
}
impl From<&MaterialStack> for ResourceMaterialStackV1 {
    fn from(v: &MaterialStack) -> Self {
        Self {
            kind: v.kind.clone(),
            amount: ResourceDecimalI64V1(v.amount),
        }
    }
}
impl From<ResourceMaterialStackV1> for MaterialStack {
    fn from(v: ResourceMaterialStackV1) -> Self {
        Self {
            kind: v.kind,
            amount: v.amount.0,
        }
    }
}
pub(crate) fn stacks(v: &[MaterialStack]) -> Vec<ResourceMaterialStackV1> {
    v.iter().map(Into::into).collect()
}
pub(crate) fn native_stacks(v: Vec<ResourceMaterialStackV1>) -> Vec<MaterialStack> {
    v.into_iter().map(Into::into).collect()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFactorySpecV1 {
    pub factory_id: String,
    pub display_name: String,
    pub tier: u8,
    pub tags: Vec<String>,
    pub build_cost: Vec<ResourceMaterialStackV1>,
    pub build_time_ticks: u32,
    pub base_power_draw: ResourceDecimalI64V1,
    pub recipe_slots: u16,
    pub throughput_bps: u32,
    pub maintenance_per_tick: ResourceDecimalI64V1,
}
impl From<&FactoryModuleSpec> for ResourceFactorySpecV1 {
    fn from(v: &FactoryModuleSpec) -> Self {
        Self {
            factory_id: v.factory_id.clone(),
            display_name: v.display_name.clone(),
            tier: v.tier,
            tags: v.tags.clone(),
            build_cost: stacks(&v.build_cost),
            build_time_ticks: v.build_time_ticks,
            base_power_draw: ResourceDecimalI64V1(v.base_power_draw),
            recipe_slots: v.recipe_slots,
            throughput_bps: v.throughput_bps,
            maintenance_per_tick: ResourceDecimalI64V1(v.maintenance_per_tick),
        }
    }
}
impl From<ResourceFactorySpecV1> for FactoryModuleSpec {
    fn from(v: ResourceFactorySpecV1) -> Self {
        Self {
            factory_id: v.factory_id,
            display_name: v.display_name,
            tier: v.tier,
            tags: v.tags,
            build_cost: native_stacks(v.build_cost),
            build_time_ticks: v.build_time_ticks,
            base_power_draw: v.base_power_draw.0,
            recipe_slots: v.recipe_slots,
            throughput_bps: v.throughput_bps,
            maintenance_per_tick: v.maintenance_per_tick.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRecipePlanV1 {
    pub accepted_batches: u32,
    pub consume: Vec<ResourceMaterialStackV1>,
    pub produce: Vec<ResourceMaterialStackV1>,
    pub byproducts: Vec<ResourceMaterialStackV1>,
    pub power_required: ResourceDecimalI64V1,
    pub duration_ticks: u32,
    #[serde(deserialize_with = "required_option")]
    pub reject_reason: Option<String>,
}
impl From<&RecipeExecutionPlan> for ResourceRecipePlanV1 {
    fn from(v: &RecipeExecutionPlan) -> Self {
        Self {
            accepted_batches: v.accepted_batches,
            consume: stacks(&v.consume),
            produce: stacks(&v.produce),
            byproducts: stacks(&v.byproducts),
            power_required: ResourceDecimalI64V1(v.power_required),
            duration_ticks: v.duration_ticks,
            reject_reason: v.reject_reason.clone(),
        }
    }
}
impl From<ResourceRecipePlanV1> for RecipeExecutionPlan {
    fn from(v: ResourceRecipePlanV1) -> Self {
        Self {
            accepted_batches: v.accepted_batches,
            consume: native_stacks(v.consume),
            produce: native_stacks(v.produce),
            byproducts: native_stacks(v.byproducts),
            power_required: v.power_required.0,
            duration_ticks: v.duration_ticks,
            reject_reason: v.reject_reason,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBuildPowerV1 {
    pub payer_agent_id: String,
    pub profile_key: String,
    pub profile_revision: DecimalU64,
    pub electricity_amount: ResourceDecimalI64V1,
    pub mode: native::FactoryConstructionPowerMode,
    #[serde(deserialize_with = "required_option")]
    pub material_ledger: Option<ResourceLedgerV1>,
    #[serde(deserialize_with = "required_option")]
    pub electricity_before: Option<ResourceDecimalI64V1>,
    #[serde(deserialize_with = "required_option")]
    pub electricity_after: Option<ResourceDecimalI64V1>,
    #[serde(deserialize_with = "strict_optional_map")]
    pub material_balances_before: Option<BTreeMap<String, ResourceDecimalI64V1>>,
    #[serde(deserialize_with = "strict_optional_map")]
    pub material_balances_after: Option<BTreeMap<String, ResourceDecimalI64V1>>,
}
impl From<&native::FactoryBuildPowerObligationV1> for ResourceBuildPowerV1 {
    fn from(v: &native::FactoryBuildPowerObligationV1) -> Self {
        Self {
            payer_agent_id: v.payer_agent_id.clone(),
            profile_key: v.profile_key.clone(),
            profile_revision: DecimalU64(v.profile_revision),
            electricity_amount: ResourceDecimalI64V1(v.electricity_amount),
            mode: v.mode,
            material_ledger: v.material_ledger.as_ref().map(Into::into),
            electricity_before: v.electricity_before.map(ResourceDecimalI64V1),
            electricity_after: v.electricity_after.map(ResourceDecimalI64V1),
            material_balances_before: v.material_balances_before.as_ref().map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), ResourceDecimalI64V1(*v)))
                    .collect()
            }),
            material_balances_after: v.material_balances_after.as_ref().map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), ResourceDecimalI64V1(*v)))
                    .collect()
            }),
        }
    }
}
impl From<ResourceBuildPowerV1> for native::FactoryBuildPowerObligationV1 {
    fn from(v: ResourceBuildPowerV1) -> Self {
        Self {
            payer_agent_id: v.payer_agent_id,
            profile_key: v.profile_key,
            profile_revision: v.profile_revision.0,
            electricity_amount: v.electricity_amount.0,
            mode: v.mode,
            material_ledger: v.material_ledger.map(Into::into),
            electricity_before: v.electricity_before.map(|v| v.0),
            electricity_after: v.electricity_after.map(|v| v.0),
            material_balances_before: v
                .material_balances_before
                .map(|m| m.into_iter().map(|(k, v)| (k, v.0)).collect()),
            material_balances_after: v
                .material_balances_after
                .map(|m| m.into_iter().map(|(k, v)| (k, v.0)).collect()),
        }
    }
}
