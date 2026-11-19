//! Runtime-owned participant of one genuine Protocol staged branch.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, StagedExecutionBranchV1},
};
use std::sync::MutexGuard;
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
pub(in crate::world) struct WorldEventCounterStageV1<'c> {
    counter: &'c WorldEventCounterV1,
    counts: Option<Counts>,
    guard: Option<PreparedGuard>,
    retention: Option<usize>,
    prologue: bool,
    depth: usize,
    finished: bool,
    _fee: MemoryReservation,
}
pub(in crate::world) struct PreparedWorldEventCounterRejoinV1<'c> {
    guard: MutexGuard<'c, PreparedGuard>,
    counts: MutexGuard<'c, Counts>,
    retention: MutexGuard<'c, Option<usize>>,
    stage: WorldEventCounterStageV1<'c>,
}
impl WorldEventCounterV1 {
    pub(in crate::world) fn begin_staged_branch_v1<'c, M: MerkleNodeReader>(
        &'c self,
        branch: &StagedExecutionBranchV1<'_, M>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<WorldEventCounterStageV1<'c>, BodyMutationError> {
        branch
            .validate_current_branch(tx, common, live)
            .map_err(formal)?;
        if self.inner.failed.load(Ordering::Acquire)
            || !self.inner.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        common.charge(8, 4096).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut guard = self.inner.guard.lock().map_err(|_| invalid())?;
        branch.validate_parent_participant(&guard).map_err(formal)?;
        let mut counts = self.inner.counts.lock().map_err(|_| invalid())?;
        let retention = *self.inner.journal_retention.lock().map_err(|_| invalid())?;
        let parent_depth = self.inner.staged.load(Ordering::Acquire);
        let depth = parent_depth.checked_add(1).ok_or_else(invalid)?;
        if self
            .inner
            .staged
            .compare_exchange(parent_depth, depth, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(invalid());
        }
        let copied = Counts {
            latest_note: counts.latest_note.clone(),
            appended: counts.appended,
            last_world_fact_end: counts.last_world_fact_end,
            last_original: counts.last_original,
        };
        let old = std::mem::replace(&mut *counts, copied);
        let old_guard = std::mem::replace(&mut *guard, tx.guard());
        Ok(WorldEventCounterStageV1 {
            counter: self,
            counts: Some(old),
            guard: Some(old_guard),
            retention,
            prologue: self.inner.factory_prologue_claimed.load(Ordering::Acquire),
            depth,
            finished: false,
            _fee: fee,
        })
    }
}
impl<'c> WorldEventCounterStageV1<'c> {
    pub(in crate::world) fn prepare_rejoin<M: MerkleNodeReader>(
        self,
        branch: &StagedExecutionBranchV1<'_, M>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PreparedWorldEventCounterRejoinV1<'c>, BodyMutationError> {
        branch
            .validate_current_branch(tx, common, live)
            .map_err(formal)?;
        self.counter.validate(tx)?;
        if self.counter.inner.staged.load(Ordering::Acquire) != self.depth {
            return Err(invalid());
        }
        let guard = self.counter.inner.guard.lock().map_err(|_| invalid())?;
        let counts = self.counter.inner.counts.lock().map_err(|_| invalid())?;
        let retention = self
            .counter
            .inner
            .journal_retention
            .lock()
            .map_err(|_| invalid())?;
        let old = self.counts.as_ref().ok_or_else(invalid)?;
        let added = counts
            .appended
            .checked_sub(old.appended)
            .ok_or_else(invalid)?;
        let depth = u64::from(
            tx.ordered_fact_count()
                .checked_add(1)
                .ok_or_else(invalid)?
                .ilog2(),
        )
        .checked_add(1)
        .ok_or_else(invalid)?;
        common
            .charge(
                added
                    .checked_mul(depth)
                    .and_then(|n| n.checked_add(4))
                    .ok_or_else(invalid)?,
                added.checked_mul(128).ok_or_else(invalid)?,
            )
            .map_err(formal)?;
        let mut cursor = counts.latest_note.as_ref();
        let mut seen = 0u64;
        while seen < added {
            let note = cursor.ok_or_else(invalid)?;
            if !tx.ordered_fact(note.ordinal).is_some_and(|fact| {
                fact.kind == ExecutionFactKindV1::Event && fact.id == note.original
            }) {
                return Err(invalid());
            }
            if note.ordinal < old.last_world_fact_end {
                return Err(invalid());
            }
            if let Some(previous) = note.previous.as_ref()
                && previous.ordinal >= note.ordinal
            {
                return Err(invalid());
            }
            cursor = note.previous.as_ref();
            seen += 1;
        }
        if match (cursor, old.latest_note.as_ref()) {
            (Some(a), Some(b)) => !std::ptr::eq(Arc::as_ptr(a), Arc::as_ptr(b)),
            (None, None) => false,
            _ => true,
        } {
            return Err(invalid());
        }
        if added == 0
            && (counts.last_world_fact_end != old.last_world_fact_end
                || counts.last_original != old.last_original)
        {
            return Err(invalid());
        }
        if added > 0 {
            let latest = counts.latest_note.as_ref().ok_or_else(invalid)?;
            if counts.last_world_fact_end != latest.ordinal.checked_add(1).ok_or_else(invalid)?
                || counts.last_original != Some(latest.original)
            {
                return Err(invalid());
            }
        }
        Ok(PreparedWorldEventCounterRejoinV1 {
            stage: self,
            counts,
            guard,
            retention,
        })
    }
}
impl PreparedWorldEventCounterRejoinV1<'_> {
    /// All checks/locks/admission finished before Protocol publishes either choice.
    pub(in crate::world) fn commit(mut self) {
        *self.guard = self
            .stage
            .guard
            .take()
            .expect("private prepared parent guard");
        self.stage.finished = true;
        self.stage
            .counter
            .inner
            .staged
            .store(self.stage.depth - 1, Ordering::Release);
    }
    pub(in crate::world) fn discard(mut self) {
        *self.counts = self
            .stage
            .counts
            .take()
            .expect("private prepared parent counts");
        *self.guard = self
            .stage
            .guard
            .take()
            .expect("private prepared parent guard");
        *self.retention = self.stage.retention;
        self.stage
            .counter
            .inner
            .factory_prologue_claimed
            .store(self.stage.prologue, Ordering::Release);
        self.stage.finished = true;
        self.stage
            .counter
            .inner
            .staged
            .store(self.stage.depth - 1, Ordering::Release);
    }
}
impl Drop for WorldEventCounterStageV1<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.counter.inner.failed.store(true, Ordering::Release);
            if self.counter.inner.staged.load(Ordering::Acquire) != self.depth {
                return;
            }
            if let Ok(mut counts) = self.counter.inner.counts.lock()
                && let Some(old) = self.counts.take()
            {
                *counts = old
            }
            if let Ok(mut guard) = self.counter.inner.guard.lock()
                && let Some(old) = self.guard.take()
            {
                *guard = old
            }
            if let Ok(mut retention) = self.counter.inner.journal_retention.lock() {
                *retention = self.retention
            }
            self.counter
                .inner
                .factory_prologue_claimed
                .store(self.prologue, Ordering::Release);
            self.counter
                .inner
                .staged
                .store(self.depth - 1, Ordering::Release);
        }
    }
}
