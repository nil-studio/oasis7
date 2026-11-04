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
    let _point = d.ctx.values.memory.reserve(8192, d.ctx.values.class)?;
    let body = root(profile, cell, d)?;
    let (key, _) = d
        .ctx
        .records()
        .key(profile.field, WorldFieldRecordKindV1::Entry, "tag")?;
    let probe = StateEntryV1 {
        namespace: body.namespace,
        key: key.as_bytes().to_vec(),
        value: [0; 32],
    };
    let digest = probe.key_digest().map_err(|_| invalid())?;
    let mut current = Some(body.node_root.clone());
    for depth in 0..=256u16 {
        d.check()?;
        let page = d.page(current.as_deref(), depth)?.ok_or_else(invalid)?;
        match page {
            StatePageLinksV1::Branch { children, .. } if depth < 256 => {
                let bit = usize::from((digest[usize::from(depth) / 8] >> (7 - depth % 8)) & 1);
                current = children.into_iter().nth(bit).ok_or_else(invalid)?;
                if current.is_none() {
                    return Err(invalid());
                }
            }
            StatePageLinksV1::Value(entry) if depth == 256 => {
                d.scan.leaves_remaining = d
                    .scan
                    .leaves_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("typed enum tag leaves"))?;
                if entry.namespace != body.namespace
                    || entry.key != key.as_bytes()
                    || entry.key_digest().map_err(|_| invalid())? != digest
                {
                    return Err(invalid());
                }
                let selected = TypedCellRefV1::Entry(&entry);
                let row = row(profile, &selected, d)?;
                return match &row.cell {
                    ColumnCellV1::U64(tag) => Ok(*tag),
                    _ => Err(invalid()),
                };
            }
            _ => return Err(invalid()),
        }
    }
    Err(invalid())
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
