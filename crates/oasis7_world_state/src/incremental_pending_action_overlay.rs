//! Ordinary queue edits in one shared, poisoned-on-error execution transaction.
use crate::{
    ActionEnvelope,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_pending_action_family::{
        PendingActionIndexWorkV1, PendingActionMetaV1, PendingActionPositionV1 as Position,
        PendingActionQualificationV1,
    },
    incremental_pending_action_index::PendingActionIndexV1,
    incremental_runtime_pending_action_record::{
        PendingActionWriteWorkV1, prepare_pending_action_record_v1,
    },
    incremental_runtime_pending_actions::PENDING_ACTIONS_FIELD_V1,
    incremental_typed_record_codec::*,
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::{Cell, RefCell};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("pending action current overlay")
}
pub struct PendingActionCurrentV1 {
    parent: [u8; 32],
    guard: PreparedGuard,
    index: RefCell<PendingActionIndexV1>,
    next: Cell<Option<Position>>,
}
fn successor(p: Position) -> Option<Position> {
    p.ordinal.checked_add(1).map(|ordinal| Position {
        era: p.era,
        ordinal,
    })
}

fn check<M: MerkleNodeReader>(
    current: &ReservedValue<PendingActionCurrentV1>,
    tx: &ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    tx.validate_transaction_identity(&current.guard)?;
    tx.validate_guard(guard, live)?;
    if !tx.uses_budget(memory)
        || !current.uses_budget(memory)
        || current.reserved_bytes() < 512
        || !current.index.borrow().uses_budget(memory)
        || current.index.borrow().class() != MemoryClass::Ordinary
        || current.parent != tx.parent_fence().parent_roots
    {
        return Err(invalid().into());
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn begin_pending_action_current_v1<M: MerkleNodeReader>(
    qualification: &ReservedValue<PendingActionQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    work: &mut PendingActionIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<PendingActionCurrentV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !tx.uses_budget(memory)
            || !qualification.uses_budget(memory)
            || qualification.reserved_bytes() < 512
            || qualification.allocation_class() != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let charge = memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        qualification.validate_binding(roots)?;
        if roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots
            || roots.state_root(StateNamespaceV1::Scheduler)
                != tx.parent_state_root(StateNamespaceV1::Scheduler)
        {
            return Err(invalid().into());
        }
        let index = qualification.index().clone();
        work.step(&mut cancelled)?;
        let era = roots
            .height
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "pending append creation height",
            ))?;
        let next = Some(Position { era, ordinal: 0 });
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(charge.retain(PendingActionCurrentV1 {
            parent: tx.parent_fence().parent_roots,
            guard: tx.guard(),
            index: RefCell::new(index),
            next: Cell::new(next),
        }))
    })
}
#[allow(clippy::too_many_arguments)]
pub fn append_pending_action_v1<
    M: MerkleNodeReader,
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
>(
    current: &mut ReservedValue<PendingActionCurrentV1>,
    source: &ActionEnvelope,
    original: OriginalArtifactRefV1,
    tx: &mut ExecutionTransaction<'_, M>,
    write: &mut MemberRecordWriteContext<'_, W, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    index_work: &mut PendingActionIndexWorkV1,
    typed_work: &mut PendingActionWriteWorkV1,
) -> Result<Position, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check(current, tx, write.memory, guard, live)?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let position = current
            .next
            .get()
            .ok_or(ValueTreeError::ResourceLimited("pending append ordinals"))?;
        let row = prepare_pending_action_record_v1(position, source, original, write, typed_work)?;
        let meta = PendingActionMetaV1 {
            position,
            action_id: source.id,
            original,
            value_root: row.value_root,
        };
        let index = current
            .index
            .borrow()
            .insert(meta, index_work, &mut write.cancelled)?;
        tx.validate_guard(&tx.guard(), live)?;
        tx.write(
            StateNamespaceV1::Scheduler,
            row.field_key.as_bytes(),
            Some(row.value_root),
        )?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        *current.index.borrow_mut() = index;
        current.next.set(successor(position));
        Ok(position)
    })
}
#[allow(clippy::too_many_arguments)]
fn remove_inner<M: MerkleNodeReader>(
    current: &mut ReservedValue<PendingActionCurrentV1>,
    position: Position,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    work: &mut PendingActionIndexWorkV1,
    keys: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    cancelled: &mut impl FnMut() -> bool,
    live: &ExecutionFence,
) -> Result<Option<PendingActionMetaV1>, WorldFieldOverlayErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let Some(meta) = current.index.borrow().get(position, work, cancelled)? else {
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        return Ok(None);
    };
    let key = hash_semantic_key_v1(
        &position.key(),
        memory,
        MemoryClass::Ordinary,
        keys,
        &mut *cancelled,
    )
    .map_err(|e| WorldFieldOverlayErrorV1::Member(MemberRecordError::Key(e)))?;
    let field_key = WorldFieldKeyV1::new(
        PENDING_ACTIONS_FIELD_V1,
        WorldFieldRecordKindV1::Entry,
        Some(key.digest),
    )
    .map_err(ValueTreeError::Invalid)?;
    let index = current.index.borrow().remove(position, work, cancelled)?;
    tx.validate_guard(&tx.guard(), live)?;
    tx.write(StateNamespaceV1::Scheduler, field_key.as_bytes(), None)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *current.index.borrow_mut() = index;
    Ok(Some(meta))
}
#[allow(clippy::too_many_arguments)]
pub fn remove_pending_action_v1<M: MerkleNodeReader>(
    current: &mut ReservedValue<PendingActionCurrentV1>,
    position: Position,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    work: &mut PendingActionIndexWorkV1,
    keys: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<PendingActionMetaV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check(current, tx, memory, guard, live)?;
        remove_inner(
            current,
            position,
            tx,
            memory,
            work,
            keys,
            &mut cancelled,
            live,
        )
    })
}
#[allow(clippy::too_many_arguments)]
pub fn pop_pending_action_v1<M: MerkleNodeReader>(
    current: &mut ReservedValue<PendingActionCurrentV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    work: &mut PendingActionIndexWorkV1,
    keys: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<PendingActionMetaV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check(current, tx, memory, guard, live)?;
        let first = current.index.borrow().first(work, &mut cancelled)?;
        match first {
            Some(m) => remove_inner(
                current,
                m.position,
                tx,
                memory,
                work,
                keys,
                &mut cancelled,
                live,
            ),
            None => {
                if cancelled() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                Ok(None)
            }
        }
    })
}

#[cfg(test)]
#[path = "incremental_pending_action_overlay_tests.rs"]
mod tests;

/// Inspect the actual current FIFO head without consuming its authenticated row.
pub fn peek_pending_action_v1<M: MerkleNodeReader>(
    current: &ReservedValue<PendingActionCurrentV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    memory: &MemoryBudget,
    live: &ExecutionFence,
    work: &mut PendingActionIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Option<PendingActionMetaV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        check(current, tx, memory, &tx.guard(), live)?;
        Ok(current.index.borrow().first(work, &mut cancelled)?)
    })
}
