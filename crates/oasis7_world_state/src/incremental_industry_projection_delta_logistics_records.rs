//! Closed selected-record projection; original key/value/closure kernels only.
use super::*;
use crate::incremental_industry_projection_delta as out;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_state::{StateRootV1, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::{cell::RefCell, io, mem::size_of};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("selected Logistics original JSON projection")
}
fn scan<R: ValueNodeReader, C: FnMut() -> bool>(
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut LogisticsWorkBudgetV1,
    mut visit: impl FnMut(
        &oasis7_formal_protocol::incremental_state::StateEntryV1,
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    let source = tree::Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: RefCell::new(&mut *records.values.io),
        cancelled: RefCell::new(&mut *records.cancelled),
        error: RefCell::new(None),
    };
    let remaining = &mut work.cells_remaining;
    let result = StateScanContext {
        source: &source,
        memory: source.memory,
        class: source.class,
        io: &mut work.scan,
    }
    .visit_entries(
        child,
        |entry| {
            *remaining = remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "selected Logistics projection scan",
                ))?;
            let mut io = source.io.borrow_mut();
            let mut stop = source.cancelled.borrow_mut();
            let mut values = ValueReadContext {
                source: source.source,
                memory: source.memory,
                class: source.class,
                io: &mut io,
            };
            let mut reader = InventoryValueReader {
                values: &mut values,
                key_work: &mut *records.key_work,
                semantic_bytes: records.semantic_bytes,
                record_bytes: records.record_bytes,
                cancelled: &mut **stop,
            };
            visit(entry, &mut reader)
        },
        || source.cancelled.borrow_mut()(),
    );
    if let Some(error) = source.error.borrow_mut().take() {
        return Err(error);
    }
    match result {
        Ok(_) => Ok(()),
        Err(StateScanError::Memory(e)) => Err(ValueTreeError::Memory(e)),
        Err(StateScanError::Visitor(e)) => Err(e),
        Err(StateScanError::Cancelled) => Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => Err(ValueTreeError::ResourceLimited(
            "selected Logistics projection scan",
        )),
        _ => Err(bad()),
    }
}
pub(crate) fn project<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut LogisticsWorkBudgetV1,
    p: &out::Processing<'_>,
) -> Result<out::JsonRecord, ValueTreeError> {
    let max_payload = records.record_bytes;
    closure::verify_child(kind, child, records, max_payload, work)?;
    let memory = records.values.memory.clone();
    let mut layout = 32768usize;
    scan(child, records, work, |entry, records| {
        p.spend(entry.key.len() + 256)?;
        let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        let h = read_member_record_header_v1(
            &entry.value,
            &key,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: records.record_bytes,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        let n = usize::try_from(
            h.payload_bytes
                .checked_add(h.semantic_bytes)
                .ok_or_else(bad)?,
        )
        .map_err(|_| bad())?;
        layout = layout
            .checked_add(
                n.checked_mul(128)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(bad)?,
            )
            .ok_or_else(bad)?;
        Ok(())
    })?;
    p.spend(layout)?;
    let graph = memory.reserve(layout, MemoryClass::Ordinary)?;
    let mut assembly = out::Assembly::new();
    scan(child, records, work, |entry, records| {
        p.spend(entry.key.len() + 256)?;
        let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            return Ok(());
        }
        let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        let (parsed, _) = closure::cell(kind, entry, records, max_payload)?;
        let h = read_member_record_header_v1(
            &entry.value,
            &key,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: records.record_bytes,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        let n = usize::try_from(h.payload_bytes).map_err(|_| bad())?;
        p.spend(n)?;
        let mut text = Vec::with_capacity(n);
        if text.capacity() != n {
            return Err(bad());
        }
        let value = read::value(
            parsed.profile_cell(),
            &entry.value,
            &(key.clone(), h.semantic_bytes),
            records,
            max_payload,
            &mut text,
        )?;
        let value = out::scalar(value, text)?;
        use key_read::ParsedCell as P;
        match parsed {
            P::Record => {}
            P::Column(c) => {
                let (section, name) = c.path();
                let mut value = value;
                if c == LogisticsColumnV1::Priority {
                    value = serde_json::to_value(match value.as_u64() {
                        Some(0) => crate::MaterialTransitPriority::Standard,
                        Some(1) => crate::MaterialTransitPriority::Urgent,
                        _ => return Err(bad()),
                    })
                    .map_err(|_| bad())?;
                }
                assembly.put(section, name, value)?
            }
            P::Collection => {
                assembly.collection("record", "route_ids", value, false, child.leaf_count)?
            }
            P::Text(index) => {
                assembly.member("record", "route_ids", Some(index), None, None, value)?
            }
        }
        Ok(())
    })?;
    let record = assembly.take("record");
    let bytes = match kind {
        LogisticsRecordKindV1::Route => {
            let value: crate::LogisticsRouteV1 =
                serde_json::from_value(record).map_err(|_| bad())?;
            out::json(&value, &memory, p)?
        }
        LogisticsRecordKindV1::Path => {
            let value: crate::LogisticsPathAuthorityV1 =
                serde_json::from_value(record).map_err(|_| bad())?;
            out::json(&value, &memory, p)?
        }
    };
    Ok(out::JsonRecord {
        bytes,
        _graph: graph,
    })
}

pub(crate) fn outer<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &out::Processing<'_>,
) -> Result<Option<out::Outer>, ValueTreeError> {
    let field = kind.descriptor()?;
    let (marker, semantic) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    if key == &marker {
        records
            .values
            .verify_closure(value, records.record_bytes, &mut *records.cancelled)?;
        if read::value(
            LogisticsCellV1::Record,
            value,
            &(marker, semantic),
            records,
            0,
            &mut io::sink(),
        )? != LogisticsValueV1::Unit
        {
            return Err(bad());
        }
        return Ok(None);
    }
    use crate::incremental_record_key_read::{OriginalKeyKind, OriginalSemanticKeyProbe};
    let h = read_member_record_header_v1(
        value,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: records.record_bytes,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let n = usize::try_from(h.semantic_bytes).map_err(|_| bad())?;
    p.spend(n + 256)?;
    let fee = records.values.memory.reserve(
        n.checked_add(size_of::<out::Outer>() + 128)
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let child = family::reference(kind, key, value, records)?;
    let mut id = Vec::with_capacity(n);
    let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut id);
    let result = read_member_record_v1(
        value,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: records.record_bytes,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut io::sink(),
        &mut *records.cancelled,
    );
    let error = probe.error();
    let framing = probe.finish();
    if let Some(e) = error {
        return Err(ValueTreeError::Invalid(e));
    }
    result?;
    framing?;
    let id = String::from_utf8(id).map_err(|_| bad())?;
    Ok(Some(out::Outer {
        key: out::Key::Text(id),
        child,
        _fee: fee,
    }))
}
