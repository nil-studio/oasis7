use super::*;

#[test]
fn runtime_agent_chat_echo_env_enqueues_agent_spoke_virtual_event() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_AGENT_CHAT_ECHO_ENV, "1");
    }
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(31);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello runtime echo".to_string(),
            intent_tick: Some(9),
            intent_seq: Some(31),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        31,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        30,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let ack = server.handle_agent_chat(request).expect("chat accepted");
    assert_eq!(ack.agent_id, agent_id);
    let feedback = server
        .scoped_feedback_for_player(Some("player-a"))
        .expect("agent-chat feedback recorded");
    assert_eq!(feedback.action, "agent_chat");
    assert_eq!(feedback.stage, "accepted");
    assert_eq!(feedback.target_agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(
        feedback
            .intent_summary
            .as_deref()
            .is_some_and(|summary| summary.contains(agent_id.as_str()))
    );

    let events: Vec<_> = server.pending_virtual_events.drain(..).collect();
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, message, .. }
            if event_agent_id == &agent_id && message == "[local-mock-receipt] 已收到消息；当前本地 mock provider 不生成真实 Agent 回复：hello runtime echo"
    )));
}

#[test]
fn runtime_agent_chat_echo_env_accepts_chat_without_llm_runner_config() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_AGENT_CHAT_ECHO_ENV, "1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_MODEL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_BASE_URL);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(crate::simulator::ENV_LLM_API_KEY);
    }
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(33);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello runtime echo without llm config".to_string(),
            intent_tick: Some(11),
            intent_seq: Some(33),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        33,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        32,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let ack = server.handle_agent_chat(request).expect("chat accepted");
    assert_eq!(ack.agent_id, agent_id);

    let events: Vec<_> = server.pending_virtual_events.drain(..).collect();
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, message, .. }
            if event_agent_id == &agent_id && message == "[local-mock-receipt] 已收到消息；当前本地 mock provider 不生成真实 Agent 回复：hello runtime echo without llm config"
    )));
}

#[test]
fn runtime_agent_chat_echo_removed_old_brand_env_is_ignored() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            removed_old_brand_runtime_live_env("RUNTIME_AGENT_CHAT_ECHO"),
            "1",
        );
    }
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(32);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello removed old brand runtime echo".to_string(),
            intent_tick: Some(10),
            intent_seq: Some(32),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        32,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        31,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let ack = server.handle_agent_chat(request).expect("chat accepted");
    assert_eq!(ack.agent_id, agent_id);

    let events: Vec<_> = server.pending_virtual_events.drain(..).collect();
    assert!(!events.iter().any(|event| matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, message, .. }
            if event_agent_id == &agent_id && message == "[local-mock-receipt] 已收到消息；当前本地 mock provider 不生成真实 Agent 回复：hello removed old brand runtime echo"
    )));
}
