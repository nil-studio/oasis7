use super::*;

const GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY: &str = "proposal.viewer-governance-quote";

impl ViewerRuntimeLiveServer {
    /// Seeds the opt-in local S6 world used to verify governance vote quotes.
    pub fn seed_governance_vote_quote_debug_scenario(&mut self) -> Result<(), String> {
        self.seed_governance_vote_quote_debug_scenario_inner()
    }

    /// Seeds one unvoted proposal through the normal runtime action path.
    /// The viewer-live CLI keeps this unreachable without `--allow-debug-scenario`.
    pub(super) fn seed_governance_vote_quote_debug_scenario_inner(&mut self) -> Result<(), String> {
        let proposer_agent_id = self
            .world
            .state()
            .agents
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "debug scenario requires a seeded runtime agent".to_string())?;
        self.world
            .submit_action(RuntimeAction::OpenGovernanceProposal {
                proposer_agent_id: proposer_agent_id.clone(),
                proposal_key: GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY.to_string(),
                title: "Keep the solar reserve".to_string(),
                description: "Prioritize the solar reserve over an emergency drawdown.".to_string(),
                options: vec!["approve".to_string(), "reject".to_string()],
                voting_window_ticks: 12,
                quorum_weight: 3,
                pass_threshold_bps: 6_000,
            });
        self.world
            .step()
            .map_err(|err| format!("seed governance vote quote proposal failed: {err:?}"))?;
        let proposal = self
            .world
            .state()
            .governance_proposals
            .get(GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY)
            .ok_or_else(|| "debug scenario did not open governance proposal".to_string())?;
        if proposal.options != ["approve", "reject"]
            || proposal.quorum_weight != 3
            || proposal.pass_threshold_bps != 6_000
        {
            return Err("debug scenario governance proposal is not deterministic".to_string());
        }
        if self
            .world
            .state()
            .governance_votes
            .get(GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY)
            .is_none_or(|votes| !votes.votes_by_agent.is_empty())
        {
            return Err("debug scenario must not cast a governance vote".to_string());
        }
        self.governance_vote_quote_debug_agent_id = Some(proposer_agent_id);
        Ok(())
    }

    pub(super) fn governance_vote_quote_debug_agent_for_local_test_player(
        &self,
        player_id: &str,
    ) -> Option<&str> {
        player_id
            .trim()
            .starts_with("local-test-player-")
            .then_some(self.governance_vote_quote_debug_agent_id.as_deref())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_vote_quote_debug_seed_opens_one_deterministic_unvoted_proposal() {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Script),
        )
        .expect("runtime server");

        server
            .seed_governance_vote_quote_debug_scenario()
            .expect("seed governance quote debug scenario");

        let state = server.world.state();
        assert_eq!(state.governance_proposals.len(), 1);
        let proposal = state
            .governance_proposals
            .get(GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY)
            .expect("deterministic governance proposal");
        assert_eq!(proposal.options, vec!["approve", "reject"]);
        assert_eq!(proposal.quorum_weight, 3);
        assert_eq!(proposal.pass_threshold_bps, 6_000);
        assert_eq!(proposal.closes_at.saturating_sub(state.time), 12);
        assert!(
            state
                .governance_votes
                .get(GOVERNANCE_VOTE_QUOTE_DEBUG_PROPOSAL_KEY)
                .expect("proposal vote state")
                .votes_by_agent
                .is_empty()
        );
    }
}
