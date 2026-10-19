//! Canonical owner resolution for primary and claim-backed industrial subjects.
use super::World;
use crate::{WorldState, capability_authorization::CapabilityAgentIdentity};
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
impl World {
    pub(crate) fn world_rule_binding(&self) -> Option<String> {
        let m = &self.chain_resource_manifest;
        let genesis = m.genesis_ref.as_deref().filter(|s| !s.is_empty())?;
        if m.world_id.is_empty() || m.chain_id.is_empty() {
            return None;
        }
        crate::util::hash_json(&(
            "oasis7-world-rule-world-v1",
            &m.world_id,
            &m.chain_id,
            genesis,
        ))
        .ok()
    }
    /// Native grants bind the current credential generation as well as the
    /// immutable economic claim source. Reauthentication cannot revive a grant.
    pub fn current_agent_owner(&self, agent: &str) -> Option<ResolvedAgentOwnerV1> {
        let mut owner = self.state.resolved_agent_owner(agent)?;
        if !self.state.is_manufactured_agent(agent) {
            return Some(owner);
        }
        let identity = self.current_agent_identity(agent)?;
        if owner.player_id != identity.owner_binding {
            return None;
        }
        if self.state.is_manufactured_agent(agent) {
            owner.source_id = format!("{}:identity:{}", owner.source_id, identity.generation);
            owner.generation = identity.generation;
        }
        Some(owner)
    }
    pub fn current_agent_identity(&self, agent: &str) -> Option<CapabilityAgentIdentity> {
        if !self.state.is_manufactured_agent(agent) {
            return self
                .capability_revocation_state
                .agent_identities
                .get(agent)
                .cloned();
        }
        let order = self
            .state
            .world_rules
            .as_ref()?
            .agent_orders
            .values()
            .find(|order| order.target_agent_id == agent && order.delivered_at.is_some())?;
        if order.quote.world_binding != self.world_rule_binding() {
            return None;
        }
        let owner = self.state.resolved_agent_owner(agent)?;
        let root = self
            .capability_revocation_state
            .agent_identities
            .get(&owner.primary_agent_id)?;
        if root.owner_binding != owner.player_id {
            return None;
        }
        let generation = manufactured_identity_generation(
            &self.capability_revocation_state,
            agent,
            owner.generation,
        )?;
        Some(CapabilityAgentIdentity {
            owner_binding: owner.player_id,
            generation,
        })
    }
}

/// A checkpoint advances on root credential changes; acquisitions advance the
/// persistent per-target sequence without requiring another identity event.
pub(crate) fn manufactured_identity_generation(
    revocation: &crate::capability_authorization::CapabilityRevocationState,
    target: &str,
    acquisition_sequence: u64,
) -> Option<u64> {
    if acquisition_sequence == 0 {
        return None;
    }
    match revocation
        .manufactured_identity_acquisition_sequences
        .get(target)
    {
        None => Some(acquisition_sequence),
        Some(checkpoint_sequence) => {
            let checkpoint = revocation.agent_identities.get(target)?;
            let delta = acquisition_sequence.checked_sub(*checkpoint_sequence)?;
            checkpoint.generation.checked_add(delta).filter(|g| *g > 0)
        }
    }
}

impl World {
    pub(super) fn validate_manufactured_identity_acquisition(
        &self,
        event: &crate::DomainEvent,
    ) -> Result<(), crate::WorldError> {
        let target = match event {
            crate::DomainEvent::AgentClaimed {
                acquisition_contract_version: 1,
                target_agent_id,
                ..
            } if self.state.is_manufactured_agent(target_agent_id) => {
                Some(target_agent_id.as_str())
            }
            crate::DomainEvent::WorldRuleCommitted {
                command:
                    oasis7_agent_api::world_action::WorldRuleCommandV1::DeliverAgentOrder { order_id },
                ..
            } => self
                .state
                .world_rules
                .as_ref()
                .and_then(|r| r.agent_orders.get(order_id))
                .filter(|o| o.delivered_at.is_none())
                .map(|o| o.target_agent_id.as_str()),
            _ => None,
        };
        if let Some(target) = target {
            let sequence = self
                .state
                .agent_claim_acquisition_sequences
                .get(target)
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| {
                    crate::world_rules::invalid("manufactured_identity_generation_overflow")
                })?;
            manufactured_identity_generation(&self.capability_revocation_state, target, sequence)
                .ok_or_else(|| {
                crate::world_rules::invalid("manufactured_identity_generation_overflow")
            })?;
        }
        Ok(())
    }
}
