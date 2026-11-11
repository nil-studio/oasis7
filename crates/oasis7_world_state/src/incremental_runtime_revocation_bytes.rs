//! Original byte strings use the existing ByteMap/Bytes wire contract.
use super::*;
pub(super) fn descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_revocation_state.bytes",
        shape: FieldShape::ByteMap,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
pub(super) fn byte_schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-revocation.bytes.v1\0").as_bytes()
}
pub(super) fn reference_schema(key: &ColumnKeyV1) -> Option<[u8; 32]> {
    Some(match key {
        ColumnKeyV1::Text(name) if name == "stop_predecessor_canonical_cbor" => byte_schema(),
        _ => schema(),
    })
}
pub(super) fn put_original<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    parent: &mut TypedTreeBuilderV1,
    name: &str,
    source: &[u8],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let child = crate::incremental_byte_map::write_byte_map_v1(
        descriptor(),
        &profile(),
        source,
        ctx,
        work,
    )?;
    let owner = hash_semantic_key_v1(
        name,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let _reference_charge = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference = RecordRootRefV1::new(byte_schema(), owner.digest, (*child).clone())
        .map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        field(),
        WorldFieldRecordKindV1::Entry,
        name,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    crate::incremental_typed_record_codec::append(
        &profile(),
        parent,
        &row.field_key,
        row.value_root,
        ctx,
        work,
    )
}
use crate::incremental_byte_map::CapturedByteMapV1 as Captured;
fn read_bytes<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    source: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    capture: Option<usize>,
) -> Result<Option<Captured>, ValueTreeError> {
    crate::incremental_byte_map::read_byte_map_v1(descriptor(), root, source, scan, capture)
}
pub(super) fn recover<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    d: &mut Decoder<'_, '_, M, R, E, C>,
) -> Result<Vec<u8>, ValueTreeError> {
    d.reserve_slot()?;
    let Captured { value, charge } =
        read_bytes(root, d.ctx, d.scan, Some(d.work.max_tree_heap))?.ok_or_else(invalid)?;
    d.hold(charge)?;
    Ok(value)
}
pub(super) fn accept_changed<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
    after: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
    d: &mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if crate::incremental_typed_record_diff::unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else {
        return Ok(());
    };
    let crate::incremental_typed_record_diff::TypedCellRefV1::Entry(entry) = after else {
        return Err(invalid());
    };
    let row = crate::incremental_record_columns_read::read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        entry,
        &mut d.ctx.records(),
        d.work.max_tree_heap,
    )?;
    match &row.cell {
        ColumnCellV1::Unit => (),
        ColumnCellV1::Root(root) => {
            d.trees_remaining =
                d.trees_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "revocation bytes changed trees",
                    ))?;
            read_bytes(root, d.ctx, d.scan, None)?;
        }
        _ => return Err(invalid()),
    }
    d.check()
}
