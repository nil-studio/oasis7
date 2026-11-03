//! Preserve qualified record closure using only authenticated changed paths.
use super::key_read::ParsedCell;
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{incremental_state::StateRootV1, incremental_value::ValueNodeReader};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("logistics changed closure relation")
}
fn count<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u64, ValueTreeError> {
    let cell = LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds);
    let key = with_logistics_cell_key!(cell, encoded => records.key(kind.descriptor()?, WorldFieldRecordKindV1::Entry, encoded))?;
    let keys = std::array::from_fn(|index| if index == 0 { Some(key.clone()) } else { None });
    let value = read::inner_roots(root, &keys, records)?[0].ok_or_else(invalid)?;
    match read::value(cell, &value, &key, records, 8, &mut std::io::sink())? {
        LogisticsValueV1::Unsigned(n) => Ok(n),
        _ => Err(invalid()),
    }
}
#[derive(Default)]
struct Changes {
    touched: bool,
    added: u64,
    removed: u64,
    old_max: Option<u64>,
    new_max: Option<u64>,
    added_min: Option<u64>,
    removed_min: Option<u64>,
}
impl Changes {
    fn observe(&mut self, index: u64, old: bool, new: bool) -> Result<(), ValueTreeError> {
        self.touched = true;
        if old {
            self.old_max = Some(self.old_max.map_or(index, |n| n.max(index)));
        }
        if new {
            self.new_max = Some(self.new_max.map_or(index, |n| n.max(index)));
        }
        if !old {
            self.added = self.added.checked_add(1).ok_or_else(invalid)?;
            self.added_min = Some(self.added_min.map_or(index, |n| n.min(index)));
        }
        if !new {
            self.removed = self.removed.checked_add(1).ok_or_else(invalid)?;
            self.removed_min = Some(self.removed_min.map_or(index, |n| n.min(index)));
        }
        Ok(())
    }
}
pub(super) fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: LogisticsRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut LogisticsWorkBudgetV1,
    max: u64,
) -> Result<(), ValueTreeError> {
    let _workspace = records
        .values
        .memory
        .reserve(16 << 10, records.values.class)?;
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(invalid());
    }
    let mut changes = Changes::default();
    diff::visit(before, after, records, work, |old, new, records| {
        let entry = new.or(old).ok_or_else(invalid)?;
        let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        // A qualified record always retains its metadata, marker and required columns.
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            return Err(invalid());
        }
        let (parsed, _) = closure::cell(kind, new.unwrap_or(entry), records, max)?;
        match parsed {
            ParsedCell::Record | ParsedCell::Column(_) => {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
            }
            ParsedCell::Collection => {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
                changes.touched = true;
            }
            ParsedCell::Text(index) => changes.observe(index, old.is_some(), new.is_some())?,
        }
        Ok(())
    })?;
    if changes.touched {
        let old = count(kind, before, records)?;
        let new = count(kind, after, records)?;
        if changes.old_max.is_some_and(|index| index >= old)
            || changes.new_max.is_some_and(|index| index >= new)
            || changes.added_min.is_some_and(|index| index < old)
            || changes.removed_min.is_some_and(|index| index < new)
            || old
                .checked_add(changes.added)
                .and_then(|n| n.checked_sub(changes.removed))
                != Some(new)
        {
            return Err(invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
