//! CAS publication through the original native codec; no whole-progress clone.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_typed_record_codec::{
    self as codec, Codec, Encoded, TypedRecordProfileV1, TypedRecordWriteWorkV1, TypedTreeBuilderV1,
};
use oasis7_formal_protocol::incremental_state::StatePageWriter;

fn check_expected<
    T,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<T>>,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.validate_guard(&expected.guard, live)?;
    read::check(ctx)?;
    if ctx.values.class != MemoryClass::Ordinary
        || !expected.uses_budget(ctx.values.memory)
        || !expected.memory.same_pool(ctx.values.memory)
        || !tx.uses_budget(ctx.values.memory)
    {
        return Err(invalid().into());
    }
    let parsed =
        WorldFieldKeyV1::from_bytes(expected.key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let actual = tx.read(parsed.namespace, expected.key.as_bytes())?;
    read::check(ctx)?;
    if actual != expected.value_root {
        return Err(invalid().into());
    }
    Ok(())
}
/// The caller owns business receipt conflict/replay checks. This primitive only
/// replaces the exact genuinely observed record; it cannot issue action approval.
#[allow(clippy::too_many_arguments)]
pub fn write_current_factory_recycle_receipt_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::FactoryRecycleReceiptV1>>,
    id: &str,
    after: &crate::FactoryRecycleReceiptV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check_expected(expected, tx, ctx, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        let _phase = write
            .memory
            .reserve(
                id.len().checked_add(32 << 10).ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut output = None;
        prepare_world_native_record_v1::<FactoryRecycleReceiptsFieldV1>(
            &id.to_owned(),
            after,
            write,
            records,
            work,
            &mut |row| {
                if output.is_some() {
                    return Err(invalid());
                }
                output = Some(row);
                Ok(())
            },
        )?;
        let row = output.ok_or_else(invalid)?;
        if row.mutation.key != expected.key.as_bytes()
            || row.namespace != FactoryRecycleReceiptsFieldV1::profile()?.namespace()?
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(row.namespace, expected.key.as_bytes(), row.mutation.after)?;
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

/// Replace the exact Current construction-receipt outer entry through its
/// original native typed codec. The caller supplies the genuine point-read
/// owner and remains responsible for the FactoryBuilt business decision.
#[allow(clippy::too_many_arguments)]
pub fn write_current_factory_construction_receipt_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<
        CurrentFactoryNativeRecordV1<oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1>,
    >,
    id: &str,
    after: &oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    records: &mut u64,
    work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check_expected(expected, tx, ctx, live)?;
        if !tx.uses_budget(write.memory)
            || !expected.memory.same_pool(write.memory)
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let _phase = write
            .memory
            .reserve(
                id.len().checked_add(32 << 10).ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let field = FactoryConstructionReceiptsFieldV1::descriptor()?;
        let (expected_key, _) = ctx.key(field, WorldFieldRecordKindV1::Entry, id)?;
        if expected.key.as_bytes() != expected_key.as_bytes() {
            return Err(invalid().into());
        }
        let key = id.to_owned();
        let mut output = None;
        prepare_world_native_record_v1::<FactoryConstructionReceiptsFieldV1>(
            &key,
            after,
            write,
            records,
            work,
            &mut |row| {
                if output.is_some() {
                    return Err(invalid());
                }
                output = Some(row);
                Ok(())
            },
        )?;
        let row = output.ok_or_else(invalid)?;
        let profile = FactoryConstructionReceiptsFieldV1::profile()?;
        if row.mutation.key != expected.key.as_bytes()
            || row.namespace != profile.namespace()?
            || row.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(row.namespace, expected.key.as_bytes(), row.mutation.after)?;
        read::check(ctx)?;
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let tail = tx.guard();
        tx.validate_guard(&tail, live)?;
        Ok(())
    })
}
fn value_root<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    profile: &TypedRecordProfileV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    work.json.cells_remaining =
        work.json
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory progress wrapper cells",
            ))?;
    let mut columns = codec::read_columns_v1(
        codec::ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        body,
        ctx,
        scan,
        work.max_tree_heap,
    )?;
    let codec::ColumnCellV1::Root(root) = codec::take_named(&mut columns, "value")? else {
        return Err(invalid());
    };
    // This keeps the original column-reader fee with the moved root String;
    // no universal Reserved extraction API or uncharged clone is introduced.
    let fee = columns.finish_parts()?;
    Ok(fee.retain(root))
}
#[allow(clippy::too_many_arguments)]
fn replace_cell<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    field: &str,
    after: Encoded<'_>,
    profile: &TypedRecordProfileV1,
    root: &mut TypedTreeBuilderV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let key = ctx.key(profile.field, WorldFieldRecordKindV1::Entry, field)?;
    let before = {
        let mut records = ctx.records();
        crate::incremental_record_tree::lookup(root.root(), &key.0, &mut records, profile.labels)?
    }
    .ok_or_else(invalid)?;
    // Encode exactly the original cell using the existing put owner. A small
    // temporary tree avoids duplicating scalar/tagged/root-reference grammar.
    let mut temporary = codec::new_tree(profile, write, work)?;
    codec::put(profile, &mut temporary, field, after, write, work)?;
    let next = {
        let mut cancelled = || (ctx.cancelled)() || (write.cancelled)();
        let mut values = oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &*write.sink,
            memory: write.memory,
            class: MemoryClass::Ordinary,
            io: &mut *write.io,
        };
        let mut records = InventoryValueReader {
            values: &mut values,
            key_work: &mut *write.key_work,
            semantic_bytes: write.limits.semantic_bytes,
            record_bytes: write.limits.record_bytes,
            cancelled: &mut cancelled,
        };
        crate::incremental_record_tree::lookup(
            temporary.root(),
            &key.0,
            &mut records,
            profile.labels,
        )?
    }
    .ok_or_else(invalid)?;
    codec::edit(
        profile,
        root,
        &key.0,
        (Some(before), Some(next)),
        ctx,
        write,
        work,
    )
}
enum ProgressUpdateV1 {
    Stage,
    TransitCompleted,
}

