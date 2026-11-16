//! Pause the one actual Current factory, preserving the original complete
//! production reset and material normalization. No caller DTO grants access.
use super::*;
use oasis7_world_state::incremental_factory_schedule_factory::{
    FactoryScheduleFactoryWorkV1, recover_current_schedule_complete_factory_v1,
};

// Transfer, rather than copy, the caller's continuous scalar allowance to the
// existing complete selected reader. Drop reconciles errors and unwinds too.
struct SelectedReadWork<'a> {
    steps: &'a mut u64,
    bytes: &'a mut u64,
    selected: FactoryScheduleFactoryWorkV1,
}
impl<'a> SelectedReadWork<'a> {
    fn take(work: &'a mut FactoryLifecycleWorkV1) -> Self {
        let cells_remaining = std::mem::take(&mut work.steps_remaining);
        let bytes_remaining = std::mem::take(&mut work.bytes_remaining);
        Self {
            steps: &mut work.steps_remaining,
            bytes: &mut work.bytes_remaining,
            selected: FactoryScheduleFactoryWorkV1 {
                cells_remaining,
                bytes_remaining,
                max_text_bytes: work.max_scalar_bytes.max(8),
                max_tags: work.max_material_entries,
                max_stacks: work.max_material_entries,
            },
        }
    }
}
impl Drop for SelectedReadWork<'_> {
    fn drop(&mut self) {
        *self.steps = self.selected.cells_remaining;
        *self.bytes = self.selected.bytes_remaining;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply<M, P, V, W, E, C, ME, D>(
    intent: &LifecycleIntentV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryLifecycleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryLifecycleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    // Original PreparedRecipeLifecycle::simple normalizes world materials even
    // though pausing itself neither consumes nor produces material.
    let guard = step.transaction.guard();
    materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
    let before = {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: read.values.source,
                memory: step.mutation.memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
        };
        let guard = ctx.transaction.guard();
        let mut child = SelectedReadWork::take(work);
        recover_current_schedule_complete_factory_v1(
            intent.factory,
            sources.factories,
            &mut ctx,
            &mut child.selected,
            &guard,
            live,
        )
        .map_err(overlay_error)?
        .ok_or(BodyMutationError::Invalid(
            "pause Current factory disappeared",
        ))?
    };
    // The existing complete reader reserves the bound for this exact selected
    // DTO, including vectors and optional snapshot. Pay a second peak before
    // cloning; both allocations remain live through the actual row CAS.
    work.spend(before.reserved_bytes())?;
    let _after_fee = step
        .mutation
        .memory
        .reserve(before.reserved_bytes(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut after = (*before).clone();
    if after.builder_agent_id != intent.operator
        || after.production.status != oasis7_world_state::FactoryProductionStatus::Idle
    {
        return Err(BodyMutationError::Invalid(
            "pause Current preparation changed",
        ));
    }
    after.production.status = oasis7_world_state::FactoryProductionStatus::Paused;
    after.production.current_job_id = None;
    after.production.current_recipe_id = None;
    after.production.current_blocker_kind = None;
    after.production.current_blocker_detail = None;
    after.production.last_completed_recipe_id = None;
    after.production.same_recipe_repeat_count = 0;
    after.production.last_completed_canonical_snapshot = None;
    let stop = RefCell::new(&mut read.cancelled);
    let mutation = RefCell::new(&mut step.mutation.cancelled);
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: MemoryClass::Ordinary,
            io: &mut *materials.read_io,
        },
        key_work: &mut *materials.read_keys,
        semantic_bytes: materials.semantic_bytes,
        record_bytes: materials.record_bytes,
        take_state_error: &mut materials.take_state_error,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        key_work: &mut *materials.write_keys,
        limits: materials.write_limits,
        cancelled: || (stop.borrow_mut())() || (mutation.borrow_mut())(),
    };
    let guard = ctx.transaction.guard();
    replace_factory_record_overlay_v1(
        FactoryRecordChangeV1 {
            key: FactoryRecordKeyV1::Factory(intent.factory),
            expected: Some(FactoryRecordRefV1::Factory(&before)),
            after: Some(FactoryRecordRefV1::Factory(&after)),
            max_current_payload_bytes: work.max_scalar_bytes.max(8) as u64,
        },
        &mut ctx,
        &mut write,
        &mut work.factory,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    Ok(())
}
