//! Requester body/mailbox and typed original sink for immediate
//! MaterialTransferred. This does not expand the generic AgentEvent allowlist.
use super::{
    AppliedMaterialTransferEventV1, MaterialTransferWorkV1, PreparedMaterialTransferEventV1,
};
use crate::world::{
    incremental_body::{BodyMutationError, write_body_value, write_field_bytes},
    incremental_body_event::{AgentEventOriginalV1, original, read_fixed},
    incremental_module_execution_event::contribute_material_transfer_event_v1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_event_fields::{
        MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1, mailbox_event_ref_v1,
    },
};

#[expect(
    clippy::too_many_arguments,
    reason = "The prepared transfer, Current agent proof, event counter, work and original sink stay explicit."
)]
pub(super) fn sink_prepared_material_transfer_v1<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    prepared: &PreparedMaterialTransferEventV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    work: &mut MaterialTransferWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist_original: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        if !agents.uses_budget(step.mutation.memory)
            || agents.allocation_class() != MemoryClass::Ordinary
            || !step.transaction.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "MaterialTransfer event sink pool/class",
            ));
        }
        let _phase_fee = step
            .mutation
            .memory
            .reserve(3 * 1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let event = prepared.event();
        let crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransferred {
            requester_agent_id,
            ..
        }) = &event.body
        else {
            return Err(BodyMutationError::Invalid(
                "MaterialTransfer event sink body",
            ));
        };

        // Mailboxes refer to the canonical original object digest. Measure once
        // here to obtain that link; the narrow shared bridge independently
        // remeasures while it persists the actual original and Event fact.
        // Both passes consume this step's same finite CanonicalCborWork.
        let measured = original::measure(
            event,
            step.mutation.memory,
            step.original_work,
            &mut step.mutation.cancelled,
            max_event_bytes,
        )?;
        let original_id = measured.id();

        // The Prepared identity still describes the exact Current material and
        // receipt writes. Run the shared validation/persistence bridge before
        // any further transaction writes, then append the real requester sink.
        contribute_material_transfer_event_v1(
            step.time(),
            prepared,
            live,
            step.transaction,
            step.values,
            step.mutation,
            step.original_work,
            max_event_bytes,
            persist_original,
        )?;
        update_last_active(step, requester_agent_id, event.time, live)?;
        append_requester_mailbox(step, requester_agent_id, original_id, work, live)?;
        let applied = AppliedMaterialTransferEventV1 {
            prepared,
            identity: step.transaction.guard(),
        };
        applied.validate(step.transaction, step.mutation.memory, step.time())?;
        if (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn update_last_active<M, V, W, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    agent: &str,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
{
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    let key = AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
    let current =
        AgentBodyValueV1::from_canonical_bytes(&read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
            step.transaction,
            Ns::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
    if current.last_active == now {
        return Ok(());
    }
    let updated = AgentBodyValueV1 {
        body_view: current.body_view,
        last_active: now,
    };
    let reference = write_body_value(&updated, step.mutation)?;
    step.transaction
        .write(Ns::Business, key.as_bytes(), Some(reference))?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

fn append_requester_mailbox<M, V, W, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    requester: &str,
    original_id: [u8; 32],
    work: &mut MaterialTransferWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
{
    work.spend(
        requester
            .len()
            .checked_add(64)
            .ok_or(ValueTreeError::ResourceLimited(
                "MaterialTransfer mailbox work",
            ))?,
    )?;
    let key = MailboxKeyV1::new(requester, None).map_err(BodyMutationError::Invalid)?;
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
        MailboxKeyV1::new(requester, Some(frontier.tail)).map_err(BodyMutationError::Invalid)?;
    let next_tail = frontier
        .tail
        .checked_add(1)
        .ok_or(BodyMutationError::Invalid(
            "MaterialTransfer mailbox overflow",
        ))?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    if step
        .transaction
        .read(Ns::Scheduler, item.as_bytes())?
        .is_some()
    {
        return Err(BodyMutationError::Invalid(
            "MaterialTransfer mailbox slot occupied",
        ));
    }
    let item_ref = write_field_bytes(&mailbox_event_ref_v1(original_id), step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, item.as_bytes(), Some(item_ref))?;
    let frontier_bytes = MailboxFrontierV1 {
        head: frontier.head,
        tail: next_tail,
    }
    .canonical_bytes()
    .map_err(BodyMutationError::Invalid)?;
    let frontier_ref = write_field_bytes(&frontier_bytes, step.mutation)?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Scheduler, key.as_bytes(), Some(frontier_ref))?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
