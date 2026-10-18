use super::{AgentCell, DomainEvent};
use serde::Serialize;
use serde::ser::{SerializeMap, SerializeSeq, SerializeStruct};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, PartialEq)]
pub(super) enum BodyOverlayMutation {
    Body {
        body_view: crate::agent_state::BodyKernelView,
        last_active: super::WorldTime,
    },
    RouteOnly,
}

/// A typed, borrowed overlay for the state fields needed while preparing a
/// domain transition. The overlay is intentionally narrow: it cannot mutate
/// the canonical [`WorldState`] and it can either update the target agent's
/// body fields or represent a route-only event with no body mutation.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyOverlay {
    pub(super) agent_id: String,
    pub(super) mutation: BodyOverlayMutation,
    pub(super) routed_domain_event: Option<DomainEvent>,
}

impl BodyOverlay {
    pub fn new(
        agent_id: impl Into<String>,
        body_view: crate::agent_state::BodyKernelView,
        last_active: super::WorldTime,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            mutation: BodyOverlayMutation::Body {
                body_view,
                last_active,
            },
            routed_domain_event: None,
        }
    }

    pub(crate) fn route_only(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            mutation: BodyOverlayMutation::RouteOnly,
            routed_domain_event: None,
        }
    }

    pub(crate) fn with_routed_domain_event(mut self, event: DomainEvent) -> Self {
        self.routed_domain_event = Some(event);
        self
    }

    pub(super) fn requires_body_target(&self) -> bool {
        matches!(self.mutation, BodyOverlayMutation::Body { .. })
    }
}

pub(super) struct AgentMapProjection<'a> {
    pub(super) agents: &'a BTreeMap<String, AgentCell>,
    pub(super) body_overlay: Option<&'a BodyOverlay>,
}

impl Serialize for AgentMapProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.agents.len()))?;
        for (agent_id, cell) in self.agents {
            if let Some(overlay) = self
                .body_overlay
                .filter(|overlay| overlay.agent_id == *agent_id)
            {
                map.serialize_entry(
                    agent_id,
                    &AgentCellProjection {
                        cell,
                        body_overlay: overlay,
                    },
                )?;
            } else {
                map.serialize_entry(agent_id, cell)?;
            }
        }
        map.end()
    }
}

struct AgentCellProjection<'a> {
    cell: &'a AgentCell,
    body_overlay: &'a BodyOverlay,
}

impl Serialize for AgentCellProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let field_count =
            3 + usize::from(self.cell.activity.is_some()) + usize::from(self.cell.intent.is_some());
        let mut state = serializer.serialize_struct("AgentCell", field_count)?;
        let body_view = match &self.body_overlay.mutation {
            BodyOverlayMutation::Body { body_view, .. } => body_view,
            BodyOverlayMutation::RouteOnly => &self.cell.state.body_view,
        };
        let last_active = match &self.body_overlay.mutation {
            BodyOverlayMutation::Body { last_active, .. } => last_active,
            BodyOverlayMutation::RouteOnly => &self.cell.last_active,
        };
        state.serialize_field(
            "state",
            &AgentStateProjection {
                state: &self.cell.state,
                body_view,
            },
        )?;
        state.serialize_field(
            "mailbox",
            &MailboxProjection {
                mailbox: &self.cell.mailbox,
                appended_event: self.body_overlay.routed_domain_event.as_ref(),
            },
        )?;
        state.serialize_field("last_active", last_active)?;
        if self.cell.activity.is_some() {
            state.serialize_field("activity", &self.cell.activity)?;
        }
        if self.cell.intent.is_some() {
            state.serialize_field("intent", &self.cell.intent)?;
        }
        state.end()
    }
}

struct MailboxProjection<'a> {
    mailbox: &'a VecDeque<DomainEvent>,
    appended_event: Option<&'a DomainEvent>,
}

impl Serialize for MailboxProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(
            self.mailbox.len() + usize::from(self.appended_event.is_some()),
        ))?;
        for event in self.mailbox {
            sequence.serialize_element(event)?;
        }
        if let Some(event) = self.appended_event {
            sequence.serialize_element(event)?;
        }
        sequence.end()
    }
}

struct AgentStateProjection<'a> {
    state: &'a crate::agent_state::AgentState,
    body_view: &'a crate::agent_state::BodyKernelView,
}

impl Serialize for AgentStateProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AgentState", 6)?;
        state.serialize_field("agent_id", &self.state.agent_id)?;
        state.serialize_field("pos", &self.state.pos)?;
        state.serialize_field("body", &self.state.body)?;
        state.serialize_field("resources", &self.state.resources)?;
        state.serialize_field("body_view", self.body_view)?;
        state.serialize_field("body_state", &self.state.body_state)?;
        state.end()
    }
}
