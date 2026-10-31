//! Exact-root reads. A parsed outer edge alone is not a closure proof.
use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as MaterialValueV1;
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
    utf8: "material text UTF-8",
    ledger_utf8: "material ledger UTF-8",
    ledger_frame: "material ledger frame",
    boolean: "material bool",
    fixed: "material fixed codec",
    profile: "material expected profile",
    integer: "material integer range",
};
pub(super) fn cell_key<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: MaterialRecordKindV1,
    cell: MaterialCellV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    if !cell.accepts(kind) {
        return Err(ValueTreeError::Invalid("material cell owner"));
    }
    if cell == MaterialCellV1::Record {
        records.key(kind.descriptor()?, WorldFieldRecordKindV1::Entry, &())
    } else {
        records.key(
            kind.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &((), cell.name()),
        )
    }
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: MaterialCellV1,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<MaterialValueV1, ValueTreeError> {
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
            labels: LABELS,
        },
    )
}
pub(super) fn outer_key<R: ValueNodeReader, C: FnMut() -> bool>(
    key: MaterialRecordKeyV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    with_material_outer_key!(key,encoded=>records.key(key.kind().descriptor()?,WorldFieldRecordKindV1::Entry,encoded))
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: MaterialRecordKindV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let bytes: [u8; RECORD_ROOT_BYTES] =
        records.fixed(root, &key.0, key.1, MemberPayloadCodecV1::RecordRoot)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("material record root framing"))?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &kind.schema(),
            &binding
                .semantic_key
                .ok_or(ValueTreeError::Invalid("material outer key"))?,
            kind.namespace(),
        )
        .map_err(|_| ValueTreeError::Invalid("material record root binding"))?
        .clone())
}
/// Inspect only an integrity edge after checking the outer map's metadata and
/// format marker. The returned root remains unqualified.
pub fn inspect_material_record_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: MaterialRecordKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != key.kind().namespace() {
        return Err(ValueTreeError::Invalid("material outer namespace"));
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let field = key.kind().descriptor()?;
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
        .ok_or(ValueTreeError::Invalid("material metadata missing"))?;
    let marker_root = ctx
        .lookup(&marker.0)?
        .ok_or(ValueTreeError::Invalid("material format missing"))?;
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
        MaterialCellV1::Record,
        &marker_root,
        &marker,
        &mut records,
        0,
        &mut std::io::sink(),
    )?;
    let child = root
        .map(|r| outer(key.kind(), &outer_key, &r, &mut records))
        .transpose()?;
    ctx.check()?;
    Ok(child.map(|root| retained.retain(root)))
}
pub struct MaterialCellRequestV1<'a> {
    pub key: MaterialRecordKeyV1<'a>,
    pub cell: MaterialCellV1,
    pub qualification: &'a ReservedValue<MaterialRecordQualificationV1>,
}
/// `None` is solely an original absent route_id Option. Output is provisional
/// until the whole call and its final cancellation check succeed.
pub fn read_qualified_material_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: MaterialCellRequestV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut impl std::io::Write,
) -> Result<Option<MaterialValueV1>, ValueTreeError> {
    let kind = request.key.kind();
    if !request.cell.accepts(kind) || !request.qualification.uses_budget(ctx.values.memory) {
        return Err(ValueTreeError::Invalid("material qualified read binding"));
    }
    let _workspace = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let child = inspect_material_record_root_v1(request.key, ctx)?
        .ok_or(ValueTreeError::Invalid("material qualified record absent"))?;
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
        .ok_or(ValueTreeError::Invalid("material qualified owner"))?;
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
    if request.cell == MaterialCellV1::RouteId {
        let flag = {
            let mut records = InventoryValueReader {
                values: &mut inner.values,
                key_work: inner.key_work,
                semantic_bytes: inner.semantic_bytes,
                record_bytes: inner.record_bytes,
                cancelled: &mut inner.cancelled,
            };
            cell_key(kind, MaterialCellV1::RoutePresent, &mut records)?
        };
        let flag_root = inner
            .lookup(&flag.0)?
            .ok_or(ValueTreeError::Invalid("material option flag missing"))?;
        let mut records = InventoryValueReader {
            values: &mut inner.values,
            key_work: inner.key_work,
            semantic_bytes: inner.semantic_bytes,
            record_bytes: inner.record_bytes,
            cancelled: &mut inner.cancelled,
        };
        let MaterialValueV1::Bool(present) = value(
            MaterialCellV1::RoutePresent,
            &flag_root,
            &flag,
            &mut records,
            1,
            &mut std::io::sink(),
        )?
        else {
            return Err(ValueTreeError::Invalid("material option flag codec"));
        };
        if present != root.is_some() {
            return Err(ValueTreeError::Invalid("material option relationship"));
        }
        if !present {
            inner.check()?;
            return Ok(None);
        }
    }
    let root = root.ok_or(ValueTreeError::Invalid("material required cell missing"))?;
    let mut records = InventoryValueReader {
        values: &mut inner.values,
        key_work: inner.key_work,
        semantic_bytes: inner.semantic_bytes,
        record_bytes: inner.record_bytes,
        cancelled: &mut inner.cancelled,
    };
    let value = value(request.cell, &root, &key, &mut records, max_payload, output)?;
    inner.check()?;
    Ok(Some(value))
}
