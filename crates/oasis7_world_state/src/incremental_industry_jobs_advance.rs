//! Derive a closed child from a previously qualified child using changed paths.
use super::key_read::ParsedCell;
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{incremental_state::StateRootV1, incremental_value::ValueNodeReader};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("industry changed closure relation")
}
#[derive(Clone, Copy)]
struct Probe {
    unit: bool,
    unsigned: Option<u64>,
    boolean: Option<bool>,
}
fn probe<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: IndustryRecordKindV1,
    root: &StateRootV1,
    cell: IndustryCellV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
) -> Result<Probe, ValueTreeError> {
    let key = with_industry_cell_key!(cell,encoded=>records.key(kind.descriptor()?,WorldFieldRecordKindV1::Entry,encoded))?;
    let keys = std::array::from_fn(|index| if index == 0 { Some(key.clone()) } else { None });
    let value = read::inner_roots(root, &keys, records)?[0].ok_or(invalid())?;
    let (codec, nullable) = cell.profile();
    if matches!(
        codec,
        MemberPayloadCodecV1::Text | MemberPayloadCodecV1::Ledger
    ) {
        let info = read_member_record_header_v1(
            &value,
            &key.0,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: max,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        if info.semantic_bytes != key.1
            || !(info.codec == codec || nullable && info.codec == MemberPayloadCodecV1::Unit)
        {
            return Err(invalid());
        }
        Ok(Probe {
            unit: info.codec == MemberPayloadCodecV1::Unit,
            unsigned: None,
            boolean: None,
        })
    } else {
        let value = read::value(
            cell,
            &value,
            &key,
            records,
            max.min(8),
            &mut std::io::sink(),
        )?;
        Ok(Probe {
            unit: value == IndustryValueV1::Unit,
            unsigned: if let IndustryValueV1::Unsigned(n) = value {
                Some(n)
            } else {
                None
            },
            boolean: if let IndustryValueV1::Bool(b) = value {
                Some(b)
            } else {
                None
            },
        })
    }
}
fn count(value: Probe) -> Result<u64, ValueTreeError> {
    if value.unit {
        Ok(0)
    } else {
        value.unsigned.ok_or(invalid())
    }
}
#[derive(Default)]
struct Members {
    added: [u64; 2],
    removed: [u64; 2],
    old_max: Option<u64>,
    new_max: Option<u64>,
    added_min: Option<u64>,
    removed_min: Option<u64>,
    touched: bool,
}
impl Members {
    fn observe(
        &mut self,
        index: Option<u64>,
        part: usize,
        old: bool,
        new: bool,
    ) -> Result<(), ValueTreeError> {
        self.touched = true;
        if old && let Some(index) = index {
            self.old_max = Some(self.old_max.map_or(index, |n| n.max(index)));
        }
        if new && let Some(index) = index {
            self.new_max = Some(self.new_max.map_or(index, |n| n.max(index)));
        }
        if !old {
            self.added[part] = self.added[part].checked_add(1).ok_or(invalid())?;
            if let Some(index) = index {
                self.added_min = Some(self.added_min.map_or(index, |n| n.min(index)));
            }
        }
        if !new {
            self.removed[part] = self.removed[part].checked_add(1).ok_or(invalid())?;
            if let Some(index) = index {
                self.removed_min = Some(self.removed_min.map_or(index, |n| n.min(index)));
            }
        }
        Ok(())
    }
}
const PARENTS: [IndustryColumnV1; 2] = [
    IndustryColumnV1::RecordIndustrialRootRef,
    IndustryColumnV1::RecordCommittedRecipeOrigin,
];
fn parent_slot(parent: IndustryColumnV1) -> Result<usize, ValueTreeError> {
    PARENTS
        .iter()
        .position(|candidate| *candidate == parent)
        .ok_or(invalid())
}
fn relation(cell: IndustryCellV1<'_>, unit: bool, present: bool) -> Result<(), ValueTreeError> {
    if !present && !unit || present && read::required_nested(cell) && unit {
        Err(invalid())
    } else {
        Ok(())
    }
}
pub(super) fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: IndustryRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut IndustryWorkBudgetV1,
    max: u64,
) -> Result<(), ValueTreeError> {
    let _workspace = records
        .values
        .memory
        .reserve(16 << 10, records.values.class)?;
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(invalid());
    }
    let mut columns = [None; 52];
    let mut members: [Members; 10] = std::array::from_fn(|_| Members::default());
    let mut parent_changed = [false; 2];
    let mut parent_needed = [false; 2];
    diff::visit(before, after, records, work, |old, new, records| {
        let entry = new.or(old).ok_or(invalid())?;
        let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            return Err(invalid());
        }
        let (parsed, value) = closure::cell(kind, new.unwrap_or(entry), records, max)?;
        match parsed {
            ParsedCell::Record => {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
            }
            ParsedCell::Column(column) => {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
                let index = IndustryColumnV1::ALL
                    .iter()
                    .position(|c| *c == column)
                    .ok_or(invalid())?;
                if columns[index].replace(value).is_some() {
                    return Err(invalid());
                }
                if let Some(parent) = read::optional_parent(IndustryCellV1::Column(column)) {
                    parent_needed[parent_slot(parent)?] = true;
                }
                if let Some(index) = PARENTS.iter().position(|c| *c == column) {
                    parent_changed[index] = true;
                    parent_needed[index] = true;
                }
            }
            ParsedCell::Collection(collection) => {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
                members[collection.index()].touched = true;
            }
            ParsedCell::Text(collection, index) => {
                members[collection.index()].observe(Some(index), 0, old.is_some(), new.is_some())?
            }
            ParsedCell::Stack(collection, index, part) => members[collection.index()].observe(
                Some(index),
                match part {
                    IndustryStackColumnV1::Kind => 0,
                    IndustryStackColumnV1::Amount => 1,
                },
                old.is_some(),
                new.is_some(),
            )?,
            ParsedCell::Balance(collection) => {
                members[collection.index()].observe(None, 0, old.is_some(), new.is_some())?
            }
        }
        Ok(())
    })?;
    // Only changed dependencies are queried. A parent flag flip is the explicit
    // operation that requests validation of its fixed descendant schema group.
    for collection in IndustryCollectionV1::ALL {
        if !collection.accepts(kind) {
            continue;
        }
        if let Some(parent) = read::optional_parent(IndustryCellV1::Collection(collection)) {
            let slot = parent_slot(parent)?;
            if parent_changed[slot] {
                members[collection.index()].touched = true;
            }
            if members[collection.index()].touched {
                parent_needed[slot] = true;
            }
        }
    }
    let mut parents = [None; 2];
    for (index, parent) in PARENTS.into_iter().enumerate() {
        if parent_needed[index] {
            parents[index] = Some(
                probe(kind, after, IndustryCellV1::Column(parent), records, 1)?
                    .boolean
                    .ok_or(invalid())?,
            );
        }
    }
    for (index, column) in IndustryColumnV1::ALL.into_iter().enumerate() {
        if !kind.sample().accepts(column) {
            continue;
        }
        let cell = IndustryCellV1::Column(column);
        let Some(parent) = read::optional_parent(cell) else {
            continue;
        };
        let slot = parent_slot(parent)?;
        let unit = if let Some(value) = columns[index] {
            Some(value == IndustryValueV1::Unit)
        } else if parent_changed[slot] {
            Some(probe(kind, after, cell, records, max)?.unit)
        } else {
            None
        };
        if let Some(unit) = unit {
            relation(cell, unit, parents[slot].ok_or(invalid())?)?;
        }
    }
    use IndustryCollectionV1 as C;
    for collection in C::ALL {
        let changes = &members[collection.index()];
        if !changes.touched {
            continue;
        }
        let cell = IndustryCellV1::Collection(collection);
        let old = probe(kind, before, cell, records, 8)?;
        let new = probe(kind, after, cell, records, 8)?;
        if let Some(parent) = read::optional_parent(cell) {
            relation(
                cell,
                new.unit,
                parents[parent_slot(parent)?].ok_or(invalid())?,
            )?;
        }
        let old_count = count(old)?;
        let new_count = count(new)?;
        if changes.old_max.is_some_and(|index| index >= old_count)
            || changes.new_max.is_some_and(|index| index >= new_count)
            || changes.added_min.is_some_and(|index| index < old_count)
            || changes.removed_min.is_some_and(|index| index < new_count)
        {
            return Err(invalid());
        }
        let parts = if collection.member_type() == IndustryCollectionTypeV1::Stack {
            2
        } else {
            1
        };
        for part in 0..parts {
            if old_count
                .checked_add(changes.added[part])
                .and_then(|n| n.checked_sub(changes.removed[part]))
                != Some(new_count)
            {
                return Err(invalid());
            }
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
