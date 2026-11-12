//! Original organization continuity and emergency supply, without DTO serialization.
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

#[path = "incremental_world_rule_continuity_dto.rs"]
mod dto;
#[path = "incremental_world_rule_emergency_dto.rs"]
mod emergency_dto;
#[path = "incremental_world_rule_continuity_enums.rs"]
mod enums;

/// Contributes only the two branches. The common parent marker is produced
/// once by the complete WorldState assembly, including for None rules.
pub fn prepare_world_rule_continuity_emergency_genesis_v1<
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
            organization_continuity,
            emergency_supply,
            agent_orders: _,
            industrial_work: _,
            substitution: _,
        } = source;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::OrganizationContinuity,
            organization_continuity,
            ctx,
            work,
            sink,
        )?;
        prepare_branch_v1(
            WorldRuleCompositeBranchV1::EmergencySupply,
            emergency_supply,
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

pub fn recover_world_rule_continuity_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<RecoveredWorldRuleCompositeV1<crate::world_rules::OrganizationContinuityStateV1>>,
    ValueTreeError,
> {
    recover_branch_v1(
        WorldRuleCompositeBranchV1::OrganizationContinuity,
        ctx,
        scan,
        work,
    )
}

pub fn recover_world_rule_emergency_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<
    Option<RecoveredWorldRuleCompositeV1<crate::world_rules::EmergencySupplyStateV1>>,
    ValueTreeError,
> {
    recover_branch_v1(WorldRuleCompositeBranchV1::EmergencySupply, ctx, scan, work)
}

#[cfg(test)]
#[path = "incremental_world_rule_continuity_tests.rs"]
mod tests;

#[path = "incremental_world_rule_continuity_family.rs"]
mod family;
pub use family::*;
#[path = "incremental_world_rule_continuity_overlay.rs"]
mod overlay;
pub use overlay::*;

#[path = "incremental_world_rule_continuity_current.rs"]
mod current;
pub use current::{CurrentOrganizationContinuityV1, read_current_organization_continuity_v1};

#[path = "incremental_world_rule_continuity_membership.rs"]
mod membership;
pub use membership::{
    ContinuityMembershipErrorV1, ContinuityMembershipKindV1, CurrentOrganizationMembershipV1,
    PreparedContinuityMembershipV1, apply_prepared_continuity_membership_v1,
    prepare_current_continuity_membership_v1, read_current_organization_membership_v1,
};

pub use membership::{
    CurrentContinuitySettlementRowV1, PreparedContinuitySettlementEffectV1,
    apply_prepared_continuity_settlement_v1, prepare_continuity_settlement_pause_v1,
    prepare_continuity_settlement_receipt_v1, read_current_continuity_settlement_row_v1,
};
