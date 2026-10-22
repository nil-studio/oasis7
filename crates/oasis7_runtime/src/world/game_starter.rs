//! Execute the existing starter economy exactly once, independently of registration.
use super::World;
use crate::service_admission::{SignedReadRequest, starter};
use crate::{Action, ActionEnvelope, ActionId, CausedBy, DomainEvent, WorldEventBody};
use oasis7_agent_api::game::GameStarterClaimV1;
impl World {
    pub fn game_starter_eligibility(
        &self,
        agent: &str,
        owner_public_key: &str,
    ) -> Result<serde_json::Value, String> {
        if crate::service_admission::agent_authority::owner_public_key(self, agent)?
            != owner_public_key
        {
            return Err("starter eligibility requires current owner".into());
        }
        if let Some(claim) = self.state.starter_oc_claims.get(agent) {
            return Ok(serde_json::json!({"status":"claimed","claim":claim}));
        }
        let owner = self
            .current_agent_owner(agent)
            .ok_or("starter owner unavailable")?;
        let envelope = ActionEnvelope {
            id: self.next_action_id,
            governance_authorization: None,
            committed_recipe_origin: None,
            action: Action::ClaimStarterOc {
                agent_id: agent.into(),
                player_id: owner.player_id,
                public_key: Some(owner_public_key.into()),
            },
        };
        let event = self
            .action_to_event(&envelope)
            .map_err(|e| format!("{e:?}"))?;
        match &event {
            WorldEventBody::Domain(DomainEvent::StarterOcClaimed { .. }) => {
                match self.preflight_domain_event(&event) {
                    Ok(_) => Ok(serde_json::json!({"status":"eligible"})),
                    Err(error) => {
                        Ok(serde_json::json!({"status":"blocked","blocker":format!("{error:?}")}))
                    }
                }
            }
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                Ok(serde_json::json!({"status":"blocked","blocker":format!("{reason:?}")}))
            }
            _ => Err("starter eligibility produced unexpected event".into()),
        }
    }
    pub fn apply_game_starter_claim(
        &mut self,
        signed: &SignedReadRequest<GameStarterClaimV1>,
        current_branch: &str,
        current_reorg: u64,
    ) -> Result<(serde_json::Value, Option<ActionId>), String> {
        starter::validate_owner(self, signed, current_branch, current_reorg)?;
        let r = &signed.request;
        if let Some(claim) = self.state.starter_oc_claims.get(&r.agent_id) {
            return Ok((serde_json::json!({"claim":claim}), None));
        }
        let mut candidate = self.clone();
        let id = candidate.allocate_next_action_id();
        let envelope = ActionEnvelope {
            id,
            governance_authorization: None,
            committed_recipe_origin: None,
            action: Action::ClaimStarterOc {
                agent_id: r.agent_id.clone(),
                player_id: r.owner_binding.clone(),
                public_key: Some(signed.subject_public_key.clone()),
            },
        };
        let event = candidate
            .action_to_event(&envelope)
            .map_err(|e| format!("{e:?}"))?;
        if !matches!(&event,WorldEventBody::Domain(DomainEvent::StarterOcClaimed {agent_id,..}) if agent_id==&r.agent_id)
        {
            return Err(format!("starter action rejected: {event:?}"));
        }
        candidate
            .preflight_domain_event(&event)
            .map_err(|e| format!("{e:?}"))?;
        candidate
            .append_event(event, Some(CausedBy::Action(id)))
            .map_err(|e| format!("{e:?}"))?;
        let claim = candidate
            .state
            .starter_oc_claims
            .get(&r.agent_id)
            .ok_or("starter action did not create actual claim")?;
        let receipt = serde_json::json!({"claim":claim});
        *self = candidate;
        Ok((receipt, Some(id)))
    }
}
