//! Qualified-parent inheritance plus authenticated changed-cell relations.
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
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance changed closure relation")
}
#[derive(Clone, Copy, Default)]
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
    fn observe(
        &mut self,
        old: bool,
        new: bool,
        ordinal: Option<u64>,
    ) -> Result<(), ValueTreeError> {
        self.touched |= old != new;
        if !old {
            self.added = self.added.checked_add(1).ok_or_else(invalid)?;
            if let Some(i) = ordinal {
                self.added_min = Some(self.added_min.map_or(i, |n| n.min(i)));
            }
        }
        if !new {
            self.removed = self.removed.checked_add(1).ok_or_else(invalid)?;
            if let Some(i) = ordinal {
                self.removed_min = Some(self.removed_min.map_or(i, |n| n.min(i)));
            }
        }
        if let Some(i) = ordinal {
            if old {
                self.old_max = Some(self.old_max.map_or(i, |n| n.max(i)));
            }
            if new {
                self.new_max = Some(self.new_max.map_or(i, |n| n.max(i)));
            }
        }
        Ok(())
    }
    fn check(self, old: u64, new: u64, ordered: bool) -> Result<(), ValueTreeError> {
        if old
            .checked_add(self.added)
            .and_then(|n| n.checked_sub(self.removed))
            != Some(new)
            || ordered
                && (self.old_max.is_some_and(|i| i >= old)
                    || self.new_max.is_some_and(|i| i >= new)
                    || self.added_min.is_some_and(|i| i < old)
                    || self.removed_min.is_some_and(|i| i < new))
        {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
fn index(c: GovernanceCollectionV1) -> usize {
    match c {
        GovernanceCollectionV1::Options => 0,
        GovernanceCollectionV1::Snapshot => 1,
        GovernanceCollectionV1::Ballots => 2,
        GovernanceCollectionV1::Tallies => 3,
    }
}
fn count<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    group: GovernanceCollectionV1,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u64, ValueTreeError> {
    let cell = GovernanceCellV1::Count(group);
    let key = read::cell_key(kind, cell, records)?;
    let value = incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?
        .ok_or_else(invalid)?;
    match read::value(cell, &value, &key, records, 8, &mut std::io::sink())? {
        GovernanceValueV1::Unsigned(n) => Ok(n),
        _ => Err(invalid()),
    }
}
fn row<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    id: &str,
    snapshot: bool,
    root: &StateRootV1,
    present: bool,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    if snapshot {
        for c in GovernanceSnapshotColumnV1::ALL {
            let key = read::cell_key(kind, GovernanceCellV1::Snapshot(id, c), records)?;
            if incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?.is_some()
                != present
            {
                return Err(invalid());
            }
        }
    } else {
        for c in GovernanceBallotColumnV1::ALL {
            let key = read::cell_key(kind, GovernanceCellV1::Ballot(id, c), records)?;
            if incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?.is_some()
                != present
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}
fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    entry: &StateEntryV1,
    new: bool,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    visit: impl FnOnce(
        GovernanceCellV1<'_>,
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    key_read::with_cell(kind, entry, records, max, !new, |cell, key, records| {
        if new {
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            read::value(cell, &entry.value, key, records, max, &mut std::io::sink())?;
        }
        visit(cell, records)
    })
}
pub(super) fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(invalid());
    }
    let mut changes = [Changes::default(); 4];
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "governance diff",
            cells: "governance changed paths",
            stack: "governance diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            if entry.namespace != kind.namespace() {
                return Err(invalid());
            }
            if entry.key == meta.as_bytes() {
                if old.is_none() || new.is_none() {
                    return Err(invalid());
                }
                records.values.verify_closure(
                    &entry.value,
                    records.record_bytes,
                    &mut *records.cancelled,
                )?;
                records.presence(&entry.value, &meta, 0)?;
                return Ok(());
            }
            changed(kind, entry, new.is_some(), records, max, |cell, records| {
                match cell {
                    GovernanceCellV1::Record | GovernanceCellV1::Column(_) => {
                        if old.is_none() || new.is_none() {
                            return Err(invalid());
                        }
                    }
                    GovernanceCellV1::Count(group) => {
                        if old.is_none() || new.is_none() {
                            return Err(invalid());
                        }
                        changes[index(group)].touched = true;
                    }
                    GovernanceCellV1::Option(i) => {
                        changes[0].observe(old.is_some(), new.is_some(), Some(i))?
                    }
                    GovernanceCellV1::Snapshot(id, c) => {
                        if old.is_some() != new.is_some() {
                            row(kind, id, true, after, new.is_some(), records)?;
                        }
                        if c == GovernanceSnapshotColumnV1::AgentId {
                            changes[1].observe(old.is_some(), new.is_some(), None)?;
                        }
                    }
                    GovernanceCellV1::Ballot(id, c) => {
                        if old.is_some() != new.is_some() {
                            row(kind, id, false, after, new.is_some(), records)?;
                        }
                        if c == GovernanceBallotColumnV1::Option {
                            changes[2].observe(old.is_some(), new.is_some(), None)?;
                        }
                    }
                    GovernanceCellV1::Tally(_) => {
                        changes[3].observe(old.is_some(), new.is_some(), None)?
                    }
                }
                Ok(())
            })
        },
    )?;
    for group in GovernanceCollectionV1::ALL {
        let delta = changes[index(group)];
        if delta.touched {
            delta.check(
                count(kind, group, before, records)?,
                count(kind, group, after, records)?,
                group == GovernanceCollectionV1::Options,
            )?;
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
