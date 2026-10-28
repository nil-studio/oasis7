//! Agent-only genesis preparation using borrowed typed fields. No World clone,
//! ordinary export, history hydration, active cache install or head publication.
use super::{
    World,
    incremental_body::{
        BodyMutationContext, BodyMutationError, write_body_value, write_field_bytes,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::StateNamespaceV1 as Ns,
    incremental_value::{ValueNodeWriter, ValueStreamWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    DomainEvent,
    incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::*,
    incremental_event_fields::{MailboxFrontierV1, MailboxKeyV1, mailbox_event_ref_v1},
};

/// Stream only this scalar. Updating a status never serializes an unrelated
/// large summary, mailbox, agent, World or history object.
pub fn write_agent_scalar_value<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: AgentFieldV1,
    value: AgentScalarRefV1<'_>,
    context: &mut BodyMutationContext<'_, W, C>,
    max_bytes: u64,
) -> Result<[u8; 32], BodyMutationError> {
    let _metadata = context
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    field
        .validate_value(value)
        .map_err(BodyMutationError::Invalid)?;
    let length = value.encoded_len().map_err(BodyMutationError::Invalid)?;
    if length > max_bytes {
        return Err(ExecutionError::ResourceLimited("agent scalar bytes").into());
    }
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        length,
        &mut context.cancelled,
    )?;
    if value.write_to(&mut writer).is_err() {
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("agent scalar write failed"))
            .into());
    }
    let root = writer.finish()?;
    if root.0.logical_len != length {
        return Err(BodyMutationError::Invalid("agent scalar length"));
    }
    Ok(root.digest()?)
}
pub struct AgentGenesisContext<'a, W, C, F, L> {
    pub mutation: BodyMutationContext<'a, W, C>,
    /// Admit output metadata and provisional state writes in the same pool.
    /// This callback must not install an active World or publish a head.
    pub emit: F,
    /// Return the actual persisted original WorldEvent ID matching this Domain
    /// occurrence. Missing provenance is an error; never synthesize envelopes.
    pub mailbox_original: L,
    pub max_scalar_bytes: u64,
}
struct Visitor<'a, 'b, W, C, F, L> {
    agent: &'a str,
    context: &'a mut AgentGenesisContext<'b, W, C, F, L>,
    emitted: u64,
}
impl<W, C: FnMut() -> bool, F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>, L>
    Visitor<'_, '_, W, C, F, L>
{
    fn emit(&mut self, namespace: Ns, key: &[u8], id: [u8; 32]) -> Result<(), BodyMutationError> {
        if (self.context.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        (self.context.emit)(namespace, key, id)?;
        self.emitted = self
            .emitted
            .checked_add(1)
            .ok_or(BodyMutationError::Invalid("agent field count overflow"))?;
        Ok(())
    }
}
impl<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    L: FnMut(u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
> AgentCellFieldVisitor for Visitor<'_, '_, W, C, F, L>
{
    type Error = BodyMutationError;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        let key =
            AgentFieldKeyV1::new(self.agent, field, ordinal).map_err(BodyMutationError::Invalid)?;
        let id = write_agent_scalar_value(
            field,
            value,
            &mut self.context.mutation,
            self.context.max_scalar_bytes,
        )?;
        self.emit(field.namespace(), key.as_bytes(), id)
    }
    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error> {
        let key = AgentBodyKeyV1::new(self.agent).map_err(BodyMutationError::Invalid)?;
        let id = write_body_value(value, &mut self.context.mutation)?;
        self.emit(Ns::Business, key.as_bytes(), id)
    }
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        let key = MailboxKeyV1::new(self.agent, None).map_err(BodyMutationError::Invalid)?;
        let bytes = MailboxFrontierV1 {
            head: 0,
            tail: count,
        }
        .canonical_bytes()
        .map_err(BodyMutationError::Invalid)?;
        let id = write_field_bytes(&bytes, &mut self.context.mutation)?;
        self.emit(Ns::Scheduler, key.as_bytes(), id)
    }
    fn mailbox(&mut self, ordinal: u64, event: &DomainEvent) -> Result<(), Self::Error> {
        let key =
            MailboxKeyV1::new(self.agent, Some(ordinal)).map_err(BodyMutationError::Invalid)?;
        if (self.context.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let original = (self.context.mailbox_original)(ordinal, event)?;
        let id = write_field_bytes(&mailbox_event_ref_v1(original), &mut self.context.mutation)?;
        self.emit(Ns::Scheduler, key.as_bytes(), id)
    }
}
impl World {
    /// Emit one borrowed agent's full subfield component for a new world. A
    /// successful count is not a complete world, closure receipt or activation.
    pub fn write_iws_agent_genesis_fields<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
        L: FnMut(u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
    >(
        &self,
        agent: &str,
        context: &mut AgentGenesisContext<'_, W, C, F, L>,
    ) -> Result<u64, BodyMutationError> {
        let cell = self
            .state
            .agents
            .get(agent)
            .ok_or(BodyMutationError::Invalid("genesis agent absent"))?;
        write_agent_cell_fields(agent, cell, context)
    }
}

/// Lower a newly constructed or borrowed cell using the same typed field
/// writer. It does not inspect or copy any other agent or World collection.
pub(super) fn write_agent_cell_fields<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(Ns, &[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    L: FnMut(u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    agent: &str,
    cell: &oasis7_world_state::AgentCell,
    context: &mut AgentGenesisContext<'_, W, C, F, L>,
) -> Result<u64, BodyMutationError> {
    let _metadata = context
        .mutation
        .memory
        .reserve(16 * 1024, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let mut visitor = Visitor {
        agent,
        context,
        emitted: 0,
    };
    visit_agent_cell(agent, cell, &mut visitor).map_err(|e| match e {
        AgentVisitError::Invalid(e) => BodyMutationError::Invalid(e),
        AgentVisitError::Sink(e) => e,
    })?;
    if (visitor.context.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(visitor.emitted)
}

#[cfg(test)]
#[path = "incremental_agent_genesis_tests.rs"]
mod tests;