/// Changes only stage and stage_updated_at when the actual stage differs.
/// Quotes, milestone and both counters retain their original authenticated roots.
/// The stage decision belongs to the genuine lifecycle business consumer.
#[allow(clippy::too_many_arguments)]
pub fn change_current_factory_industry_stage_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::IndustryProgressState>>,
    stage: crate::events::IndustryStage,
    now: crate::WorldTime,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut TypedRecordReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    change_current_factory_industry_progress_v1(
        expected,
        stage,
        now,
        tx,
        ctx,
        write,
        scan,
        read_work,
        write_work,
        live,
        ProgressUpdateV1::Stage,
    )
}

/// Increment the actual authenticated transit counter with the original saturating rule.
/// Stage time changes only when the supplied business stage actually changes.
#[allow(clippy::too_many_arguments)]
pub fn increment_current_material_transit_progress_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::IndustryProgressState>>,
    stage: crate::events::IndustryStage,
    now: crate::WorldTime,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut TypedRecordReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    change_current_factory_industry_progress_v1(
        expected,
        stage,
        now,
        tx,
        ctx,
        write,
        scan,
        read_work,
        write_work,
        live,
        ProgressUpdateV1::TransitCompleted,
    )
}

#[allow(clippy::too_many_arguments)]
fn change_current_factory_industry_progress_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::IndustryProgressState>>,
    stage: crate::events::IndustryStage,
    now: crate::WorldTime,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut TypedRecordReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
    update: ProgressUpdateV1,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check_expected(expected, tx, ctx, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        let _phase = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let old = expected.value().ok_or_else(invalid)?;
        let transit_count = match update {
            ProgressUpdateV1::Stage => old.completed_material_transits,
            ProgressUpdateV1::TransitCompleted => old.completed_material_transits.saturating_add(1),
        };
        if old.stage == stage && old.completed_material_transits == transit_count {
            read::check(ctx)?;
            return Ok(false);
        }
        let profile = IndustryProgressFieldV1::profile()?;
        let body = expected.body.as_ref().ok_or_else(invalid)?;
        let value = value_root(body, &profile, ctx, scan, read_work)?;
        let mut progress = TypedTreeBuilderV1::from_root(&value, write.memory)?;
        if old.completed_material_transits != transit_count {
            let encoded = transit_count.encode(&profile, write, write_work)?;
            replace_cell(
                "completed_material_transits",
                encoded,
                &profile,
                &mut progress,
                ctx,
                write,
                write_work,
            )?;
        }
        if old.stage != stage {
            let encoded = stage.encode(&profile, write, write_work)?;
            replace_cell(
                "stage",
                encoded,
                &profile,
                &mut progress,
                ctx,
                write,
                write_work,
            )?;
            let encoded = now.encode(&profile, write, write_work)?;
            replace_cell(
                "stage_updated_at",
                encoded,
                &profile,
                &mut progress,
                ctx,
                write,
                write_work,
            )?;
        }
        let mut wrapper = TypedTreeBuilderV1::from_root(body, write.memory)?;
        replace_cell(
            "value",
            Encoded::Tree(progress),
            &profile,
            &mut wrapper,
            ctx,
            write,
            write_work,
        )?;
        let owner = crate::incremental_semantic_key::hash_semantic_key_v1(
            &(),
            write.memory,
            MemoryClass::Ordinary,
            write.key_work,
            &mut write.cancelled,
        )
        .map_err(MemberRecordError::Key)?;
        let reference = RecordRootRefV1::new(profile.schema, owner.digest, wrapper.root().clone())
            .map_err(|_| invalid())?;
        let row = write_member_record_v1(
            IndustryProgressFieldV1::descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&reference),
            write,
        )?;
        if row.field_key != expected.key {
            return Err(invalid().into());
        }
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            profile.namespace()?,
            expected.key.as_bytes(),
            Some(row.value_root),
        )?;
        read::check(ctx)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

#[path = "incremental_factory_schedule_native_write.rs"]
mod recipe_quotes;
pub use recipe_quotes::write_current_recipe_market_quotes_v1;
