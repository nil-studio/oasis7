use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
pub(super) fn scalar<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: &str,
    column: FactoryColumnV1,
    work: &mut FactoryLifecycleIndexWorkV1,
    output: &mut impl std::io::Write,
) -> Result<FactoryValueV1, WorldFieldOverlayErrorV1> {
    work.cell()?;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let max = match column {
        FactoryColumnV1::ProductionLastCompletedRecipeId | FactoryColumnV1::SnapshotRecipeId => {
            work.max_text_bytes as u64
        }
        _ => 8,
    };
    match read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(column),
        &mut overlay,
        max,
        output,
    )? {
        FactoryCellReadV1::Value(v) => Ok(v),
        _ => Err(invalid().into()),
    }
}
fn text<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: &str,
    column: FactoryColumnV1,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<Option<ReservedValue<String>>, WorldFieldOverlayErrorV1> {
    let n = work.max_text_bytes;
    let fee = ctx
        .values
        .memory
        .reserve(
            n.checked_add(128).ok_or_else(invalid)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(n);
    match scalar(tx, ctx, id, column, work, &mut bytes)? {
        FactoryValueV1::Unit => Ok(None),
        FactoryValueV1::Text { utf8_bytes }
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            Ok(Some(
                fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
            ))
        }
        _ => Err(invalid().into()),
    }
}
fn number<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: &str,
    column: FactoryColumnV1,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<u64, WorldFieldOverlayErrorV1> {
    match scalar(tx, ctx, id, column, work, &mut std::io::sink())? {
        FactoryValueV1::Unsigned(n) => Ok(n),
        _ => Err(invalid().into()),
    }
}
pub(super) fn factory<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: &str,
    after: &FactoryOuterRecordRootV1,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<HeldFactory, WorldFieldOverlayErrorV1> {
    if id.len() > work.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("factory lifecycle Current ID").into());
    }
    let fee = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<ReservedValue<FactoryProjection>>() + 128 + id.len(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let completed = number(tx, ctx, id, FactoryColumnV1::ProductionCompletedJobs, work)?;
    let repeat = number(
        tx,
        ctx,
        id,
        FactoryColumnV1::ProductionSameRecipeRepeatCount,
        work,
    )?;
    let last = text(
        tx,
        ctx,
        id,
        FactoryColumnV1::ProductionLastCompletedRecipeId,
        work,
    )?;
    let present = match scalar(
        tx,
        ctx,
        id,
        FactoryColumnV1::ProductionSnapshotPresent,
        work,
        &mut std::io::sink(),
    )? {
        FactoryValueV1::Bool(v) => v,
        _ => return Err(invalid().into()),
    };
    let snapshot = if present {
        text(tx, ctx, id, FactoryColumnV1::SnapshotRecipeId, work)?
    } else {
        None
    };
    let stable = crate::state::industry_transition::factory_has_canonical_stable_line_parts_v1(
        repeat,
        snapshot.as_ref().map(|s| s.as_str()),
        last.as_ref().map(|s| s.as_str()),
    );
    ctx.check()?;
    Ok(Arc::new(fee.retain(FactoryProjection {
        id: id.to_owned(),
        value: after.value_root,
        child: after.record_root.clone(),
        completed,
        stable,
    })))
}
pub(super) fn job_factory<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: u64,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<ReservedValue<String>, WorldFieldOverlayErrorV1> {
    work.cell()?;
    let n = work.max_text_bytes;
    let fee = ctx
        .values
        .memory
        .reserve(
            n.checked_add(128).ok_or_else(invalid)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(n);
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let actual = read_industry_cell_overlay_v1(
        IndustryRecordKeyV1 {
            kind: IndustryRecordKindV1::Recipe,
            id,
        },
        IndustryCellV1::Column(IndustryColumnV1::RecordFactoryId),
        &mut overlay,
        n as u64,
        &mut bytes,
    )?;
    let IndustryCellReadV1::Value(IndustryValueV1::Text { utf8_bytes }) = actual else {
        return Err(invalid().into());
    };
    if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
        return Err(invalid().into());
    }
    Ok(fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?))
}
pub(super) fn job<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    id: u64,
    after: &IndustryOuterRecordRootV1,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<HeldJob, WorldFieldOverlayErrorV1> {
    let text = job_factory(tx, ctx, id, work)?;
    let fee = ctx
        .values
        .memory
        .reserve(
            std::mem::size_of::<ReservedValue<JobProjection>>() + 128 + text.len(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    ctx.check()?;
    Ok(Arc::new(fee.retain(JobProjection {
        id,
        value: after.value_root,
        child: after.record_root.clone(),
        factory: text.to_string(),
    })))
}
pub(super) fn recheck_job<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &HeldJob,
    target: &str,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut FactoryLifecycleIndexWorkV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let actual = {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        read_industry_record_root_overlay_v1(
            IndustryRecordKeyV1 {
                kind: IndustryRecordKindV1::Recipe,
                id: row.id,
            },
            &mut overlay,
        )?
    };
    if actual != Some(row.value) {
        return Err(invalid().into());
    }
    let factory = job_factory(tx, ctx, row.id, work)?;
    if !work.comparison(&factory, target)?.is_eq() {
        return Err(invalid().into());
    }
    ctx.check()?;
    Ok(())
}
