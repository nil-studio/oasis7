//! Atomic sparse projections for governance, crisis, meta, and product events.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{
    AgentCell, CrisisState, CrisisStatus, DomainEvent, GovernanceProposalState,
    GovernanceProposalStatus, GovernanceVoteBallotState, GovernanceVoteState, MetaProgressState,
    WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;
use crate::state::LastProductValidationState;

#[derive(Debug)]
pub(crate) enum PreparedGovernanceMetaEvent {
    Proposal(PreparedProposal),
    Crisis(PreparedCrisis),
    Meta(PreparedMetaProgress),
    Product(PreparedProductValidation),
}

#[derive(Debug)]
pub(crate) struct PreparedProposal {
    event: DomainEvent,
    proposals: BTreeMap<String, GovernanceProposalState>,
    votes: BTreeMap<String, GovernanceVoteState>,
    agents: BTreeMap<String, AgentCell>,
}

#[derive(Debug)]
pub(crate) struct PreparedCrisis {
    event: DomainEvent,
    crises: BTreeMap<String, CrisisState>,
    agents: BTreeMap<String, AgentCell>,
}

#[derive(Debug)]
pub(crate) struct PreparedMetaProgress {
    event: DomainEvent,
    progress: BTreeMap<String, MetaProgressState>,
    agents: BTreeMap<String, AgentCell>,
}

#[derive(Debug)]
pub(crate) struct PreparedProductValidation {
    event: DomainEvent,
    validation: LastProductValidationState,
    agents: BTreeMap<String, AgentCell>,
}

impl PreparedGovernanceMetaEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::GovernanceProposalOpened { .. }
            | DomainEvent::GovernanceVoteCast { .. }
            | DomainEvent::GovernanceProposalFinalized { .. } => Ok(Self::Proposal(
                PreparedProposal::prepare(state, event, now)?,
            )),
            DomainEvent::CrisisSpawned { .. }
            | DomainEvent::CrisisResolved { .. }
            | DomainEvent::CrisisTimedOut { .. } => {
                Ok(Self::Crisis(PreparedCrisis::prepare(state, event, now)?))
            }
            DomainEvent::MetaProgressGranted { .. } => Ok(Self::Meta(
                PreparedMetaProgress::prepare(state, event, now)?,
            )),
            DomainEvent::ProductValidated { .. } => Ok(Self::Product(
                PreparedProductValidation::prepare(state, event, now),
            )),
            _ => Err(invalid(
                "governance/meta preparation requires a supported event",
            )),
        }
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        match self {
            Self::Proposal(value) => &value.event == event,
            Self::Crisis(value) => &value.event == event,
            Self::Meta(value) => &value.event == event,
            Self::Product(value) => &value.event == event,
        }
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        match self {
            Self::Proposal(value) => {
                state.governance_proposals.extend(value.proposals);
                state.governance_votes.extend(value.votes);
                state.agents.extend(value.agents);
            }
            Self::Crisis(value) => {
                state.crises.extend(value.crises);
                state.agents.extend(value.agents);
            }
            Self::Meta(value) => {
                state.meta_progress.extend(value.progress);
                state.agents.extend(value.agents);
            }
            Self::Product(value) => {
                state.latest_product_validation = Some(value.validation);
                state.agents.extend(value.agents);
            }
        }
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut agents = match self {
            Self::Proposal(value) => value.agents.clone(),
            Self::Crisis(value) => value.agents.clone(),
            Self::Meta(value) => value.agents.clone(),
            Self::Product(value) => value.agents.clone(),
        };
        let event = self.event();
        if let Some(id) = event.agent_id()
            && let Some(cell) = agents.get_mut(id)
        {
            cell.mailbox.push_back(event.clone());
        }
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &agents,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_product<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Self::Product(value) = self {
            out.serialize_field("latest_product_validation", &Some(&value.validation))
        } else if state.latest_product_validation.is_some() {
            out.serialize_field(
                "latest_product_validation",
                &state.latest_product_validation,
            )
        } else {
            Ok(())
        }
    }
    pub(crate) fn serialize_governance<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Self::Proposal(value) = self {
            out.serialize_field(
                "governance_votes",
                &SparseMapProjection {
                    base: &state.governance_votes,
                    updates: &value.votes,
                    deletion: None,
                },
            )?;
            out.serialize_field(
                "governance_proposals",
                &SparseMapProjection {
                    base: &state.governance_proposals,
                    updates: &value.proposals,
                    deletion: None,
                },
            )
        } else {
            out.serialize_field("governance_votes", &state.governance_votes)?;
            out.serialize_field("governance_proposals", &state.governance_proposals)
        }
    }
    pub(crate) fn serialize_crises<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Self::Crisis(value) = self {
            out.serialize_field(
                "crises",
                &SparseMapProjection {
                    base: &state.crises,
                    updates: &value.crises,
                    deletion: None,
                },
            )
        } else {
            out.serialize_field("crises", &state.crises)
        }
    }
    pub(crate) fn serialize_meta<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Self::Meta(value) = self {
            out.serialize_field(
                "meta_progress",
                &SparseMapProjection {
                    base: &state.meta_progress,
                    updates: &value.progress,
                    deletion: None,
                },
            )
        } else {
            out.serialize_field("meta_progress", &state.meta_progress)
        }
    }

    fn event(&self) -> &DomainEvent {
        match self {
            Self::Proposal(v) => &v.event,
            Self::Crisis(v) => &v.event,
            Self::Meta(v) => &v.event,
            Self::Product(v) => &v.event,
        }
    }
}

