//! Borrowed native WorldState genesis assembly. This emits provisional writes;
//! Runtime owns the delegated children/byte maps and the complete sealed World.
use crate::{WorldState, incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
pub use crate::{
    incremental_agent_authority_records::AgentAuthorityWorkBudgetV1,
    incremental_factory_authority_records::FactoryAuthorityWorkV1,
    incremental_factory_records::FactoryWorkBudgetV1,
    incremental_fixed_economy_records::FixedEconomyWorkV1,
    incremental_governance_records::GovernanceWorkBudgetV1,
    incremental_industry_jobs::IndustryWorkBudgetV1,
    incremental_logistics_records::LogisticsWorkBudgetV1,
    incremental_main_token_policy_records::MainTokenPolicyWorkV1,
    incremental_material_records::MaterialWorkBudgetV1,
    incremental_reward_budget_records::RewardRecordsWorkV1,
    incremental_typed_record_codec::TypedRecordWriteWorkV1,
    incremental_world_rule_composite::WorldRuleCompositeWriteWorkV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
/// All allowances are supplied by the owning factory. The flat/outer producers
/// taking a record counter share `records_remaining`; typed families retain
/// their explicit inner work. No value IO/key/composite budget is reset.
pub struct WorldStateGenesisWorkV1 {
    pub records_remaining: u64,
    pub factory: FactoryWorkBudgetV1,
    pub factory_authority: FactoryAuthorityWorkV1,
    pub material: MaterialWorkBudgetV1,
    pub logistics: LogisticsWorkBudgetV1,
    pub industry: IndustryWorkBudgetV1,
    pub agent_authority: AgentAuthorityWorkBudgetV1,
    pub governance: GovernanceWorkBudgetV1,
    pub fixed_economy: FixedEconomyWorkV1,
    pub main_token_policy: MainTokenPolicyWorkV1,
    pub rewards: RewardRecordsWorkV1,
    pub rule_composites: WorldRuleCompositeWriteWorkV1,
    /// One continuous typed allowance for all three remaining native groups.
    pub native: TypedRecordWriteWorkV1,
    pub rules_max_pending_node_bytes: usize,
    pub diplomacy_max_pending_node_bytes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldStateGenesisSummaryV1 {
    pub native_fields: u64,
    /// module_states (Runtime), agents/module_instances (Children), history root
    /// (ProofLedger) must be completed by the factory; this is no readiness token.
    pub delegated_fields: u8,
    pub writes: u64,
}
fn invalid() -> MemberRecordError {
    MemberRecordError::Invalid("closed WorldState native GEN coverage")
}
#[path = "incremental_world_state_genesis_coverage.rs"]
mod coverage;
#[path = "incremental_world_state_genesis_inventory.rs"]
mod inventory;
/// The sink consumes one charged write at a time and must discard its complete
/// provisional prefix on any error. Successful native coverage alone never
/// authorizes WorldReady; delegated roots and complete qualification are required.
pub fn prepare_world_state_genesis_v1<
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut WorldStateGenesisWorkV1,
    sink: &mut S,
) -> Result<WorldStateGenesisSummaryV1, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    coverage::validate()?;
    coverage::source_guard(source);
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let memory = ctx.memory;
    let mut coverage = coverage::Coverage::new();
    let mut emit = |row: ReservedValue<ExecutionWriteV1>| {
        if !row.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("WorldState native GEN write pool"));
        }
        coverage.write(&row)?;
        sink(row)
    };
    inventory::prepare(source, ctx, &mut work.records_remaining, &mut emit)?;
    crate::incremental_world_u64::prepare_world_u64_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_balances::prepare_world_balances_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_id_sets::prepare_world_id_sets_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_number_maps::prepare_world_number_maps_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_text_maps::prepare_world_text_maps_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_authorization_mapsets::prepare_authorization_mapsets_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_authenticated_nonces::prepare_authenticated_nonces_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_delivery_cursor::prepare_delivery_cursor_genesis_v1(
        &source.product_validation_delivery_cursor,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_installed_module_targets::prepare_installed_module_targets_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_module_marketplace::prepare_module_marketplace_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_factory_records::prepare_factory_records_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.factory,
        &mut emit,
    )?;
    crate::incremental_factory_authority_records::prepare_factory_authority_genesis_v1(
        source,
        ctx,
        &mut work.factory_authority,
        &mut emit,
    )?;
    crate::incremental_material_records::prepare_material_records_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.material,
        &mut emit,
    )?;
    crate::incremental_logistics_records::prepare_logistics_records_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.logistics,
        &mut emit,
    )?;
    crate::incremental_industry_jobs::prepare_industry_jobs_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.industry,
        &mut emit,
    )?;
    crate::incremental_agent_authority_records::prepare_agent_authority_records_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.agent_authority,
        &mut emit,
    )?;
    crate::incremental_governance_records::prepare_governance_records_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.governance,
        &mut emit,
    )?;
    crate::incremental_fixed_economy_records::prepare_fixed_economy_genesis_v1(
        source,
        ctx,
        &mut work.fixed_economy,
        &mut emit,
    )?;
    crate::incremental_main_token_policy_records::prepare_main_token_policy_genesis_v1(
        source,
        ctx,
        &mut work.main_token_policy,
        &mut emit,
    )?;
    crate::incremental_reward_budget_records::prepare_reward_records_genesis_v1(
        source,
        ctx,
        &mut work.rewards,
        &mut emit,
    )?;
    crate::incremental_world_native_industry::prepare_world_native_industry_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.native,
        &mut emit,
    )?;
    crate::incremental_world_native_governance::prepare_world_native_governance_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.native,
        &mut emit,
    )?;
    crate::incremental_world_native_gameplay::prepare_world_native_gameplay_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut work.native,
        &mut emit,
    )?;
    crate::incremental_diplomacy::prepare_diplomacy_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        work.diplomacy_max_pending_node_bytes,
        &mut emit,
    )?;
    // Exactly one optional world_rules parent Bool/metadata producer.
    crate::incremental_world_rule_effects::prepare_world_rule_effects_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_rule_organization::root::prepare_qualified_organization_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        work.rules_max_pending_node_bytes,
        &mut emit,
    )?;
    crate::incremental_world_rule_supply_root::prepare_supply_root_genesis_v1(
        source,
        ctx,
        &mut work.records_remaining,
        work.rules_max_pending_node_bytes,
        &mut emit,
    )?;
    crate::incremental_world_rule_lineages::prepare_world_rule_lineages_genesis_members_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_rule_shipments::prepare_world_rule_shipments_genesis_members_v1(
        source,
        ctx,
        &mut work.records_remaining,
        &mut emit,
    )?;
    crate::incremental_world_rule_receipts::prepare_world_rule_receipts_genesis_v1(
        source,
        ctx,
        &mut work.rule_composites,
        &mut emit,
    )?;
    crate::incremental_world_rule_continuity::prepare_world_rule_continuity_emergency_genesis_v1(
        source,
        ctx,
        &mut work.rule_composites,
        &mut emit,
    )?;
    crate::incremental_world_rule_industrial::prepare_world_rule_industrial_genesis_v1(
        source,
        ctx,
        &mut work.rule_composites,
        &mut emit,
    )?;
    drop(emit);
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    coverage.finish()
}

#[cfg(test)]
#[path = "incremental_world_state_genesis_tests.rs"]
mod tests;
