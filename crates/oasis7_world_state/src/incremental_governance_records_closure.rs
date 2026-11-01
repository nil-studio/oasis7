//! Explicit bounded full-record bootstrap. Ordinary updates inherit unchanged closure.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader, incremental_record_tree,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateRootV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance closed record")
}
#[derive(Debug)]
pub struct GovernanceRecordQualificationV1 {
    kind: GovernanceRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
}
impl GovernanceRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        kind: GovernanceRecordKindV1,
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.kind != kind
            || self.owner != *owner
            || self.root != *root
            || root.namespace != kind.namespace()
        {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn mint(
    kind: GovernanceRecordKindV1,
    owner: [u8; 32],
    root: StateRootV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<GovernanceRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(GovernanceRecordQualificationV1 { kind, owner, root }))
}
fn spend(cells: &mut u64) -> Result<(), ValueTreeError> {
    *cells = cells
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("governance closure cells"))?;
    Ok(())
}
struct CheckedCell {
    key: (WorldFieldKeyV1, u64),
    root: [u8; 32],
    value: GovernanceValueV1,
}
fn check<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<CheckedCell, ValueTreeError> {
    spend(cells)?;
    let key = read::cell_key(kind, cell, records)?;
    let value = incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?
        .ok_or_else(invalid)?;
    records
        .values
        .verify_closure(&value, records.record_bytes, &mut *records.cancelled)?;
    let decoded = read::value(cell, &value, &key, records, max, &mut std::io::sink())?;
    Ok(CheckedCell {
        key,
        root: value,
        value: decoded,
    })
}
fn index(c: GovernanceCollectionV1) -> usize {
    match c {
        GovernanceCollectionV1::Options => 0,
        GovernanceCollectionV1::Snapshot => 1,
        GovernanceCollectionV1::Ballots => 2,
        GovernanceCollectionV1::Tallies => 3,
    }
}
pub fn qualify_governance_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: GovernanceRecordKindV1,
    owner: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<GovernanceRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != kind.namespace() {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(16 << 10, ctx.values.class)?;
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    spend(cells)?;
    let meta_root = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    ctx.values
        .verify_closure(&meta_root, ctx.record_bytes, &mut ctx.cancelled)?;
    ctx.records().presence(&meta_root, &meta, 0)?;
    let mut keys = std::array::from_fn::<_, 32, _>(|_| None);
    let mut known = 0usize;
    let mut counts = [0u64; 4];
    for cell in std::iter::once(GovernanceCellV1::Record)
        .chain(
            GovernanceColumnV1::ALL
                .into_iter()
                .filter(|c| c.accepts(kind))
                .map(GovernanceCellV1::Column),
        )
        .chain(
            GovernanceCollectionV1::ALL
                .into_iter()
                .filter(|c| c.accepts(kind))
                .map(GovernanceCellV1::Count),
        )
    {
        let CheckedCell {
            key,
            root: value,
            value: decoded,
        } = check(kind, cell, ctx.root, &mut ctx.records(), cells, max)?;
        if let GovernanceCellV1::Count(c) = cell {
            let GovernanceValueV1::Unsigned(n) = decoded else {
                return Err(invalid());
            };
            counts[index(c)] = n;
        }
        keys[known] = Some((key, value));
        known += 1;
    }
    let expected = (known as u64)
        .checked_add(1)
        .and_then(|v| v.checked_add(counts[0]))
        .and_then(|v| counts[1].checked_mul(5).and_then(|n| v.checked_add(n)))
        .and_then(|v| counts[2].checked_mul(3).and_then(|n| v.checked_add(n)))
        .and_then(|v| v.checked_add(counts[3]))
        .ok_or_else(invalid)?;
    if ctx.root.leaf_count != expected {
        return Err(invalid());
    }
    if expected > *cells {
        return Err(ValueTreeError::ResourceLimited(
            "governance full scan cells",
        ));
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut observed = [0u64; 4];
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: || (cancel.borrow_mut())(),
    };
    let result = StateScanContext {
        source: ctx.state,
        memory: local.values.memory,
        class: local.values.class,
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| -> Result<(), ValueTreeError> {
            spend(cells)?;
            if entry.namespace != kind.namespace() {
                return Err(invalid());
            }
            if entry.key == meta.as_bytes() {
                return if entry.value == meta_root {
                    Ok(())
                } else {
                    Err(invalid())
                };
            }
            if let Some((_, value)) = keys
                .iter()
                .flatten()
                .find(|(key, _)| key.0.as_bytes() == entry.key)
            {
                return if entry.value == *value {
                    Ok(())
                } else {
                    Err(invalid())
                };
            }
            key_read::with_cell(
                kind,
                entry,
                &mut local.records(),
                max,
                false,
                |cell, key, records| {
                    records.values.verify_closure(
                        &entry.value,
                        records.record_bytes,
                        &mut *records.cancelled,
                    )?;
                    read::value(cell, &entry.value, key, records, max, &mut std::io::sink())?;
                    let group = match cell {
                        GovernanceCellV1::Option(i) => {
                            if i >= counts[0] {
                                return Err(invalid());
                            }
                            Some(0)
                        }
                        GovernanceCellV1::Snapshot(id, c) => {
                            if c == GovernanceSnapshotColumnV1::AgentId {
                                for c in GovernanceSnapshotColumnV1::ALL
                                    .into_iter()
                                    .filter(|c| *c != GovernanceSnapshotColumnV1::AgentId)
                                {
                                    check(
                                        kind,
                                        GovernanceCellV1::Snapshot(id, c),
                                        ctx.root,
                                        records,
                                        cells,
                                        max,
                                    )?;
                                }
                                Some(1)
                            } else {
                                None
                            }
                        }
                        GovernanceCellV1::Ballot(id, c) => {
                            if c == GovernanceBallotColumnV1::Option {
                                for c in GovernanceBallotColumnV1::ALL
                                    .into_iter()
                                    .filter(|c| *c != GovernanceBallotColumnV1::Option)
                                {
                                    check(
                                        kind,
                                        GovernanceCellV1::Ballot(id, c),
                                        ctx.root,
                                        records,
                                        cells,
                                        max,
                                    )?;
                                }
                                Some(2)
                            } else {
                                None
                            }
                        }
                        GovernanceCellV1::Tally(_) => Some(3),
                        _ => return Err(invalid()),
                    };
                    if let Some(group) = group {
                        observed[group] = observed[group].checked_add(1).ok_or_else(invalid)?;
                    }
                    Ok(())
                },
            )
        },
        || (cancel.borrow_mut())(),
    );
    let error = (local.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = error {
        return Err(error);
    }
    match result {
        Ok(n) if n == expected && counts == observed => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("governance full scan"));
        }
        _ => return Err(invalid()),
    }
    Ok(retained.retain(GovernanceRecordQualificationV1 {
        kind,
        owner,
        root: ctx.root.clone(),
    }))
}
