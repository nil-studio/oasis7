//! Bounded point reads via one bound outer reference and relative child keys.
use super::*;
pub use crate::incremental_record_value_read::RecordValueV1 as LogisticsValueV1;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader, incremental_record_tree,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use std::{
    cell::RefCell,
    io::{self, Write},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsCellReadV1 {
    RecordAbsent,
    MemberAbsent,
    Value(LogisticsValueV1),
}
fn collection(cell: LogisticsCellV1) -> Option<LogisticsCollectionV1> {
    match cell {
        LogisticsCellV1::TextItem(c, _) => Some(c),
        _ => None,
    }
}
pub(super) fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    record: LogisticsRecordKeyV1,
    cell: LogisticsCellV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    valid_cell(record, cell)?;
    with_logistics_cell_key!(cell,encoded=>records.key(record.descriptor()?,WorldFieldRecordKindV1::Entry,encoded))
}
pub(super) fn inner_roots<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    keys: &[Option<(WorldFieldKeyV1, u64)>; 4],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[Option<[u8; 32]>; 4], ValueTreeError> {
    let _scratch = records
        .values
        .memory
        .reserve(64 << 10, records.values.class)?;
    let io = RefCell::new(&mut *records.values.io);
    let cancelled = RefCell::new(&mut *records.cancelled);
    let error = RefCell::new(None);
    let source = incremental_record_tree::Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: &io,
        cancelled: &cancelled,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut out = [None; 4];
    for (index, key) in keys.iter().enumerate() {
        if let Some((key, _)) = key {
            out[index] = source.finish(root.get(key.as_bytes(), &source))?;
        }
    }
    Ok(out)
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: LogisticsCellV1,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<LogisticsValueV1, ValueTreeError> {
    use crate::incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, read_record_value_v1,
    };
    let (codec, nullable) = cell.profile();
    read_record_value_v1(
        root,
        key,
        records,
        max_payload_bytes,
        output,
        RecordReadProfileV1 {
            codec,
            nullable,
            integer_max: cell.integer_max(),
            labels: RecordReadLabelsV1 {
                utf8: "logistics UTF-8",
                ledger_utf8: "logistics ledger UTF-8",
                ledger_frame: "logistics ledger frame",
                boolean: "logistics bool",
                fixed: "logistics fixed codec",
                profile: "logistics expected profile",
                integer: "logistics integer range",
            },
        },
    )
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    record: LogisticsRecordKeyV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let bytes: [u8; RECORD_ROOT_BYTES] =
        records.fixed(root, &key.0, key.1, MemberPayloadCodecV1::RecordRoot)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("logistics record root framing"))?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &record.kind.schema(),
            &binding
                .semantic_key
                .ok_or(ValueTreeError::Invalid("logistics outer key"))?,
            record.namespace(),
        )
        .map_err(|_| ValueTreeError::Invalid("logistics record root binding"))?
        .clone())
}
pub(super) fn child_keys<R: ValueNodeReader, C: FnMut() -> bool>(
    record: LogisticsRecordKeyV1,
    cell: LogisticsCellV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[Option<(WorldFieldKeyV1, u64)>; 4], ValueTreeError> {
    let mut out = std::array::from_fn(|_| None);
    out[0] = Some((
        WorldFieldKeyV1::new(record.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    ));
    for (index, cell) in [
        (1, Some(LogisticsCellV1::Record)),
        (2, Some(cell)),
        (3, collection(cell).map(LogisticsCellV1::Collection)),
    ] {
        if let Some(cell) = cell {
            out[index] = Some(key(record, cell, records)?);
        }
    }
    Ok(out)
}
pub(super) fn resolved<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: LogisticsCellV1,
    keys: &[Option<(WorldFieldKeyV1, u64)>; 4],
    roots: &[Option<[u8; 32]>; 4],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<LogisticsCellReadV1, ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("logistics child point relation");
    records.presence(
        &roots[0].ok_or_else(invalid)?,
        &keys[0].as_ref().unwrap().0,
        0,
    )?;
    value(
        LogisticsCellV1::Record,
        &roots[1].ok_or_else(invalid)?,
        keys[1].as_ref().unwrap(),
        records,
        0,
        &mut io::sink(),
    )?;
    let count = if let Some(c) = collection(cell) {
        let r = roots[3].ok_or_else(invalid)?;
        match value(
            LogisticsCellV1::Collection(c),
            &r,
            keys[3].as_ref().unwrap(),
            records,
            8,
            &mut io::sink(),
        )? {
            LogisticsValueV1::Unsigned(n) => Some(n),
            _ => return Err(invalid()),
        }
    } else {
        None
    };
    if let Some(n) = count
        && let LogisticsCellV1::TextItem(_, index) = cell
        && index >= n
    {
        if roots[2].is_some() {
            return Err(invalid());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(LogisticsCellReadV1::MemberAbsent);
    }
    let root = roots[2].ok_or_else(invalid)?;
    let result = value(
        cell,
        &root,
        keys[2].as_ref().unwrap(),
        records,
        max_payload_bytes,
        output,
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(LogisticsCellReadV1::Value(result))
}
/// Point scope authenticates only the requested dependencies, never complete raw
/// family absence/closure. New-record authority requires a family qualification.
pub fn read_logistics_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: LogisticsRecordKeyV1,
    cell: LogisticsCellV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<LogisticsCellReadV1, ValueTreeError> {
    valid_cell(record, cell)?;
    if context.root.namespace != record.namespace() {
        return Err(ValueTreeError::Invalid("logistics point namespace"));
    }
    let field = record.descriptor()?;
    let metadata = (
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    );
    let marker = context.records().key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let outer_key = context
        .records()
        .key(field, WorldFieldRecordKindV1::Entry, &record.id)?;
    let meta_root = context
        .lookup(&metadata.0)?
        .ok_or(ValueTreeError::Invalid("logistics metadata absent"))?;
    let marker_root = context
        .lookup(&marker.0)?
        .ok_or(ValueTreeError::Invalid("logistics format absent"))?;
    let outer_root = context.lookup(&outer_key.0)?;
    let mut records = context.records();
    records.presence(&meta_root, &metadata.0, 0)?;
    value(
        LogisticsCellV1::Record,
        &marker_root,
        &marker,
        &mut records,
        0,
        &mut io::sink(),
    )?;
    let Some(root) = outer_root else {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(LogisticsCellReadV1::RecordAbsent);
    };
    let child = outer(record, &outer_key, &root, &mut records)?;
    let keys = child_keys(record, cell, &mut records)?;
    let roots = inner_roots(&child, &keys, &mut records)?;
    resolved(cell, &keys, &roots, &mut records, max_payload_bytes, output)
}
