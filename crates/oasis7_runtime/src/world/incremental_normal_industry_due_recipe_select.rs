//! Exact Current recipe directory selection after the build phase. This file
//! only selects paid jobs; it never mints a terminal/completion capability.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_factory_directory_records::{
        CurrentFactoryRecipeDirectoryV1, FactoryRecipeDirectoryWorkV1,
    },
    incremental_normal_action_factory_schedule::{
        DuePriorityJobV1, FactoryScheduleReadErrorV1, FactoryScheduleSourcesV1,
        FactoryScheduleWorkV1, current_due_priority_v1,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryTransitionReadContextV1,
};
use std::mem::size_of;
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due recipe selection").into()
}
fn order_error(
    error: crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1,
) -> BodyMutationError {
    use crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1 as E;
    match error {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Body(e) => e,
        E::Overlay(e) => match e {
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                e,
            ) => e.into(),
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
                e.into()
            }
            e => BodyMutationError::Original(Box::new(e)),
        },
    }
}
fn data_error(error: FactoryScheduleReadErrorV1) -> BodyMutationError {
    match error {
        FactoryScheduleReadErrorV1::Body(e) => e,
        FactoryScheduleReadErrorV1::World(cause) => {
            BodyMutationError::Original(Box::new(RecipeSelectionCause(cause)))
        }
    }
}
#[derive(Debug)]
struct RecipeSelectionCause(crate::WorldError);
impl std::fmt::Display for RecipeSelectionCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for RecipeSelectionCause {}
pub(super) struct DueRecipeCandidateV1 {
    pub pending: ReservedValue<crate::RecipeJobState>,
    pub priority: u8,
    _row: MemoryReservation,
}
pub(super) struct DueRecipeQueueV1 {
    jobs: Vec<DueRecipeCandidateV1>,
    _slots: MemoryReservation,
}
pub(super) struct DueRecipeQueueIntoIterV1 {
    iter: std::vec::IntoIter<DueRecipeCandidateV1>,
    _slots: MemoryReservation,
}
impl Iterator for DueRecipeQueueIntoIterV1 {
    type Item = DueRecipeCandidateV1;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
impl IntoIterator for DueRecipeQueueV1 {
    type Item = DueRecipeCandidateV1;
    type IntoIter = DueRecipeQueueIntoIterV1;
    fn into_iter(self) -> Self::IntoIter {
        DueRecipeQueueIntoIterV1 {
            iter: self.jobs.into_iter(),
            _slots: self._slots,
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn select_due_recipes_v1<M, P, V, W, D, E, C, PE, PC, CW, ME>(
    directory: &ReservedValue<CurrentFactoryRecipeDirectoryV1<'_>>,
    directory_work: &mut FactoryRecipeDirectoryWorkV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryScheduleSourcesV1<'_>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<DueRecipeQueueV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    let memory = common.memory().clone();
    let now = step.time();
    if !directory.uses_budget(&memory) {
        return Err(BodyMutationError::Invalid("due recipe directory pool"));
    }
    // The two additional directory lanes use the same Core and are debited even
    // when the original validation fails. Other scalar IO lanes use the shared
    // write bridge; no directory budget is manufactured or replenished.
    directory_work.comparisons_remaining = directory_work
        .comparisons_remaining
        .min(common.work().remaining_read_bytes());
    directory_work.pending_entries_remaining = directory_work
        .pending_entries_remaining
        .min(usize::try_from(common.work().remaining_steps()).unwrap_or(usize::MAX));
    let before = (
        directory_work.comparisons_remaining,
        directory_work.pending_entries_remaining,
    );
    let result = crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, parent, _write, _materials, work, tail| {
            let mut factory = FactoryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                max_payload_bytes: read.record_bytes,
                cancelled: &mut read.cancelled,
            };
            let mut recipe = IndustryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: parent.values.source,
                    memory: parent.values.memory,
                    class: parent.values.class,
                    io: &mut *parent.values.io,
                },
                key_work: &mut *parent.key_work,
                semantic_bytes: parent.semantic_bytes,
                record_bytes: parent.record_bytes,
                max_payload_bytes: parent.record_bytes,
                cancelled: &mut parent.cancelled,
            };
            let guard = step.transaction.guard();
            let original = directory
                .validate_current(
                    step.transaction,
                    &mut factory,
                    &mut recipe,
                    &mut work.lifecycle.factory,
                    &mut work.lifecycle.industry,
                    directory_work,
                    &guard,
                    live,
                )
                .map_err(order_error);
            let bytes = before
                .0
                .checked_sub(directory_work.comparisons_remaining)
                .ok_or_else(limited)?;
            let entries = before
                .1
                .checked_sub(directory_work.pending_entries_remaining)
                .ok_or_else(limited)?;
            let accounting = (|| {
                tail.charge(usize::try_from(bytes).map_err(|_| limited())?)?;
                for _ in 0..entries {
                    tail.charge_record()?;
                }
                Ok(())
            })();
            match original {
                Err(e) => Err(e),
                Ok(()) => accounting,
            }
        },
    );
    result?;
    let mut fee = memory
        .reserve(
            size_of::<DueRecipeQueueV1>().max(size_of::<DueRecipeQueueIntoIterV1>()),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut jobs = Vec::new();
    for id in directory.iter_recipe_job_ids() {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        let pending = current::recover_current_pending_recipe_v1(
            id, sources, &mut ctx, parent, work, common, live,
        )
        .map_err(data_error)?;
        if pending.ready_at > now {
            continue;
        }
        let priority = current_due_priority_v1(
            DuePriorityJobV1::Recipe(&pending),
            sources,
            &mut ctx,
            parent,
            work,
            common,
            now,
            live,
        )? as u8;
        build_select::grow_slots(&mut jobs, &memory, &mut fee, &mut |n| {
            common
                .charge(1, u64::try_from(n).map_err(|_| limited())?)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))
        })?;
        common
            .charge(
                1,
                u64::try_from(size_of::<DueRecipeCandidateV1>()).map_err(|_| limited())?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let row = memory
            .reserve(size_of::<DueRecipeCandidateV1>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        jobs.push(DueRecipeCandidateV1 {
            pending,
            priority,
            _row: row,
        });
    }
    let comparisons = jobs
        .len()
        .checked_mul(jobs.len())
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(limited)?;
    common
        .charge(u64::try_from(comparisons).map_err(|_| limited())?, 0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let scratch = jobs
        .len()
        .checked_mul(size_of::<DueRecipeCandidateV1>())
        .ok_or_else(limited)?;
    common
        .charge(1, u64::try_from(scratch).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    // Stable slice sorting may allocate a temporary element buffer. Its source
    // and scratch buffers coexist; admit the whole actual-length buffer bound.
    let _sort = memory
        .reserve(scratch, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    jobs.sort_by_key(|job| (job.priority, job.pending.ready_at, job.pending.job_id));
    Ok(DueRecipeQueueV1 { jobs, _slots: fee })
}
