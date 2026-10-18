use super::*;

#[test]
fn runtime_gameplay_action_claim_first_agent_registers_starter_agent() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();
    assert!(server.world.state().agents.is_empty());

    let (public_key, private_key) = test_signer(93);
    let register_ack = register_runtime_session(
        &mut server,
        "player-first-agent",
        None,
        93,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-first-agent".to_string(),
            public_key: None,
            auth: None,
        },
        94,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_gameplay_action(request)
        .expect("first-agent claim action accepted");
    assert_eq!(ack.action_id, crate::viewer::ACTION_CLAIM_FIRST_AGENT);
    assert_eq!(
        ack.target_agent_id,
        crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID
    );

    server.world.step().expect("apply first-agent claim");
    assert!(
        server
            .world
            .state()
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    let starter_agent = server
        .world
        .state()
        .agents
        .get(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
        .expect("starter agent should exist after claim");
    assert_eq!(
        starter_agent.state.pos,
        crate::viewer::runtime_live::test_first_agent_spawn_pos()
            .expect("formal release starter spawn")
    );
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("player-first-agent"),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
}

#[test]
fn runtime_gameplay_action_claim_first_agent_recovers_stale_binding_without_agent() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();
    server
        .llm_sidecar
        .bind_agent_player(
            crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID,
            "stale-player",
            None,
            false,
        )
        .expect("seed stale binding");
    assert!(server.world.state().agents.is_empty());

    let (public_key, private_key) = test_signer(95);
    let register_ack = register_runtime_session(
        &mut server,
        "player-recovered-first-agent",
        None,
        95,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-recovered-first-agent".to_string(),
            public_key: None,
            auth: None,
        },
        96,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_gameplay_action(request)
        .expect("stale first-agent claim action accepted");
    assert_eq!(ack.action_id, crate::viewer::ACTION_CLAIM_FIRST_AGENT);
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("player-recovered-first-agent"),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player("stale-player"),
        None
    );
}

#[test]
fn runtime_gameplay_action_claim_starter_oc_grants_first_llm_budget() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();
    let (public_key, private_key) = test_signer(97);
    let register_ack = register_runtime_session(
        &mut server,
        "player-starter-oc",
        None,
        97,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let first_agent_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-starter-oc".to_string(),
            public_key: None,
            auth: None,
        },
        98,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_gameplay_action(first_agent_request)
        .expect("first-agent claim accepted");
    server.world.step().expect("apply first-agent claim");

    let starter_oc_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-starter-oc".to_string(),
            public_key: None,
            auth: None,
        },
        99,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_gameplay_action(starter_oc_request)
        .expect("starter OC claim accepted");
    assert_eq!(ack.action_id, crate::viewer::ACTION_CLAIM_STARTER_OC);
    server.world.step().expect("apply starter OC claim");

    let starter_agent_id = crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID;
    assert_eq!(
        server.world.main_token_liquid_balance(starter_agent_id),
        100_000_000
    );
    let claim = server
        .world
        .state()
        .starter_oc_claims
        .get(starter_agent_id)
        .expect("starter OC claim persisted");
    assert_eq!(claim.player_id, "player-starter-oc");
    assert_eq!(claim.public_key.as_deref(), Some(public_key.as_str()));
}

#[test]
fn compat_snapshot_waits_for_claimed_first_agent_before_starter_oc_action() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();
    let (public_key, private_key) = test_signer(107);
    register_runtime_session(
        &mut server,
        "player-starter-oc-gated",
        None,
        107,
        public_key.as_str(),
        private_key.as_str(),
    );

    let first_agent_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-starter-oc-gated".to_string(),
            public_key: None,
            auth: None,
        },
        108,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_gameplay_action(first_agent_request)
        .expect("first-agent claim accepted");

    let pending_snapshot = server.compat_snapshot(Some("player-starter-oc-gated"));
    let pending_gameplay = pending_snapshot
        .player_gameplay
        .as_ref()
        .expect("player gameplay snapshot");
    assert!(
        !pending_snapshot
            .model
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(
        !pending_gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == crate::viewer::ACTION_CLAIM_STARTER_OC),
        "starter OC claim must wait until the first-agent runtime action creates the Agent"
    );
    assert!(
        pending_gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == "advance_step")
    );

    server.world.step().expect("apply first-agent claim");
    let ready_snapshot = server.compat_snapshot(Some("player-starter-oc-gated"));
    let ready_gameplay = ready_snapshot
        .player_gameplay
        .as_ref()
        .expect("player gameplay snapshot");
    assert!(
        ready_snapshot
            .model
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(
        ready_gameplay
            .available_actions
            .iter()
            .any(|action| action.action_id == crate::viewer::ACTION_CLAIM_STARTER_OC)
    );
}

#[test]
fn runtime_gameplay_action_claim_first_agent_rejects_non_starter_target() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();

    let (public_key, private_key) = test_signer(95);
    let register_ack = register_runtime_session(
        &mut server,
        "player-first-agent-invalid",
        None,
        95,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: "custom-agent-id".to_string(),
            actor_agent_id: None,
            player_id: "player-first-agent-invalid".to_string(),
            public_key: None,
            auth: None,
        },
        96,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_gameplay_action(request)
        .expect_err("first-agent claim should only accept the starter target");
    assert_eq!(err.code, "invalid_first_agent_claim_target");
    assert!(server.world.state().agents.is_empty());
}

