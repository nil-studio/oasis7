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
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("due product validation Current state")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due product validation layout").into()
}
fn overlay(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => error.into(),
        WorldFieldOverlayErrorV1::Value(error) => error.into(),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}
#[derive(Debug)]
struct StateCause {
    cause: crate::WorldError,
    _phase: MemoryReservation,
}
impl std::fmt::Display for StateCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for StateCause {}
fn text(n: &mut usize, value: &str) -> Result<(), BodyMutationError> {
    *n = n.checked_add(value.len()).ok_or_else(limited)?;
    Ok(())
}
fn attempt_heap(value: &crate::ProductValidationAttemptV1) -> Result<usize, BodyMutationError> {
    let mut n = 0;
    for value in [
        &value.requester_agent_id,
        &value.module_id,
        &value.stack.kind,
    ] {
        text(&mut n, value)?;
    }
    Ok(n)
}
fn decision_heap(
    value: &oasis7_wasm_abi::ProductValidationDecision,
) -> Result<usize, BodyMutationError> {
    let mut n = value.product_id.len();
    for list in [&value.notes, &value.quality_levels] {
        n = n
            .checked_add(
                list.len()
                    .checked_mul(size_of::<String>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
        for value in list {
            text(&mut n, value)?;
        }
    }
    Ok(n)
}
fn receipt_heap(value: &crate::ProductValidationReceiptV1) -> Result<usize, BodyMutationError> {
    let mut n = decision_heap(&value.decision)?;
    for value in [
        &value.requester_agent_id,
        &value.module_id,
        &value.stack.kind,
    ] {
        text(&mut n, value)?;
    }
    if let Some(value) = &value.failure_detail {
        text(&mut n, value)?;
    }
    Ok(n)
}
fn phase_error(cause: crate::WorldError, phase: MemoryReservation) -> BodyMutationError {
    BodyMutationError::Original(Box::new(StateCause {
        cause,
        _phase: phase,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_attempt_v1<
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
    attempt: &crate::ProductValidationAttemptV1,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductAttemptV1, BodyMutationError>
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
        |step, read, _parent, write, _materials, work, tail| {
            let now = step.time();
            let memory = step.mutation.memory;
            let error_size = attempt
                .module_id
                .len()
                .checked_mul(24)
                .and_then(|n| n.checked_add(1024 + size_of::<StateCause>()))
                .ok_or_else(limited)?;
            tail.charge(error_size)?;
            let error_fee = memory
                .reserve(error_size, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            oasis7_world_state::industry_history_transition::validate_product_validation_attempt_identity_v1(attempt).map_err(|cause|phase_error(cause,error_fee))?;
            let settled = {
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
                world_id_set_contains_overlay_v1(
                    WorldIdSetFieldV1::SettledRecipeJobs,
                    WorldIdSetKeyRefV1::Action(attempt.job_id),
                    &mut ctx,
                )
                .map_err(overlay)?
            };
            let mut local = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Effects),
                state: read.state,
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
            let guard = step.transaction.guard();
            let before = recover_current_product_validation_attempts_v1(
                attempt.job_id,
                sources.lifecycle.native,
                sources.roots,
                step.transaction,
                &mut local,
                &mut work.lifecycle.native_scan,
                &mut work.lifecycle.native_read,
                &guard,
                live,
            )
            .map_err(overlay)?;
            let rows = before.value().map(Vec::as_slice).unwrap_or(&[]);
            let mut heap = attempt_heap(attempt)?
                .checked_mul(2)
                .and_then(|n| {
                    n.checked_add(
                        size_of::<PreparedNormalProductAttemptV1>()
                            + size_of::<crate::WorldEvent>()
                            + size_of::<Vec<crate::ProductValidationAttemptV1>>(),
                    )
                })
                .ok_or_else(limited)?;
            heap = heap
                .checked_add(
                    rows.len()
                        .checked_add(1)
                        .and_then(|n| n.checked_mul(size_of::<crate::ProductValidationAttemptV1>()))
                        .ok_or_else(limited)?,
                )
                .ok_or_else(limited)?;
            for row in rows {
                tail.charge_record()?;
                heap = heap.checked_add(attempt_heap(row)?).ok_or_else(limited)?;
            }
            tail.charge(heap)?;
            let phase = memory
                .reserve(heap, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let conflict_fee = memory
                .reserve(1024 + size_of::<StateCause>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let append = if settled {
                false
            } else {
                oasis7_world_state::industry_history_transition::product_validation_attempt_needs_append_v1(rows,attempt).map_err(|cause|phase_error(cause,conflict_fee))?
            };
            let mut after = Vec::with_capacity(rows.len() + usize::from(append));
            for row in rows {
                after.push(row.clone());
            }
            if append {
                after.push(attempt.clone());
            }
            write_current_product_validation_attempts_v1(
                &before,
                attempt.job_id,
                &after,
                step.transaction,
                &mut local,
                write,
                &mut work.lifecycle.records_remaining,
                &mut work.lifecycle.native_write,
                live,
            )
            .map_err(overlay)?;
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let event_fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductAttemptV1(DueEventStateV1 {
                event: event_fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::Domain(
                        crate::DomainEvent::ProductValidationAttemptStarted {
                            attempt: attempt.clone(),
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

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_recorded_v1<
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
    pending: &crate::RecipeJobState,
    receipt: &crate::ProductValidationReceiptV1,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductRecordedV1, BodyMutationError>
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
        |step, read, parent, write, _materials, work, tail| {
            let now = step.time();
            let memory = step.mutation.memory;
            let settled = {
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
                world_id_set_contains_overlay_v1(
                    WorldIdSetFieldV1::SettledRecipeJobs,
                    WorldIdSetKeyRefV1::Action(receipt.job_id),
                    &mut ctx,
                )
                .map_err(overlay)?
            };
            let phase = {
                let mut local = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(StateNamespaceV1::Authorization),
                    state: read.state,
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
                let guard = step.transaction.guard();
                let before = recover_current_product_validation_receipts_v1(
                    receipt.job_id,
                    sources.lifecycle.native,
                    sources.roots,
                    step.transaction,
                    &mut local,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
                let rows = before.value().map(Vec::as_slice).unwrap_or(&[]);
                let mut heap = receipt_heap(receipt)?
                    .checked_mul(2)
                    .and_then(|n| {
                        n.checked_add(
                            size_of::<PreparedNormalProductRecordedV1>()
                                + size_of::<crate::WorldEvent>()
                                + size_of::<Vec<crate::ProductValidationReceiptV1>>(),
                        )
                    })
                    .ok_or_else(limited)?;
                heap = heap
                    .checked_add(
                        rows.len()
                            .checked_add(1)
                            .and_then(|n| {
                                n.checked_mul(size_of::<crate::ProductValidationReceiptV1>())
                            })
                            .ok_or_else(limited)?,
                    )
                    .ok_or_else(limited)?;
                for row in rows {
                    tail.charge_record()?;
                    heap = heap.checked_add(receipt_heap(row)?).ok_or_else(limited)?;
                }
                tail.charge(heap)?;
                let phase = memory
                    .reserve(heap, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let error_fee = memory
                    .reserve(1024 + size_of::<StateCause>(), MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let append = if settled {
                    false
                } else {
                    oasis7_world_state::industry_history_transition::product_validation_receipt_needs_append_v1(rows,receipt).map_err(|cause|phase_error(cause,error_fee))?
                };
                let mut after = Vec::with_capacity(rows.len() + usize::from(append));
                for row in rows {
                    after.push(row.clone());
                }
                if append {
                    after.push(receipt.clone());
                }
                write_current_product_validation_receipts_v1(
                    &before,
                    receipt.job_id,
                    &after,
                    step.transaction,
                    &mut local,
                    write,
                    &mut work.lifecycle.records_remaining,
                    &mut work.lifecycle.native_write,
                    live,
                )
                .map_err(overlay)?;
                phase
            };
            if !settled {
                if oasis7_world_state::industry_history_transition::product_validation_receipt_sets_latest_v1(receipt){
        tail.charge(receipt.stack.kind.len())?;
        let _copy=memory.reserve(receipt.stack.kind.len()+size_of::<Option<oasis7_world_state::LastProductValidationState>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let after=Some(oasis7_world_state::LastProductValidationState{product_id:receipt.stack.kind.clone(),tradable:receipt.decision.tradable});
        let mut local=WorldFieldReadContextV1{root:sources.roots.state_root(StateNamespaceV1::Business),state:read.state,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
        let guard=step.transaction.guard();let before=recover_current_latest_product_validation_v1(sources.lifecycle.native,sources.roots,step.transaction,&mut local,&mut work.lifecycle.native_scan,&mut work.lifecycle.native_read,&guard,live).map_err(overlay)?;
        write_current_latest_product_validation_v1(&before,&after,step.transaction,&mut local,write,&mut work.lifecycle.records_remaining,&mut work.lifecycle.native_write,live).map_err(overlay)?;
      }
                super::super::recipe_commit::update_requester_v1(
                    pending, false, sources, step, read, parent, work, live,
                )?;
            }
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let event_fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductRecordedV1(DueEventStateV1 {
                event: event_fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::Domain(
                        crate::DomainEvent::ProductValidationRecorded {
                            receipt: receipt.clone(),
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

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_result_v1<
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
    pending: &crate::RecipeJobState,
    module_id: &str,
    stack: &crate::MaterialStack,
    decision: &oasis7_wasm_abi::ProductValidationDecision,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductResultV1, BodyMutationError>
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
        |step, read, parent, write, _materials, work, tail| {
            let memory = step.mutation.memory;
            let now = step.time();
            let exists = {
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
                let guard = ctx.transaction.guard();
                oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(&pending.requester_agent_id,sources.lifecycle.agents,sources.roots,&mut ctx,parent,&mut work.lifecycle.pending_entries_remaining,&guard,live).map_err(overlay)?==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing
            };
            let payload = decision_heap(decision)?
                .checked_add(pending.requester_agent_id.len())
                .and_then(|n| n.checked_add(module_id.len()))
                .and_then(|n| n.checked_add(stack.kind.len()))
                .ok_or_else(limited)?;
            let bytes = payload
                .checked_mul(24)
                .and_then(|n| {
                    n.checked_add(
                        2048 + size_of::<crate::WorldEvent>()
                            + size_of::<PreparedNormalProductResultV1>(),
                    )
                })
                .ok_or_else(limited)?;
            tail.charge(bytes)?;
            let phase = memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let body = crate::world::World::product_validation_result_from_presence_v1(
                exists,
                pending.job_id,
                &pending.requester_agent_id,
                module_id,
                stack,
                decision,
            );
            if matches!(
                &body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidated { .. })
            ) {
                let after = Some(oasis7_world_state::LastProductValidationState {
                    product_id: stack.kind.clone(),
                    tradable: decision.tradable,
                });
                let mut local = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(StateNamespaceV1::Business),
                    state: read.state,
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
                let guard = step.transaction.guard();
                let before = recover_current_latest_product_validation_v1(
                    sources.lifecycle.native,
                    sources.roots,
                    step.transaction,
                    &mut local,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
                write_current_latest_product_validation_v1(
                    &before,
                    &after,
                    step.transaction,
                    &mut local,
                    write,
                    &mut work.lifecycle.records_remaining,
                    &mut work.lifecycle.native_write,
                    live,
                )
                .map_err(overlay)?;
                drop(local);
                super::super::recipe_commit::update_requester_v1(
                    pending, false, sources, step, read, parent, work, live,
                )?;
            }
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let event_fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductResultV1(DueEventStateV1 {
                event: event_fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body,
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

/// Only genuine post-write Product sources can advance the delivery cursor.
/// The fixed driver calls this after the corresponding synchronous route succeeds.
pub(in crate::world::incremental_normal_industry_due) enum RoutedProductSourceV1<'a> {
    Attempt(&'a PreparedNormalProductAttemptV1),
    Recorded(&'a PreparedNormalProductRecordedV1),
    Result(&'a PreparedNormalProductResultV1),
    Replay(&'a super::delivery::RoutedProductReplayV1),
}
impl RoutedProductSourceV1<'_> {
    fn state(&self) -> Option<&DueEventStateV1> {
        match self {
            Self::Attempt(v) => Some(&v.0),
            Self::Recorded(v) => Some(&v.0),
            Self::Result(v) => Some(&v.0),
            Self::Replay(_) => None,
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_cursor_v1<
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
    source: RoutedProductSourceV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductCursorV1, BodyMutationError>
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
        |step, read, _parent, write, _materials, _work, tail| {
            let now = step.time();
            let memory = step.mutation.memory;
            let (id, era) = match &source {
                RoutedProductSourceV1::Replay(replay) => {
                    replay.validate_evidence_identity(
                        step.transaction,
                        memory,
                        now,
                        tail.identity(),
                        live,
                    )?;
                    (replay.event_id(), replay.event_id_era())
                }
                _ => {
                    let source = source.state().ok_or_else(invalid)?;
                    source.validate_evidence_identity(
                        step.transaction,
                        memory,
                        now,
                        tail.identity(),
                        live,
                    )?;
                    if !matches!(
                        &source.event.body,
                        crate::WorldEventBody::Domain(
                            crate::DomainEvent::ProductValidationAttemptStarted { .. }
                                | crate::DomainEvent::ProductValidationRecorded { .. }
                                | crate::DomainEvent::ProductValidated { .. }
                        )
                    ) {
                        return Err(invalid());
                    }
                    (source.event.id, source.event_era)
                }
            };
            let cursor = oasis7_world_state::ProductValidationDeliveryCursor {
                routed_through_event_id: id,
                event_id_era: era,
            };
            let phase_bytes = size_of::<PreparedNormalProductCursorV1>()
                + size_of::<crate::WorldEvent>()
                + 2 * size_of::<oasis7_world_state::ProductValidationDeliveryCursor>();
            tail.charge(phase_bytes)?;
            let phase = memory
                .reserve(phase_bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            {
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
                let before=oasis7_world_state::incremental_delivery_cursor::read_delivery_cursor_overlay_v1(&mut ctx).map_err(overlay)?;
                let mut after = before.clone();
                after.advance_to(cursor.event_id_era, cursor.routed_through_event_id);
                let guard = ctx.transaction.guard();
                oasis7_world_state::incremental_delivery_cursor::change_delivery_cursor_overlay_v1(
                    &before, &after, &mut ctx, write, &guard, live,
                )
                .map_err(overlay)?;
            }
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let event_fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductCursorV1(DueEventStateV1 {
                event: event_fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::ProductValidationDeliveryCursorUpdated(cursor),
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

#[path = "incremental_normal_industry_due_validation_history.rs"]
mod history;
pub(in crate::world::incremental_normal_industry_due) use history::{
    CurrentProductHistoryV1, read_product_history_v1,
};

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn commit_product_receipt_delivery_v1<
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
    pending: &crate::RecipeJobState,
    history: &ReservedValue<CurrentProductHistoryV1>,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalProductResultV1, BodyMutationError>
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
        |step, read, parent, write, _materials, work, tail| {
            let memory = step.mutation.memory;
            let now = step.time();
            let receipt = history.receipt().ok_or_else(invalid)?;
            tail.charge(
                receipt
                    .requester_agent_id
                    .len()
                    .checked_add(pending.requester_agent_id.len())
                    .and_then(|n| n.checked_add(receipt.stack.kind.len().checked_mul(2)?))
                    .ok_or_else(limited)?,
            )?;
            history.validate_evidence_identity(
                step.transaction,
                memory,
                now,
                tail.identity(),
                pending,
                live,
            )?;
            // Exact original ensure-delivery projection. This deliberately has no
            // new-call agent/module/stack-limit rejection conversion.
            let mut bytes = receipt
                .requester_agent_id
                .len()
                .checked_add(receipt.module_id.len())
                .and_then(|n| n.checked_add(receipt.stack.kind.len().checked_mul(2)?))
                .and_then(|n| {
                    n.checked_add(
                        size_of::<PreparedNormalProductResultV1>()
                            + size_of::<crate::WorldEvent>()
                            + size_of::<Option<oasis7_world_state::LastProductValidationState>>(),
                    )
                })
                .ok_or_else(limited)?;
            for list in [&receipt.decision.quality_levels, &receipt.decision.notes] {
                bytes = bytes
                    .checked_add(
                        list.len()
                            .checked_mul(size_of::<String>())
                            .ok_or_else(limited)?,
                    )
                    .ok_or_else(limited)?;
                for value in list {
                    tail.charge_record()?;
                    bytes = bytes.checked_add(value.len()).ok_or_else(limited)?;
                }
            }
            tail.charge(bytes)?;
            let phase = memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            {
                let after = Some(oasis7_world_state::LastProductValidationState {
                    product_id: receipt.stack.kind.clone(),
                    tradable: receipt.decision.tradable,
                });
                let mut local = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(StateNamespaceV1::Business),
                    state: read.state,
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
                let guard = step.transaction.guard();
                let before = recover_current_latest_product_validation_v1(
                    sources.lifecycle.native,
                    sources.roots,
                    step.transaction,
                    &mut local,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
                write_current_latest_product_validation_v1(
                    &before,
                    &after,
                    step.transaction,
                    &mut local,
                    write,
                    &mut work.lifecycle.records_remaining,
                    &mut work.lifecycle.native_write,
                    live,
                )
                .map_err(overlay)?;
            }
            super::super::recipe_commit::update_requester_v1(
                pending, false, sources, step, read, parent, work, live,
            )?;
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let fee = memory
                .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(PreparedNormalProductResultV1(DueEventStateV1 {
                event: fee.retain(crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidated {
                        requester_agent_id: receipt.requester_agent_id.clone(),
                        module_id: receipt.module_id.clone(),
                        stack: receipt.stack.clone(),
                        stack_limit: receipt.decision.stack_limit,
                        tradable: receipt.decision.tradable,
                        quality_levels: receipt.decision.quality_levels.clone(),
                        notes: receipt.decision.notes.clone(),
                    }),
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
