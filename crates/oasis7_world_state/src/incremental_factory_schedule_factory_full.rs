//! One complete selected Factory, for the original starter snapshot tuple.
//! Every field is recovered from its real typed column; no unused defaults.
use super::*;
use crate::{
    FactoryProductionSnapshot, FactoryProductionState, FactoryProductionStatus, FactoryState,
    MaterialStack,
};
use oasis7_wasm_abi::FactoryModuleSpec;

fn signed<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<i64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match cell(
        id,
        FactoryCellV1::Column(column),
        ctx,
        work,
        &mut std::io::sink(),
    )? {
        FactoryCellReadV1::Value(FactoryValueV1::Signed(v)) => Ok(v),
        _ => Err(invalid().into()),
    }
}
fn optional_unsigned<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<Option<u64>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match cell(
        id,
        FactoryCellV1::Column(column),
        ctx,
        work,
        &mut std::io::sink(),
    )? {
        FactoryCellReadV1::Value(FactoryValueV1::Unsigned(v)) => Ok(Some(v)),
        FactoryCellReadV1::Value(FactoryValueV1::Unit) => Ok(None),
        _ => Err(invalid().into()),
    }
}
fn optional_text<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<Option<String>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut bytes = Vec::with_capacity(work.max_text_bytes);
    if bytes.capacity() != work.max_text_bytes {
        return Err(invalid().into());
    }
    match cell(id, FactoryCellV1::Column(column), ctx, work, &mut bytes)? {
        FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            work.charge(bytes.len())?;
            Ok(Some(String::from_utf8(bytes).map_err(|_| invalid())?))
        }
        FactoryCellReadV1::Value(FactoryValueV1::Unit) => Ok(None),
        _ => Err(invalid().into()),
    }
}
fn strings<M, R, E, C>(
    id: &str,
    which: FactoryCollectionV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<Vec<String>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let n = usize::try_from(unsigned(id, FactoryCellV1::Collection(which), ctx, work)?)
        .map_err(|_| invalid())?;
    if n > work.max_tags {
        return Err(
            ValueTreeError::ResourceLimited("factory schedule complete string list").into(),
        );
    }
    let mut values = Vec::with_capacity(n);
    if values.capacity() != n {
        return Err(invalid().into());
    }
    for i in 0..n {
        values.push(text(
            id,
            FactoryCellV1::TextItem(which, i as u64),
            ctx,
            work,
        )?);
    }
    Ok(values)
}
fn stacks<M, R, E, C>(
    id: &str,
    which: FactoryCollectionV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
) -> Result<Vec<MaterialStack>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let n = usize::try_from(unsigned(id, FactoryCellV1::Collection(which), ctx, work)?)
        .map_err(|_| invalid())?;
    if n > work.max_stacks {
        return Err(ValueTreeError::ResourceLimited("factory schedule complete stacks").into());
    }
    let mut values = Vec::with_capacity(n);
    if values.capacity() != n {
        return Err(invalid().into());
    }
    for i in 0..n {
        let kind = text(
            id,
            FactoryCellV1::StackItem(which, i as u64, FactoryStackColumnV1::Kind),
            ctx,
            work,
        )?;
        let amount = match cell(
            id,
            FactoryCellV1::StackItem(which, i as u64, FactoryStackColumnV1::Amount),
            ctx,
            work,
            &mut std::io::sink(),
        )? {
            FactoryCellReadV1::Value(FactoryValueV1::Signed(v)) => v,
            _ => return Err(invalid().into()),
        };
        values.push(MaterialStack { kind, amount });
    }
    Ok(values)
}

