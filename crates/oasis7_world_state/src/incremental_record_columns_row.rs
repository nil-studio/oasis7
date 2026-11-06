//! One changed closed column, or authenticated old key without old payload I/O.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use oasis7_proto::memory_budget::ReservedValue;
pub(crate) struct ColumnRowV1 {
    pub key: ColumnKeyV1,
    pub cell: ColumnCellV1,
}
fn validated(
    profile: ColumnReadProfileV1,
    entry: &StateEntryV1,
) -> Result<WorldFieldKeyV1, ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if profile.field.destination != FieldDestination::State(entry.namespace)
        || parsed.namespace != entry.namespace
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || key.as_bytes() != entry.key
    {
        return Err(invalid());
    }
    Ok(key)
}
pub(super) fn read_row_into<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: ColumnReadProfileV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    left: &mut usize,
) -> Result<ColumnRowV1, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let key = validated(profile, entry)?;
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: records.record_bytes,
        record_bytes: records.record_bytes,
    };
    let header = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let sem = usize::try_from(header.semantic_bytes).map_err(|_| invalid())?;
    let bytes = usize::try_from(header.payload_bytes).map_err(|_| invalid())?;
    spend(
        left,
        sem.checked_add(bytes)
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(invalid)?,
    )?;
    let mut semantic = vec![0; sem];
    let mut payload = vec![0; bytes];
    let info = read_member_record_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut std::io::Cursor::new(semantic.as_mut_slice()),
        &mut std::io::Cursor::new(payload.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let name = path(
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?,
        0,
        left,
    )?;
    let cell = match info.codec {
        MemberPayloadCodecV1::Unit if payload.is_empty() => ColumnCellV1::Unit,
        MemberPayloadCodecV1::Bool if payload.len() == 1 && payload[0] <= 1 => {
            ColumnCellV1::Bool(payload[0] == 1)
        }
        MemberPayloadCodecV1::U64 if payload.len() == 8 => {
            ColumnCellV1::U64(u64::from_be_bytes(payload.as_slice().try_into().unwrap()))
        }
        MemberPayloadCodecV1::I64 if payload.len() == 8 => {
            ColumnCellV1::I64(i64::from_be_bytes(payload.as_slice().try_into().unwrap()))
        }
        MemberPayloadCodecV1::Text => {
            ColumnCellV1::Text(String::from_utf8(payload).map_err(|_| invalid())?)
        }
        MemberPayloadCodecV1::RecordRoot => {
            let reference =
                RecordRootRefV1::from_canonical_bytes(&payload).map_err(|_| invalid())?;
            let expected_schema = match profile.reference_schema {
                Some(resolve) => resolve(&name).ok_or_else(invalid)?,
                None => profile.schema,
            };
            ColumnCellV1::Root(
                reference
                    .validate_binding(
                        &expected_schema,
                        &parsed.semantic_key.ok_or_else(invalid)?,
                        entry.namespace,
                    )
                    .map_err(|_| invalid())?
                    .clone(),
            )
        }
        _ => return Err(invalid()),
    };

    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(ColumnRowV1 { key: name, cell })
}
pub(crate) fn read_column_row_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: ColumnReadProfileV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_heap: usize,
) -> Result<ReservedValue<ColumnRowV1>, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let mut charge = records
        .values
        .memory
        .reserve(max_heap, records.values.class)?;
    let mut left = max_heap;
    spend(&mut left, 1024)?;
    let row = read_row_into(profile, entry, records, &mut left)?;
    let retained = charge
        .split(max_heap - left)
        .map_err(ValueTreeError::Invalid)?;
    drop(charge);
    if (records.cancelled)() {
        drop(row);
        drop(retained);
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(row))
}
/// The caller must inherit a fully qualified old value. This authenticates its
/// original key only, and intentionally never walks or hydrates its payload.
pub(crate) fn read_column_key_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: ColumnReadProfileV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_heap: usize,
) -> Result<ReservedValue<ColumnKeyV1>, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let mut charge = records
        .values
        .memory
        .reserve(max_heap, records.values.class)?;
    let mut left = max_heap;
    spend(&mut left, 1024)?;
    let key = validated(profile, entry)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: records.record_bytes,
        record_bytes: records.record_bytes,
    };
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let length = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    spend(&mut left, length.checked_add(128).ok_or_else(invalid)?)?;
    let mut semantic = vec![0; length];
    crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut std::io::Cursor::new(semantic.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let decoded = path(
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?,
        0,
        &mut left,
    )?;
    drop(semantic);
    let retained = charge
        .split(max_heap - left)
        .map_err(ValueTreeError::Invalid)?;
    drop(charge);
    if (records.cancelled)() {
        drop(decoded);
        drop(retained);
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(decoded))
}

/// Decode an already authenticated canonical semantic address for selecting a
/// closed reference profile before creating a new member. No value is admitted.
pub(crate) fn parse_column_key_v1(
    semantic: &[u8],
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
    max_heap: usize,
) -> Result<ReservedValue<ColumnKeyV1>, ValueTreeError> {
    if semantic.len() > max_heap {
        return Err(ValueTreeError::ResourceLimited(
            "column semantic address admission",
        ));
    }
    let mut charge = memory.reserve(max_heap, class)?;
    let mut left = max_heap;
    spend(&mut left, 1024)?;
    let key = path(
        read_semantic_key_v1(semantic).map_err(ValueTreeError::Invalid)?,
        0,
        &mut left,
    )?;
    let retained = charge
        .split(max_heap - left)
        .map_err(ValueTreeError::Invalid)?;
    drop(charge);
    Ok(retained.retain(key))
}
