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
    ValueTreeError::Invalid("industry closed record schema")
}

pub(super) fn cell<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: IndustryRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload: u64,
) -> Result<(ParsedCell, IndustryValueV1), ValueTreeError> {
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
    let mut sink = io::sink();
    let mut decoder = CellKeyDecoder::new(&mut sink);
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

#[derive(Default)]
struct Members {
    count: [u64; 2],
    maximum: [Option<u64>; 2],
}
impl Members {
    fn add(&mut self, part: usize, index: Option<u64>) -> Result<(), ValueTreeError> {
        self.count[part] = self.count[part].checked_add(1).ok_or(invalid())?;
        if let Some(index) = index {
            self.maximum[part] = Some(self.maximum[part].map_or(index, |old| old.max(index)));
        }
        Ok(())
    }
}
struct Closure {
    metadata: bool,
    marker: bool,
    columns: [Option<IndustryValueV1>; 52],
    collections: [Option<IndustryValueV1>; 10],
    members: [Members; 10],
}
impl Closure {
    fn new() -> Self {
        Self {
            metadata: false,
            marker: false,
            columns: [None; 52],
            collections: [None; 10],
            members: std::array::from_fn(|_| Members::default()),
        }
    }
    fn accept(&mut self, parsed: ParsedCell, value: IndustryValueV1) -> Result<(), ValueTreeError> {
        match parsed {
            ParsedCell::Record => {
                if self.marker || value != IndustryValueV1::Unit {
                    return Err(invalid());
                }
                self.marker = true;
            }
            ParsedCell::Column(c) => {
                let index = IndustryColumnV1::ALL
                    .iter()
                    .position(|candidate| *candidate == c)
                    .ok_or(invalid())?;
                if self.columns[index].replace(value).is_some() {
                    return Err(invalid());
                }
            }
            ParsedCell::Collection(c) => {
                if self.collections[c.index()].replace(value).is_some() {
                    return Err(invalid());
                }
            }
            ParsedCell::Text(c, index) => self.members[c.index()].add(0, Some(index))?,
            ParsedCell::Stack(c, index, part) => self.members[c.index()].add(
                match part {
                    IndustryStackColumnV1::Kind => 0,
                    IndustryStackColumnV1::Amount => 1,
                },
                Some(index),
            )?,
            ParsedCell::Balance(c) => self.members[c.index()].add(0, None)?,
        }
        Ok(())
    }
    fn column(&self, column: IndustryColumnV1) -> Result<IndustryValueV1, ValueTreeError> {
        let index = IndustryColumnV1::ALL
            .iter()
            .position(|candidate| *candidate == column)
            .ok_or(invalid())?;
        self.columns[index].ok_or(invalid())
    }
    fn finish(&self, kind: IndustryRecordKindV1) -> Result<(), ValueTreeError> {
        if !self.metadata || !self.marker {
            return Err(invalid());
        }
        let key = kind.sample();
        for (index, column) in IndustryColumnV1::ALL.into_iter().enumerate() {
            if self.columns[index].is_some() != key.accepts(column) {
                return Err(invalid());
            }
            if !key.accepts(column) {
                continue;
            }
            if let Some(parent) = read::optional_parent(IndustryCellV1::Column(column)) {
                let present = match self.column(parent)? {
                    IndustryValueV1::Bool(present) => present,
                    _ => return Err(invalid()),
                };
                let value = self.columns[index].unwrap();
                if !present && value != IndustryValueV1::Unit
                    || present
                        && read::required_nested(IndustryCellV1::Column(column))
                        && value == IndustryValueV1::Unit
                {
                    return Err(invalid());
                }
            }
        }
        use IndustryCollectionV1 as C;
        for collection in C::ALL {
            let index = collection.index();
            if self.collections[index].is_some() != collection.accepts(key.kind) {
                return Err(invalid());
            }
            if !collection.accepts(key.kind) {
                continue;
            }
            let value = self.collections[index].unwrap();
            let present = if let Some(parent) =
                read::optional_parent(IndustryCellV1::Collection(collection))
            {
                match self.column(parent)? {
                    IndustryValueV1::Bool(present) => present,
                    _ => return Err(invalid()),
                }
            } else {
                true
            };
            if !present && value != IndustryValueV1::Unit
                || present
                    && read::required_nested(IndustryCellV1::Collection(collection))
                    && value == IndustryValueV1::Unit
            {
                return Err(invalid());
            }
            let count = match value {
                IndustryValueV1::Unit => 0,
                IndustryValueV1::Unsigned(n) => n,
                _ => return Err(invalid()),
            };
            let parts = if collection.member_type() == IndustryCollectionTypeV1::Stack {
                2
            } else {
                1
            };
            for part in 0..parts {
                if self.members[index].count[part] != count
                    || self.members[index].maximum[part].is_some_and(|maximum| maximum >= count)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
}

/// Full explicit child scope only; this does not mint a family/World token.
pub(super) fn verify_child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: IndustryRecordKindV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload: u64,
    work: &mut IndustryWorkBudgetV1,
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
                .ok_or(ValueTreeError::ResourceLimited("industry closure cells"))?;
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
            return Err(ValueTreeError::ResourceLimited("industry closure scan"));
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
