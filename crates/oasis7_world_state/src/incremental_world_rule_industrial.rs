//! Original industrial work and substitution, without DTO serialization.
use crate::{
    WorldState,
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_columns_read::ColumnCellV1,
    incremental_typed_record_codec::*,
    incremental_world_rule_composite::{
        RecoveredWorldRuleCompositeV1, WorldRuleCompositeBranchV1, WorldRuleCompositeReadWorkV1,
        WorldRuleCompositeWriteWorkV1, prepare_branch_v1, recover_branch_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

#[path = "incremental_world_rule_industrial_dto.rs"]
mod dto;
#[path = "incremental_world_rule_industrial_enums.rs"]
mod enums;

#[cfg(test)]
#[path = "incremental_world_rule_industrial_tests.rs"]
mod tests;

/// Contributes only the two branches. The common parent marker is produced
/// once by the complete WorldState assembly, including for None rules.
pub fn prepare_world_rule_industrial_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut WorldRuleCompositeWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(source) = &source.world_rules {
        let crate::WorldRulesStateV1 {
            requests: _,
            governance_effects: _,
            organization_cases: _,
            organization_estate: _,
            supply_authorizations: _,
            supply_batches: _,
            supply_lineages: _,
            supply_shipments: _,
            organization_continuity: _,
            emergency_supply: _,
            agent_orders: _,
            industrial_work,
            substitution,
        } = source;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::IndustrialWork,
            industrial_work,
            ctx,
            work,
            sink,
        )?;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::Substitution,
            substitution,
            ctx,
            work,
            sink,
        )?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

pub fn recover_world_rule_industrial_work_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<RecoveredWorldRuleCompositeV1<oasis7_agent_api::world_action::IndustrialWorkStateV1>>,
    ValueTreeError,
> {
    recover_branch_v1(WorldRuleCompositeBranchV1::IndustrialWork, ctx, scan, work)
}

pub fn recover_world_rule_substitution_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<RecoveredWorldRuleCompositeV1<oasis7_agent_api::world_action::SubstitutionStateV1>>,
    ValueTreeError,
> {
    recover_branch_v1(WorldRuleCompositeBranchV1::Substitution, ctx, scan, work)
}
