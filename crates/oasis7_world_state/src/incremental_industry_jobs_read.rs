//! Bounded point reads via one bound outer reference and relative child keys.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::{LEDGER_VALUE_HEADER_BYTES, LedgerValueInfoV1, header_v1},
    incremental_record_tree,
    incremental_utf8_output::Utf8Output,
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
pub enum IndustryValueV1 {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text { utf8_bytes: u64 },
    Ledger(LedgerValueInfoV1),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryCellReadV1 {
    RecordAbsent,
    MemberAbsent,
    Value(IndustryValueV1),
}
pub(super) fn optional_parent(cell: IndustryCellV1<'_>) -> Option<IndustryColumnV1> {
    match cell {
        IndustryCellV1::Column(c) => match c.path().0 {
            IndustrySectionV1::Record => None,
            IndustrySectionV1::IndustrialRoot => Some(IndustryColumnV1::RecordIndustrialRootRef),
            IndustrySectionV1::Origin | IndustrySectionV1::Submission => {
                Some(IndustryColumnV1::RecordCommittedRecipeOrigin)
            }
        },
        _ => None,
    }
}
pub(super) fn required_nested(cell: IndustryCellV1<'_>) -> bool {
    match cell {
        IndustryCellV1::Column(c) => {
            optional_parent(cell).is_some()
                && !matches!(
                    c.value_type(),
                    IndustryColumnTypeV1::OptionalText | IndustryColumnTypeV1::OptionalU64
                )
        }
        _ => false,
    }
}
fn collection(cell: IndustryCellV1<'_>) -> Option<IndustryCollectionV1> {
    match cell {
        IndustryCellV1::TextItem(c, _)
        | IndustryCellV1::StackItem(c, _, _)
        | IndustryCellV1::Balance(c, _) => Some(c),
        _ => None,
    }
}
pub(super) fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    record: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    valid_cell(record, cell)?;
    with_industry_cell_key!(cell,encoded=>records.key(record.descriptor()?,WorldFieldRecordKindV1::Entry,encoded))
}
pub(super) fn inner_roots<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    keys: &[Option<(WorldFieldKeyV1, u64)>; 5],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[Option<[u8; 32]>; 5], ValueTreeError> {
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
    let mut out = [None; 5];
    for (index, key) in keys.iter().enumerate() {
        if let Some((key, _)) = key {
            out[index] = source.finish(root.get(key.as_bytes(), &source))?;
        }
    }
    Ok(out)
}
struct LedgerOutput<'a, W> {
    header: [u8; LEDGER_VALUE_HEADER_BYTES],
    used: usize,
    text: Utf8Output<'a, W>,
}
impl<W: Write> Write for LedgerOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let take = bytes.len().min(self.header.len() - self.used);
        self.header[self.used..self.used + take].copy_from_slice(&bytes[..take]);
        self.used += take;
        // Report consumed header bytes before delegating to a fallible sink.
        // Interrupted then retries only the ID slice, never an eaten header.
        if take != 0 {
            return Ok(take);
        }
        self.text.write(&bytes[take..]).map(|n| n + take)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: IndustryCellV1<'_>,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<IndustryValueV1, ValueTreeError> {
    let (codec, nullable) = cell.profile();
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: max_payload_bytes,
        record_bytes: records.record_bytes,
    };
    let mut fixed = [0u8; 8];
    let (info, decoded) = match codec {
        MemberPayloadCodecV1::Text => {
            let mut checked = Utf8Output::new(output, "industry UTF-8");
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.is_invalid() {
                return Err(ValueTreeError::Invalid("industry UTF-8"));
            }
            let info = result?;
            if !checked.is_complete() {
                return Err(ValueTreeError::Invalid("industry UTF-8"));
            }
            (
                info,
                IndustryValueV1::Text {
                    utf8_bytes: info.payload_bytes,
                },
            )
        }
        MemberPayloadCodecV1::Ledger => {
            let mut checked = LedgerOutput {
                header: [0; LEDGER_VALUE_HEADER_BYTES],
                used: 0,
                text: Utf8Output::new(output, "industry ledger UTF-8"),
            };
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.text.is_invalid() {
                return Err(ValueTreeError::Invalid("industry ledger UTF-8"));
            }
            let info = result?;
            let decoded = if info.codec == MemberPayloadCodecV1::Unit {
                IndustryValueV1::Unit
            } else {
                if !checked.text.is_complete() || checked.used != LEDGER_VALUE_HEADER_BYTES {
                    return Err(ValueTreeError::Invalid("industry ledger frame"));
                }
                IndustryValueV1::Ledger(
                    header_v1(&checked.header, info.payload_bytes)
                        .map_err(ValueTreeError::Invalid)?,
                )
            };
            (info, decoded)
        }
        _ => {
            let mut cursor = io::Cursor::new(fixed.as_mut_slice());
            let info = read_member_record_v1(
                root,
                &key.0,
                records.values,
                MemberRecordLimitsV1 {
                    payload_bytes: max_payload_bytes.min(8),
                    ..limits
                },
                &mut io::sink(),
                &mut cursor,
                &mut *records.cancelled,
            )?;
            let decoded = match info.codec {
                MemberPayloadCodecV1::Unit => IndustryValueV1::Unit,
                MemberPayloadCodecV1::U64 => IndustryValueV1::Unsigned(u64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::I64 => IndustryValueV1::Signed(i64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::Bool => match fixed[0] {
                    0 => IndustryValueV1::Bool(false),
                    1 => IndustryValueV1::Bool(true),
                    _ => return Err(ValueTreeError::Invalid("industry bool")),
                },
                _ => return Err(ValueTreeError::Invalid("industry fixed codec")),
            };
            (info, decoded)
        }
    };
    if info.semantic_bytes != key.1
        || !(info.codec == codec || nullable && info.codec == MemberPayloadCodecV1::Unit)
    {
        return Err(ValueTreeError::Invalid("industry expected profile"));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if info.codec == MemberPayloadCodecV1::Unit {
        return Ok(IndustryValueV1::Unit);
    }
    if matches!(decoded, IndustryValueV1::Unsigned(v) if v > cell.integer_max()) {
        return Err(ValueTreeError::Invalid("industry integer range"));
    }
    Ok(decoded)
}
pub(super) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    record: IndustryRecordKeyV1,
    key: &(WorldFieldKeyV1, u64),
    root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let _scratch = records.values.memory.reserve(8192, records.values.class)?;
    let bytes: [u8; RECORD_ROOT_BYTES] =
        records.fixed(root, &key.0, key.1, MemberPayloadCodecV1::RecordRoot)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("industry record root framing"))?;
    let binding = WorldFieldKeyV1::from_bytes(key.0.as_bytes()).map_err(ValueTreeError::Invalid)?;
    Ok(reference
        .validate_binding(
            &record.kind.schema(),
            &binding
                .semantic_key
                .ok_or(ValueTreeError::Invalid("industry outer key"))?,
            record.namespace(),
        )
        .map_err(|_| ValueTreeError::Invalid("industry record root binding"))?
        .clone())
}
pub(super) fn child_keys<R: ValueNodeReader, C: FnMut() -> bool>(
    record: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<[Option<(WorldFieldKeyV1, u64)>; 5], ValueTreeError> {
    let mut out = std::array::from_fn(|_| None);
    out[0] = Some((
        WorldFieldKeyV1::new(record.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?,
        0,
    ));
    for (index, cell) in [
        (1, Some(IndustryCellV1::Record)),
        (2, Some(cell)),
        (3, collection(cell).map(IndustryCellV1::Collection)),
        (4, optional_parent(cell).map(IndustryCellV1::Column)),
    ] {
        if let Some(cell) = cell {
            out[index] = Some(key(record, cell, records)?);
        }
    }
    Ok(out)
}
pub(super) fn resolved<R: ValueNodeReader, C: FnMut() -> bool>(
    cell: IndustryCellV1<'_>,
    keys: &[Option<(WorldFieldKeyV1, u64)>; 5],
    roots: &[Option<[u8; 32]>; 5],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<IndustryCellReadV1, ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("industry child point relation");
    records.presence(
        &roots[0].ok_or_else(invalid)?,
        &keys[0].as_ref().unwrap().0,
        0,
    )?;
    value(
        IndustryCellV1::Record,
        &roots[1].ok_or_else(invalid)?,
        keys[1].as_ref().unwrap(),
        records,
        0,
        &mut io::sink(),
    )?;
    let count = if let Some(c) = collection(cell) {
        let r = roots[3].ok_or_else(invalid)?;
        match value(
            IndustryCellV1::Collection(c),
            &r,
            keys[3].as_ref().unwrap(),
            records,
            8,
            &mut io::sink(),
        )? {
            IndustryValueV1::Unsigned(n) => Some(n),
            _ => return Err(invalid()),
        }
    } else {
        None
    };
    if let Some(n) = count
        && let IndustryCellV1::TextItem(_, index) | IndustryCellV1::StackItem(_, index, _) = cell
        && index >= n
    {
        if roots[2].is_some() {
            return Err(invalid());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        return Ok(IndustryCellReadV1::MemberAbsent);
    }
    let Some(root) = roots[2] else {
        if matches!(cell, IndustryCellV1::Balance(_, _)) {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            return Ok(IndustryCellReadV1::MemberAbsent);
        }
        return Err(invalid());
    };
    let result = value(
        cell,
        &root,
        keys[2].as_ref().unwrap(),
        records,
        max_payload_bytes,
        output,
    )?;
    if let Some(parent) = optional_parent(cell) {
        let root = roots[4].ok_or_else(invalid)?;
        let present = match value(
            IndustryCellV1::Column(parent),
            &root,
            keys[4].as_ref().unwrap(),
            records,
            1,
            &mut io::sink(),
        )? {
            IndustryValueV1::Bool(b) => b,
            _ => return Err(invalid()),
        };
        if !present && result != IndustryValueV1::Unit
            || present && required_nested(cell) && result == IndustryValueV1::Unit
        {
            return Err(invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(IndustryCellReadV1::Value(result))
}
/// Point scope authenticates only the requested dependencies, never complete raw
/// family absence/closure. New-record authority requires a family qualification.
pub fn read_industry_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<IndustryCellReadV1, ValueTreeError> {
    valid_cell(record, cell)?;
    if context.root.namespace != record.namespace() {
        return Err(ValueTreeError::Invalid("industry point namespace"));
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
        .ok_or(ValueTreeError::Invalid("industry metadata absent"))?;
    let marker_root = context
        .lookup(&marker.0)?
        .ok_or(ValueTreeError::Invalid("industry format absent"))?;
    let outer_root = context.lookup(&outer_key.0)?;
    let mut records = context.records();
    records.presence(&meta_root, &metadata.0, 0)?;
    value(
        IndustryCellV1::Record,
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
        return Ok(IndustryCellReadV1::RecordAbsent);
    };
    let child = outer(record, &outer_key, &root, &mut records)?;
    let keys = child_keys(record, cell, &mut records)?;
    let roots = inner_roots(&child, &keys, &mut records)?;
    resolved(cell, &keys, &roots, &mut records, max_payload_bytes, output)
}
