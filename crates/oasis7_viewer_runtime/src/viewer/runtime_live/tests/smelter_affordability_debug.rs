use super::*;

#[test]
fn smelter_affordability_debug_seed_publishes_real_factory_and_disabled_replenish_action() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed real runtime state");

    assert!(server.world.has_factory(crate::viewer::FACTORY_SMELTER_MK1));
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("seeded agent")
        .clone();
    assert!(
        server
            .world
            .state()
            .starter_oc_claims
            .contains_key(&agent_id)
    );
    assert_eq!(
        server
            .world
            .state()
            .agents
            .get(agent_id.as_str())
            .expect("seeded agent state")
            .state
            .resources
            .get(ResourceKind::Electricity),
        0
    );
    assert_eq!(
        server
            .world
            .state()
            .agents
            .get(agent_id.as_str())
            .expect("seeded agent state")
            .state
            .resources
            .get(ResourceKind::Data),
        0
    );
    let smelter_input_ledger = server
        .world
        .state()
        .factories
        .get(crate::viewer::FACTORY_SMELTER_MK1)
        .expect("seeded smelter")
        .input_ledger
        .clone();
    server
        .world
        .set_ledger_material_balance(smelter_input_ledger.clone(), "iron_ore", 48)
        .expect("seed enough authoritative input to isolate the electricity blocker");
    server
        .world
        .set_ledger_material_balance(smelter_input_ledger, "carbon_fuel", 12)
        .expect("seed enough authoritative fuel to isolate the electricity blocker");

    server
        .session_policy
        .register_session("s6-debug-player", "s6-debug-public-key")
        .expect("register debug test session");
    server
        .bind_player_session_agent(
            agent_id.as_str(),
            "s6-debug-player",
            Some("s6-debug-public-key"),
            false,
        )
        .expect("bind debug test player");
    let gameplay = server
        .compat_snapshot(Some("s6-debug-player"))
        .player_gameplay
        .expect("debug scenario gameplay snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT)
        .expect("smelter schedule action");
    let reason = action.disabled_reason.as_deref().expect("disabled action");
    assert!(
        reason.contains("insufficient factory-owner electricity"),
        "unexpected readiness reason: {reason}"
    );
    assert!(reason.contains("replenish electricity"));
}

#[test]
fn smelter_affordability_debug_seed_auto_binds_only_local_test_registration() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed debug scenario");
    let seeded_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .expect("seeded agent")
        .clone();
    let (public_key, private_key) = test_signer(81);

    let ack = register_runtime_session_with_options(
        &mut server,
        "local-test-player-s6",
        Some("caller-chosen-agent-must-not-win"),
        true,
        80,
        public_key.as_str(),
        private_key.as_str(),
    );

    assert_eq!(ack.agent_id.as_deref(), Some(seeded_agent_id.as_str()));
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("local-test-player-s6"),
        Some(seeded_agent_id.as_str())
    );
}

#[test]
fn smelter_affordability_debug_seed_does_not_auto_bind_non_local_registration() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    server
        .seed_smelter_affordability_debug_scenario()
        .expect("seed debug scenario");
    let (public_key, private_key) = test_signer(82);

    let ack = register_runtime_session(
        &mut server,
        "non-local-player-s6",
        None,
        81,
        public_key.as_str(),
        private_key.as_str(),
    );

    assert_eq!(ack.agent_id, None);
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("non-local-player-s6"),
        None
    );
}

#[test]
fn default_runtime_session_registration_does_not_auto_bind_local_test_player() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let (public_key, private_key) = test_signer(83);

    let ack = register_runtime_session(
        &mut server,
        "local-test-player-default",
        None,
        82,
        public_key.as_str(),
        private_key.as_str(),
    );

    assert_eq!(ack.agent_id, None);
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("local-test-player-default"),
        None
    );
}
