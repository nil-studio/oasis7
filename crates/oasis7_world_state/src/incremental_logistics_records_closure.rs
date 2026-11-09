//! Explicit bootstrap/recovery closure checks. Ordinary updates use a diff.
use super::key_read::{CellKeyDecoder, ParsedCell};
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateRootV1, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use std::{cell::RefCell, io};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("logistics closed record schema")
}

pub(super) fn cell<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload: u64,
) -> Result<(ParsedCell, LogisticsValueV1), ValueTreeError> {
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    let field = kind.descriptor()?;
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(field, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != kind.namespace()
        || key.as_bytes() != entry.key
        || binding.kind != WorldFieldRecordKindV1::Entry
    {
        return Err(invalid());
    }
    let mut decoder = CellKeyDecoder::new();
    let result = read_member_record_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: max_payload,
            record_bytes: records.record_bytes,
        },
        &mut decoder,
        &mut io::sink(),
        &mut *records.cancelled,
    );
    if let Some(reason) = decoder.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    let info = result?;
    let parsed = decoder.finish()?;
    parsed.validate(kind)?;
    let value = read::value(
        parsed.profile_cell(),
        &entry.value,
        &(key, info.semantic_bytes),
        records,
        max_payload,
        &mut io::sink(),
    )?;
    Ok((parsed, value))
}

struct Closure {
    metadata: bool,
    marker: bool,
    columns: [bool; 14],
    declared: Option<u64>,
    members: u64,
    maximum: Option<u64>,
}
impl Closure {
    fn new() -> Self {
        Self {
            metadata: false,
            marker: false,
            columns: [false; 14],
            declared: None,
            members: 0,
            maximum: None,
        }
    }
    fn accept(
        &mut self,
        parsed: ParsedCell,
        value: LogisticsValueV1,
    ) -> Result<(), ValueTreeError> {
        match parsed {
            ParsedCell::Record => {
                if self.marker || value != LogisticsValueV1::Unit {
                    return Err(invalid());
                }
                self.marker = true;
            }
            ParsedCell::Column(c) => {
                let index = LogisticsColumnV1::ALL
                    .iter()
                    .position(|candidate| *candidate == c)
                    .ok_or_else(invalid)?;
                if self.columns[index] {
                    return Err(invalid());
                }
                self.columns[index] = true;
            }
            ParsedCell::Collection => {
                let LogisticsValueV1::Unsigned(n) = value else {
                    return Err(invalid());
                };
                if self.declared.replace(n).is_some() {
                    return Err(invalid());
                }
            }
            ParsedCell::Text(index) => {
                self.members = self.members.checked_add(1).ok_or_else(invalid)?;
                self.maximum = Some(self.maximum.map_or(index, |old| old.max(index)));
            }
        }
        Ok(())
    }
    fn finish(&self, kind: LogisticsRecordKindV1) -> Result<(), ValueTreeError> {
        if !self.metadata || !self.marker {
            return Err(invalid());
        }
        for (index, column) in LogisticsColumnV1::ALL.into_iter().enumerate() {
            if self.columns[index] != kind.sample().accepts(column) {
                return Err(invalid());
            }
        }
        match kind {
            LogisticsRecordKindV1::Route => {
                if self.declared.is_some() || self.members != 0 {
                    return Err(invalid());
                }
            }
            LogisticsRecordKindV1::Path => {
                let n = self.declared.ok_or_else(invalid)?;
                if self.members != n || self.maximum.is_some_and(|index| index >= n) {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

/// Full explicit child scope only; this does not mint a family/World token.
pub(super) fn verify_child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload: u64,
    work: &mut LogisticsWorkBudgetV1,
) -> Result<(), ValueTreeError> {
    if child.namespace != kind.namespace() {
        return Err(invalid());
    }
    let _workspace = records
        .values
        .memory
        .reserve(16 << 10, records.values.class)?;
    let mut closure = Closure::new();
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
                .ok_or(ValueTreeError::ResourceLimited("logistics closure cells"))?;
            let mut io = source.io.borrow_mut();
            let mut cancel = source.cancelled.borrow_mut();
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
                cancelled: &mut **cancel,
            };
            let binding =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if binding.kind == WorldFieldRecordKindV1::Metadata {
                let key = WorldFieldKeyV1::new(
                    kind.descriptor()?,
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .map_err(ValueTreeError::Invalid)?;
                if closure.metadata
                    || entry.namespace != kind.namespace()
                    || key.as_bytes() != entry.key
                {
                    return Err(invalid());
                }
                reader.values.verify_closure(
                    &entry.value,
                    reader.record_bytes,
                    &mut *reader.cancelled,
                )?;
                reader.presence(&entry.value, &key, 0)?;
                closure.metadata = true;
                Ok(())
            } else {
                let (parsed, value) = cell(kind, entry, &mut reader, max_payload)?;
                closure.accept(parsed, value)
            }
        },
        || (source.cancelled.borrow_mut())(),
    );
    match result {
        Ok(_) => source.finish(Ok(()))?,
        Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("logistics closure scan"));
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Invalid(_)) => return Err(invalid()),
        Err(StateScanError::Tree(e)) => return source.finish(Err(e)),
    }
    closure.finish(kind)?;
    if (source.cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
