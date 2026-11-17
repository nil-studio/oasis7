//! Authenticated selected columns share the diff reader and its finite scan
//! budget. This grants no complete record or field qualification.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
pub(crate) struct PointedTypedEntryV1 {
    pub(crate) entry: StateEntryV1,
    pub(crate) semantic_bytes: u64,
}
pub(crate) fn point_typed_column_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    body: &StateRootV1,
    name: &str,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    max_heap: usize,
) -> Result<
    oasis7_proto::memory_budget::ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>,
    ValueTreeError,
> {
    let selected = point_typed_entry_v1(profile, body, name, d)?;
    let ctx = &mut *d.ctx;
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let row = crate::incremental_record_columns_read::read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        &selected.entry,
        &mut records,
        max_heap,
    )?;
    d.check()?;
    Ok(row)
}
/// Retain the original point workspace with the authenticated leaf. A caller
/// may stream a selected primitive without hydrating an unrelated DTO.
pub(crate) fn point_typed_entry_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized,
>(
    profile: &TypedRecordProfileV1,
    body: &StateRootV1,
    name: &K,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<ReservedValue<PointedTypedEntryV1>, ValueTreeError> {
    d.check()?;
    body.validate().map_err(|_| invalid())?;
    if body.namespace != profile.namespace()? {
        return Err(invalid());
    }
    let point = d.ctx.values.memory.reserve(8192, d.ctx.values.class)?;
    let (key, semantic_bytes) =
        d.ctx
            .records()
            .key(profile.field, WorldFieldRecordKindV1::Entry, name)?;
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
                    .ok_or(ValueTreeError::ResourceLimited("typed point leaves"))?;
                if entry.namespace != body.namespace
                    || entry.key != key.as_bytes()
                    || entry.key_digest().map_err(|_| invalid())? != digest
                {
                    return Err(invalid());
                }
                return Ok(point.retain(PointedTypedEntryV1 {
                    entry,
                    semantic_bytes,
                }));
            }
            _ => return Err(invalid()),
        }
    }
    Err(invalid())
}
