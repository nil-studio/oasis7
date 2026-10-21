use super::*;

const SNAPSHOT_PLAYER_ID: &str = "player-snapshot";

#[test]
fn compat_snapshot_quotes_when_a_queued_intent_can_be_safely_waited_on() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.latest_player_gameplay_feedback = Some(crate::simulator::PlayerGameplayRecentFeedback {
        request_auth_nonce: None,
        action: "gameplay_action:build_factory_smelter_mk1".to_string(),
        stage: "queued".to_string(),
        effect: "queued gameplay action build_factory_smelter_mk1 for agent-0 as runtime action 7"
            .to_string(),
        intent_summary: Some(
            "queue gameplay action build_factory_smelter_mk1 for agent-0".to_string(),
        ),
        target_agent_id: Some("agent-0".to_string()),
        reason: None,
        hint: Some("advance 1-2 steps to apply the queued gameplay action".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    });

    let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
    let serialized = serde_json::to_value(&snapshot).expect("compat snapshot serializes");
    let gameplay = serialized
        .get("player_gameplay")
        .and_then(serde_json::Value::as_object)
        .expect("queued gameplay snapshot");
    let quote = gameplay
        .get("wait_resolution_quote")
        .and_then(serde_json::Value::as_object)
        .expect("queued committed intent publishes its authoritative wait-resolution quote");

    for field in [
        "resolution_trigger",
        "recheck_tick_or_event",
        "expected_change",
        "unresolved_risk",
        "alternative_unlock_condition",
    ] {
        assert!(
            quote
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_some(),
            "wait-resolution quote publishes {field}"
        );
    }
    assert_eq!(
        quote
            .get("safe_to_wait")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "an available advance_step is not, by itself, a safe-wait authorization"
    );
    assert!(
        gameplay["available_actions"]
            .as_array()
            .is_some_and(|actions| actions
                .iter()
                .any(|action| action["action_id"] == "advance_step")),
        "the contract distinguishes the offered advance action from a safe wait"
    );

    let mut legacy_gameplay = serde_json::Value::Object(gameplay.clone());
    legacy_gameplay
        .as_object_mut()
        .expect("legacy gameplay object")
        .remove("wait_resolution_quote");
    serde_json::from_value::<crate::simulator::PlayerGameplaySnapshot>(legacy_gameplay)
        .expect("snapshots written before wait-resolution quotes remain readable");
}

#[test]
fn compat_snapshot_omits_wait_resolution_quote_for_blocked_feedback() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.latest_player_gameplay_feedback = Some(crate::simulator::PlayerGameplayRecentFeedback {
        request_auth_nonce: None,
        action: "gameplay_action:build_factory_smelter_mk1".to_string(),
        stage: "blocked".to_string(),
        effect: "factory construction remains blocked".to_string(),
        intent_summary: Some(
            "queue gameplay action build_factory_smelter_mk1 for agent-0".to_string(),
        ),
        target_agent_id: Some("agent-0".to_string()),
        reason: Some("required material is unavailable".to_string()),
        hint: Some("repair the material supply before retrying".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    });

    let serialized = serde_json::to_value(server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID)))
        .expect("compat snapshot serializes");
    let gameplay = serialized
        .get("player_gameplay")
        .and_then(serde_json::Value::as_object)
        .expect("blocked gameplay snapshot");

    assert!(
        gameplay
            .get("wait_resolution_quote")
            .is_none_or(serde_json::Value::is_null),
        "blocked feedback is not presented as a safe-wait quote"
    );
}

#[test]
fn compat_snapshot_quotes_a_real_pending_runtime_gameplay_action() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let target_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(221);
    register_runtime_session(
        &mut server,
        "player-pending-runtime-action",
        Some(target_agent_id.as_str()),
        220,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id,
            actor_agent_id: None,
            player_id: "player-pending-runtime-action".to_string(),
            public_key: None,
            auth: None,
        },
        221,
        public_key.as_str(),
        private_key.as_str(),
    );

    let ack = server
        .handle_gameplay_action(request)
        .expect("runtime gameplay action is accepted for later application");
    assert!(
        ack.runtime_action_id > 0,
        "the normal gameplay path must remain a pending runtime action"
    );

    let gameplay = server
        .compat_snapshot(Some("player-pending-runtime-action"))
        .player_gameplay
        .expect("player gameplay snapshot");
    assert!(
        gameplay.wait_resolution_quote.is_some(),
        "a real queued runtime action must tell the player when to recheck"
    );
}

#[test]
fn compat_snapshot_omits_wait_quote_after_synchronous_existing_first_agent_claim() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.world = crate::runtime::World::new_production_hardened();
    server
        .world
        .submit_action(crate::runtime::Action::RegisterAgent {
            agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            pos: crate::viewer::runtime_live::test_first_agent_spawn_pos()
                .expect("formal release starter spawn"),
        });
    server.world.step().expect("register existing first Agent");

    let (public_key, private_key) = test_signer(222);
    register_runtime_session(
        &mut server,
        "player-synchronous-first-agent-claim",
        None,
        222,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
            actor_agent_id: None,
            player_id: "player-synchronous-first-agent-claim".to_string(),
            public_key: None,
            auth: None,
        },
        223,
        public_key.as_str(),
        private_key.as_str(),
    );

    let ack = server
        .handle_gameplay_action(request)
        .expect("existing first Agent binding succeeds immediately");
    assert_eq!(
        ack.runtime_action_id, 0,
        "this claim did not enter the runtime queue"
    );

    let gameplay = server
        .compat_snapshot(Some("player-synchronous-first-agent-claim"))
        .player_gameplay
        .expect("player gameplay snapshot");
    assert!(
        gameplay.wait_resolution_quote.is_none(),
        "a synchronously completed claim must not be presented as pending runtime work"
    );
}

#[test]
fn compat_snapshot_omits_wait_quote_after_accepted_agent_chat() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_agent_chat_echo_enabled(true),
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(224);
    register_runtime_session(
        &mut server,
        "player-agent-chat",
        Some(agent_id.as_str()),
        224,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-agent-chat".to_string()),
            public_key: None,
            auth: None,
            message: "inspect the current production line".to_string(),
            intent_tick: None,
            intent_seq: Some(225),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        225,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_agent_chat(request)
        .expect("agent chat is accepted without becoming a runtime gameplay action");

    let gameplay = server
        .compat_snapshot(Some("player-agent-chat"))
        .player_gameplay
        .expect("player gameplay snapshot");
    assert!(
        gameplay.wait_resolution_quote.is_none(),
        "accepted agent chat must not be framed as pending runtime application"
    );
}
