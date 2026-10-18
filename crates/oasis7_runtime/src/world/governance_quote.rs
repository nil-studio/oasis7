use std::collections::BTreeMap;

use super::super::GovernanceProposalStatus;
use super::World;

const GOVERNANCE_VOTE_QUOTE_MISSING: &str = "governance_vote_quote_missing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceVoteQuote {
    pub proposal_id: String,
    pub proposal_topic: String,
    pub actor_id: String,
    pub action_kind: String,
    pub closes_at_tick: u64,
    pub ticks_remaining: u64,
    pub current_quorum_weight: u64,
    pub required_quorum_weight: u64,
    pub current_pass_bps: u16,
    pub required_pass_bps: u16,
    pub actor_vote_weight: u32,
    pub vote_swing_potential: u32,
    pub likely_outcome_before_action: String,
    pub likely_outcome_after_action: String,
    pub affected_rule_or_priority: String,
    pub world_change_if_passed: String,
    pub cost_or_cooldown_if_failed: String,
    pub recommended_governance_action: String,
    pub why_this_vote_matters: String,
}

impl World {
    pub fn governance_vote_quote(
        &self,
        proposal_key: &str,
        actor_id: &str,
        option: &str,
        weight: u32,
    ) -> Result<GovernanceVoteQuote, &'static str> {
        let Some(proposal) = self.state.governance_proposals.get(proposal_key) else {
            return Err(GOVERNANCE_VOTE_QUOTE_MISSING);
        };
        if proposal.status != GovernanceProposalStatus::Open
            || proposal.closes_at <= self.state.time
            || actor_id.trim().is_empty()
            || option.trim().is_empty()
            || weight == 0
            || !proposal.options.iter().any(|candidate| candidate == option)
        {
            return Err(GOVERNANCE_VOTE_QUOTE_MISSING);
        }
        if !self.state.agents.contains_key(actor_id)
            || self
                .state
                .governance_effective_vote_weight_for_agent(proposal, actor_id, weight)
                .is_err()
        {
            return Err(GOVERNANCE_VOTE_QUOTE_MISSING);
        }

        let vote_state = self.state.governance_votes.get(proposal_key);
        let current_tallies = vote_state
            .map(|state| state.tallies.clone())
            .unwrap_or_default();
        let current_total = vote_state.map(|state| state.total_weight).unwrap_or(0);
        let previous_ballot = vote_state.and_then(|state| state.votes_by_agent.get(actor_id));
        let (projected_tallies, projected_total) = project_vote(
            current_tallies.clone(),
            current_total,
            previous_ballot.map(|ballot| (ballot.option.as_str(), ballot.weight)),
            option,
            weight,
        );
        let (current_winning_option, _) = winning_option_and_weight(&current_tallies);
        let (projected_winning_option, _) = winning_option_and_weight(&projected_tallies);
        let current_option_weight = current_tallies.get(option).copied().unwrap_or(0);
        let projected_option_weight = projected_tallies.get(option).copied().unwrap_or(0);
        let current_pass_bps = pass_bps(current_option_weight, current_total);
        let projected_pass_bps = pass_bps(projected_option_weight, projected_total);
        let likely_outcome_before_action = likely_outcome(
            current_total,
            current_winning_option,
            option,
            current_option_weight,
            proposal.quorum_weight,
            proposal.pass_threshold_bps,
        );
        let likely_outcome_after_action = likely_outcome(
            projected_total,
            projected_winning_option,
            option,
            projected_option_weight,
            proposal.quorum_weight,
            proposal.pass_threshold_bps,
        );
        let vote_swing_potential = previous_ballot
            .map(|ballot| ballot.weight.abs_diff(weight))
            .unwrap_or(weight);
        let action_kind = if previous_ballot.is_some() {
            "recast_governance_vote"
        } else {
            "cast_governance_vote"
        };
        let (recommended_governance_action, why_this_vote_matters) = if likely_outcome_before_action
            != likely_outcome_after_action
        {
            (
                "cast_vote",
                format!(
                    "This vote changes the likely outcome from {likely_outcome_before_action} to {likely_outcome_after_action}."
                ),
            )
        } else if projected_total < proposal.quorum_weight {
            let shortfall = proposal.quorum_weight.saturating_sub(projected_total);
            (
                "seek_quorum",
                format!("quorum remains short by {shortfall} projected vote weight."),
            )
        } else {
            (
                "cast_vote",
                format!(
                    "Projected pass support is {projected_pass_bps} bps against the required {} bps.",
                    proposal.pass_threshold_bps
                ),
            )
        };

