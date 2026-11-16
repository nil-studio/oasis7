//! Closed selected-record projection; original key/value/closure kernels only.
use super::*;
use crate::incremental_industry_projection_delta as out;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_state::{StateRootV1, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use serde_json::{Map, Value};
use std::{cell::RefCell, io, mem::size_of};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("selected Factory original JSON projection")
}
fn scan<R: ValueNodeReader, C: FnMut() -> bool>(
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut FactoryWorkBudgetV1,
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
                    "selected Factory projection scan",
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
            "selected Factory projection scan",
        )),
        _ => Err(bad()),
    }
}
pub(crate) fn project<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryRecordKindV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut FactoryWorkBudgetV1,
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
        let value = out::scalar(
            match value {
                read::FactoryValueV1::Unit => {
                    crate::incremental_record_value_read::RecordValueV1::Unit
                }
                read::FactoryValueV1::Unsigned(v) => {
                    crate::incremental_record_value_read::RecordValueV1::Unsigned(v)
                }
                read::FactoryValueV1::Signed(v) => {
                    crate::incremental_record_value_read::RecordValueV1::Signed(v)
                }
                read::FactoryValueV1::Bool(v) => {
                    crate::incremental_record_value_read::RecordValueV1::Bool(v)
                }
                read::FactoryValueV1::Text { utf8_bytes } => {
                    crate::incremental_record_value_read::RecordValueV1::Text { utf8_bytes }
                }
                read::FactoryValueV1::Ledger(v) => {
                    crate::incremental_record_value_read::RecordValueV1::Ledger(v)
                }
            },
            text,
        )?;
        use key_read::ParsedCell as P;
        match parsed {
            P::Record => {}
            P::Column(c) => {
                let (section, name) = c.path();
                let mut value = value;
                if c == FactoryColumnV1::ProductionStatus {
                    value = serde_json::to_value(match value.as_u64() {
                        Some(0) => crate::FactoryProductionStatus::Idle,
                        Some(1) => crate::FactoryProductionStatus::Running,
                        Some(2) => crate::FactoryProductionStatus::Blocked,
                        Some(3) => crate::FactoryProductionStatus::Paused,
                        _ => return Err(bad()),
                    })
                    .map_err(|_| bad())?;
                }
                if c == FactoryColumnV1::ObligationMode && !value.is_null() {
                    if value.as_u64() != Some(0) {
                        return Err(bad());
                    }
                    value =
                        serde_json::to_value(crate::FactoryConstructionPowerMode::StartOnlySink)
                            .map_err(|_| bad())?;
                }
                assembly.put(section, name, value)?;
            }
            P::Collection(c) => {
                let (section, name) = c.path();
                assembly.collection(
                    section,
                    name,
                    value,
                    matches!(
                        c,
                        FactoryCollectionV1::ObligationBalancesBefore
                            | FactoryCollectionV1::ObligationBalancesAfter
                    ),
                    child.leaf_count,
                )?;
            }
            P::Text(c, index) => {
                let (section, name) = c.path();
                assembly.member(section, name, Some(index), None, None, value)?;
            }
            P::Stack(c, index, part) => {
                let (section, name) = c.path();
                let part = match part {
                    FactoryStackColumnV1::Kind => "kind",
                    FactoryStackColumnV1::Amount => "amount",
                };
                assembly.member(section, name, Some(index), Some(part), None, value)?;
            }
            P::Balance(c) => {
                let (section, name) = c.path();
                let semantic_len = usize::try_from(h.semantic_bytes).map_err(|_| bad())?;
                p.spend(semantic_len)?;
                let mut member = Vec::with_capacity(semantic_len);
                let mut decoder = key_read::CellKeyDecoder::new(&mut member);
                let read = read_member_record_v1(
                    &entry.value,
                    &key,
                    records.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: records.semantic_bytes,
                        payload_bytes: records.record_bytes,
                        record_bytes: records.record_bytes,
                    },
                    &mut decoder,
                    &mut io::sink(),
                    &mut *records.cancelled,
                );
                if let Some(e) = decoder.bad {
                    return Err(ValueTreeError::Invalid(e));
                }
                read?;
                decoder.finish()?;
                drop(decoder);
                let member = String::from_utf8(member).map_err(|_| bad())?;
                assembly.member(section, name, None, None, Some(&member), value)?;
            }
        }
        Ok(())
    })?;
    let mut record = assembly.take(if kind == FactoryRecordKindV1::Factory {
        "factory"
    } else {
        "build"
    });
    record
        .as_object_mut()
        .ok_or_else(bad)?
        .insert("spec".into(), assembly.take("spec"));
    if kind == FactoryRecordKindV1::Factory {
        let mut production = assembly.take("production");
        let present = production
            .get("last_completed_canonical_snapshot")
            .and_then(Value::as_bool)
            .ok_or_else(bad)?;
        production.as_object_mut().ok_or_else(bad)?.insert(
            "last_completed_canonical_snapshot".into(),
            if present {
                assembly.take("last_completed_canonical_snapshot")
            } else {
                Value::Null
            },
        );
        record
            .as_object_mut()
            .ok_or_else(bad)?
            .insert("production".into(), production);
    } else {
        let mut obligation = assembly.take("construction_power_obligation");
        let present = obligation
            .as_object_mut()
            .ok_or_else(bad)?
            .remove("construction_power_obligation")
            .and_then(|v| v.as_bool())
            .ok_or_else(bad)?;
        record.as_object_mut().ok_or_else(bad)?.insert(
            "construction_power_obligation".into(),
            if present { obligation } else { Value::Null },
        );
    }

    let bytes = match kind {
        FactoryRecordKindV1::Factory => {
            let value: crate::FactoryState = serde_json::from_value(record).map_err(|_| bad())?;
            out::json(&value, &memory, p)?
        }
        FactoryRecordKindV1::Build => {
            let value: crate::FactoryBuildJobState =
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
    kind: FactoryRecordKindV1,
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
            FactoryCellV1::Record,
            value,
            &(marker, semantic),
            records,
            0,
            &mut io::sink(),
        )? != FactoryValueV1::Unit
        {
            return Err(bad());
        }
        return Ok(None);
    }
    match kind {
        FactoryRecordKindV1::Factory => {
            let (id, child, fee) = current_identity::capture_factory_id(value, key, records)?;
            p.spend(id.len() + 128)?;
            Ok(Some(out::Outer {
                key: out::Key::Text(id),
                child,
                _fee: fee,
            }))
        }
        FactoryRecordKindV1::Build => {
            let fee = records
                .values
                .memory
                .reserve(size_of::<out::Outer>() + 128, MemoryClass::Ordinary)?;
            p.spend(size_of::<out::Outer>() + 128)?;
            let (id, child) = build_current_identity::decode_build_root(value, key, records)?;
            Ok(Some(out::Outer {
                key: out::Key::Id(id),
                child,
                _fee: fee,
            }))
        }
    }
}
