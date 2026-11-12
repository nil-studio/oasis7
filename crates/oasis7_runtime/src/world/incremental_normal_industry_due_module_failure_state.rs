//! Counted source for an actual diagnosed invalid Product worker output.
//! Actual selected-row ProductValidation state, child of the closed due caps.
//! Every public-to-parent function binds Current and writes before minting.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_id_sets::{
        WorldIdSetFieldV1, WorldIdSetKeyRefV1, world_id_set_contains_overlay_v1,
    },
    incremental_world_native::*,
};
use std::mem::size_of;

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_module_failure_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
>(
    source: &super::super::invalid_output::InvalidDueProductOutputV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductModuleFailureV1, BodyMutationError>
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
    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, _read, _parent, _write, _materials, _work, tail| {
            let memory = step.mutation.memory;
            let now = step.time();
            source.validate(step.transaction, memory, now, tail.identity(), live)?;
            let failure = &*source.failure;
            let bytes = failure
                .module_id
                .len()
                .checked_add(failure.trace_id.len())
                .and_then(|n| n.checked_add(failure.detail.len()))
                .and_then(|n| {
                    n.checked_add(
                        size_of::<PreparedNormalProductModuleFailureV1>()
                            + size_of::<crate::WorldEvent>(),
                    )
                })
                .ok_or(BodyMutationError::Invalid("due module failure layout"))?;
            tail.charge(bytes)?;
            let phase = memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductModuleFailureV1(DueEventStateV1 {
                event: fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::ModuleCallFailed(failure.clone()),
                }),
                guard: step.transaction.guard(),
                identity: tail.identity(),
                memory: memory.clone(),
                now,
                event_era: sequence.era,
                _phase: phase,
                _sources: Vec::new(),
            }))
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_vm_failure_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
>(
    source: &crate::world::incremental_module_invocation::PaidDueProductVmFailureV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductModuleFailureV1, BodyMutationError>
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
    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, _read, _parent, _write, _materials, _work, tail| {
            let memory = step.mutation.memory;
            let now = step.time();
            source.validate(step.transaction, memory, now, tail.identity(), live)?;
            let crate::WorldError::ModuleCallFailed {
                module_id,
                trace_id,
                code,
                detail,
            } = source.failure()
            else {
                return Err(BodyMutationError::Invalid("due real VM failure variant"));
            };
            let bytes = module_id
                .len()
                .checked_add(trace_id.len())
                .and_then(|n| n.checked_add(detail.len()))
                .and_then(|n| {
                    n.checked_add(
                        size_of::<PreparedNormalProductModuleFailureV1>()
                            + size_of::<crate::WorldEvent>(),
                    )
                })
                .ok_or(BodyMutationError::Invalid("due VM failure layout"))?;
            tail.charge(bytes)?;
            let phase = memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductModuleFailureV1(DueEventStateV1 {
                event: fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::ModuleCallFailed(
                        oasis7_wasm_abi::ModuleCallFailure {
                            module_id: module_id.clone(),
                            trace_id: trace_id.clone(),
                            code: code.clone(),
                            detail: detail.clone(),
                        },
                    ),
                }),
                guard: step.transaction.guard(),
                identity: tail.identity(),
                memory: memory.clone(),
                now,
                event_era: sequence.era,
                _phase: phase,
                _sources: Vec::new(),
            }))
        },
    )
}
