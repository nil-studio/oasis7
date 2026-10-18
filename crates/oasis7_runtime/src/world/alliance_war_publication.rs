//! Sparse atomic projection for alliance and war events.

use oasis7_agent_api::world_action::ResourceKind;
use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{
    AgentCell, AllianceState, DomainEvent, MaterialLedgerId, WarState, WorldError, WorldState,
    WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

const MIN_MEMBERS: usize = 2;

#[derive(Debug)]
pub(crate) struct PreparedAllianceWarEvent {
    event: DomainEvent,
    agents: BTreeMap<String, AgentCell>,
    alliances: BTreeMap<String, AllianceState>,
    alliance_deletion: Option<String>,
    wars: BTreeMap<String, WarState>,
    reputation: BTreeMap<String, i64>,
    materials: BTreeMap<String, i64>,
}

impl PreparedAllianceWarEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut agents = BTreeMap::new();
        let mut alliances = BTreeMap::new();
        let mut alliance_deletion = None;
        let mut wars = BTreeMap::new();
        let mut reputation = BTreeMap::new();
        match event {
            DomainEvent::AllianceFormed {
                proposer_agent_id,
                alliance_id,
                members,
                charter,
            } => {
                for member in members {
                    require_agent(state, member)?;
                }
                alliances.insert(
                    alliance_id.clone(),
                    AllianceState {
                        alliance_id: alliance_id.clone(),
                        members: members.clone(),
                        charter: charter.clone(),
                        formed_by_agent_id: proposer_agent_id.clone(),
                        formed_at: now,
                    },
                );
                touch(state, &mut agents, proposer_agent_id, now, true)?;
                for member in members {
                    touch(state, &mut agents, member, now, false)?;
                }
            }
            DomainEvent::AllianceJoined {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            } => {
                require_agent(state, operator_agent_id)?;
                require_agent(state, member_agent_id)?;
                if state.alliances.iter().any(|(id, alliance)| {
                    id != alliance_id
                        && alliance
                            .members
                            .iter()
                            .any(|member| member == member_agent_id)
                }) {
                    return Err(invalid(format!(
                        "member {member_agent_id} already belongs to another alliance"
                    )));
                }
                let mut alliance = alliance(state, alliance_id)?;
                if !alliance
                    .members
                    .iter()
                    .any(|member| member == operator_agent_id)
                {
                    return Err(invalid(format!(
                        "operator {operator_agent_id} is not a member of alliance {alliance_id}"
                    )));
                }
                if alliance
                    .members
                    .iter()
                    .any(|member| member == member_agent_id)
                {
                    return Err(invalid(format!(
                        "member {member_agent_id} already exists in alliance {alliance_id}"
                    )));
                }
                alliance.members.push(member_agent_id.clone());
                alliance.members.sort();
                alliance.members.dedup();
                alliances.insert(alliance_id.clone(), alliance);
                touch(state, &mut agents, operator_agent_id, now, false)?;
                touch(state, &mut agents, member_agent_id, now, false)?;
            }
            DomainEvent::AllianceLeft {
                operator_agent_id,
                alliance_id,
                member_agent_id,
            } => {
                require_agent(state, operator_agent_id)?;
                require_agent(state, member_agent_id)?;
                let mut alliance = alliance(state, alliance_id)?;
                if !alliance
                    .members
                    .iter()
                    .any(|member| member == operator_agent_id)
                {
                    return Err(invalid(format!(
                        "operator {operator_agent_id} is not a member of alliance {alliance_id}"
                    )));
                }
                let before = alliance.members.len();
                alliance.members.retain(|member| member != member_agent_id);
                if alliance.members.len() == before {
                    return Err(invalid(format!(
                        "member {member_agent_id} not found in alliance {alliance_id}"
                    )));
                }
                if alliance.members.len() < MIN_MEMBERS {
                    return Err(invalid(format!(
                        "alliance {alliance_id} member count below minimum {MIN_MEMBERS}"
                    )));
                }
                alliances.insert(alliance_id.clone(), alliance);
                touch(state, &mut agents, operator_agent_id, now, false)?;
                touch(state, &mut agents, member_agent_id, now, false)?;
            }
            DomainEvent::AllianceDissolved {
                operator_agent_id,
                alliance_id,
                former_members,
                ..
            } => {
                if state.wars.values().any(|war| {
                    war.active
                        && (war.aggressor_alliance_id == *alliance_id
                            || war.defender_alliance_id == *alliance_id)
                }) {
                    return Err(invalid(format!(
                        "cannot dissolve alliance {alliance_id} while active war exists"
                    )));
                }
                let alliance = alliance(state, alliance_id)?;
                if !alliance
                    .members
                    .iter()
                    .any(|member| member == operator_agent_id)
                {
                    return Err(invalid(format!(
                        "operator {operator_agent_id} is not a member of alliance {alliance_id}"
                    )));
                }
                touch(state, &mut agents, operator_agent_id, now, true)?;
                let members = if former_members.is_empty() {
                    &alliance.members
                } else {
                    former_members
                };
                for member in members {
                    touch(state, &mut agents, member, now, false)?;
                }
                alliance_deletion = Some(alliance_id.clone());
            }
            DomainEvent::WarDeclared {
                initiator_agent_id,
                war_id,
                aggressor_alliance_id,
                defender_alliance_id,
                objective,
                intensity,
                mobilization_electricity_cost,
                mobilization_data_cost,
            } => {
                if !state.alliances.contains_key(aggressor_alliance_id) {
                    return Err(invalid(format!(
                        "war declare aggressor alliance missing: {aggressor_alliance_id}"
                    )));
                }
                if !state.alliances.contains_key(defender_alliance_id) {
                    return Err(invalid(format!(
                        "war declare defender alliance missing: {defender_alliance_id}"
                    )));
                }
                let mut initiator = require_agent(state, initiator_agent_id)?.clone();
                initiator.state.resources.remove(ResourceKind::Electricity, *mobilization_electricity_cost).map_err(|err| invalid(format!("war mobilization electricity debit failed for {initiator_agent_id}: {err:?}")))?;
                initiator
                    .state
                    .resources
                    .remove(ResourceKind::Data, *mobilization_data_cost)
                    .map_err(|err| {
                        invalid(format!(
                            "war mobilization data debit failed for {initiator_agent_id}: {err:?}"
                        ))
                    })?;
                initiator.last_active = now;
                agents.insert(initiator_agent_id.clone(), initiator);
                wars.insert(
                    war_id.clone(),
                    WarState {
                        war_id: war_id.clone(),
                        initiator_agent_id: initiator_agent_id.clone(),
                        aggressor_alliance_id: aggressor_alliance_id.clone(),
                        defender_alliance_id: defender_alliance_id.clone(),
                        objective: objective.clone(),
                        intensity: *intensity,
                        active: true,
                        declared_mobilization_electricity_cost: *mobilization_electricity_cost,
                        declared_mobilization_data_cost: *mobilization_data_cost,
                        max_duration_ticks: 6_u64.saturating_add(u64::from(*intensity) * 2),
                        aggressor_score: 0,
                        defender_score: 0,
                        concluded_at: None,
                        winner_alliance_id: None,
                        loser_alliance_id: None,
                        settlement_summary: None,
                        participant_outcomes: Vec::new(),
                        declared_at: now,
                    },
                );
            }
            DomainEvent::WarConcluded {
                war_id,
                winner_alliance_id,
                loser_alliance_id,
                aggressor_score,
                defender_score,
                summary,
                participant_outcomes,
            } => {
                let mut war =
                    state.wars.get(war_id).cloned().ok_or_else(|| {
                        invalid(format!("war not found for conclusion: {war_id}"))
                    })?;
                war.active = false;
                war.aggressor_score = *aggressor_score;
                war.defender_score = *defender_score;
                war.concluded_at = Some(now);
                war.winner_alliance_id = Some(winner_alliance_id.clone());
                war.loser_alliance_id = Some(if loser_alliance_id.is_empty() {
                    if war.aggressor_alliance_id == *winner_alliance_id {
                        war.defender_alliance_id.clone()
                    } else {
                        war.aggressor_alliance_id.clone()
                    }
                } else {
                    loser_alliance_id.clone()
                });
                war.settlement_summary = Some(summary.clone());
                war.participant_outcomes = participant_outcomes.clone();
                for outcome in participant_outcomes {
                    let mut cell = agents
                        .get(&outcome.agent_id)
                        .cloned()
                        .or_else(|| state.agents.get(&outcome.agent_id).cloned())
                        .ok_or_else(|| WorldError::AgentNotFound {
                            agent_id: outcome.agent_id.clone(),
                        })?;
                    apply_delta(
                        &mut cell,
                        ResourceKind::Electricity,
                        outcome.electricity_delta,
                        &outcome.agent_id,
                        "war electricity outcome",
                    )?;
                    apply_delta(
                        &mut cell,
                        ResourceKind::Data,
                        outcome.data_delta,
                        &outcome.agent_id,
                        "war data outcome",
                    )?;
                    cell.last_active = now;
                    agents.insert(outcome.agent_id.clone(), cell);
                    if outcome.reputation_delta != 0 {
                        let current = reputation
                            .get(&outcome.agent_id)
                            .copied()
                            .or_else(|| state.reputation_scores.get(&outcome.agent_id).copied())
                            .unwrap_or(0);
                        reputation.insert(
                            outcome.agent_id.clone(),
                            current.saturating_add(outcome.reputation_delta),
                        );
                    }
                }
                wars.insert(war_id.clone(), war);
            }
            _ => {
                return Err(invalid(
                    "alliance/war preparation requires a supported event",
                ));
            }
        }
        Ok(Self {
            event: event.clone(),
            agents,
            alliances,
            alliance_deletion,
            wars,
            reputation,
            materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|value| !value.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }
    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        state.alliances.extend(self.alliances);
        if let Some(id) = self.alliance_deletion {
            state.alliances.remove(&id);
        }
        state.wars.extend(self.wars);
        state.reputation_scores.extend(self.reputation);
        state.materials = self.materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.materials);
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut updates = self.agents.clone();
        if let Some(id) = self.event.agent_id()
            && let Some(cell) = updates.get_mut(id)
        {
            cell.mailbox.push_back(self.event.clone());
        }
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
        out.serialize_field("materials", &self.materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_alliances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "alliances",
            &SparseMapProjection {
                base: &state.alliances,
                updates: &self.alliances,
                deletion: self.alliance_deletion.as_ref(),
            },
        )
    }
    pub(crate) fn serialize_reputation<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "reputation_scores",
            &SparseMapProjection {
                base: &state.reputation_scores,
                updates: &self.reputation,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_wars<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "wars",
            &SparseMapProjection {
                base: &state.wars,
                updates: &self.wars,
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
fn require_agent<'a>(state: &'a WorldState, id: &str) -> Result<&'a AgentCell, WorldError> {
    state
        .agents
        .get(id)
        .ok_or_else(|| WorldError::AgentNotFound {
            agent_id: id.into(),
        })
}
fn alliance(state: &WorldState, id: &str) -> Result<AllianceState, WorldError> {
    state
        .alliances
        .get(id)
        .cloned()
        .ok_or_else(|| invalid(format!("alliance not found: {id}")))
}
fn touch(
    state: &WorldState,
    updates: &mut BTreeMap<String, AgentCell>,
    id: &str,
    now: WorldTime,
    required: bool,
) -> Result<(), WorldError> {
    if let Some(cell) = updates.get_mut(id) {
        cell.last_active = now;
        return Ok(());
    }
    match state.agents.get(id).cloned() {
        Some(mut cell) => {
            cell.last_active = now;
            updates.insert(id.into(), cell);
            Ok(())
        }
        None if !required => Ok(()),
        None => Err(WorldError::AgentNotFound {
            agent_id: id.into(),
        }),
    }
}
fn apply_delta(
    cell: &mut AgentCell,
    kind: ResourceKind,
    delta: i64,
    id: &str,
    context: &str,
) -> Result<(), WorldError> {
    if delta == 0 {
        return Ok(());
    }
    if delta > 0 {
        cell.state
            .resources
            .add(kind, delta)
            .map_err(|err| invalid(format!("{context} apply failed for {id}: {err:?}")))
    } else {
        cell.state
            .resources
            .remove(kind, delta.saturating_abs())
            .map_err(|err| invalid(format!("{context} apply failed for {id}: {err:?}")))
    }
}