#[test]
fn runtime_gameplay_action_claim_starter_oc_credits_liquid_oc_once() {
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
    assert_eq!(server.world.main_token_liquid_balance(agent_id.as_str()), 0);

    let (public_key, private_key) = test_signer(97);
    let register_ack = register_runtime_session(
        &mut server,
        "player-starter-oc",
        None,
        97,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
            target_agent_id: agent_id.clone(),
            actor_agent_id: None,
            player_id: "player-starter-oc".to_string(),
            public_key: Some(public_key.clone()),
            auth: None,
        },
        98,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_gameplay_action(request)
        .expect("starter OC claim action accepted");
    assert_eq!(ack.action_id, crate::viewer::ACTION_CLAIM_STARTER_OC);

    server.world.step().expect("apply starter OC claim");
    assert_eq!(
        server.world.main_token_liquid_balance(agent_id.as_str()),
        100_000_000
    );
    let claim = server
        .world
        .state()
        .starter_oc_claims
        .get(agent_id.as_str())
        .expect("starter OC claim stored");
    assert_eq!(claim.player_id, "player-starter-oc");
    assert_eq!(claim.public_key.as_deref(), Some(public_key.as_str()));
    assert_eq!(claim.amount, 100_000_000);

    let repeat_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
            target_agent_id: agent_id.clone(),
            actor_agent_id: None,
            player_id: "player-starter-oc".to_string(),
            public_key: Some(public_key.clone()),
            auth: None,
        },
        99,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_gameplay_action(repeat_request)
        .expect("repeat starter OC claim can be submitted for consensus rejection");
    server
        .world
        .step()
        .expect("apply repeat starter OC rejection");
    assert_eq!(
        server.world.main_token_liquid_balance(agent_id.as_str()),
        100_000_000
    );
    assert_eq!(
        server
            .llm_sidecar
            .bound_agent_for_player("player-starter-oc"),
        None
    );
}

#[test]
fn runtime_gameplay_action_claim_uses_bound_player_agent_as_actor() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let claimer_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    server
        .world
        .set_main_token_supply(crate::runtime::MainTokenSupplyState {
            total_supply: 1_000,
            circulating_supply: 1_000,
            ..crate::runtime::MainTokenSupplyState::default()
        });
    server
        .world
        .set_agent_reputation_score(claimer_agent_id.as_str(), 0)
        .expect("set reputation");
    server
        .world
        .set_main_token_account_balance_with_restricted(claimer_agent_id.as_str(), 0, 0, 650)
        .expect("seed slot-1 claim balance");
    server
        .world
        .submit_action(crate::runtime::Action::RegisterAgent {
            agent_id: "agent-claim-target".to_string(),
            pos: crate::geometry::GeoPos::new(0, 0, 0),
        });
    server.world.step().expect("register claim target");

    let (public_key, private_key) = test_signer(88);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(claimer_agent_id.as_str()),
        87,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_AGENT.to_string(),
            target_agent_id: "agent-claim-target".to_string(),
            actor_agent_id: Some(claimer_agent_id.clone()),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        88,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_gameplay_action(request)
        .expect("claim action accepted");
    assert_eq!(ack.action_id, crate::viewer::ACTION_CLAIM_AGENT);

    server.world.step().expect("apply claim");
    let claim = server
        .world
        .state()
        .agent_claims
        .get("agent-claim-target")
        .expect("claim stored");
    assert_eq!(claim.claim_owner_id, claimer_agent_id);
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player("player-a"),
        Some(claimer_agent_id.as_str())
    );
}

#[test]
fn runtime_gameplay_action_claim_rejects_actor_agent_mismatch() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let claimer_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    server
        .world
        .submit_action(crate::runtime::Action::RegisterAgent {
            agent_id: "agent-claim-target".to_string(),
            pos: crate::geometry::GeoPos::new(0, 0, 0),
        });
    server.world.step().expect("register claim target");

    let (public_key, private_key) = test_signer(89);
    let _ = register_runtime_session(
        &mut server,
        "player-a",
        Some(claimer_agent_id.as_str()),
        88,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_AGENT.to_string(),
            target_agent_id: "agent-claim-target".to_string(),
            actor_agent_id: Some("other-agent".to_string()),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        89,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_gameplay_action(request)
        .expect_err("mismatched actor must fail");
    assert_eq!(err.code, "actor_agent_mismatch");
}

#[test]
fn runtime_gameplay_action_claim_rejects_target_agent_bound_to_other_player() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let claimer_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    server
        .world
        .submit_action(crate::runtime::Action::RegisterAgent {
            agent_id: "agent-claim-target".to_string(),
            pos: crate::geometry::GeoPos::new(0, 0, 0),
        });
    server.world.step().expect("register claim target");

    let (claimer_public_key, claimer_private_key) = test_signer(90);
    let _ = register_runtime_session(
        &mut server,
        "player-a",
        Some(claimer_agent_id.as_str()),
        89,
        claimer_public_key.as_str(),
        claimer_private_key.as_str(),
    );
    let (target_public_key, target_private_key) = test_signer(91);
    let target_register_ack = register_runtime_session(
        &mut server,
        "player-b",
        Some("agent-claim-target"),
        90,
        target_public_key.as_str(),
        target_private_key.as_str(),
    );
    assert_eq!(
        target_register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_AGENT.to_string(),
            target_agent_id: "agent-claim-target".to_string(),
            actor_agent_id: Some(claimer_agent_id.clone()),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        91,
        claimer_public_key.as_str(),
        claimer_private_key.as_str(),
    );
    let err = server
        .handle_gameplay_action(request)
        .expect_err("target agent bound to another player must fail");
    assert_eq!(err.code, "agent_control_forbidden");
    assert_eq!(err.target_agent_id.as_deref(), Some("agent-claim-target"));
}