        Ok(GovernanceVoteQuote {
            proposal_id: proposal.proposal_key.clone(),
            proposal_topic: proposal.title.clone(),
            actor_id: actor_id.to_string(),
            action_kind: action_kind.to_string(),
            closes_at_tick: proposal.closes_at,
            ticks_remaining: proposal.closes_at.saturating_sub(self.state.time),
            current_quorum_weight: current_total,
            required_quorum_weight: proposal.quorum_weight,
            current_pass_bps,
            required_pass_bps: proposal.pass_threshold_bps,
            actor_vote_weight: weight,
            vote_swing_potential,
            likely_outcome_before_action: likely_outcome_before_action.to_string(),
            likely_outcome_after_action: likely_outcome_after_action.to_string(),
            affected_rule_or_priority: proposal.title.clone(),
            world_change_if_passed: proposal.description.clone(),
            cost_or_cooldown_if_failed:
                "No governance action cost or cooldown is defined for this proposal.".to_string(),
            recommended_governance_action: recommended_governance_action.to_string(),
            why_this_vote_matters,
        })
    }
}

fn project_vote(
    mut tallies: BTreeMap<String, u64>,
    total_weight: u64,
    previous_ballot: Option<(&str, u32)>,
    option: &str,
    weight: u32,
) -> (BTreeMap<String, u64>, u64) {
    let mut projected_total = total_weight;
    if let Some((previous_option, previous_weight)) = previous_ballot {
        if let Some(tally) = tallies.get_mut(previous_option) {
            *tally = tally.saturating_sub(u64::from(previous_weight));
            if *tally == 0 {
                tallies.remove(previous_option);
            }
        }
        projected_total = projected_total.saturating_sub(u64::from(previous_weight));
    }
    *tallies.entry(option.to_string()).or_default() += u64::from(weight);
    projected_total = projected_total.saturating_add(u64::from(weight));
    (tallies, projected_total)
}

fn winning_option_and_weight(tallies: &BTreeMap<String, u64>) -> (Option<&str>, u64) {
    tallies
        .iter()
        .max_by(|(left_option, left_weight), (right_option, right_weight)| {
            left_weight
                .cmp(right_weight)
                .then_with(|| right_option.cmp(left_option))
        })
        .map(|(option, weight)| (Some(option.as_str()), *weight))
        .unwrap_or((None, 0))
}

fn pass_bps(winning_weight: u64, total_weight: u64) -> u16 {
    if total_weight == 0 {
        return 0;
    }
    let bps = u128::from(winning_weight)
        .saturating_mul(10_000)
        .saturating_div(u128::from(total_weight));
    u16::try_from(bps).unwrap_or(u16::MAX)
}

fn likely_outcome(
    total_weight: u64,
    winning_option: Option<&str>,
    quoted_option: &str,
    quoted_option_weight: u64,
    quorum_weight: u64,
    pass_threshold_bps: u16,
) -> &'static str {
    if total_weight >= quorum_weight
        && total_weight > 0
        && winning_option == Some(quoted_option)
        && (u128::from(quoted_option_weight) * 10_000)
            >= (u128::from(total_weight) * u128::from(pass_threshold_bps))
    {
        "passed"
    } else {
        "rejected"
    }
}
