//! Caller-admitted selected DTO materialization, never an owner qualification.
//! The depth-64 ceiling is a selected DTO resource limit, not a restriction on
//! valid persisted JSON. Its admission must outlive the returned DTO's children.
use super::{read, *};
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_record_value_read::RecordValueV1;
use oasis7_proto::memory_budget::ReservedValue;
use serde_json::Value;
use std::io::Cursor;

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("selected JSON materialization")
}
fn spend(heap: &mut usize, bytes: usize) -> Result<(), ValueTreeError> {
    *heap = heap
        .checked_sub(bytes)
        .ok_or(ValueTreeError::ResourceLimited("selected JSON heap"))?;
    Ok(())
}
fn node<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
    heap: &mut usize,
    depth: usize,
) -> Result<Value, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    spend(heap, 1024)?;
    if depth > work.max_depth || depth > 64 {
        return Err(ValueTreeError::ResourceLimited("selected JSON depth"));
    }
    let mut frame = read::frame(profile, root, records, work)?;
    if !frame.kind.container() {
        let key = read::cell_key(profile, "json.value", records)?;
        let codec = match frame.kind {
            read::Kind::Null => MemberPayloadCodecV1::Unit,
            read::Kind::Bool => MemberPayloadCodecV1::Bool,
            read::Kind::I64 => MemberPayloadCodecV1::I64,
            read::Kind::U64 | read::Kind::F64 => MemberPayloadCodecV1::U64,
            read::Kind::String => MemberPayloadCodecV1::Text,
            _ => return Err(invalid()),
        };
        let info = read_member_record_header_v1(
            &frame.ids[3],
            &key.0,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: work.max_payload_bytes,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        let length = usize::try_from(info.payload_bytes).map_err(|_| invalid())?;
        spend(heap, length.checked_add(128).ok_or_else(invalid)?)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        let mut bytes = vec![0; length];
        if bytes.capacity() > length.checked_add(128).ok_or_else(invalid)? {
            return Err(invalid());
        }
        let observed = read::value(
            &frame.ids[3],
            &key,
            codec,
            work.max_payload_bytes,
            records,
            &mut Cursor::new(bytes.as_mut_slice()),
        )?;
        return Ok(match observed {
            RecordValueV1::Unit => Value::Null,
            RecordValueV1::Bool(v) => Value::Bool(v),
            RecordValueV1::Signed(v) => Value::Number(v.into()),
            RecordValueV1::Unsigned(v) if frame.kind == read::Kind::F64 => {
                Value::Number(serde_json::Number::from_f64(f64::from_bits(v)).ok_or_else(invalid)?)
            }
            RecordValueV1::Unsigned(v) => Value::Number(v.into()),
            RecordValueV1::Text { .. } => {
                Value::String(String::from_utf8(bytes).map_err(|_| invalid())?)
            }
            _ => return Err(invalid()),
        });
    }
    let count = usize::try_from(frame.length).map_err(|_| invalid())?;
    // Cover both the temporary Option slots and the final Value vector;
    // the object branch also has ample bounded BTreeMap node metadata.
    let container_bytes = count
        .checked_mul(std::mem::size_of::<Option<Value>>() + std::mem::size_of::<Value>() + 192)
        .ok_or_else(invalid)?;
    spend(heap, container_bytes)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let array = frame.kind == read::Kind::Array;
    let mut children = if array {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    if array {
        children.resize_with(count, || None);
    }
    if children
        .capacity()
        .checked_mul(std::mem::size_of::<Option<Value>>())
        .is_none_or(|n| n > container_bytes)
    {
        return Err(invalid());
    }
    let mut object = serde_json::Map::new();
    let mut actual = 0u64;
    while let Some(entry) = frame.cursor.next(records)? {
        if frame.keys.iter().any(|key| key.as_bytes() == entry.key) {
            continue;
        }
        let (child, ordinal) =
            read::child_with_ordinal(profile, &entry, array, frame.length, records)?;
        let mut member = Vec::new();
        if !array {
            let key = WorldFieldKeyV1::new(
                profile.field,
                WorldFieldRecordKindV1::Entry,
                WorldFieldKeyV1::from_bytes(&entry.key)
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key,
            )
            .map_err(ValueTreeError::Invalid)?;
            let limits = MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: 165,
                record_bytes: records.record_bytes,
            };
            let info = read_member_record_header_v1(
                &entry.value,
                &key,
                records.values,
                limits,
                &mut *records.cancelled,
            )?;
            let bound = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
            spend(heap, bound.checked_add(128).ok_or_else(invalid)?)?;
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            member.reserve_exact(bound);
            if member.capacity() > bound.checked_add(128).ok_or_else(invalid)? {
                return Err(invalid());
            }
            let mut probe = read::Probe::new(false, &mut member);
            read_member_record_profile_v1(
                &entry.value,
                &key,
                records.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::RecordRoot,
                    semantic_bytes: info.semantic_bytes,
                    limits,
                },
                &mut probe,
                &mut std::io::sink(),
                &mut *records.cancelled,
            )?;
            probe.finish()?;
        }
        let value = node(profile, &child, records, work, heap, depth + 1)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "selected JSON output cells",
                ))?;
        if array {
            let index = usize::try_from(ordinal.ok_or_else(invalid)?).map_err(|_| invalid())?;
            let slot = children.get_mut(index).ok_or_else(invalid)?;
            if slot.replace(value).is_some() {
                return Err(invalid());
            }
        } else if object
            .insert(String::from_utf8(member).map_err(|_| invalid())?, value)
            .is_some()
        {
            return Err(invalid());
        }
        actual = actual.checked_add(1).ok_or_else(invalid)?;
    }
    if actual != frame.length {
        return Err(invalid());
    }
    if array {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        let mut values = Vec::with_capacity(count);
        if children
            .capacity()
            .checked_mul(std::mem::size_of::<Option<Value>>())
            .and_then(|n| {
                values
                    .capacity()
                    .checked_mul(std::mem::size_of::<Value>())
                    .and_then(|m| n.checked_add(m))
            })
            .is_none_or(|n| n > container_bytes)
        {
            return Err(invalid());
        }
        for value in children {
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            work.cells_remaining =
                work.cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "selected JSON array output cells",
                    ))?;
            values.push(value.ok_or_else(invalid)?);
        }
        Ok(Value::Array(values))
    } else {
        Ok(Value::Object(object))
    }
}

