//! Canonical owner resolution for primary and claim-backed industrial subjects.

use crate::WorldState;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentOwnerV1 {
    pub primary_agent_id: String,
    pub player_id: String,
    pub public_key: Option<String>,
    pub source_id: String,
    pub generation: u64,
}
impl WorldState {
    pub fn is_manufactured_agent(&self, agent: &str) -> bool {
        self.world_rules
            .as_ref()
            .is_some_and(|r| r.agent_orders.values().any(|o| o.target_agent_id == agent))
    }
    pub fn resolved_agent_owner(&self, agent: &str) -> Option<ResolvedAgentOwnerV1> {
        if !self.agents.contains_key(agent) {
            return None;
        }
        if self.is_manufactured_agent(agent) {
            self.world_rules.as_ref()?.agent_orders.values().find(|o| {
                o.target_agent_id == agent
                    && o.delivered_at.is_some()
                    && o.phase == crate::world_rules::AgentIndustrialPhaseV1::Delivered
            })?;
            let claim = self.agent_claims.get(agent)?;
            let source = claim.control_source.as_ref()?;
            if claim.target_agent_id != agent
                || claim.claim_owner_id != source.root_agent_id
                || source.acquired_event_digest.len() != 64
                || source.acquisition_sequence == 0
                || self.agent_claim_acquisition_sequences.get(agent)
                    != Some(&source.acquisition_sequence)
            {
                return None;
            }
            let root = self.starter_oc_claims.get(&source.root_agent_id)?;
            if root.agent_id != source.root_agent_id
                || root.player_id != source.player_id
                || root.claimed_at != source.root_claimed_at
                || root.player_id.is_empty()
                || !self.agents.contains_key(&root.agent_id)
                || root.public_key.as_ref().is_none_or(|key| key.is_empty())
            {
                return None;
            }
            let source_id = format!("manufactured_claim:{}", source.acquired_event_digest);
            return Some(ResolvedAgentOwnerV1 {
                primary_agent_id: source.root_agent_id.clone(),
                player_id: source.player_id.clone(),
                public_key: root.public_key.clone(),
                generation: source.acquisition_sequence,
                source_id,
            });
        }
        let root = self.starter_oc_claims.get(agent)?;
        if root.agent_id != agent || root.player_id.is_empty() {
            return None;
        }
        Some(ResolvedAgentOwnerV1 {
            primary_agent_id: agent.into(),
            player_id: root.player_id.clone(),
            public_key: root.public_key.clone(),
            source_id: format!("starter_claim:{agent}:{}", root.claimed_at),
            generation: 1,
        })
    }
}
