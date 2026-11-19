//! Specialized Current data access balance, two-recipient mailbox and original sink.
//! This path is action-owner-only and does not expand the generic event reducer.
use super::{DataAccessWorkV1, PreparedDataAccessV1, overlay_error};
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
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    incremental_event_fields::{
        EVENT_SEQUENCE_KEY_V1, EventSequenceV1, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1,
        MailboxKeyV1, mailbox_event_ref_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
#[expect(
    clippy::too_many_arguments,
    reason = "Prepared action, qualified Current source, shared work and original sink stay explicit."
)]
pub(super) fn sink_prepared_data_access_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    prepared: &PreparedDataAccessV1,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    controls: &ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    writer: &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext<
        '_,
        W,
        impl FnMut() -> bool,
    >,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut DataAccessWorkV1,
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
            return Err(BodyMutationError::Invalid("DataAccess sink pool/class"));
        }
        let _key_fee = step
            .mutation
            .memory
            .reserve(3 * 1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let event = prepared.event();
        let (from, to, after) = prepared.route();
        // The original EconomyData owner installs the same two-map mirror
        // as CorePolicy. Normalize the actual Current maps in this transaction,
        // before either resource write or the event/mailbox publication.
        step.transaction.validate_guard(&prepared.identity, live)?;
        let material_guard = step.transaction.guard();
        materials.contribute(
            step.transaction,
            step.values,
            step.mutation,
            &material_guard,
            live,
        )?;
        let guard = step.transaction.guard();
        oasis7_world_state::incremental_world_collection_controls::change_current_data_access_membership_v1(
            oasis7_world_state::incremental_world_collection_controls::DataAccessMembershipChangeV1 {subject:from,member:to,expected:prepared.expected,after},
            controls,roots,step.transaction,read,writer,&mut work.controls,&mut work.pending_entries_remaining,&guard,live,
        ).map_err(overlay_error)?;
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
    prepared: &PreparedDataAccessV1,
    max_event_bytes: usize,
    work: &mut DataAccessWorkV1,
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
    let (from, to, _) = prepared.route();
    let sequence = World::read_iws_allocator(
        crate::world::incremental_allocators::RuntimeAllocatorV1::Event,
        step.transaction,
        step.values,
        step.mutation,
    )?;
    let (allocated, next, era) = World::preview_next_event_id(sequence.next, sequence.era);
    if event.id != allocated {
        return Err(BodyMutationError::Invalid("data access event allocator"));
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
                .ok_or(ValueTreeError::ResourceLimited("data access mailbox work"))?,
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
            .ok_or(BodyMutationError::Invalid("data access mailbox overflow"))?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        if step
            .transaction
            .read(Ns::Scheduler, item.as_bytes())?
            .is_some()
        {
            return Err(BodyMutationError::Invalid("data access mailbox occupied"));
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