#[allow(clippy::too_many_arguments)]
pub fn recover_current_schedule_complete_factory_v1<M, R, E, C>(
    id: &str,
    parent: &ReservedValue<FactoryFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleFactoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactoryState>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use FactoryCellV1::Column as Col;
    use FactoryColumnV1 as F;
    ctx.transaction.validate_guard(guard, live)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&ctx.transaction.parent_fence().parent_roots)?;
    match cell(id, FactoryCellV1::Record, ctx, work, &mut std::io::sink())? {
        FactoryCellReadV1::RecordAbsent => return Ok(None),
        FactoryCellReadV1::Value(FactoryValueV1::Unit) => {}
        _ => return Err(invalid().into()),
    }
    // Sixteen scalar/optional text slots; four String lists (Spec and snapshot
    // tags/routes/paths); four MaterialStack lists (Spec + snapshot triples).
    let string_slots = 16usize
        .checked_add(work.max_tags.checked_mul(4).ok_or_else(invalid)?)
        .and_then(|n| n.checked_add(work.max_stacks.checked_mul(4)?))
        .ok_or_else(invalid)?;
    let bytes = size_of::<FactoryState>()
        .checked_add(
            string_slots
                .checked_mul(work.max_text_bytes)
                .ok_or_else(invalid)?,
        )
        .and_then(|n| n.checked_add(work.max_tags.checked_mul(4 * size_of::<String>())?))
        .and_then(|n| {
            n.checked_add(
                work.max_stacks
                    .checked_mul(4 * size_of::<MaterialStack>())?,
            )
        })
        .ok_or_else(invalid)?;
    let fee = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let factory_id = text(id, Col(F::FactoryFactoryId), ctx, work)?;
    let site_id = text(id, Col(F::FactorySiteId), ctx, work)?;
    let builder_agent_id = text(id, Col(F::FactoryBuilderAgentId), ctx, work)?;
    let spec = FactoryModuleSpec {
        factory_id: text(id, Col(F::SpecFactoryId), ctx, work)?,
        display_name: text(id, Col(F::SpecDisplayName), ctx, work)?,
        tier: u8::try_from(unsigned(id, Col(F::SpecTier), ctx, work)?).map_err(|_| invalid())?,
        tags: strings(id, FactoryCollectionV1::SpecTags, ctx, work)?,
        build_cost: stacks(id, FactoryCollectionV1::SpecBuildCost, ctx, work)?,
        build_time_ticks: u32::try_from(unsigned(id, Col(F::SpecBuildTimeTicks), ctx, work)?)
            .map_err(|_| invalid())?,
        base_power_draw: signed(id, F::SpecBasePowerDraw, ctx, work)?,
        recipe_slots: u16::try_from(unsigned(id, Col(F::SpecRecipeSlots), ctx, work)?)
            .map_err(|_| invalid())?,
        throughput_bps: u32::try_from(unsigned(id, Col(F::SpecThroughputBps), ctx, work)?)
            .map_err(|_| invalid())?,
        maintenance_per_tick: signed(id, F::SpecMaintenancePerTick, ctx, work)?,
    };
    let input_ledger = ledger(id, F::FactoryInputLedger, ctx, work)?;
    let output_ledger = ledger(id, F::FactoryOutputLedger, ctx, work)?;
    let durability_ppm = signed(id, F::FactoryDurabilityPpm, ctx, work)?;
    let status = match unsigned(id, Col(F::ProductionStatus), ctx, work)? {
        0 => FactoryProductionStatus::Idle,
        1 => FactoryProductionStatus::Running,
        2 => FactoryProductionStatus::Blocked,
        3 => FactoryProductionStatus::Paused,
        _ => return Err(invalid().into()),
    };
    let active_jobs = u16::try_from(unsigned(id, Col(F::ProductionActiveJobs), ctx, work)?)
        .map_err(|_| invalid())?;
    let current_job_id = optional_unsigned(id, F::ProductionCurrentJobId, ctx, work)?;
    let current_recipe_id = optional_text(id, F::ProductionCurrentRecipeId, ctx, work)?;
    let last_started_at = optional_unsigned(id, F::ProductionLastStartedAt, ctx, work)?;
    let last_completed_at = optional_unsigned(id, F::ProductionLastCompletedAt, ctx, work)?;
    let last_blocked_at = optional_unsigned(id, F::ProductionLastBlockedAt, ctx, work)?;
    let last_resumed_at = optional_unsigned(id, F::ProductionLastResumedAt, ctx, work)?;
    let current_blocker_kind = optional_text(id, F::ProductionCurrentBlockerKind, ctx, work)?;
    let current_blocker_detail = optional_text(id, F::ProductionCurrentBlockerDetail, ctx, work)?;
    let current_blocker_action_id =
        optional_unsigned(id, F::ProductionCurrentBlockerActionId, ctx, work)?;
    let completed_jobs = unsigned(id, Col(F::ProductionCompletedJobs), ctx, work)?;
    let last_completed_recipe_id =
        optional_text(id, F::ProductionLastCompletedRecipeId, ctx, work)?;
    let same_recipe_repeat_count = u32::try_from(unsigned(
        id,
        Col(F::ProductionSameRecipeRepeatCount),
        ctx,
        work,
    )?)
    .map_err(|_| invalid())?;
    let present = match cell(
        id,
        Col(F::ProductionSnapshotPresent),
        ctx,
        work,
        &mut std::io::sink(),
    )? {
        FactoryCellReadV1::Value(FactoryValueV1::Bool(v)) => v,
        _ => return Err(invalid().into()),
    };
    let last_completed_canonical_snapshot = if present {
        Some(FactoryProductionSnapshot {
            recipe_id: text(id, Col(F::SnapshotRecipeId), ctx, work)?,
            consume: stacks(id, FactoryCollectionV1::SnapshotConsume, ctx, work)?,
            produce: stacks(id, FactoryCollectionV1::SnapshotProduce, ctx, work)?,
            byproducts: stacks(id, FactoryCollectionV1::SnapshotByproducts, ctx, work)?,
            consume_ledger: ledger(id, F::SnapshotConsumeLedger, ctx, work)?,
            power_required: signed(id, F::SnapshotPowerRequired, ctx, work)?,
            output_ledger: ledger(id, F::SnapshotOutputLedger, ctx, work)?,
            bottleneck_tags: strings(id, FactoryCollectionV1::SnapshotBottleneckTags, ctx, work)?,
            logistics_route_ids: strings(
                id,
                FactoryCollectionV1::SnapshotLogisticsRouteIds,
                ctx,
                work,
            )?,
            logistics_path_ids: strings(
                id,
                FactoryCollectionV1::SnapshotLogisticsPathIds,
                ctx,
                work,
            )?,
        })
    } else {
        None
    };
    let production = FactoryProductionState {
        status,
        active_jobs,
        current_job_id,
        current_recipe_id,
        last_started_at,
        last_completed_at,
        last_blocked_at,
        last_resumed_at,
        current_blocker_kind,
        current_blocker_detail,
        current_blocker_action_id,
        completed_jobs,
        last_completed_recipe_id,
        same_recipe_repeat_count,
        last_completed_canonical_snapshot,
    };
    let site_authority_revision =
        optional_unsigned(id, F::FactorySiteAuthorityRevision, ctx, work)?;
    let site_location_id = optional_text(id, F::FactorySiteLocationId, ctx, work)?;
    let location_anchor_revision =
        optional_unsigned(id, F::FactoryLocationAnchorRevision, ctx, work)?;
    let construction_power_profile_key =
        optional_text(id, F::FactoryConstructionPowerProfileKey, ctx, work)?;
    let construction_power_profile_revision =
        optional_unsigned(id, F::FactoryConstructionPowerProfileRevision, ctx, work)?;
    let built_at = unsigned(id, Col(F::FactoryBuiltAt), ctx, work)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(Some(fee.retain(FactoryState {
        factory_id,
        site_id,
        builder_agent_id,
        spec,
        input_ledger,
        output_ledger,
        durability_ppm,
        production,
        site_authority_revision,
        site_location_id,
        location_anchor_revision,
        construction_power_profile_key,
        construction_power_profile_revision,
        built_at,
    })))
}
