//! Original rule receipts and agent orders, without DTO serialization.
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

#[path = "incremental_world_rule_receipts_dto.rs"]
mod dto;
#[path = "incremental_world_rule_receipts_enums.rs"]
mod enums;

#[path = "incremental_world_rule_receipts_keys.rs"]
mod keys;

/// Contributes only the two branches. The common parent marker is produced
/// once by the complete WorldState assembly, including for None rules.
pub fn prepare_world_rule_receipts_genesis_v1<
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
            requests,
            governance_effects: _,
            organization_cases: _,
            organization_estate: _,
            supply_authorizations: _,
            supply_batches: _,
            supply_lineages: _,
            supply_shipments: _,
            organization_continuity: _,
            emergency_supply: _,
            agent_orders,
            industrial_work: _,
            substitution: _,
        } = source;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::Requests,
            requests,
            ctx,
            work,
            sink,
        )?;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::AgentOrders,
            agent_orders,
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

pub fn recover_world_rule_requests_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<
        RecoveredWorldRuleCompositeV1<
            std::collections::BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>,
        >,
    >,
    ValueTreeError,
> {
    recover_branch_v1(WorldRuleCompositeBranchV1::Requests, ctx, scan, work)
}

pub fn recover_world_rule_agent_orders_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<
        RecoveredWorldRuleCompositeV1<
            std::collections::BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>,
        >,
    >,
    ValueTreeError,
> {
    recover_branch_v1(WorldRuleCompositeBranchV1::AgentOrders, ctx, scan, work)
}

#[cfg(test)]
#[path = "incremental_world_rule_receipts_tests.rs"]
mod tests;

impl crate::incremental_world_rule_composite_family::CompositeBranchCodecV1
    for std::collections::BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>
{
    const BRANCH: WorldRuleCompositeBranchV1 = WorldRuleCompositeBranchV1::Requests;
}
impl crate::incremental_world_rule_composite_family::CompositeBranchCodecV1
    for std::collections::BTreeMap<String, crate::world_rules::AgentIndustrialOrderV1>
{
    const BRANCH: WorldRuleCompositeBranchV1 = WorldRuleCompositeBranchV1::AgentOrders;
}

#[path = "incremental_world_rule_receipts_family.rs"]
mod family;
pub use family::*;
#[path = "incremental_world_rule_receipts_edits.rs"]
mod edits;
pub use edits::{AgentOrderEditV1, WorldRuleRequestEditV1};
#[path = "incremental_world_rule_receipts_overlay.rs"]
mod overlay;
pub use overlay::*;
