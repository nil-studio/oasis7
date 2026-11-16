//! A genuine Current release rejection is published while its complete source
//! phases stay paid. A bare caller reason cannot construct this event owner.
use super::*;
use crate::world::{
    incremental_action_execution::SelectedActionAuditV1,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn publish_current_release_rejection_v1<M, P, V, W, D, E, C, ME, F>(
    rejection: super::release_plan::PreparedCurrentReleaseRejectionV1,
    accepted: Option<SelectedActionAuditV1>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<super::event::PreparedNormalModuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    rejection.validate_selection(
        selected,
        outcome,
        step.transaction,
        common,
        step.time(),
        live,
    )?;
    let heap = rejection
        .reason
        .reserved_bytes()
        .checked_mul(2)
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::WorldEvent>()))
        .ok_or_else(invalid)?;
    pay(common, heap)?;
    let fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let id = {
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
        crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(
            &mut ctx, common,
        )?
    };
    let action_id = rejection.action_id();
    let event = fee.retain(crate::WorldEvent {
        id,
        time: step.time(),
        caused_by: Some(crate::CausedBy::Action(action_id)),
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            action_id,
            reason: (*rejection.reason).clone(),
        }),
    });
    let bytes = std::mem::size_of::<super::event::PreparedNormalModuleLifecycleEventV1>()
        .checked_add(std::mem::size_of::<
            super::release_plan::PreparedCurrentReleaseRejectionV1,
        >())
        .ok_or_else(invalid)?;
    pay(common, bytes)?;
    let layout = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let cap = super::event::PreparedNormalModuleLifecycleEventV1 {
        event,
        accepted,
        guard: step.transaction.guard(),
        core: common.work().identity(),
        action_id,
        memory: common.memory().clone(),
        state: super::event::ModuleLifecycleCommittedStateV1::ReleaseRejection(Box::new(rejection)),
        _layout: layout,
        _prepare_fees: std::array::from_fn(|_| None),
    };
    step.append_prepared_module_lifecycle_event_v1(
        &cap,
        materials,
        common,
        live,
        max_event_bytes,
        persist,
    )?;
    Ok(cap)
}
