use super::*;

fn signed_governance_vote_quote_request(
    proposal_key: &str,
    option: &str,
    weight: u32,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::GovernanceVoteQuoteRequest {
    let mut request = crate::viewer::GovernanceVoteQuoteRequest {
        proposal_key: proposal_key.to_string(),
        option: option.to_string(),
        weight,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_governance_vote_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign governance vote quote auth"),
    );
    request
}

#[test]
fn runtime_governance_vote_quote_is_authenticated_non_mutating_and_serializes_player_decision_fields()
 {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    server
        .world
        .submit_action(RuntimeAction::OpenGovernanceProposal {
            proposer_agent_id: agent_id.clone(),
            proposal_key: "proposal.viewer-governance-quote".to_string(),
            title: "Keep the solar reserve".to_string(),
            description: "Prioritize the solar reserve over an emergency drawdown.".to_string(),
            options: vec!["approve".to_string(), "reject".to_string()],
            voting_window_ticks: 12,
            quorum_weight: 3,
            pass_threshold_bps: 6_000,
        });
    server.world.step().expect("open governance proposal");
    let (public_key, private_key) = test_signer(43);
    register_runtime_session(
        &mut server,
        "player-governance-vote-quote",
        Some(agent_id.as_str()),
        3043,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_governance_vote_quote_request(
        "proposal.viewer-governance-quote",
        "approve",
        3,
        "player-governance-vote-quote",
        3044,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();

    let quote = server
        .handle_governance_vote_quote(request.clone())
        .expect("signed governance vote quote");
    assert_eq!(quote.proposal_id, "proposal.viewer-governance-quote");
    assert_eq!(quote.proposal_topic, "Keep the solar reserve");
    assert_eq!(quote.actor_id, agent_id);
    assert_eq!(quote.action_kind, "cast_governance_vote");
    assert_eq!(quote.current_quorum_weight, 0);
    assert_eq!(quote.required_quorum_weight, 3);
    assert_eq!(quote.current_pass_bps, 0);
    assert_eq!(quote.required_pass_bps, 6_000);
    assert_eq!(quote.actor_vote_weight, 3);
    assert_eq!(quote.vote_swing_potential, 3);
    assert_eq!(quote.likely_outcome_before_action, "rejected");
    assert_eq!(quote.likely_outcome_after_action, "passed");
    assert_eq!(quote.affected_rule_or_priority, "Keep the solar reserve");
    assert_eq!(
        quote.world_change_if_passed,
        "Prioritize the solar reserve over an emergency drawdown."
    );
    assert_eq!(quote.recommended_governance_action, "cast_vote");
    assert!(
        quote
            .why_this_vote_matters
            .contains("changes the likely outcome")
    );
    assert_eq!(server.world.state(), &state_before);
    assert_eq!(
        server
            .handle_governance_vote_quote(request.clone())
            .expect("repeat quote"),
        quote
    );

    let serialized =
        serde_json::to_value(crate::viewer::ViewerResponse::GovernanceVoteQuotePreflight { quote })
            .expect("governance quote serializes for the viewer");
    for field in [
        "proposal_id",
        "proposal_topic",
        "actor_id",
        "action_kind",
        "closes_at_tick",
        "ticks_remaining",
        "current_quorum_weight",
        "required_quorum_weight",
        "current_pass_bps",
        "required_pass_bps",
        "actor_vote_weight",
        "vote_swing_potential",
        "likely_outcome_before_action",
        "likely_outcome_after_action",
        "affected_rule_or_priority",
        "world_change_if_passed",
        "cost_or_cooldown_if_failed",
        "recommended_governance_action",
        "why_this_vote_matters",
    ] {
        assert!(
            serialized["quote"].get(field).is_some(),
            "serialized quote includes {field}"
        );
    }

    let mut missing_auth = request;
    missing_auth.auth = None;
    let error = server
        .handle_governance_vote_quote(missing_auth)
        .expect_err("governance vote quote requires auth");
    assert_eq!(error.code, "auth_proof_required");
    assert_eq!(error.action_id.as_deref(), Some("quote_governance_vote"));
}
