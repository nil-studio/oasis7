//! Exact-root reads. A parsed outer edge alone is not a closure proof.
use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as GovernanceValueV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "governance text UTF-8",
    ledger_utf8: "governance ledger UTF-8",
    ledger_frame: "governance ledger frame",
    boolean: "governance bool",
    fixed: "governance fixed codec",
    profile: "governance expected profile",
    integer: "governance integer range",
};
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if !cell.accepts(kind) {
        return Err(ValueTreeError::Invalid("governance cell owner"));
    }
    with_governance_cell_key!(cell,encoded=>records.key(kind.descriptor()?,WorldFieldRecordKindV1::Entry,encoded))
}

pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: GovernanceCellV1<'_>,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<GovernanceValueV1, ValueTreeError> {
    read_record_value_v1(
        root,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec: cell.codec(),
            nullable: cell.nullable(),
            integer_max: cell.integer_max(),
            labels: LABELS,
        },
    )
}
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: GovernanceRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    records.key(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.id,
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    use crate::incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe};
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let mut sink = std::io::sink();
    let key_kind = OriginalKeyKind::Text;
    let mut probe = OriginalSemanticKeyProbe::new(key_kind, &mut sink);
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
    if let Some(reason) = probe.error() {
        return Err(ValueTreeError::Invalid(reason));
    }
    result?;
    probe.finish()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("governance record root framing"))?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &kind.schema(),
            &binding
                .semantic_key
                .ok_or(ValueTreeError::Invalid("governance outer key"))?,
            kind.namespace(),
        )
        .map_err(|_| ValueTreeError::Invalid("governance record root binding"))?
        .clone())
}
/// Inspect only an integrity edge after checking the outer map's metadata and
/// format marker. The returned root remains unqualified.
pub fn inspect_governance_record_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind.namespace() {
        return Err(ValueTreeError::Invalid("governance outer namespace"));
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let field = key.kind.descriptor()?;
    let meta = (
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    );
    let (marker, outer_key) = {
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        (
            records.key(
                field,
                WorldFieldRecordKindV1::Entry,
                &(RECORD_FORMAT_KEY, ()),
            )?,
            outer_key(key, &mut records)?,
        )
    };
    let meta_root = ctx
        .lookup(&meta.0)?
        .ok_or(ValueTreeError::Invalid("governance metadata missing"))?;
    let marker_root = ctx
        .lookup(&marker.0)?
        .ok_or(ValueTreeError::Invalid("governance format missing"))?;
    let root = ctx.lookup(&outer_key.0)?;
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    records.presence(&meta_root, &meta.0, 0)?;
    value(
        GovernanceCellV1::Record,
        &marker_root,
        &marker,
        &mut records,
        0,
        &mut std::io::sink(),
    )?;
    let child = root
        .map(|r| outer(key.kind, &outer_key, &r, &mut records))
        .transpose()?;
    ctx.check()?;
    Ok(child.map(|root| retained.retain(root)))
}
pub struct GovernanceCellRequestV1<'a> {
    pub key: GovernanceRecordKeyV1<'a>,
    pub cell: GovernanceCellV1<'a>,
    pub qualification: &'a ReservedValue<GovernanceRecordQualificationV1>,
}
/// Unit is an original Option None in its fixed required column. Output is provisional
/// until the whole call and its final cancellation check succeed.
pub fn read_qualified_governance_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: GovernanceCellRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut impl std::io::Write,
) -> Result<GovernanceValueV1, ValueTreeError> {
    let kind = request.key.kind;
    if !request.cell.accepts(kind) || !request.qualification.uses_budget(ctx.values.memory) {
        return Err(ValueTreeError::Invalid("governance qualified read binding"));
    }
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let child = inspect_governance_record_root_v1(request.key, ctx)?.ok_or(
        ValueTreeError::Invalid("governance qualified record absent"),
    )?;
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let outer = outer_key(request.key, &mut records)?;
    let digest = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("governance qualified owner"))?;
    request
        .qualification
        .validate_binding(kind, &digest, &child)?;
    let key = cell_key(kind, request.cell, &mut records)?;
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
    let root = inner.lookup(&key.0)?;
    let root = root.ok_or(ValueTreeError::Invalid("governance required cell missing"))?;
    let mut records = InventoryValueReader {
        values: &mut inner.values,
        key_work: inner.key_work,
        semantic_bytes: inner.semantic_bytes,
        record_bytes: inner.record_bytes,
        cancelled: &mut inner.cancelled,
    };
    let value = value(request.cell, &root, &key, &mut records, max_payload, output)?;
    inner.check()?;
    Ok(value)
}
