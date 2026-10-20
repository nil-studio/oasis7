//! Atomic projection for one AgentIntent lifecycle event.

use std::collections::BTreeMap;

use super::super::agent_cell::AgentCell;
use super::super::{AgentIntentV2, DomainEvent};

#[derive(Debug)]
pub struct PreparedAgentIntent {
    pub event: DomainEvent,
    pub agent_id: String,
    pub agent: AgentCell,
    pub ledger_updates: BTreeMap<String, AgentIntentV2>,
}

impl PreparedAgentIntent {
    pub fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agent = self.agent.clone();
        agent.mailbox.push_back(self.event.clone());
        BTreeMap::from([(self.agent_id.clone(), agent)])
    }

    pub fn install_infallible(self, state: &mut super::super::WorldState) {
        state.agents.insert(self.agent_id, self.agent);
        state.agent_intent_ledger.extend(self.ledger_updates);
    }
}