impl PreparedProposal {
    fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut proposals = BTreeMap::new();
        let mut votes = BTreeMap::new();
        let mut agents = BTreeMap::new();
        match event {
            DomainEvent::GovernanceProposalOpened {
                proposer_agent_id,
                proposal_key,
                title,
                description,
                options,
                voting_window_ticks,
                closes_at,
                quorum_weight,
                pass_threshold_bps,
            } => {
                let mut proposer = agent(state, proposer_agent_id)?.clone();
                let vote_weight_snapshot = state
                    .agents
                    .keys()
                    .map(|id| {
                        (
                            id.clone(),
                            state.governance_identity_snapshot_for_agent(id, now),
                        )
                    })
                    .collect();
                proposals.insert(
                    proposal_key.clone(),
                    GovernanceProposalState {
                        proposal_key: proposal_key.clone(),
                        proposer_agent_id: proposer_agent_id.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        options: options.clone(),
                        voting_window_ticks: *voting_window_ticks,
                        quorum_weight: *quorum_weight,
                        pass_threshold_bps: *pass_threshold_bps,
                        opened_at: now,
                        closes_at: *closes_at,
                        status: GovernanceProposalStatus::Open,
                        finalized_at: None,
                        winning_option: None,
                        winning_weight: 0,
                        total_weight_at_finalize: 0,
                        snapshot_at_tick: now,
                        vote_weight_snapshot,
                    },
                );
                if !state.governance_votes.contains_key(proposal_key) {
                    votes.insert(
                        proposal_key.clone(),
                        GovernanceVoteState {
                            proposal_key: proposal_key.clone(),
                            votes_by_agent: BTreeMap::new(),
                            tallies: BTreeMap::new(),
                            total_weight: 0,
                            last_updated_at: now,
                        },
                    );
                }
                proposer.last_active = now;
                agents.insert(proposer_agent_id.clone(), proposer);
            }
            DomainEvent::GovernanceVoteCast {
                voter_agent_id,
                proposal_key,
                option,
                weight,
            } => {
                let mut voter = agent(state, voter_agent_id)?.clone();
                let proposal = state
                    .governance_proposals
                    .get(proposal_key)
                    .cloned()
                    .ok_or_else(|| {
                        invalid(format!(
                            "governance vote references unknown proposal: {proposal_key}"
                        ))
                    })?;
                if proposal.status != GovernanceProposalStatus::Open {
                    return Err(invalid(format!(
                        "governance proposal is not open: {proposal_key}"
                    )));
                }
                if now > proposal.closes_at {
                    return Err(invalid(format!(
                        "governance proposal already closed at {}: {}",
                        proposal.closes_at, proposal_key
                    )));
                }
                let effective = state.governance_effective_vote_weight_for_agent(
                    &proposal,
                    voter_agent_id,
                    *weight,
                )?;
                let mut next = state.governance_votes.get(proposal_key).cloned().unwrap_or(
                    GovernanceVoteState {
                        proposal_key: proposal_key.clone(),
                        votes_by_agent: BTreeMap::new(),
                        tallies: BTreeMap::new(),
                        total_weight: 0,
                        last_updated_at: now,
                    },
                );
                if let Some(old) = next.votes_by_agent.get(voter_agent_id).cloned() {
                    let old_weight = u64::from(old.weight);
                    next.total_weight = next.total_weight.saturating_sub(old_weight);
                    if let Some(tally) = next.tallies.get_mut(&old.option) {
                        *tally = tally.saturating_sub(old_weight);
                        if *tally == 0 {
                            next.tallies.remove(&old.option);
                        }
                    }
                }
                next.votes_by_agent.insert(
                    voter_agent_id.clone(),
                    GovernanceVoteBallotState {
                        option: option.clone(),
                        weight: effective,
                        voted_at: now,
                    },
                );
                let added = u64::from(effective);
                *next.tallies.entry(option.clone()).or_insert(0) = next
                    .tallies
                    .get(option)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(added);
                next.total_weight = next.total_weight.saturating_add(added);
                next.last_updated_at = now;
                votes.insert(proposal_key.clone(), next);
                voter.last_active = now;
                agents.insert(voter_agent_id.clone(), voter);
            }
            DomainEvent::GovernanceProposalFinalized {
                proposal_key,
                winning_option,
                winning_weight,
                total_weight,
                passed,
            } => {
                let mut proposal = state
                    .governance_proposals
                    .get(proposal_key)
                    .cloned()
                    .ok_or_else(|| {
                        invalid(format!("governance proposal missing: {proposal_key}"))
                    })?;
                proposal.status = if *passed {
                    GovernanceProposalStatus::Passed
                } else {
                    GovernanceProposalStatus::Rejected
                };
                proposal.finalized_at = Some(now);
                proposal.winning_option = winning_option.clone();
                proposal.winning_weight = *winning_weight;
                proposal.total_weight_at_finalize = *total_weight;
                proposals.insert(proposal_key.clone(), proposal);
                if let Some(mut vote) = state.governance_votes.get(proposal_key).cloned() {
                    vote.last_updated_at = now;
                    votes.insert(proposal_key.clone(), vote);
                }
            }
            _ => unreachable!(),
        }
        Ok(Self {
            event: event.clone(),
            proposals,
            votes,
            agents,
        })
    }
}

