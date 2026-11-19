//! Sparse original IndustryProgress.latest_market_quotes insertion.
//! Parent of this private child is factory_lifecycle_native_write: all root,
//! wrapper, CAS and typed-codec helpers are reused rather than mirrored.
use super::*;

#[allow(clippy::too_many_arguments)]
pub fn write_current_recipe_market_quotes_v1<M, P, R, E, C, W, CW>(
    expected: &ReservedValue<CurrentFactoryNativeRecordV1<crate::IndustryProgressState>>,
    quotes: &[crate::events::MaterialMarketQuote],
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut TypedRecordReadWorkV1,
    write_work: &mut TypedRecordWriteWorkV1,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
{
    tx.contribute(|tx| {
        check_expected(expected, tx, ctx, live)?;
        if !tx.uses_budget(write.memory) || !expected.memory.same_pool(write.memory) {
            return Err(invalid().into());
        }
        if quotes.is_empty() {
            read::check(ctx)?;
            return Ok(false);
        }
        let _phase = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let old = expected.value().ok_or_else(invalid)?;
        let profile = IndustryProgressFieldV1::profile()?;
        let body = expected.body.as_ref().ok_or_else(invalid)?;
        let value = value_root(body, &profile, ctx, scan, read_work)?;
        let mut columns = codec::read_columns_v1(
            codec::ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            &value,
            ctx,
            scan,
            read_work.max_tree_heap,
        )?;
        let codec::ColumnCellV1::Root(map_root) =
            codec::take_named(&mut columns, "latest_market_quotes")?
        else {
            return Err(invalid().into());
        };
        // The source owner already ran the complete original IndustryProgress
        // codec. Still consume exactly its five other named cells and reject
        // any extra rather than treating an unknown struct field as unchanged.
        for field in [
            "stage",
            "stage_updated_at",
            "completed_recipe_jobs",
            "completed_material_transits",
            "starter_industrial_milestone",
        ] {
            drop(codec::take_named(&mut columns, field)?);
        }
        let map_fee = columns.finish_parts()?;
        let map_root = map_fee.retain(map_root);
        let mut map = TypedTreeBuilderV1::from_root(&map_root, write.memory)?;
        let mut length = u64::try_from(old.latest_market_quotes.len()).map_err(|_| invalid())?;
        for quote in quotes {
            read::check(ctx)?;
            if (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            read_work.json.cells_remaining = read_work
                .json
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("recipe quote update cells"))?;
            let entry = ("entry", quote.kind.as_str());
            let key = ctx.key(profile.field, WorldFieldRecordKindV1::Entry, &entry)?;
            let before = {
                let mut records = ctx.records();
                crate::incremental_record_tree::lookup(
                    map.root(),
                    &key.0,
                    &mut records,
                    profile.labels,
                )?
            };
            let encoded = quote.encode(&profile, write, write_work)?;
            let mut temporary = codec::new_tree(&profile, write, write_work)?;
            codec::put(&profile, &mut temporary, &entry, encoded, write, write_work)?;
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
                &profile,
                &mut map,
                &key.0,
                (before, Some(next)),
                ctx,
                write,
                write_work,
            )?;
            if before.is_none() {
                length = length
                    .checked_add(1)
                    .ok_or(ValueTreeError::ResourceLimited("recipe quote count"))?;
            }
        }
        if length != u64::try_from(old.latest_market_quotes.len()).map_err(|_| invalid())? {
            let encoded = length.encode(&profile, write, write_work)?;
            replace_cell(
                "_length", encoded, &profile, &mut map, ctx, write, write_work,
            )?;
        }
        let mut progress = TypedTreeBuilderV1::from_root(&value, write.memory)?;
        replace_cell(
            "latest_market_quotes",
            Encoded::Tree(map),
            &profile,
            &mut progress,
            ctx,
            write,
            write_work,
        )?;
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
