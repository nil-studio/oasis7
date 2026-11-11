//! Genuine settled lifecycle event, one operator mailbox and exact original.
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
pub(super) fn sink<
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    prepared: &PreparedFactoryLifecycleEventV1,
    work: &mut FactoryLifecycleWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<(), BodyMutationError> {
    prepared.validate(step.transaction, step.mutation.memory, step.time())?;
    let event = prepared.event();
    let operator = match &event.body {
        crate::WorldEventBody::Domain(
            crate::DomainEvent::FactoryMaintained {
                operator_agent_id, ..
            }
            | crate::DomainEvent::FactoryRecycled {
                operator_agent_id, ..
            },
        ) => operator_agent_id.as_str(),
        _ => return Err(BodyMutationError::Invalid("factory sink domain")),
    };
    let _phase = step
        .mutation
        .memory
        .reserve(
            operator
                .len()
                .checked_add(4096)
                .ok_or(ValueTreeError::ResourceLimited("factory event sink"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let measured = original::measure(
        event,
        step.mutation.memory,
        step.original_work,
        &mut step.mutation.cancelled,
        max_event_bytes,
    )?;
    let original_id = measured.id();
    crate::world::incremental_module_execution_event::contribute_factory_lifecycle_event_v1(
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
    if prepared.touch_operator {
        let key = AgentBodyKeyV1::new(operator).map_err(BodyMutationError::Invalid)?;
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
        if before.last_active != event.time {
            let guard = step.transaction.guard();
            step.transaction.validate_guard(&guard, live)?;
            let root = write_body_value(
                &AgentBodyValueV1 {
                    body_view: before.body_view,
                    last_active: event.time,
                },
                step.mutation,
            )?;
            step.transaction
                .write(Ns::Business, key.as_bytes(), Some(root))?;
        }
    }
    work.spend(
        operator
            .len()
            .checked_add(64)
            .ok_or(ValueTreeError::ResourceLimited("factory mailbox"))?,
    )?;
    let key = MailboxKeyV1::new(operator, None).map_err(BodyMutationError::Invalid)?;
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
        MailboxKeyV1::new(operator, Some(frontier.tail)).map_err(BodyMutationError::Invalid)?;
    let tail = frontier
        .tail
        .checked_add(1)
        .ok_or(BodyMutationError::Invalid("factory mailbox overflow"))?;
    if step
        .transaction
        .read(Ns::Scheduler, item.as_bytes())?
        .is_some()
    {
        return Err(BodyMutationError::Invalid("factory mailbox occupied"));
    }
    let item_root = write_field_bytes(&mailbox_event_ref_v1(original_id), step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, item.as_bytes(), Some(item_root))?;
    let bytes = MailboxFrontierV1 {
        head: frontier.head,
        tail,
    }
    .canonical_bytes()
    .map_err(BodyMutationError::Invalid)?;
    let root = write_field_bytes(&bytes, step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, key.as_bytes(), Some(root))?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
