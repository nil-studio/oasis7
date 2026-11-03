use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if entry.namespace != field.namespace() {
        return Err(read::invalid());
    }
    let meta = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    if entry.key == meta.as_bytes() {
        return records.presence(&entry.value, &meta, 0);
    }
    for column in std::iter::once(None).chain(
        FixedEconomyColumnV1::ALL
            .into_iter()
            .filter(|column| column.field() == field)
            .map(Some),
    ) {
        let key = read::cell_key(field, column, records)?;
        if entry.key == key.0.as_bytes() {
            read::value(column, &entry.value, &key, records)?;
            return Ok(());
        }
    }
    Err(read::invalid())
}
fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != field.namespace() || after.namespace != field.namespace() {
        return Err(read::invalid());
    }
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "fixed economy diff",
            cells: "fixed economy changed cells",
            stack: "fixed economy diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let (Some(_), Some(new)) = (old, new) else {
                return Err(read::invalid());
            };
            changed(field, new, records)
        },
    )
}
/// Exact qualified parent plus independent generic replay. Neither alone is a
/// complete owner/World acceptance claim.
pub fn accept_fixed_economy_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
) -> Result<ReservedValue<FixedEconomyFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(read::invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| read::invalid())?)?;
    let verified = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    let verified = verified?;
    if !verified
        .record
        .after
        .state
        .iter()
        .any(|root| root == ctx.root)
    {
        return Err(read::invalid().into());
    }
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "fixed economy outer changed cells",
        ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(field) = FixedEconomyFieldV1::ALL
            .into_iter()
            .find(|field| field.name() == binding.name)
        else {
            continue;
        };
        let edge = |value: Option<[u8; 32]>, ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>| {
            value
                .map(|value| {
                    family::outer(
                        &StateEntryV1 {
                            namespace: write.namespace,
                            key: write.mutation.key.clone(),
                            value,
                        },
                        ctx,
                    )
                })
                .transpose()
                .map(Option::flatten)
        };
        let old = edge(write.mutation.before, ctx)?;
        let new = edge(write.mutation.after, ctx)?;
        match (old, new) {
            (Some((old_field, None)), Some((new_field, None)))
                if binding.kind == WorldFieldRecordKindV1::Metadata
                    && old_field == field
                    && new_field == field => {}
            (Some((old_field, Some(old))), Some((new_field, Some(new))))
                if old_field == field && new_field == field =>
            {
                child(field, &old, &new, &mut ctx.records(), cells)?
            }
            _ => return Err(read::invalid().into()),
        }
        ctx.check()?;
    }
    let result = family::mint(
        verified
            .record
            .after
            .digest()
            .map_err(|_| read::invalid())?,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
