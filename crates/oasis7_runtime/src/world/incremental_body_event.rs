//! Original agent-event fact, allocator and mailbox contribution in one overlay.
//! Acceptance, consensus and cache installation remain outer driver operations.
use super::body::PreparedBodyAttributesUpdate;
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
};
use crate::{DomainEvent, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_wasm_abi::CanonicalCborWorkV1;
#[path = "incremental_agent_event_original.rs"]
mod original;
use oasis7_world_state::incremental_agent_body::{
    AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1,
};
use oasis7_world_state::incremental_event_fields::*;
pub use original::{AgentEventOriginalSinkV1, AgentEventOriginalStreamV1, AgentEventOriginalV1};

pub struct AgentEventContext<'a, V, W, C, F> {
    pub values: &'a V,
    pub mutation: BodyMutationContext<'a, W, C>,
    /// Consume the one-shot borrowed producer and finish its sink. Callback Ok
    /// alone cannot append a fact; original persistence does not grant acceptance.
    pub persist_original: F,
    pub max_event_bytes: usize,
    /// Both measurement and actual output consume this same continuous work.
    pub original_work: CanonicalCborWorkV1,
}

pub(super) fn read_fixed<const N: usize, V: ValueNodeReader, R: MerkleNodeReader, W, C>(
    txn: &mut ExecutionTransaction<'_, R>,
    namespace: Ns,
    key: &[u8],
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; N], BodyMutationError>
where
    C: FnMut() -> bool,
{
    let id = txn
        .read(namespace, key)?
        .ok_or(BodyMutationError::Invalid("missing event control leaf"))?;
    let mut reader = ValueReadContext {
        source: values,
        memory: context.memory,
        class: MemoryClass::Ordinary,
        io: context.io,
    };
    let root = reader.load_root(&id)?;
    if root.0.logical_len != N as u64 {
        return Err(BodyMutationError::Invalid("event control value length"));
    }
    let mut bytes = [0; N];
    reader.read_range(
        &root,
        0,
        N as u64,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut context.cancelled,
    )?;
    Ok(bytes)
}

