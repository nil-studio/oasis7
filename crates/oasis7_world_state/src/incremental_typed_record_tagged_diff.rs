//! Read only the fixed enum discriminator before dispatching its closed fields.
use super::*;

pub(crate) struct TaggedChoiceV1 {
    pub(crate) tag: u64,
    pub(crate) inherit_before: bool,
}

fn tag<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    cell: &TypedCellRefV1<'_>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    d.check()?;
    let body = root(profile, cell, d)?;
    let max_heap = d.work.max_tree_heap;
    let row = super::point::point_typed_column_v1(profile, &body, "tag", d, max_heap)?;
    match &row.cell {
        ColumnCellV1::U64(tag) => Ok(*tag),
        _ => Err(invalid()),
    }
}

pub(crate) fn tagged_choice_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<Option<TaggedChoiceV1>, ValueTreeError> {
    d.check()?;
    if unchanged(before, after) || after.is_none() {
        return Ok(None);
    }
    let after_tag = tag(profile, after.ok_or_else(invalid)?, d)?;
    let inherit_before = match before {
        Some(before) => tag(profile, before, d)? == after_tag,
        None => false,
    };
    Ok(Some(TaggedChoiceV1 {
        tag: after_tag,
        inherit_before,
    }))
}