/// Internal owner callers must first check their own class/root/profile token.
pub(crate) fn materialize_records<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    proof: &ReservedValue<JsonSubtreeQualificationV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
    max_heap_bytes: usize,
) -> Result<(Value, oasis7_proto::memory_budget::MemoryReservation), ValueTreeError> {
    if !proof.uses_budget(records.values.memory) {
        return Err(invalid());
    }
    proof.validate_binding(profile, proof.root())?;
    let mut output = records
        .values
        .memory
        .reserve(max_heap_bytes, records.values.class)?;
    let _stack = records
        .values
        .memory
        .reserve(128 << 10, records.values.class)?;
    let mut heap = max_heap_bytes;
    let value = node(profile, proof.root(), records, work, &mut heap, 0)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    // `heap` tracks every escaping string/container plus conservative node
    // metadata. Keep that proven bound; release only the unused allowance.
    let used = max_heap_bytes.checked_sub(heap).ok_or_else(invalid)?;
    let retained = output.split(used).map_err(ValueTreeError::Invalid)?;
    drop(output);
    if (records.cancelled)() {
        drop(value);
        drop(retained);
        return Err(ValueTreeError::Cancelled);
    }
    Ok((value, retained))
}

#[cfg(test)]
#[path = "incremental_json_records_materialize_tests.rs"]
mod tests;
