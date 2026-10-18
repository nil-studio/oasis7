//! Sparse preparation for lightweight agent-claim lifecycle events.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{AgentCell, DomainEvent, MaterialLedgerId, WorldError, WorldState, WorldTime};
use super::economy_data_publication::SparseMapProjection;
use crate::gameplay_state::AgentClaimState;

#[derive(Debug)]
pub(crate) struct PreparedAgentClaimLightLifecycle {
    event: DomainEvent,
    claimer_agent_id: String,
    claim: AgentClaimState,
    activity: Option<AgentCell>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedAgentClaimLightLifecycle {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let (claimer_agent_id, target_agent_id) = match event {
            DomainEvent::AgentClaimReleaseRequested {
                claimer_agent_id,
                target_agent_id,
                ..
            }
            | DomainEvent::AgentClaimEnteredGrace {
                claimer_agent_id,
                target_agent_id,
                ..
            }
            | DomainEvent::AgentClaimIdleWarning {
                claimer_agent_id,
                target_agent_id,
                ..
            } => (claimer_agent_id, target_agent_id),
            _ => {
                return Err(invalid(
                    "light claim preparation requires a supported event",
                ));
            }
        };
        let mut activity = state.agents.get(claimer_agent_id).cloned().ok_or_else(|| {
            WorldError::AgentNotFound {
                agent_id: claimer_agent_id.clone(),
            }
        })?;
        let mut claim = state
            .agent_claims
            .get(target_agent_id)
            .cloned()
            .ok_or_else(|| invalid(format!("agent claim not found: target={target_agent_id}")))?;

        match event {
            DomainEvent::AgentClaimReleaseRequested {
                requested_at_epoch,
                ready_at_epoch,
                ..
            } => {
                if claim.claim_owner_id != *claimer_agent_id {
                    return Err(invalid(format!(
                        "agent claim release owner mismatch: target={} owner={} claimer={}",
                        target_agent_id, claim.claim_owner_id, claimer_agent_id
                    )));
                }
                if claim.release_requested_at_epoch.is_some() {
                    return Err(invalid(format!(
                        "agent claim release already requested: target={target_agent_id}"
                    )));
                }
                if *ready_at_epoch < *requested_at_epoch {
                    return Err(invalid(format!(
                        "agent claim release epoch mismatch: target={} requested={} ready={}",
                        target_agent_id, requested_at_epoch, ready_at_epoch
                    )));
                }
                claim.release_requested_at_epoch = Some(*requested_at_epoch);
                claim.release_ready_at_epoch = Some(*ready_at_epoch);
                activity.last_active = now;
            }
            DomainEvent::AgentClaimEnteredGrace {
                delinquent_since_epoch,
                grace_deadline_epoch,
                upkeep_arrears_amount,
                ..
            } => {
                if claim.claim_owner_id != *claimer_agent_id {
                    return Err(invalid(format!(
                        "agent claim grace owner mismatch: target={} owner={} claimer={}",
                        target_agent_id, claim.claim_owner_id, claimer_agent_id
                    )));
                }
                if *upkeep_arrears_amount == 0 {
                    return Err(invalid(format!(
                        "agent claim grace arrears must be positive: target={target_agent_id}"
                    )));
                }
                if *grace_deadline_epoch < *delinquent_since_epoch {
                    return Err(invalid(format!(
                        "agent claim grace epoch mismatch: target={} delinquent={} deadline={}",
                        target_agent_id, delinquent_since_epoch, grace_deadline_epoch
                    )));
                }
                claim.delinquent_since_epoch = Some(*delinquent_since_epoch);
                claim.grace_deadline_epoch = Some(*grace_deadline_epoch);
            }
            DomainEvent::AgentClaimIdleWarning {
                warning_emitted_at_epoch,
                forced_reclaim_at_epoch,
                ..
            } => {
                if claim.claim_owner_id != *claimer_agent_id {
                    return Err(invalid(format!(
                        "agent claim idle warning owner mismatch: target={} owner={} claimer={}",
                        target_agent_id, claim.claim_owner_id, claimer_agent_id
                    )));
                }
                if *forced_reclaim_at_epoch < *warning_emitted_at_epoch {
                    return Err(invalid(format!(
                        "agent claim idle warning epoch mismatch: target={} warning={} forced={}",
                        target_agent_id, warning_emitted_at_epoch, forced_reclaim_at_epoch
                    )));
                }
                claim.idle_warning_emitted_at_epoch = Some(*warning_emitted_at_epoch);
            }
            _ => unreachable!("event kind checked above"),
        }

        Ok(Self {
            event: event.clone(),
            claimer_agent_id: claimer_agent_id.clone(),
            claim,
            activity: matches!(event, DomainEvent::AgentClaimReleaseRequested { .. })
                .then_some(activity),
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        if let Some(activity) = self.activity {
            state.agents.insert(self.claimer_agent_id, activity);
        }
        state
            .agent_claims
            .insert(self.claim.target_agent_id.clone(), self.claim);
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut agent = self
            .activity
            .clone()
            .unwrap_or_else(|| state.agents[&self.claimer_agent_id].clone());
        agent.mailbox.push_back(self.event.clone());
        let updates = BTreeMap::from([(self.claimer_agent_id.clone(), agent)]);
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_claims<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = BTreeMap::from([(self.claim.target_agent_id.clone(), self.claim.clone())]);
        out.serialize_field(
            "agent_claims",
            &SparseMapProjection {
                base: &state.agent_claims,
                updates: &updates,
                deletion: None,
            },
        )
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
