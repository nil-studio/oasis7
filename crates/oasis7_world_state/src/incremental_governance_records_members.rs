//! Atomic complete-member insertion/removal. Fixed cells remain point-editable.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as record_tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance member relation/expected")
}
#[derive(Clone, Copy)]
pub enum GovernanceMemberValueV1<'a> {
    Option(&'a str),
    Snapshot {
        id: &'a str,
        value: &'a GovernanceVoteWeightSnapshotState,
    },
    Ballot {
        id: &'a str,
        value: &'a GovernanceVoteBallotState,
    },
    Tally {
        id: &'a str,
        value: u64,
    },
}
#[derive(Clone, Copy)]
pub enum GovernanceMemberKeyV1<'a> {
    Option(u64),
    Snapshot(&'a str),
    Ballot(&'a str),
    Tally(&'a str),
}
#[derive(Clone, Copy)]
pub enum GovernanceMemberEditV1<'a> {
    Insert(GovernanceMemberValueV1<'a>),
    Remove(GovernanceMemberKeyV1<'a>),
}
impl GovernanceMemberEditV1<'_> {
    fn collection(self) -> GovernanceCollectionV1 {
        match self {
            Self::Insert(GovernanceMemberValueV1::Option(_))
            | Self::Remove(GovernanceMemberKeyV1::Option(_)) => GovernanceCollectionV1::Options,
            Self::Insert(GovernanceMemberValueV1::Snapshot { .. })
            | Self::Remove(GovernanceMemberKeyV1::Snapshot(_)) => GovernanceCollectionV1::Snapshot,
            Self::Insert(GovernanceMemberValueV1::Ballot { .. })
            | Self::Remove(GovernanceMemberKeyV1::Ballot(_)) => GovernanceCollectionV1::Ballots,
            _ => GovernanceCollectionV1::Tallies,
        }
    }
}
pub struct GovernanceMemberChangeV1<'a> {
    pub key: GovernanceRecordKeyV1<'a>,
    pub qualification: &'a ReservedValue<GovernanceRecordQualificationV1>,
    pub expected_count: u64,
    pub edit: GovernanceMemberEditV1<'a>,
}
fn visit_edit<'a>(
    edit: GovernanceMemberEditV1<'a>,
    old: u64,
    mut emit: impl FnMut(
        GovernanceCellV1<'a>,
        Option<MemberPayloadRefV1<'a>>,
    ) -> Result<(), WorldFieldOverlayErrorV1>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    use GovernanceMemberEditV1 as E;
    use GovernanceMemberKeyV1 as K;
    use GovernanceMemberValueV1 as V;
    match edit {
        E::Insert(V::Option(value)) => emit(
            GovernanceCellV1::Option(old),
            Some(MemberPayloadRefV1::Text(value)),
        ),
        E::Insert(V::Tally { id, value }) => emit(
            GovernanceCellV1::Tally(id),
            Some(MemberPayloadRefV1::U64(value)),
        ),
        E::Insert(V::Snapshot { id, value }) => {
            visit::snapshot(id, value, &mut |cell, payload| emit(cell, Some(payload)))
        }
        E::Insert(V::Ballot { id, value }) => {
            visit::ballot(id, value, &mut |cell, payload| emit(cell, Some(payload)))
        }
        E::Remove(K::Option(index)) => {
            if old.checked_sub(1) != Some(index) {
                return Err(invalid().into());
            }
            emit(GovernanceCellV1::Option(index), None)
        }
        E::Remove(K::Tally(id)) => emit(GovernanceCellV1::Tally(id), None),
        E::Remove(K::Snapshot(id)) => {
            for c in GovernanceSnapshotColumnV1::ALL {
                emit(GovernanceCellV1::Snapshot(id, c), None)?;
            }
            Ok(())
        }
        E::Remove(K::Ballot(id)) => {
            for c in GovernanceBallotColumnV1::ALL {
                emit(GovernanceCellV1::Ballot(id, c), None)?;
            }
            Ok(())
        }
    }
}
/// The exact qualified current child is the complete expected member value.
/// Vec structural edits are append/pop only; map keys retain original bytes.
pub fn change_qualified_governance_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: GovernanceMemberChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut GovernanceWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<GovernanceRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let collection = change.edit.collection();
        if !collection.accepts(change.key.kind)
            || ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let _workspace = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        work.cell()?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            point::resolve(
                GovernanceCellRequestV1 {
                    key: change.key,
                    cell: GovernanceCellV1::Count(collection),
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
                8,
                &mut std::io::sink(),
            )?
        };
        let Some(GovernanceValueV1::Unsigned(old)) = resolved.value else {
            return Err(invalid().into());
        };
        if old != change.expected_count {
            return Err(invalid().into());
        }
        let insert = matches!(change.edit, GovernanceMemberEditV1::Insert(_));
        let new = if insert {
            old.checked_add(1)
        } else {
            old.checked_sub(1)
        }
        .ok_or_else(invalid)?;
        let mut child = resolved.child;
        let policy = RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: genesis::LABELS,
        };
        visit_edit(change.edit, old, |cell, payload| {
            work.cell()?;
            let (key, before) = {
                let mut combined = || cancel() || (write.cancelled)();
                let mut records = InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut combined,
                };
                let key = read::cell_key(change.key.kind, cell, &mut records)?;
                let before = record_tree::lookup(&child, &key.0, &mut records, genesis::LABELS)?;
                if before.is_some() == insert {
                    return Err(invalid().into());
                }
                (key, before)
            };
            let after = if let Some(payload) = payload {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = write_cell(change.key, cell, payload, &mut combined)?;
                if row.field_key != key.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            child = record_tree::edit(
                child.clone(),
                &key.0,
                (before, after),
                &mut ctx.values,
                write,
                cancel,
                policy,
            )?;
            Ok(())
        })?;
        work.cell()?;
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            write_cell(
                change.key,
                GovernanceCellV1::Count(collection),
                MemberPayloadRefV1::U64(new),
                &mut combined,
            )?
        };
        if row.field_key != resolved.key.0 {
            return Err(invalid().into());
        }
        child = record_tree::edit(
            child,
            &resolved.key.0,
            (resolved.before, Some(row.value_root)),
            &mut ctx.values,
            write,
            cancel,
            policy,
        )?;
        work.cell()?;
        write::publish(change.key, child, resolved.outer, tx, write, cancel)
    })
}
