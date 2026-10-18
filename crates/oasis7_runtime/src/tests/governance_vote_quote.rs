use super::super::*;
use super::pos;

fn register_agents(world: &mut World, agent_ids: &[&str]) {
    for (index, agent_id) in agent_ids.iter().enumerate() {
        world.submit_action(Action::RegisterAgent {
            agent_id: (*agent_id).to_string(),
            pos: pos(index as i64, 0),
        });
    }
    world.step().expect("register agents");
}

fn open_governance_proposal(
    world: &mut World,
    proposal_key: &str,
    window_ticks: u64,
    quorum_weight: u64,
    pass_threshold_bps: u16,
) {
    world.submit_action(Action::OpenGovernanceProposal {
        proposer_agent_id: "a".to_string(),
        proposal_key: proposal_key.to_string(),
        title: format!("title.{proposal_key}"),
        description: "runtime proposal".to_string(),
        options: vec!["approve".to_string(), "reject".to_string()],
        voting_window_ticks: window_ticks,
        quorum_weight,
        pass_threshold_bps,
    });
    world.step().expect("open governance proposal");
}

#[test]
fn governance_vote_quote_exposes_quorum_shortfall_and_player_consequences_before_vote() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    open_governance_proposal(&mut world, "proposal.quote.quorum", 6, 5, 6_000);

    let quote = world
        .governance_vote_quote("proposal.quote.quorum", "a", "approve", 3)
        .expect("open governance proposal has a deterministic quote");

    assert_eq!(quote.proposal_id, "proposal.quote.quorum");
    assert_eq!(quote.proposal_topic, "title.proposal.quote.quorum");
    assert_eq!(quote.actor_id, "a");
    assert_eq!(quote.action_kind, "cast_governance_vote");
    assert!(quote.ticks_remaining > 0);
    assert_eq!(quote.current_quorum_weight, 0);
    assert_eq!(quote.required_quorum_weight, 5);
    assert_eq!(quote.current_pass_bps, 0);
    assert_eq!(quote.required_pass_bps, 6_000);
    assert_eq!(quote.actor_vote_weight, 3);
    assert_eq!(quote.vote_swing_potential, 3);
    assert_eq!(quote.likely_outcome_before_action, "rejected");
    assert_eq!(quote.likely_outcome_after_action, "rejected");
    assert!(!quote.affected_rule_or_priority.is_empty());
    assert!(!quote.world_change_if_passed.is_empty());
    assert!(!quote.cost_or_cooldown_if_failed.is_empty());
    assert_eq!(quote.recommended_governance_action, "seek_quorum");
    assert!(quote.why_this_vote_matters.contains("quorum"));
}

#[test]
fn governance_vote_quote_accounts_for_recast_replacing_tally_and_flipping_outcome() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    open_governance_proposal(&mut world, "proposal.quote.recast", 6, 4, 5_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.quote.recast".to_string(),
        option: "approve".to_string(),
        weight: 1,
    });
    world.step().expect("seed recast ballot");
    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "b".to_string(),
        proposal_key: "proposal.quote.recast".to_string(),
        option: "reject".to_string(),
        weight: 3,
    });
    world.step().expect("seed opposing ballot");

    let quote = world
        .governance_vote_quote("proposal.quote.recast", "a", "approve", 4)
        .expect("open governance proposal has a deterministic recast quote");

    assert_eq!(quote.action_kind, "recast_governance_vote");
    assert_eq!(quote.current_quorum_weight, 4);
    assert_eq!(quote.actor_vote_weight, 4);
    assert_eq!(quote.vote_swing_potential, 3);
    assert_eq!(quote.likely_outcome_before_action, "rejected");
    assert_eq!(quote.likely_outcome_after_action, "passed");
    assert_eq!(quote.recommended_governance_action, "cast_vote");
    assert!(quote.why_this_vote_matters.contains("outcome"));
}

#[test]
fn governance_vote_quote_returns_explicit_missing_semantics_for_unknown_or_expired_proposals() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);

    assert_eq!(
        world
            .governance_vote_quote("proposal.quote.missing", "a", "approve", 1)
            .expect_err("unknown proposal must not fabricate governance advice"),
        "governance_vote_quote_missing"
    );

    open_governance_proposal(&mut world, "proposal.quote.expired", 1, 1, 5_000);
    world.step().expect("advance proposal to expiry");
    assert_eq!(
        world
            .governance_vote_quote("proposal.quote.expired", "a", "approve", 1)
            .expect_err("expired proposal must not fabricate governance advice"),
        "governance_vote_quote_missing"
    );
}

#[test]
fn governance_vote_quote_rejects_recovered_inactive_identity_snapshot_with_stale_cap() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);
    open_governance_proposal(&mut world, "proposal.quote.inactive.identity", 6, 1, 5_000);

    let mut recovered_state = world.state().clone();
    let snapshot = recovered_state
        .governance_proposals
        .get_mut("proposal.quote.inactive.identity")
        .and_then(|proposal| proposal.vote_weight_snapshot.get_mut("a"))
        .expect("governance identity snapshot");
    assert!(snapshot.vote_weight_cap > 0);
    snapshot.status = GovernanceIdentityStatus::Frozen;
    let recovered_world = World::new_with_state(recovered_state);

    assert_eq!(
        recovered_world
            .governance_vote_quote("proposal.quote.inactive.identity", "a", "approve", 1)
            .expect_err("inactive identity must not receive actionable governance advice"),
        "governance_vote_quote_missing"
    );
}
