//! Pending-only byte roots. The shared primitive cannot mint owner schemas.
use super::*;
use crate::incremental_typed_record_diff::{ChangeDecoder, TypedCellRefV1};
use oasis7_proto::memory_budget::MemoryClass;
pub(super) fn descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "pending_actions.bytes",
        shape: FieldShape::ByteMap,
        destination: FieldDestination::State(StateNamespaceV1::Scheduler),
    }
}
pub(super) fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.pending-actions.bytes.v1\0").as_bytes()
}
pub(super) fn put_original<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    p: &TypedRecordProfileV1,
    parent: &mut TypedTreeBuilderV1,
    key: &K,
    source: &[u8],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    if p.namespace()? != StateNamespaceV1::Scheduler {
        return Err(invalid().into());
    }
    let root = crate::incremental_byte_map::write_byte_map_v1(descriptor(), p, source, ctx, work)?;
    let owner = hash_semantic_key_v1(
        key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let _charge = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference =
        RecordRootRefV1::new(schema(), owner.digest, (*root).clone()).map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        p.field,
        WorldFieldRecordKindV1::Entry,
        key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    append(p, parent, &row.field_key, row.value_root, ctx, work)
}
pub(super) fn decode<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    cell: ColumnCellV1,
    d: &mut Decoder<'_, '_, M, R, E, C>,
) -> Result<Vec<u8>, ValueTreeError> {
    let ColumnCellV1::Root(root) = cell else {
        return Err(invalid());
    };
    d.reserve_slot()?;
    let captured = crate::incremental_byte_map::read_byte_map_v1(
        descriptor(),
        &root,
        d.ctx,
        d.scan,
        Some(d.work.max_tree_heap),
    )?
    .ok_or_else(invalid)?;
    d.hold(captured.charge)?;
    Ok(captured.value)
}
pub(super) fn accept_changed<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    p: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if crate::incremental_typed_record_diff::unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else {
        return Ok(());
    };
    let TypedCellRefV1::Entry(entry) = after else {
        return Err(invalid());
    };
    let row = read_column_row_v1(
        ColumnReadProfileV1 {
            field: p.field,
            schema: p.schema,
            groups: &[],
            reference_schema: p.reference_schema,
        },
        entry,
        &mut d.ctx.records(),
        d.work.max_tree_heap,
    )?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    d.trees_remaining = d
        .trees_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "pending bytes changed trees",
        ))?;
    crate::incremental_byte_map::read_byte_map_v1(descriptor(), root, d.ctx, d.scan, None)?;
    d.check()
}