impl PreparedCrisis {
    fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut crises = BTreeMap::new();
        let mut agents = BTreeMap::new();
        match event {
            DomainEvent::CrisisSpawned {
                crisis_id,
                kind,
                severity,
                expires_at,
            } => {
                crises.insert(
                    crisis_id.clone(),
                    CrisisState {
                        crisis_id: crisis_id.clone(),
                        kind: kind.clone(),
                        severity: *severity,
                        status: CrisisStatus::Active,
                        opened_at: now,
                        expires_at: *expires_at,
                        resolver_agent_id: None,
                        strategy: None,
                        success: None,
                        impact: 0,
                        resolved_at: None,
                    },
                );
            }
            DomainEvent::CrisisResolved {
                resolver_agent_id,
                crisis_id,
                strategy,
                success,
                impact,
            } => {
                let mut crisis = state.crises.get(crisis_id).cloned().unwrap_or(CrisisState {
                    crisis_id: crisis_id.clone(),
                    kind: "legacy".into(),
                    severity: 1,
                    status: CrisisStatus::Resolved,
                    opened_at: now,
                    expires_at: now,
                    resolver_agent_id: None,
                    strategy: None,
                    success: None,
                    impact: 0,
                    resolved_at: None,
                });
                crisis.status = CrisisStatus::Resolved;
                crisis.resolver_agent_id = Some(resolver_agent_id.clone());
                crisis.strategy = Some(strategy.clone());
                crisis.success = Some(*success);
                crisis.impact = *impact;
                crisis.resolved_at = Some(now);
                crises.insert(crisis_id.clone(), crisis);
                let mut resolver = agent(state, resolver_agent_id)?.clone();
                resolver.last_active = now;
                agents.insert(resolver_agent_id.clone(), resolver);
            }
            DomainEvent::CrisisTimedOut {
                crisis_id,
                penalty_impact,
            } => {
                let mut crisis =
                    state.crises.get(crisis_id).cloned().ok_or_else(|| {
                        invalid(format!("crisis not found for timeout: {crisis_id}"))
                    })?;
                crisis.status = CrisisStatus::TimedOut;
                crisis.success = Some(false);
                crisis.impact = *penalty_impact;
                crisis.resolved_at = Some(now);
                crises.insert(crisis_id.clone(), crisis);
            }
            _ => unreachable!(),
        }
        Ok(Self {
            event: event.clone(),
            crises,
            agents,
        })
    }
}

