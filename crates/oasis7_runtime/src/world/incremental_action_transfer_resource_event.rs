//! Specialized Current transfer balance, two-recipient mailbox and original sink.
//! This path is action-owner-only and does not expand the generic event reducer.
use super::{PreparedTransferResourceV1, TransferResourceWorkV1, overlay_error};
use crate::world::{
    World,
    incremental_body::{BodyMutationError, write_body_value, write_field_bytes},
    incremental_body_event::{AgentEventOriginalV1, original, read_fixed},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionFence,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_fields::AgentScalarRefV1,
    incremental_agent_subtree::{
        AgentSubtreeCellChangeV1, AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, change_agent_subtree_scalar_v1,
    },
    incremental_event_fields::{
        EVENT_SEQUENCE_KEY_V1, EventSequenceV1, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1,
        MailboxKeyV1, mailbox_event_ref_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_value_mutation::ValueMutationContextV1,
};
#[expect(
    clippy::too_many_arguments,
    reason = "Prepared action, qualified Current source, shared work and original sink stay explicit."
)]
pub(super) fn sink_prepared_transfer_resource_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    prepared: &PreparedTransferResourceV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut TransferResourceWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist_original: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        if read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(step.mutation.memory)
            || !agents.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "TransferResource sink pool/class",
            ));
        }
        let _key_fee = step
            .mutation
            .memory
            .reserve(3 * 1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let event = prepared.event();
        let (from, to, kind, _) = prepared.route();
        // The original EconomyData owner installs the same two-map mirror
        // as CorePolicy. Normalize the actual Current maps in this transaction,
        // before either resource write or the event/mailbox publication.
        let material_guard = step.transaction.guard();
        materials.contribute(
            step.transaction,
            step.values,
            step.mutation,
            &material_guard,
            live,
        )?;
        if from != to {
            let ordinal = match kind {
                oasis7_agent_api::world_action::ResourceKind::Electricity => 0,
                oasis7_agent_api::world_action::ResourceKind::Data => 1,
            };
            change_resource(
                step,
                read,
                agents,
                work,
                from,
                ordinal,
                prepared.from_before,
                prepared.from_after,
                live,
            )?;
            change_resource(
                step,
                read,
                agents,
                work,
                to,
                ordinal,
                prepared.to_before,
                prepared.to_after,
                live,
            )?;
        }
        update_last_active(step, from, event.time, live)?;
        if from != to {
            update_last_active(step, to, event.time, live)?;
        }
        append_mailboxes_and_original(
            step,
            prepared,
            max_event_bytes,
            work,
            live,
            persist_original,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn change_resource<M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    work: &mut TransferResourceWorkV1,
    agent: &str,
    ordinal: u64,
    before: Option<i64>,
    after: i64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
{
    work.spend(
        agent
            .len()
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "TransferResource resource write bytes",
            ))?,
    )?;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
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
    let mut output = ValueMutationContextV1 {
        sink: &mut *step.mutation.sink,
        memory: step.mutation.memory,
        io: &mut *step.mutation.io,
        cancelled: &mut step.mutation.cancelled,
    };
    let guard = overlay.transaction.guard();
    let changed = change_agent_subtree_scalar_v1(
        AgentSubtreeCellChangeV1 {
            query: AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal,
                },
                qualification: agents,
            },
            expected: before.map(AgentScalarRefV1::I64),
            after: (after != 0).then_some(AgentScalarRefV1::I64(after)),
            max_scalar_bytes: work.max_scalar_bytes,
        },
        &mut overlay,
        &mut output,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    if !changed {
        return Err(BodyMutationError::Invalid(
            "TransferResource resource write unchanged",
        ));
    }
    Ok(())
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
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    let updated = AgentBodyValueV1 {
        body_view: current.body_view,
        last_active: now,
    };
    let reference = write_body_value(&updated, step.mutation)?;
    step.transaction
        .write(Ns::Business, key.as_bytes(), Some(reference))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_mailboxes_and_original<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    prepared: &PreparedTransferResourceV1,
    max_event_bytes: usize,
    work: &mut TransferResourceWorkV1,
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
    prepared.validate(step.transaction, step.mutation.memory, step.time())?;
    let event = prepared.event();
    let (from, to, _, _) = prepared.route();
    let sequence = World::read_iws_allocator(
        crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
        step.transaction,
        step.values,
        step.mutation,
    )?;
    let (allocated, next, era) = World::preview_next_event_id(sequence.next, sequence.era);
    if event.id != allocated {
        return Err(BodyMutationError::Invalid("transfer event allocator"));
    }
    let measured = original::measure(
        event,
        step.mutation.memory,
        step.original_work,
        &mut step.mutation.cancelled,
        max_event_bytes,
    )?;
    let original_id = measured.id();
    let next_ref = write_field_bytes(
        &EventSequenceV1 { next, era }.canonical_bytes(),
        step.mutation,
    )?;
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction
        .write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;
    for (index, agent) in [from, to].into_iter().enumerate() {
        if index == 1 && from == to {
            continue;
        }
        work.spend(
            agent
                .len()
                .checked_add(64)
                .ok_or(ValueTreeError::ResourceLimited("transfer mailbox work"))?,
        )?;
        let key = MailboxKeyV1::new(agent, None).map_err(BodyMutationError::Invalid)?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        let mut frontier = MailboxFrontierV1::from_canonical_bytes(&read_fixed::<
            MAILBOX_FRONTIER_BYTES,
            _,
            _,
            _,
            _,
        >(
            step.transaction,
            Ns::Scheduler,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
        let item =
            MailboxKeyV1::new(agent, Some(frontier.tail)).map_err(BodyMutationError::Invalid)?;
        frontier.tail = frontier
            .tail
            .checked_add(1)
            .ok_or(BodyMutationError::Invalid("transfer mailbox overflow"))?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        if step
            .transaction
            .read(Ns::Scheduler, item.as_bytes())?
            .is_some()
        {
            return Err(BodyMutationError::Invalid("transfer mailbox occupied"));
        }
        let item_ref = write_field_bytes(&mailbox_event_ref_v1(original_id), step.mutation)?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        step.transaction
            .write(Ns::Scheduler, item.as_bytes(), Some(item_ref))?;
        let frontier_ref = write_field_bytes(
            &frontier
                .canonical_bytes()
                .map_err(BodyMutationError::Invalid)?,
            step.mutation,
        )?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        step.transaction
            .write(Ns::Scheduler, key.as_bytes(), Some(frontier_ref))?;
    }
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    let mut producer = original::Producer::new(
        measured,
        step.mutation.memory,
        step.original_work,
        &mut step.mutation.cancelled,
    );
    persist_original(&mut producer)?;
    producer.check_complete()?;
    drop(producer);
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let guard = step.transaction.guard();
    step.transaction.validate_guard(&guard, live)?;
    step.transaction.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Event,
        id: original_id,
    })?;
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
