use super::*;

#[test]
fn governance_vote_quote_debug_seed_auto_binds_local_test_registration_without_requested_agent() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    server
        .seed_governance_vote_quote_debug_scenario()
        .expect("seed governance quote debug scenario");
    let seeded_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("seeded agent")
        .clone();
    let (public_key, private_key) = test_signer(91);

    let ack = register_runtime_session(
        &mut server,
        "local-test-player-governance-vote-quote",
        None,
        90,
        public_key.as_str(),
        private_key.as_str(),
    );

    assert_eq!(ack.agent_id.as_deref(), Some(seeded_agent_id.as_str()));
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("local-test-player-governance-vote-quote"),
        Some(seeded_agent_id.as_str())
    );
}

#[test]
fn default_runtime_session_registration_does_not_auto_bind_governance_local_test_player() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let (public_key, private_key) = test_signer(92);

    let ack = register_runtime_session(
        &mut server,
        "local-test-player-governance-default",
        None,
        91,
        public_key.as_str(),
        private_key.as_str(),
    );

    assert_eq!(ack.agent_id, None);
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("local-test-player-governance-default"),
        None
    );
}