impl PreparedMetaProgress {
    fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::MetaProgressGranted {
            operator_agent_id,
            target_agent_id,
            track,
            points,
            achievement_id,
        } = event
        else {
            unreachable!()
        };
        agent(state, operator_agent_id)?;
        agent(state, target_agent_id)?;
        let mut progress =
            state
                .meta_progress
                .get(target_agent_id)
                .cloned()
                .unwrap_or(MetaProgressState {
                    agent_id: target_agent_id.clone(),
                    track_points: BTreeMap::new(),
                    total_points: 0,
                    achievements: Vec::new(),
                    unlocked_tiers: BTreeMap::new(),
                    last_granted_at: now,
                });
        let next = progress
            .track_points
            .get(track)
            .copied()
            .unwrap_or(0)
            .saturating_add(*points);
        progress.track_points.insert(track.clone(), next);
        progress.total_points = progress.total_points.saturating_add(*points);
        progress.last_granted_at = now;
        if let Some(id) = achievement_id
            && !progress.achievements.contains(id)
        {
            progress.achievements.push(id.clone());
            progress.achievements.sort();
        }
        unlock_tiers(track, next, &mut progress);
        let mut agents = BTreeMap::new();
        touch(state, &mut agents, operator_agent_id, now);
        touch(state, &mut agents, target_agent_id, now);
        Ok(Self {
            event: event.clone(),
            progress: BTreeMap::from([(target_agent_id.clone(), progress)]),
            agents,
        })
    }
}

impl PreparedProductValidation {
    fn prepare(state: &WorldState, event: &DomainEvent, now: WorldTime) -> Self {
        let DomainEvent::ProductValidated {
            requester_agent_id,
            stack,
            tradable,
            ..
        } = event
        else {
            unreachable!()
        };
        let mut agents = BTreeMap::new();
        touch(state, &mut agents, requester_agent_id, now);
        Self {
            event: event.clone(),
            validation: LastProductValidationState {
                product_id: stack.kind.clone(),
                tradable: *tradable,
            },
            agents,
        }
    }
}

fn agent<'a>(state: &'a WorldState, id: &str) -> Result<&'a AgentCell, WorldError> {
    state
        .agents
        .get(id)
        .ok_or_else(|| WorldError::AgentNotFound {
            agent_id: id.into(),
        })
}
fn touch(state: &WorldState, updates: &mut BTreeMap<String, AgentCell>, id: &str, now: WorldTime) {
    if let Some(cell) = updates.get_mut(id) {
        cell.last_active = now;
    } else if let Some(mut cell) = state.agents.get(id).cloned() {
        cell.last_active = now;
        updates.insert(id.into(), cell);
    }
}
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
fn unlock_tiers(track: &str, points: i64, progress: &mut MetaProgressState) {
    for (tier, threshold) in [("bronze", 20), ("silver", 50), ("gold", 100)] {
        if points >= threshold {
            let tiers = progress.unlocked_tiers.entry(track.into()).or_default();
            if !tiers.iter().any(|v| v == tier) {
                tiers.push(tier.into());
            }
            let id = format!("tier.{track}.{tier}");
            if !progress.achievements.contains(&id) {
                progress.achievements.push(id);
            }
        }
    }
    if let Some(tiers) = progress.unlocked_tiers.get_mut(track) {
        tiers.sort();
        tiers.dedup();
    }
    progress.achievements.sort();
    progress.achievements.dedup();
}