impl World {
    /// Stage body-attribute or move state changes and the exact original fact.
    /// No hot journal scan/copy, tick consensus reconstruction or state hash.
    /// The caller supplies a borrowed original envelope and the common txn.
    pub fn contribute_iws_agent_event<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    >(
        &self,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
    ) -> Result<(), BodyMutationError> {
        txn.contribute(|txn| {
            let WorldEvent {
                id: _,
                time: _,
                caused_by: _,
                body: _,
            } = event;
            let WorldEventBody::Domain(domain) = &event.body else {
                return Err(BodyMutationError::Invalid(
                    "unsupported incremental agent event",
                ));
            };
            let agent_id = match domain {
                DomainEvent::BodyAttributesUpdated { agent_id, .. }
                | DomainEvent::AgentMoved { agent_id, .. } => agent_id,
                _ => {
                    return Err(BodyMutationError::Invalid(
                        "unsupported incremental agent event",
                    ));
                }
            };
            if !txn.uses_budget(context.mutation.memory) {
                return Err(BodyMutationError::Invalid("transaction memory pool"));
            }
            if event.time != self.state.time {
                return Err(BodyMutationError::Invalid(
                    "event time differs from reducer clock",
                ));
            }
            self.validate_agent_intent_receipt_reference(domain, Some(event.id))
                .map_err(|_| BodyMutationError::Invalid("event receipt reference"))?;
            let sequence = EventSequenceV1::from_canonical_bytes(&read_fixed::<
                EVENT_SEQUENCE_BYTES,
                _,
                _,
                _,
                _,
            >(
                txn,
                Ns::Allocator,
                EVENT_SEQUENCE_KEY_V1,
                context.values,
                &mut context.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            let (allocated, next, era) = Self::preview_next_event_id(sequence.next, sequence.era);
            if event.id != allocated {
                return Err(BodyMutationError::Invalid("event allocator precondition"));
            }
            let frontier_key =
                MailboxKeyV1::new(agent_id, None).map_err(BodyMutationError::Invalid)?;
            let mut frontier = MailboxFrontierV1::from_canonical_bytes(&read_fixed::<
                MAILBOX_FRONTIER_BYTES,
                _,
                _,
                _,
                _,
            >(
                txn,
                Ns::Scheduler,
                frontier_key.as_bytes(),
                context.values,
                &mut context.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            let item_key = MailboxKeyV1::new(agent_id, Some(frontier.tail))
                .map_err(BodyMutationError::Invalid)?;
            frontier.tail = frontier
                .tail
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("mailbox ordinal overflow"))?;
            if txn.read(Ns::Scheduler, item_key.as_bytes())?.is_some() {
                return Err(BodyMutationError::Invalid(
                    "mailbox append position occupied",
                ));
            }

            let original = original::measure(
                event,
                context.mutation.memory,
                &mut context.original_work,
                &mut context.mutation.cancelled,
                context.max_event_bytes,
            )?;
            let id = original.id();

            match domain {
                DomainEvent::BodyAttributesUpdated { view, .. } => {
                    let key = AgentBodyKeyV1::new(agent_id).map_err(BodyMutationError::Invalid)?;
                    let current = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
                        AGENT_BODY_VALUE_BYTES,
                        _,
                        _,
                        _,
                        _,
                    >(
                        txn,
                        Ns::Business,
                        key.as_bytes(),
                        context.values,
                        &mut context.mutation,
                    )?)
                    .map_err(BodyMutationError::Invalid)?;
                    super::body::validate_body_kernel_view(&current.body_view, view)
                        .map_err(BodyMutationError::Rejected)?;
                    let _prepared_charge = context
                        .mutation
                        .memory
                        .reserve(agent_id.len() + 512, MemoryClass::Ordinary)
                        .map_err(ExecutionError::Memory)?;
                    let prepared = PreparedBodyAttributesUpdate::new(
                        agent_id.clone(),
                        view.clone(),
                        event.time,
                    );
                    prepared.contribute_iws(
                        &current.body_view,
                        current.last_active,
                        txn,
                        &mut context.mutation,
                    )?;
                }
                DomainEvent::AgentMoved { to, .. } => {
                    super::incremental_agent_move::contribute_move(
                        agent_id,
                        to,
                        event.time,
                        txn,
                        context.values,
                        &mut context.mutation,
                    )?;
                }
                _ => unreachable!("event kind was checked before any contribution"),
            }
            let next_ref = write_field_bytes(
                &EventSequenceV1 { next, era }.canonical_bytes(),
                &mut context.mutation,
            )?;
            txn.write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;
            let item_ref = write_field_bytes(&mailbox_event_ref_v1(id), &mut context.mutation)?;
            txn.write(Ns::Scheduler, item_key.as_bytes(), Some(item_ref))?;
            let frontier_ref = write_field_bytes(
                &frontier
                    .canonical_bytes()
                    .map_err(BodyMutationError::Invalid)?,
                &mut context.mutation,
            )?;
            txn.write(Ns::Scheduler, frontier_key.as_bytes(), Some(frontier_ref))?;
            let mut producer = original::Producer::new(
                original,
                context.mutation.memory,
                &mut context.original_work,
                &mut context.mutation.cancelled,
            );
            (context.persist_original)(&mut producer)?;
            producer.check_complete()?;
            drop(producer);
            if (context.mutation.cancelled)() {
                return Err(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into(),
                );
            }
            txn.append_fact(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Event,
                id,
            })?;
            if (context.mutation.cancelled)() {
                return Err(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into(),
                );
            }
            Ok(())
        })
    }
}
#[cfg(test)]
#[path = "incremental_body_event_tests.rs"]
mod tests;
