//! Sole conversion of actual lifecycle writes/retries into counted event
//! owners. Business effects are never replayed by this publication stage.
use super::*;
use crate::world::{
    incremental_body::BodyMutationContext,
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_tail::NormalTailWorkV1,
};
use delivery::{IndustrialDeliveryErrorV1, IndustrialDeliverySourcesV1, IndustrialDeliveryWorkV1};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_composite_overlay::WorldRuleCompositeOverlayV1,
    incremental_world_rule_requests_current::{
        insert_current_world_rule_request_v1, read_current_world_rule_request_v1,
    },
    incremental_world_rules_family::WorldRulesQualificationV1,
    world_rules::WorldRuleReceiptV1,
};
use std::collections::BTreeMap;

pub(super) struct PaidLifecycleRequestCommitV1 {
    _receipt: ReservedValue<WorldRuleReceiptV1>,
    _overlay: ReservedValue<WorldRuleCompositeOverlayV1<BTreeMap<String, WorldRuleReceiptV1>>>,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("world-rule lifecycle publication")
}
fn bytes(event: &crate::DomainEvent) -> Result<usize, BodyMutationError> {
    let crate::DomainEvent::WorldRuleCommitted {
        action_id: 0,
        actor_id,
        request_id,
        command,
        world_binding,
        ..
    } = event
    else {
        return Err(invalid());
    };
    actor_id
        .len()
        .checked_add(request_id.len())
        .and_then(|n| n.checked_add(world_binding.as_ref().map_or(0, String::len)))
        .and_then(|n| n.checked_add(header::command_comparison_bytes_v1(command, command).ok()?))
        .ok_or_else(invalid)
}

/// The original allocator reader, clamped and reconciled against one child of
/// the existing Common. This is a preview only; counted append advances it.

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_fresh_lifecycle_publication_v1<M, P, V, R, W, E, C, PE, PC, CW, ME>(
    written: written::FreshWrittenLifecycleCommandV1,
    rules: &ReservedValue<WorldRulesQualificationV1>,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedNormalWorldRuleLifecycleEventV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    let result = (|| {
        {
            let mut tail = NormalTailWorkV1::new(common);
            written.validate(ctx.transaction, &mut tail, now, live)?;
        }
        let operation = written.header.operation;
        let observe = matches!(operation, LifecycleOperationV1::Observe);
        let receipt = if observe {
            None
        } else {
            Some(delivery::paid_io::run(
                ctx,
                parent,
                write,
                materials,
                work,
                common,
                |ctx, _parent, write, _materials, work, tail| {
                    let crate::DomainEvent::WorldRuleCommitted {
                        actor_id,
                        request_id,
                        command,
                        ..
                    } = written.header.event()
                    else {
                        return Err(invalid().into());
                    };
                    let guard = ctx.transaction.guard();
                    let current = read_current_world_rule_request_v1(
                        actor_id,
                        request_id,
                        rules.requests(),
                        sources.eligibility.roots,
                        state,
                        ctx,
                        &mut work.fixed_scan,
                        &mut work.composite_read,
                        &guard,
                        live,
                    )?;
                    if current.value().is_some() {
                        return Err(invalid().into());
                    }
                    let payload = bytes(written.header.event())?
                        .checked_add(written.result_id().len())
                        .and_then(|n| n.checked_add(std::mem::size_of::<WorldRuleReceiptV1>()))
                        .ok_or_else(invalid)?;
                    tail.charge(payload)?;
                    let fee = tail.reserve(payload, MemoryClass::Ordinary)?;
                    let receipt = fee.retain(WorldRuleReceiptV1 {
                        actor_id: actor_id.clone(),
                        request_id: request_id.clone(),
                        command: command.clone(),
                        committed_at: now,
                        result_id: written.result_id().to_owned(),
                    });
                    let guard = ctx.transaction.guard();
                    let overlay = insert_current_world_rule_request_v1(
                        &current,
                        &receipt,
                        rules.requests(),
                        sources.eligibility.roots,
                        state,
                        ctx,
                        write,
                        &mut work.fixed_scan,
                        &mut work.composite_read,
                        &mut work.composite_write,
                        &guard,
                        live,
                    )?
                    .ok_or_else(invalid)?;
                    Ok(PaidLifecycleRequestCommitV1 {
                        _receipt: receipt,
                        _overlay: overlay,
                    })
                },
            )?)
        };
        let payload = bytes(written.header.event())?;
        common
            .charge(1, payload as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let phase = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedNormalWorldRuleLifecycleEventV1>()
                    + std::mem::size_of::<PaidLifecycleRequestCommitV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let event_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<crate::WorldEvent>()
                    .checked_add(payload)
                    .ok_or_else(invalid)?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let id = crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(ctx, common)?;
        let event = event_fee.retain(crate::WorldEvent {
            id,
            time: now,
            caused_by: None,
            body: crate::WorldEventBody::Domain(written.header.event().clone()),
        });
        Ok(PreparedNormalWorldRuleLifecycleEventV1 {
            event,
            source: LifecycleEventSourceV1::Fresh(CommittedLifecycleMutationV1 {
                operation,
                guard: ctx.transaction.guard(),
                identity: common.work().identity(),
                written,
                receipt,
            }),
            memory: common.memory().clone(),
            now,
            _phase: phase,
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

pub(super) fn prepare_retained_lifecycle_publication_v1<M, R, E, C>(
    retained: header::PreparedRetainedLifecycleHeaderV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedNormalWorldRuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(&retained.guard, live)?;
        if retained.identity != common.work().identity()
            || !retained.memory.same_pool(common.memory())
            || retained.now != now
            || !retained.request.uses_budget(common.memory())
            || retained.request.value().is_none()
        {
            return Err(invalid());
        }
        common
            .charge(1, 0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let phase = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedNormalWorldRuleLifecycleEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let event_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<crate::WorldEvent>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let id = crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(ctx, common)?;
        let event = event_fee.retain(crate::WorldEvent {
            id,
            time: now,
            caused_by: None,
            body: crate::WorldEventBody::Domain(retained.event),
        });
        Ok(PreparedNormalWorldRuleLifecycleEventV1 {
            event,
            source: LifecycleEventSourceV1::Retained(RetainedLifecycleRequestNoopV1 {
                operation: retained.operation,
                guard: ctx.transaction.guard(),
                identity: retained.identity,
                request: retained.request,
                _phase: retained.phase,
            }),
            memory: common.memory().clone(),
            now,
            _phase: phase,
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
