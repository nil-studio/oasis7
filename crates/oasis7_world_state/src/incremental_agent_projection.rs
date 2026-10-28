//! Semantic visitor adapter; its serde view is not the frozen scalar wire.
use super::*;
use crate::incremental_projection::*;
use crate::{
    AgentCell, DomainEvent, incremental_agent_body::AgentBodyValueV1,
    incremental_event_fields::MailboxFrontierV1,
};
use std::collections::BTreeMap;
fn descriptor(field: AgentFieldV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: field.name(),
        destination: FieldDestination::State(field.namespace()),
        shape: FieldShape::NestedMap,
    }
}
fn special(name: &'static str, namespace: Ns) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        destination: FieldDestination::State(namespace),
        shape: FieldShape::ByteMap,
    }
}
struct Adapter<'a, S> {
    agent: &'a str,
    sink: &'a mut S,
}
impl<S: GenesisFieldVisitor> AgentCellFieldVisitor for Adapter<'_, S> {
    type Error = S::Error;
    fn scalar(
        &mut self,
        field: AgentFieldV1,
        ordinal: u64,
        value: AgentScalarRefV1<'_>,
    ) -> Result<(), Self::Error> {
        self.sink
            .entry(descriptor(field), &(self.agent, ordinal), &value)
    }
    fn body(&mut self, value: &AgentBodyValueV1) -> Result<(), Self::Error> {
        self.sink.bytes(
            special("agents.body", Ns::Business),
            self.agent,
            &value.canonical_bytes(),
        )
    }
    fn mailbox_frontier(&mut self, count: u64) -> Result<(), Self::Error> {
        let bytes = MailboxFrontierV1 {
            head: 0,
            tail: count,
        }
        .canonical_bytes()
        .expect("genesis frontier ordered");
        self.sink.bytes(
            special("agents.mailbox_frontier", Ns::Scheduler),
            self.agent,
            &bytes,
        )
    }
    fn mailbox(&mut self, ordinal: u64, event: &DomainEvent) -> Result<(), Self::Error> {
        self.sink.entry(
            special("agents.mailbox", Ns::Scheduler),
            &(self.agent, ordinal),
            event,
        )
    }
}
pub fn visit_agents_projection<S: GenesisFieldVisitor>(
    agents: &BTreeMap<String, AgentCell>,
    sink: &mut S,
) -> Result<(), ProjectionError<S::Error>> {
    for field in AgentFieldV1::ALL {
        sink.begin(descriptor(*field), None)
            .map_err(ProjectionError::Sink)?;
    }
    let special = [
        special("agents.body", Ns::Business),
        special("agents.mailbox_frontier", Ns::Scheduler),
        special("agents.mailbox", Ns::Scheduler),
    ];
    for field in special {
        sink.begin(field, None).map_err(ProjectionError::Sink)?;
    }
    for (agent, cell) in agents {
        visit_agent_cell(agent, cell, &mut Adapter { agent, sink }).map_err(|e| match e {
            AgentVisitError::Invalid(e) => ProjectionError::Invalid(e),
            AgentVisitError::Sink(e) => ProjectionError::Sink(e),
        })?;
    }
    for field in AgentFieldV1::ALL {
        sink.end(descriptor(*field))
            .map_err(ProjectionError::Sink)?;
    }
    for field in special {
        sink.end(field).map_err(ProjectionError::Sink)?;
    }
    Ok(())
}
