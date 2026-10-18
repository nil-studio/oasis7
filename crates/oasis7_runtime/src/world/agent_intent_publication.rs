//! Atomic projection for one AgentIntent lifecycle event.

use std::collections::BTreeMap;

use super::super::agent_cell::AgentCell;
use super::super::{AgentIntentV2, DomainEvent, WorldError, WorldEventId};
use super::World;

#[derive(Debug)]
pub(crate) struct PreparedAgentIntent {
    pub(crate) event: DomainEvent,
    pub(crate) agent_id: String,
    pub(crate) agent: AgentCell,
    pub(crate) ledger_updates: BTreeMap<String, AgentIntentV2>,
}

impl PreparedAgentIntent {
    pub(super) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agent = self.agent.clone();
        agent.mailbox.push_back(self.event.clone());
        BTreeMap::from([(self.agent_id.clone(), agent)])
    }

    pub(crate) fn install_infallible(self, state: &mut super::super::WorldState) {
        state.agents.insert(self.agent_id, self.agent);
        state.agent_intent_ledger.extend(self.ledger_updates);
    }
}

impl World {
    pub(super) fn prepare_raw_agent_intent_event(
        &self,
        event: &DomainEvent,
    ) -> Result<PreparedAgentIntent, WorldError> {
        let envelope_event_seq: WorldEventId = self.next_event_id.max(1);
        let committed_receipt_event_id =
            self.validate_agent_intent_receipt_reference(event, Some(envelope_event_seq))?;
        self.state.prepare_agent_intent_event(
            event,
            self.state.time,
            Some(envelope_event_seq),
            committed_receipt_event_id,
        )
    }
}
