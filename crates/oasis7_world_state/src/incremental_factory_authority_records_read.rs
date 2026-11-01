//! Exact authenticated edges. Inspection alone is not a closed record proof.
use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as FactoryAuthorityValueV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory authority binding/profile")
}
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    cell: FactoryAuthorityCellV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if !cell.accepts(kind) {
        return Err(invalid());
    }
    with_cell_key!(cell, key => records.key(kind.descriptor()?, WorldFieldRecordKindV1::Entry, key))
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: FactoryAuthorityCellV1,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<FactoryAuthorityValueV1, ValueTreeError> {
    read_record_value_v1(
        root,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec: cell.codec(),
            nullable: false,
            integer_max: cell.integer_max(),
            labels: RecordReadLabelsV1 {
                utf8: "factory authority UTF-8",
                ledger_utf8: "factory authority ledger UTF-8",
                ledger_frame: "factory authority ledger",
                boolean: "factory authority bool",
                fixed: "factory authority fixed",
                profile: "factory authority profile",
                integer: "factory authority integer",
            },
        },
    )
}
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryAuthorityRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.id,
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let mut sink = std::io::sink();
    let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut sink);
    let result = read_member_record_profile_v1(
        root,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: records.record_bytes,
            },
        },
        &mut probe,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    );
    if let Some(error) = probe.error() {
        return Err(ValueTreeError::Invalid(error));
    }
    result?;
    probe.finish()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &kind.schema(),
            &binding.semantic_key.ok_or_else(invalid)?,
            kind.namespace(),
        )
        .map_err(|_| invalid())?
        .clone())
}
/// Checks metadata, format marker and one exact String-keyed outer edge only.
pub fn inspect_factory_authority_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryAuthorityRecordKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind.namespace() {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let marker = ctx.records().key(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(genesis::FORMAT_KEY, ()),
    )?;
    let outer_key = outer_key(key, &mut ctx.records())?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    let marker_root = ctx.lookup(&marker.0)?.ok_or_else(invalid)?;
    let root = ctx.lookup(&outer_key.0)?;
    ctx.records().presence(&meta_root, &meta, 0)?;
    value(
        FactoryAuthorityCellV1::Record,
        &marker_root,
        &marker,
        &mut ctx.records(),
        0,
        &mut std::io::sink(),
    )?;
    let child = root
        .map(|root| outer(key.kind, &outer_key, &root, &mut ctx.records()))
        .transpose()?;
    ctx.check()?;
    Ok(child.map(|child| retained.retain(child)))
}
pub struct FactoryAuthorityCellRequestV1<'a> {
    pub key: FactoryAuthorityRecordKeyV1<'a>,
    pub cell: FactoryAuthorityCellV1,
    pub qualification: &'a ReservedValue<FactoryAuthorityQualificationV1>,
}
/// Missing optional module ID returns None. Text output remains provisional
/// until this call and its final cancellation check succeed.
pub fn read_qualified_factory_authority_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FactoryAuthorityCellRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut impl std::io::Write,
) -> Result<Option<FactoryAuthorityValueV1>, ValueTreeError> {
    ctx.check()?;
    if !request.cell.accepts(request.key.kind)
        || !request.qualification.uses_budget(ctx.values.memory)
    {
        return Err(invalid());
    }
    let child = inspect_factory_authority_root_v1(request.key, ctx)?.ok_or_else(invalid)?;
    let outer = outer_key(request.key, &mut ctx.records())?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    request
        .qualification
        .validate_binding(request.key.kind, &owner, &child)?;
    let mut inner = WorldFieldReadContextV1 {
        root: &child,
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
    let key = cell_key(request.key.kind, request.cell, &mut inner.records())?;
    let root = inner.lookup(&key.0)?;
    if request.cell == FactoryAuthorityCellV1::SourceModuleId {
        let flag = cell_key(
            request.key.kind,
            FactoryAuthorityCellV1::SourceModulePresent,
            &mut inner.records(),
        )?;
        let flag_root = inner.lookup(&flag.0)?.ok_or_else(invalid)?;
        let FactoryAuthorityValueV1::Bool(present) = value(
            FactoryAuthorityCellV1::SourceModulePresent,
            &flag_root,
            &flag,
            &mut inner.records(),
            1,
            &mut std::io::sink(),
        )?
        else {
            return Err(invalid());
        };
        if present != root.is_some() {
            return Err(invalid());
        }
        if !present {
            inner.check()?;
            return Ok(None);
        }
    }
    let root = root.ok_or_else(invalid)?;
    let result = value(
        request.cell,
        &root,
        &key,
        &mut inner.records(),
        max_payload,
        output,
    )?;
    inner.check()?;
    Ok(Some(result))
}
