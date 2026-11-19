//! Canonical participant qualification and trusted finite starter publication.
use super::World;
use crate::{DomainEvent, WorldError, WorldEventBody};
use oasis7_agent_api::world_action::*;
use oasis7_world_state::world_rules::{
    invalid, ordinary_starter_offer_digest, ordinary_starter_owner_key,
    ordinary_starter_public_offers,
};

#[cfg(test)]
mod tests;

impl World {
    fn ordinary_starter_owner(&self, actor: &str) -> Result<(String, u64), WorldError> {
        if self.state.is_manufactured_agent(actor) {
            return Err(invalid("ordinary_starter_participant_required"));
        }
        let receipt = self
            .capability_revocation_state
            .participants
            .bindings
            .values()
            .find(|receipt| receipt.agent_id == actor)
            .ok_or_else(|| invalid("ordinary_starter_participant_required"))?;
        let owner = self
            .current_agent_owner(actor)
            .ok_or_else(|| invalid("ordinary_starter_current_owner_unavailable"))?;
        if owner.primary_agent_id != actor
            || owner.player_id != receipt.owner_binding
            || owner.generation != receipt.owner_generation.0
        {
            return Err(invalid("ordinary_starter_current_owner_mismatch"));
        }
        Ok((owner.player_id, owner.generation))
    }

    pub(crate) fn validate_ordinary_starter_event(
        &self,
        event: &DomainEvent,
    ) -> Result<(), WorldError> {
        let DomainEvent::WorldRuleCommitted {
            actor_id,
            command: WorldRuleCommandV1::OrdinaryStarter { command },
            world_binding,
            action_id,
            ..
        } = event
        else {
            return Ok(());
        };
        if world_binding.is_none() || world_binding != &self.world_rule_binding() {
            return Err(invalid("ordinary_starter_world_binding_mismatch"));
        }
        match command.as_ref() {
            OrdinaryStarterCommandV1::Publish { offer } => {
                if *action_id != 0
                    || actor_id != &offer.supplier_agent_id
                    || self.current_agent_owner(actor_id).is_some()
                    || self.current_agent_identity(actor_id).is_some()
                    || self
                        .capability_revocation_state
                        .participants
                        .bindings
                        .values()
                        .any(|receipt| receipt.agent_id == *actor_id)
                {
                    return Err(invalid("ordinary_starter_publication_authority_required"));
                }
            }
            OrdinaryStarterCommandV1::Claim {
                owner_binding,
                owner_generation,
                ..
            } => {
                let (binding, generation) = self.ordinary_starter_owner(actor_id)?;
                if *action_id == 0 || owner_binding != &binding || *owner_generation != generation {
                    return Err(invalid("ordinary_starter_current_owner_mismatch"));
                }
            }
        }
        Ok(())
    }

    /// Explicit world initialization only. Ordinary Task requests cannot call this publisher.
    pub fn publish_ordinary_starter_offer(
        &mut self,
        offer: OrdinaryStarterOfferV1,
    ) -> Result<(), WorldError> {
        let digest = ordinary_starter_offer_digest(&offer)?;
        let event = DomainEvent::WorldRuleCommitted {
            action_id: 0,
            actor_id: offer.supplier_agent_id.clone(),
            request_id: format!("ordinary-starter-publish:{digest}"),
            command: WorldRuleCommandV1::OrdinaryStarter {
                command: Box::new(OrdinaryStarterCommandV1::Publish { offer }),
            },
            governance_epoch: self.current_governance_epoch(),
            world_binding: self.world_rule_binding(),
        };
        self.validate_ordinary_starter_event(&event)?;
        self.prepare_world_rule_event(&event, self.state.time)?;
        self.append_event(WorldEventBody::Domain(event), None)?;
        Ok(())
    }

    /// Public source and availability; personal receipts require a current participant owner.
    pub fn ordinary_starter_status(&self, actor: Option<&str>) -> OrdinaryStarterStatusV1 {
        let binding = self.world_rule_binding();
        let mut status = OrdinaryStarterStatusV1 {
            offers: ordinary_starter_public_offers(
                &self.state,
                binding.as_deref(),
                self.state.time,
            ),
            world_binding: binding.clone(),
            ..Default::default()
        };
        let Some(actor) = actor else {
            status.blocker = Some("ordinary_starter_participant_required".into());
            return status;
        };
        let (owner, generation) = match self.ordinary_starter_owner(actor) {
            Ok(owner) => owner,
            Err(error) => {
                status.blocker = Some(format!("{error:?}"));
                return status;
            }
        };
        status.owner_binding = Some(owner.clone());
        status.owner_generation = Some(generation);
        let Some(binding) = binding else {
            status.blocker = Some("ordinary_starter_world_binding_unavailable".into());
            return status;
        };
        if let (Ok(key), Some(rules)) = (
            ordinary_starter_owner_key(&binding, &owner),
            &self.state.world_rules,
        ) {
            status.claim = rules
                .ordinary_starter
                .claims
                .get(&key)
                .filter(|claim| claim.agent_id == actor)
                .cloned();
        }
        if status.claim.is_some() {
            status.blocker = Some("ordinary_starter_owner_already_claimed".into());
        } else if self
            .state
            .gameplay_policy
            .blocked_agents
            .iter()
            .any(|id| id == actor)
        {
            status.blocker = Some("ordinary_starter_actor_blocked".into());
        } else if status
            .offers
            .values()
            .all(|offer| offer.available_slots.is_empty())
        {
            status.blocker = Some("ordinary_starter_supply_unavailable".into());
        } else {
            status.eligible = true;
        }
        status
    }
}
