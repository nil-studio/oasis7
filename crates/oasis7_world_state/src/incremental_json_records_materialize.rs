//! Caller-admitted selected DTO materialization, never an owner qualification.
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
        let mut bytes = vec![0; length];
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
    spend(
        heap,
        count
            .checked_mul(std::mem::size_of::<Value>() + 192)
            .ok_or_else(invalid)?,
    )?;
    let array = frame.kind == read::Kind::Array;
    let mut children = if array {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    if array {
        children.resize_with(count, || None);
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
            member.reserve_exact(bound);
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
        Ok(Value::Array(
            children
                .into_iter()
                .map(|v| v.ok_or_else(invalid))
                .collect::<Result<_, _>>()?,
        ))
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
    let output = records
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
    Ok((value, output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_inventory_value_read::InventoryValueReader;
    use crate::incremental_semantic_key::SemanticKeyBudget;
    use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
    use oasis7_proto::memory_budget::MemoryBudget;
    use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
    struct Pages(RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>);
    impl ValueNodeWriter for Pages {
        fn put_node(
            &mut self,
            id: [u8; 32],
            bytes: ReservedValue<Vec<u8>>,
        ) -> Result<(), ValueTreeError> {
            self.0.borrow_mut().insert(id, Arc::new(bytes));
            Ok(())
        }
    }
    impl StatePageWriter for Pages {}
    impl ValueNodeReader for Pages {
        fn read_node(
            &self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
            class: MemoryClass,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
            let pages = self.0.borrow();
            let bytes = pages
                .get(id)
                .ok_or(ValueTreeError::Invalid("selected fixture missing"))?;
            if bytes.len() > max {
                return Err(invalid());
            }
            Ok(Arc::new(
                memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
            ))
        }
    }
    fn io() -> ValueIoBudget {
        ValueIoBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 128 << 20,
            write_bytes_remaining: 128 << 20,
        }
    }
    fn work() -> JsonReadWorkV1 {
        JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 1 << 20,
        }
    }
    #[test]
    fn selected_json_materialization_matches_original_and_preserves_sha_json_bytes() {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let mut pages = Pages(RefCell::new(BTreeMap::new()));
        let profile = JsonRecordProfileV1 {
            field: FieldDescriptor {
                owner: FieldOwner::Runtime,
                name: "selected.fixture",
                shape: FieldShape::Map,
                destination: FieldDestination::State(StateNamespaceV1::Artifacts),
            },
            schema: [27; 32],
        };
        let original = serde_json::json!({"": [null, true, false, -7, u64::MAX, 1.5, -0.0, "原🙂\u{0000}"], "nested": {"b": [], "a": {}}});
        let root = prepare_json_subtree_v1(
            profile,
            &original,
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
            &mut JsonWriteWorkV1 {
                cells_remaining: 100_000,
                json_nodes_remaining: 1000,
                max_json_depth: 16,
                max_pending_node_bytes: 1 << 20,
            },
        )
        .unwrap();
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut stop = || false;
        let mut records = InventoryValueReader {
            values: &mut ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            cancelled: &mut stop,
        };
        let proof =
            qualify_json_subtree_records_v1(profile, &root, &mut records, &mut work()).unwrap();
        let (value, _charge) =
            materialize_records(profile, &proof, &mut records, &mut work(), 1 << 20).unwrap();
        assert_eq!(value, original);
        assert_eq!(
            serde_json::to_vec(&value).unwrap(),
            serde_json::to_vec(&original).unwrap()
        );
        assert!(matches!(
            materialize_records(profile, &proof, &mut records, &mut work(), 0),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        let mut short = work();
        short.json_nodes_remaining = 0;
        assert!(matches!(
            materialize_records(profile, &proof, &mut records, &mut short, 1 << 20),
            Err(ValueTreeError::ResourceLimited(_))
        ));
    }
}
