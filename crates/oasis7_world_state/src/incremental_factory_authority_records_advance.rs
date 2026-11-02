//! Inherit closed schema from an exact parent, validating only changed paths.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
    incremental_record_tree,
};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateRootV1},
    incremental_value::ValueNodeReader,
};
fn classify<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    new: bool,
) -> Result<Option<FactoryAuthorityCellV1>, ValueTreeError> {
    if entry.namespace != kind.namespace() {
        return Err(read::invalid());
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(read::invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        records.presence(&entry.value, &key, 0)?;
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(read::invalid());
    }
    let mut probe = key_read::CellKeyProbe::new();
    let info = read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: records.record_bytes,
            record_bytes: records.record_bytes,
        },
        &mut probe,
        &mut *records.cancelled,
    );
    if probe.bad {
        return Err(read::invalid());
    }
    let info = info?;
    let cell = probe.finish(kind)?;
    if info.codec != cell.codec() {
        return Err(read::invalid());
    }
    if new {
        records.values.verify_closure(
            &entry.value,
            records.record_bytes,
            &mut *records.cancelled,
        )?;
        read::value(
            cell,
            &entry.value,
            &(key, info.semantic_bytes),
            records,
            max,
            &mut std::io::sink(),
        )?;
    }
    Ok(Some(cell))
}
fn count<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u64, ValueTreeError> {
    let key = read::cell_key(
        FactoryAuthorityRecordKindV1::Site,
        FactoryAuthorityCellV1::AuthorizedCount,
        records,
    )?;
    let root = incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?
        .ok_or_else(read::invalid)?;
    match read::value(
        FactoryAuthorityCellV1::AuthorizedCount,
        &root,
        &key,
        records,
        8,
        &mut std::io::sink(),
    )? {
        FactoryAuthorityValueV1::Unsigned(n) => Ok(n),
        _ => Err(read::invalid()),
    }
}
#[derive(Default)]
struct ListChanges {
    touched: bool,
    added: u64,
    removed: u64,
    old_max: Option<u64>,
    new_max: Option<u64>,
    added_min: Option<u64>,
    removed_min: Option<u64>,
}
impl ListChanges {
    fn observe(&mut self, index: u64, old: bool, new: bool) -> Result<(), ValueTreeError> {
        self.touched = true;
        if old {
            self.old_max = Some(self.old_max.map_or(index, |n| n.max(index)));
        }
        if new {
            self.new_max = Some(self.new_max.map_or(index, |n| n.max(index)));
        }
        if !old {
            self.added = self.added.checked_add(1).ok_or_else(read::invalid)?;
            self.added_min = Some(self.added_min.map_or(index, |n| n.min(index)));
        }
        if !new {
            self.removed = self.removed.checked_add(1).ok_or_else(read::invalid)?;
            self.removed_min = Some(self.removed_min.map_or(index, |n| n.min(index)));
        }
        Ok(())
    }
}
pub(super) fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(read::invalid());
    }
    let mut list = ListChanges::default();
    let mut option = false;
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "factory authority diff",
            cells: "factory authority changed paths",
            stack: "factory authority diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(read::invalid)?;
            let cell = classify(kind, entry, records, max, new.is_some())?;
            match cell {
                Some(FactoryAuthorityCellV1::AuthorizedAgent(index)) => {
                    list.observe(index, old.is_some(), new.is_some())?
                }
                Some(FactoryAuthorityCellV1::SourceModuleId) => option = true,
                cell => {
                    if old.is_none() || new.is_none() {
                        return Err(read::invalid());
                    }
                    if cell == Some(FactoryAuthorityCellV1::AuthorizedCount) {
                        list.touched = true;
                    }
                    if cell == Some(FactoryAuthorityCellV1::SourceModulePresent) {
                        option = true;
                    }
                }
            }
            Ok(())
        },
    )?;
    if list.touched {
        let old = count(before, records)?;
        let new = count(after, records)?;
        if list.old_max.is_some_and(|i| i >= old)
            || list.new_max.is_some_and(|i| i >= new)
            || list.added_min.is_some_and(|i| i < old)
            || list.removed_min.is_some_and(|i| i < new)
            || old
                .checked_add(list.added)
                .and_then(|n| n.checked_sub(list.removed))
                != Some(new)
        {
            return Err(read::invalid());
        }
    }
    if option {
        let flag = read::cell_key(kind, FactoryAuthorityCellV1::SourceModulePresent, records)?;
        let flag_root = incremental_record_tree::lookup(after, &flag.0, records, genesis::LABELS)?
            .ok_or_else(read::invalid)?;
        let value = read::value(
            FactoryAuthorityCellV1::SourceModulePresent,
            &flag_root,
            &flag,
            records,
            1,
            &mut std::io::sink(),
        )?;
        let key = read::cell_key(kind, FactoryAuthorityCellV1::SourceModuleId, records)?;
        let present =
            incremental_record_tree::lookup(after, &key.0, records, genesis::LABELS)?.is_some();
        if value != FactoryAuthorityValueV1::Bool(present) {
            return Err(read::invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
