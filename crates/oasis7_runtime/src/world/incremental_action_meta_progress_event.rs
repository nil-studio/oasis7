//! Actual dual-agent activity publication and target-only domain mailbox.
use super::*;
use crate::world::incremental_body::{write_body_value, write_field_bytes};
use crate::world::incremental_body_event::{original, read_fixed};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_event_fields::{
        MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1, mailbox_event_ref_v1,
    },
};
pub(super) fn touch_agents<
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    operator: &str,
    target: &str,
    work: &mut MetaProgressWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    let bytes = operator
        .len()
        .checked_add(target.len())
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited("meta body workspace"))?;
    let _phase = step
        .mutation
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    work.spend(operator.len().max(target.len()))?;
    for (index, actor) in [operator, target].into_iter().enumerate() {
        if index == 1 && operator == target {
            continue;
        }
        work.spend(actor.len())?;
        if (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let key = AgentBodyKeyV1::new(actor).map_err(BodyMutationError::Invalid)?;
        let before = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
            AGENT_BODY_VALUE_BYTES,
            _,
            _,
            _,
            _,
        >(
            step.transaction,
            Ns::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
        if before.last_active != step.time() {
            let root = write_body_value(
                &AgentBodyValueV1 {
                    body_view: before.body_view,
                    last_active: step.time(),
                },
                step.mutation,
            )?;
            let guard = step.transaction.guard();
            step.transaction.validate_guard(&guard, live)?;
            step.transaction
                .write(Ns::Business, key.as_bytes(), Some(root))?;
        }
    }
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
pub(super) fn sink<
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    prepared: &PreparedMetaProgressEventV1,
    work: &mut MetaProgressWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<(), BodyMutationError> {
    prepared.validate(step.transaction, step.mutation.memory, step.time())?;
    let event = prepared.event();
    let crate::WorldEventBody::Domain(crate::DomainEvent::MetaProgressGranted {
        target_agent_id,
        ..
    }) = &event.body
    else {
        return Err(BodyMutationError::Invalid("meta sink domain"));
    };
    let target = target_agent_id.as_str();
    let _phase = step
        .mutation
        .memory
        .reserve(
            target
                .len()
                .checked_add(4096)
                .ok_or(ValueTreeError::ResourceLimited("meta mailbox workspace"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    work.spend(target.len())?;
    let measured = original::measure(
        event,
        step.mutation.memory,
        step.original_work,
        &mut step.mutation.cancelled,
        max_event_bytes,
    )?;
    let original_id = measured.id();
    // Check the actual destination before the original callback. Accepted has
    // already used its actor mailbox; this slot belongs only to target.
    let key = MailboxKeyV1::new(target, None).map_err(BodyMutationError::Invalid)?;
    let frontier =
        MailboxFrontierV1::from_canonical_bytes(&read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
            step.transaction,
            Ns::Scheduler,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
    let item =
        MailboxKeyV1::new(target, Some(frontier.tail)).map_err(BodyMutationError::Invalid)?;
    let tail = frontier
        .tail
        .checked_add(1)
        .ok_or(BodyMutationError::Invalid("meta mailbox overflow"))?;
    if step
        .transaction
        .read(Ns::Scheduler, item.as_bytes())?
        .is_some()
    {
        return Err(BodyMutationError::Invalid("meta mailbox occupied"));
    }
    let frontier_bytes = MailboxFrontierV1 {
        head: frontier.head,
        tail,
    }
    .canonical_bytes()
    .map_err(BodyMutationError::Invalid)?;
    crate::world::incremental_module_execution_event::contribute_meta_progress_event_v1(
        step.time(),
        prepared,
        live,
        step.transaction,
        step.values,
        step.mutation,
        step.original_work,
        max_event_bytes,
        persist,
    )?;
    let item_root = write_field_bytes(&mailbox_event_ref_v1(original_id), step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, item.as_bytes(), Some(item_root))?;
    let root = write_field_bytes(&frontier_bytes, step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, key.as_bytes(), Some(root))?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
