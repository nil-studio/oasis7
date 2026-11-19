//! Full streaming qualification uses the same changed-path traversal and
//! closed typed dispatch, without constructing an owner DTO or collection.
use super::*;
pub(super) fn visit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    root: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    mut visitor: impl FnMut(
        &TypedCellRefV1<'_>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    let _root_charge = d.ctx.values.memory.reserve(512, d.ctx.values.class)?;
    let empty = StateRootV1::empty(profile.namespace()?).map_err(|_| invalid())?;
    let mut metadata = false;
    visit_entries(&empty, root, d, |old, new, d| {
        if old.is_some() {
            return Err(invalid());
        }
        let new = new.ok_or_else(invalid)?;
        let entry = new.entry()?;
        let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            if metadata || parsed.semantic_key.is_some() {
                return Err(invalid());
            }
            let expected =
                WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            if expected.as_bytes() != entry.key || entry.namespace != profile.namespace()? {
                return Err(invalid());
            }
            let ctx = &mut *d.ctx;
            crate::incremental_inventory_value_read::InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            }
            .presence(&entry.value, &expected, 0)?;
            metadata = true;
            return Ok(());
        }
        if parsed.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid());
        }
        visitor(new, d)
    })?;
    if !metadata {
        return Err(invalid());
    }
    d.check()
}
pub(super) fn fields<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    after: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    names: &'static [&'static str],
    mut visitor: impl FnMut(
        &str,
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    let root = root(profile, after, d)?;
    if root.leaf_count
        != u64::try_from(names.len())
            .map_err(|_| invalid())?
            .checked_add(1)
            .ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    visit(profile, &root, d, |new, d| {
        let name = key(profile, new, d)?;
        let ColumnKeyV1::Text(name) = &*name else {
            return Err(invalid());
        };
        if !names.contains(&name.as_str()) {
            return Err(invalid());
        }
        visitor(name, None, Some(new), d)
    })
}
pub(super) fn container<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    after: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    vector: bool,
    mut entry: impl FnMut(
        &ColumnKeyV1,
        Option<&TypedCellRefV1<'_>>,
        Option<&TypedCellRefV1<'_>>,
        &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    let root = root(profile, after, d)?;
    let length = root.leaf_count.checked_sub(2).ok_or_else(invalid)?;
    let mut count = false;
    visit(profile, &root, d, |new, d| {
        let name = key(profile, new, d)?;
        if matches!(&*name,ColumnKeyV1::Text(s)if s=="_length") {
            if count {
                return Err(invalid());
            }
            let row = row(profile, new, d)?;
            if !matches!(row.cell,ColumnCellV1::U64(n)if n==length) {
                return Err(invalid());
            }
            count = true;
            return Ok(());
        }
        if vector && !matches!(&*name,ColumnKeyV1::Ordinal(index)if *index<length) {
            return Err(invalid());
        }
        entry(&name, None, Some(new), d)
    })?;
    if !count {
        return Err(invalid());
    }
    d.check()
}
