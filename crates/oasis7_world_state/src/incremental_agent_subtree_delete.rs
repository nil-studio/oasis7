//! Explicit whole-cell expected deletion, with stable ordinal controls.
use super::*;
use crate::{
    incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_genesis::write_agent_scalar_value,
    incremental_agent_mutation::{
        BodyMutationContext, BodyMutationError, DiscardValueNodesV1, write_body_value,
        write_field_bytes,
    },
    incremental_event_fields::{MailboxFrontierV1, MailboxKeyV1, mailbox_event_ref_v1},
    incremental_field_overlay::{WorldFieldOverlayContextV1, lookup_raw},
    incremental_inventory_value_read::InventoryValueReader,
};
use create::overlay_error;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct AgentSubtreeDeleteV1<'a> {
    pub agent: &'a str,
    pub expected: &'a crate::AgentCell,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    /// Ascending real ordinals for the borrowed Vec, not newly enumerated IDs.
    pub slot_ordinals: &'a [u64],
    pub slot_next: u64,
    pub cargo_ordinals: &'a [u64],
    pub cargo_next: u64,
    pub mailbox: MailboxFrontierV1,
    pub max_scalar_bytes: u64,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("agent deletion expected membership/value")
}
fn ordinals(values: &[u64], len: usize, next: u64) -> Result<(), BodyMutationError> {
    if values.len() != len
        || values.last().is_some_and(|ordinal| *ordinal >= next)
        || values.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(invalid());
    }
    Ok(())
}
struct Visitor<'a, 'p, 'v, 'io, M, R, E, C, L> {
    request: &'a AgentSubtreeDeleteV1<'a>,
    tx: &'a mut ExecutionTransaction<'p, M>,
    records: &'a mut InventoryValueReader<'v, 'io, R, C>,
    error: &'a mut E,
    write_io: &'a mut ValueIoBudget,
    original: &'a mut L,
    emitted: u64,
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    L,
> Visitor<'_, '_, '_, '_, M, R, E, C, L>
{
    fn emit(
        &mut self,
        namespace: Ns,
        key: &[u8],
        expected: [u8; 32],
    ) -> Result<(), BodyMutationError> {
        if lookup_raw(self.tx, namespace, key, self.records, self.error).map_err(overlay_error)?
            != Some(expected)
        {
            return Err(invalid());
        }
        self.tx.write(namespace, key, None)?;
        if (self.records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.emitted = self.emitted.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
    fn bytes(&mut self, bytes: &[u8]) -> Result<[u8; 32], BodyMutationError> {
        write_field_bytes(
            bytes,
            &mut BodyMutationContext {
                sink: &mut DiscardValueNodesV1,
                memory: self.records.values.memory,
                io: self.write_io,
                cancelled: &mut *self.records.cancelled,
            },
        )
    }
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    L: FnMut(u64, &crate::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
> AgentCellFieldVisitor for Visitor<'_, '_, '_, '_, M, R, E, C, L>
{
    type Error = BodyMutationError;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        mut value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        let ordinal = match schema::group(field) {
            schema::Group::Slot => *self
                .request
                .slot_ordinals
                .get(ordinal as usize)
                .ok_or_else(invalid)?,
            schema::Group::Cargo => *self
                .request
                .cargo_ordinals
                .get(ordinal as usize)
                .ok_or_else(invalid)?,
            _ => ordinal,
        };
        if field == AgentFieldV1::SlotNext {
            value = AgentScalarRefV1::U64(self.request.slot_next);
        }
        if field == AgentFieldV1::CargoNext {
            value = AgentScalarRefV1::U64(self.request.cargo_next);
        }
        let key = AgentFieldKeyV1::new(self.request.agent, field, ordinal)
            .map_err(BodyMutationError::Invalid)?;
        let expected = write_agent_scalar_value(
            field,
            value,
            &mut BodyMutationContext {
                sink: &mut DiscardValueNodesV1,
                memory: self.records.values.memory,
                io: self.write_io,
                cancelled: &mut *self.records.cancelled,
            },
            self.request.max_scalar_bytes,
        )?;
        self.emit(field.namespace(), key.as_bytes(), expected)
    }
    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error> {
        let key = AgentBodyKeyV1::new(self.request.agent).map_err(BodyMutationError::Invalid)?;
        let expected = write_body_value(
            value,
            &mut BodyMutationContext {
                sink: &mut DiscardValueNodesV1,
                memory: self.records.values.memory,
                io: self.write_io,
                cancelled: &mut *self.records.cancelled,
            },
        )?;
        self.emit(Ns::Business, key.as_bytes(), expected)
    }
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        if self
            .request
            .mailbox
            .tail
            .checked_sub(self.request.mailbox.head)
            != Some(count)
        {
            return Err(invalid());
        }
        let key =
            MailboxKeyV1::new(self.request.agent, None).map_err(BodyMutationError::Invalid)?;
        let expected = self.bytes(
            &self
                .request
                .mailbox
                .canonical_bytes()
                .map_err(BodyMutationError::Invalid)?,
        )?;
        self.emit(Ns::Scheduler, key.as_bytes(), expected)
    }
    fn mailbox(&mut self, ordinal: u64, event: &crate::DomainEvent) -> Result<(), Self::Error> {
        let ordinal = self
            .request
            .mailbox
            .head
            .checked_add(ordinal)
            .ok_or_else(invalid)?;
        if (self.records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let original = (self.original)(ordinal, event)?;
        let key = MailboxKeyV1::new(self.request.agent, Some(ordinal))
            .map_err(BodyMutationError::Invalid)?;
        let expected = self.bytes(&mailbox_event_ref_v1(original))?;
        self.emit(Ns::Scheduler, key.as_bytes(), expected)
    }
}
/// This explicitly requests the complete borrowed cell scope. Expected hashes
/// use the original scalar/body/mailbox producers with a discard sink; no old
/// long payload or unrelated Agent is exported. The raw original callback keeps
/// the GEN provenance contract and never mints an accepted-history capability.
pub fn delete_agent_subtree_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
    L: FnMut(u64, &crate::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    request: AgentSubtreeDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut BodyMutationContext<'_, W, CW>,
    original: &mut L,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<u64, BodyMutationError> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !request.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        ordinals(
            request.slot_ordinals,
            request.expected.state.body_state.slots.len(),
            request.slot_next,
        )?;
        ordinals(
            request.cargo_ordinals,
            request.expected.state.body_state.cargo_entries.len(),
            request.cargo_next,
        )?;
        let mut combined = || cancel() || (write.cancelled)();
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut combined,
        };
        let resolved = point::resolve(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: request.agent,
                    field: AgentFieldV1::Identity,
                    ordinal: 0,
                },
                qualification: request.qualification,
            },
            tx,
            &mut records,
            error,
            request.max_scalar_bytes,
            &mut std::io::sink(),
        )
        .map_err(overlay_error)?;
        if !resolved.agent_exists {
            return Err(invalid());
        }
        for (kind, ordinal) in [
            (oasis7_agent_api::world_action::ResourceKind::Electricity, 0),
            (oasis7_agent_api::world_action::ResourceKind::Data, 1),
        ] {
            if !request.expected.state.resources.amounts.contains_key(&kind) {
                let key =
                    AgentFieldKeyV1::new(request.agent, AgentFieldV1::ResourceAmount, ordinal)
                        .map_err(BodyMutationError::Invalid)?;
                if lookup_raw(
                    tx,
                    AgentFieldV1::ResourceAmount.namespace(),
                    key.as_bytes(),
                    &mut records,
                    error,
                )
                .map_err(overlay_error)?
                .is_some()
                {
                    return Err(invalid());
                }
            }
        }
        let mut visitor = Visitor {
            request: &request,
            tx,
            records: &mut records,
            error,
            write_io: &mut *write.io,
            original,
            emitted: 0,
        };
        visit_agent_cell(request.agent, request.expected, &mut visitor).map_err(
            |error| match error {
                AgentVisitError::Invalid(error) => BodyMutationError::Invalid(error),
                AgentVisitError::Sink(error) => error,
            },
        )?;
        if (visitor.records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(visitor.emitted)
    })
}
