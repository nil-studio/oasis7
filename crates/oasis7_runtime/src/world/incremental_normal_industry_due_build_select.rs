//! Complete parent-plus-Current build selection. Only resident parent bindings
//! are traversed; changed/new rows come from the actual coalesced write set.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_action_factory_schedule::{
        DuePriorityJobV1, FactoryScheduleSourcesV1, FactoryScheduleWorkV1, current_due_priority_v1,
    },
    incremental_normal_tail::{DecodedFactoryBuildJobV1, decode_current_factory_build_job_v1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_factory_records::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
};
use std::mem::size_of;
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("due build complete Current bindings")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due build selection").into()
}
fn overlay(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => error.into(),
        WorldFieldOverlayErrorV1::Value(error) => error.into(),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}
// Container slots only: every T instantiated here already owns its payload
// admission. Each reallocation's full new buffer is prepaid while the old
// buffer remains live; cumulative reservation deliberately keeps that overlap.
pub(super) fn grow_slots<T>(
    values: &mut Vec<T>,
    memory: &MemoryBudget,
    fee: &mut MemoryReservation,
    charge: &mut impl FnMut(usize) -> Result<(), BodyMutationError>,
) -> Result<(), BodyMutationError> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let capacity = values.capacity().checked_mul(2).ok_or_else(limited)?.max(4);
    let bytes = capacity.checked_mul(size_of::<T>()).ok_or_else(limited)?;
    charge(
        bytes
            .checked_add(
                values
                    .len()
                    .checked_mul(size_of::<T>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?,
    )?;
    let mut extra = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    values
        .try_reserve_exact(capacity.checked_sub(values.len()).ok_or_else(limited)?)
        .map_err(|_| limited())?;
    if values.capacity() != capacity {
        return Err(limited());
    }
    fee.merge(&mut extra).map_err(|_| limited())?;
    Ok(())
}
pub(super) struct DueBuildCandidateV1 {
    pub identity: ReservedValue<FactoryBuildParentIdentityV1>,
    pub selected: ReservedValue<DecodedFactoryBuildJobV1>,
    pub priority: u8,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn select_due_builds_v1<M, P, V, W, D, E, C, PE, PC, CW, ME>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    sources: &FactoryScheduleSourcesV1<'_>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<Vec<DueBuildCandidateV1>>, BodyMutationError>
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
    let binding_bytes = sources
        .roots
        .world_id
        .len()
        .checked_add(2048)
        .ok_or_else(limited)?;
    common
        .charge(3, u64::try_from(binding_bytes).map_err(|_| limited())?)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    sources.builds.validate_binding(
        sources.lifecycle.factories,
        sources.roots,
        &memory,
        MemoryClass::Ordinary,
    )?;
    if sources.builds.records() > work.builds.max_builds {
        return Err(limited());
    }
    let mut bindings_fee = memory
        .reserve(
            size_of::<Vec<ReservedValue<FactoryBuildParentIdentityV1>>>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bindings = Vec::new();
    // Copy only the authenticated fixed identity, never a whole parent job.
    for (id, value, root) in sources.builds.iter_parent_build_bindings() {
        common
            .charge(
                1,
                u64::try_from(32 + root.node_root.len()).map_err(|_| limited())?,
            )
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        let fee = memory
            .reserve(
                size_of::<FactoryBuildParentIdentityV1>() + root.node_root.len(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        grow_slots(&mut bindings, &memory, &mut bindings_fee, &mut |bytes| {
            common
                .charge(1, u64::try_from(bytes).map_err(|_| limited())?)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))
        })?;
        bindings.push(fee.retain(FactoryBuildParentIdentityV1 {
            id,
            value_root: *value,
            record_root: root.clone(),
        }));
    }
    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, _parent, _write, _materials, work, tail| {
            sources.builds.validate_binding(
                sources.lifecycle.factories,
                sources.roots,
                &memory,
                MemoryClass::Ordinary,
            )?;
            let mut ctx = FactoryTransitionReadContextV1 {
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
            let mut first = None;
            let mut changes = 0usize;
            let visited = visit_build_current_outer_changes_v1(
                step.transaction,
                sources.lifecycle.factories,
                &mut ctx,
                &mut work.lifecycle.factory,
                |change| {
                    let result = (|| {
                        tail.charge(128)?;
                        changes = changes.checked_add(1).ok_or_else(limited)?;
                        if changes > work.builds.max_pending_changes {
                            return Err(limited());
                        }
                        tail.charge(bindings.len().checked_mul(8).ok_or_else(limited)?)?;
                        if let FactoryBuildCurrentOuterChangeV1::Record { id, before, after } =
                            &*change
                        {
                            let position = bindings.iter().position(|binding| binding.id == *id);
                            match (position, before, after) {
                                (Some(index), Some(before), None) => {
                                    if bindings[index].value_root != before.value_root {
                                        return Err(invalid());
                                    }
                                    bindings.remove(index);
                                }
                                (Some(index), Some(before), Some(_)) => {
                                    if bindings[index].value_root != before.value_root {
                                        return Err(invalid());
                                    }
                                }
                                (None, None, Some(after)) => {
                                    grow_slots(
                                        &mut bindings,
                                        &memory,
                                        &mut bindings_fee,
                                        &mut |bytes| tail.charge(bytes),
                                    )?;
                                    tail.charge(after.record_root.node_root.len() + 32)?;
                                    let fee = memory
                                        .reserve(
                                            size_of::<FactoryBuildParentIdentityV1>()
                                                + after.record_root.node_root.len(),
                                            MemoryClass::Ordinary,
                                        )
                                        .map_err(ValueTreeError::Memory)?;
                                    bindings.push(fee.retain(FactoryBuildParentIdentityV1 {
                                        id: *id,
                                        value_root: after.value_root,
                                        record_root: after.record_root.clone(),
                                    }));
                                }
                                _ => return Err(invalid()),
                            }
                        }
                        Ok(())
                    })();
                    match result {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            first = Some(error);
                            Err(
                                ValueTreeError::ResourceLimited("due build Current fold stopped")
                                    .into(),
                            )
                        }
                    }
                },
            );
            if let Some(error) = first {
                return Err(error);
            }
            visited.map_err(overlay)?;
            Ok(())
        },
    )?;
    let mut output_fee = memory
        .reserve(size_of::<Vec<DueBuildCandidateV1>>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut output = Vec::new();
    for identity in bindings {
        let id = identity.id;
        let selected = crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
            step,
            read,
            parent,
            write,
            materials,
            work,
            common,
            live,
            |step, read, _parent, _write, _materials, work, tail| {
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
                work.lifecycle.factory.cells_remaining = work
                    .lifecycle
                    .factory
                    .cells_remaining
                    .checked_sub(1)
                    .ok_or_else(limited)?;
                let ready = match read_factory_cell_overlay_v1(
                    FactoryRecordKeyV1::Build(id),
                    FactoryCellV1::Column(FactoryColumnV1::BuildReadyAt),
                    &mut ctx,
                    8,
                    &mut std::io::sink(),
                )
                .map_err(overlay)?
                {
                    FactoryCellReadV1::Value(FactoryValueV1::Unsigned(n)) => n,
                    _ => return Err(invalid()),
                };
                if ready > now {
                    return Ok(None);
                }
                let decoded = decode_current_factory_build_job_v1(
                    id,
                    work.lifecycle.max_scalar_bytes,
                    work.lifecycle.max_material_entries,
                    &memory,
                    tail,
                    &mut |cell, output| {
                        work.lifecycle.factory.cells_remaining = work
                            .lifecycle
                            .factory
                            .cells_remaining
                            .checked_sub(1)
                            .ok_or_else(limited)?;
                        read_factory_cell_overlay_v1(
                            FactoryRecordKeyV1::Build(id),
                            cell,
                            &mut ctx,
                            work.lifecycle.max_scalar_bytes as u64,
                            &mut { output },
                        )
                        .map_err(overlay)
                    },
                )?;
                Ok(Some(decoded))
            },
        )?;
        let Some(selected) = selected else {
            continue;
        };
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
        let priority = current_due_priority_v1(
            DuePriorityJobV1::Build(selected.job()),
            sources,
            &mut ctx,
            parent,
            work,
            common,
            now,
            live,
        )? as u8;
        grow_slots(&mut output, &memory, &mut output_fee, &mut |bytes| {
            common
                .charge(1, u64::try_from(bytes).map_err(|_| limited())?)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))
        })?;
        output.push(DueBuildCandidateV1 {
            identity,
            selected,
            priority,
        });
    }
    let comparisons = output
        .len()
        .checked_mul(output.len())
        .and_then(|n| n.checked_mul(3))
        .ok_or_else(limited)?;
    common
        .charge(u64::try_from(comparisons).map_err(|_| limited())?, 0)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    let scratch = output
        .len()
        .checked_mul(size_of::<DueBuildCandidateV1>())
        .ok_or_else(limited)?;
    common
        .charge(1, u64::try_from(scratch).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    // Stable slice sorting may allocate a temporary element buffer. Its source
    // and scratch buffers coexist; admit the whole actual-length buffer bound.
    let _sort = memory
        .reserve(scratch, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    output.sort_by_key(|candidate| {
        (
            candidate.priority,
            candidate.selected.job().ready_at,
            candidate.selected.job().job_id,
        )
    });
    Ok(output_fee.retain(output))
}
