use super::*;

#[path = "auth_actions_agent_chat_echo.rs"]
mod echo_tests;

#[test]
fn runtime_agent_chat_provider_mode_reports_feedback_failure() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
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
    let (public_key, private_key) = test_signer(35);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        34,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(35),
            intent_seq: Some(35),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        35,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_agent_chat(request)
        .expect_err("provider feedback failure should reject chat ack");
    assert_eq!(err.code, "provider_unreachable");
    clear_runtime_provider_env();
}

#[test]
fn runtime_agent_chat_provider_mode_accepts_feedback_without_echo_receipt() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let base_url = spawn_runtime_live_mock_http_server_with_provider_probes(6, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/v1/provider/info") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "provider_id": "provider_local_bridge",
                        "capabilities": ["decision", "feedback", "agent_chat"],
                        "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
                        "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
                        "supported_action_sets": ["phase1_low_frequency"]
                    })
                    .to_string(),
                },
                ("GET", "/v1/provider/health") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true, "status": "ok"}).to_string(),
                },
                ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                },
                ("POST", "/v1/world-simulator/agent-chat") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "agent_id": "agent-0",
                        "message": "我在测试地点，资源是 electricity=32 data=8。"
                    })
                    .to_string(),
                },
                _ => MockHttpResponse {
                    status_code: 404,
                    body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
                },
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_AGENT_CHAT_ECHO_ENV, "0");
    }
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    assert!(server.llm_sidecar.supports_agent_chat());
    let anonymous_gameplay = server
        .compat_snapshot(None)
        .player_gameplay
        .expect("anonymous player gameplay snapshot");
    assert!(
        anonymous_gameplay
            .available_actions
            .iter()
            .all(|action| action.protocol_action != "agent_chat")
    );
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(34);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        33,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let gameplay = server
        .compat_snapshot(Some("player-a"))
        .player_gameplay
        .expect("player gameplay snapshot");
    assert!(
        gameplay
            .available_actions
            .iter()
            .any(|action| action.protocol_action == "agent_chat"
                && action.target_agent_id.as_deref() == Some(agent_id.as_str()))
    );
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello provider feedback".to_string(),
            intent_tick: Some(12),
            intent_seq: Some(34),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        34,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server.handle_agent_chat(request).expect("chat accepted");
    assert_eq!(ack.agent_id, agent_id);
    server.enqueue_pending_provider_agent_chat_replies();
    let recorded = recorded.lock().expect("recorded lock");
    assert_eq!(recorded.len(), 6);
    assert_eq!(recorded[0].path, "/v1/provider/info");
    assert_eq!(recorded[1].path, "/v1/provider/health");
    assert_eq!(recorded[2].method, "GET");
    assert_eq!(recorded[2].path, "/v1/provider/info");
    assert_eq!(recorded[3].method, "POST");
    assert_eq!(recorded[3].path, "/v1/world-simulator/feedback");
    assert_eq!(recorded[4].method, "GET");
    assert_eq!(recorded[4].path, "/v1/provider/info");
    assert_eq!(recorded[5].method, "POST");
    assert_eq!(recorded[5].path, "/v1/world-simulator/agent-chat");
    let feedback: crate::simulator::FeedbackEnvelope =
        serde_json::from_slice(recorded[3].body.as_slice()).expect("decode feedback");
    assert_eq!(
        feedback.world_delta_summary.as_deref(),
        Some("player_message: hello provider feedback")
    );
    assert!(server.pending_virtual_events.iter().any(|event| matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, message, .. }
            if event_agent_id == &agent_id && message == "我在测试地点，资源是 electricity=32 data=8。"
    )));
    assert!(!server.pending_virtual_events.iter().any(|event| matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, message, .. }
            if event_agent_id == &agent_id && message.contains("[local-mock-receipt]")
    )));
    clear_runtime_provider_env();
}

#[test]
fn runtime_agent_chat_provider_mode_surfaces_async_reply_failure_after_ack() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let base_url = spawn_runtime_live_mock_http_server_with_provider_probes(4, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/v1/provider/info") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "provider_id": "provider_local_bridge",
                        "capabilities": ["decision", "feedback", "agent_chat"],
                        "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
                        "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
                        "supported_action_sets": ["phase1_low_frequency"]
                    })
                    .to_string(),
                },
                ("GET", "/v1/provider/health") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true, "status": "ok"}).to_string(),
                },
                ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                },
                ("POST", "/v1/world-simulator/agent-chat") => MockHttpResponse {
                    status_code: 500,
                    body: serde_json::json!({
                        "error_code": "provider_agent_chat_failed",
                        "error": "upstream chat completion returned HTTP 401: Invalid token"
                    })
                    .to_string(),
                },
                _ => MockHttpResponse {
                    status_code: 404,
                    body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
                },
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_AGENT_CHAT_ECHO_ENV, "0");
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
    let (public_key, private_key) = test_signer(55);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        54,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello provider async failure".to_string(),
            intent_tick: Some(55),
            intent_seq: Some(55),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        55,
        public_key.as_str(),
        private_key.as_str(),
    );

    let ack = server.handle_agent_chat(request).expect("chat accepted");
    assert_eq!(ack.agent_id, agent_id);
    let errors = server.enqueue_pending_provider_agent_chat_replies();

    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "provider_unreachable");
    assert_eq!(errors[0].agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(errors[0].message.contains("provider_agent_chat_failed"));
    assert!(server.pending_virtual_events.iter().all(|event| !matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { agent_id: event_agent_id, .. }
            if event_agent_id == &agent_id
    )));
    clear_runtime_provider_env();
}

#[test]
fn runtime_agent_chat_rejects_unbound_agent_after_session_registration() {
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(39);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        None,
        38,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(register_ack.agent_id, None);
    assert!(
        !server
            .llm_sidecar
            .agent_player_bindings
            .contains_key(agent_id.as_str()),
        "seed agent should start without an account binding"
    );

    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello unbound".to_string(),
            intent_tick: Some(39),
            intent_seq: Some(39),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        39,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_agent_chat(request)
        .expect_err("agent chat must not implicitly bind an unbound agent");
    assert_eq!(err.code, "agent_control_forbidden");
    assert!(err.message.contains("has no player binding"));
    assert!(
        !server
            .llm_sidecar
            .agent_player_bindings
            .contains_key(agent_id.as_str()),
        "rejected chat must leave the agent unbound"
    );
}

#[test]
fn runtime_agent_chat_provider_mode_persists_rejection_without_agent_chat_capability() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let recorded = Arc::new(Mutex::new(Vec::<RecordedHttpRequest>::new()));
    let base_url = spawn_runtime_live_mock_http_server_with_provider_probes(1, {
        let recorded = Arc::clone(&recorded);
        move |request| {
            recorded
                .lock()
                .expect("recorded lock")
                .push(request.clone());
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/v1/provider/info") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({
                        "provider_id": "phase1_provider",
                        "capabilities": ["decision", "feedback"],
                        "chain_resource_manifest_schema_version": "oasis7.world_resource_manifest.v1",
                        "chain_resource_delta_schema_version": "oasis7.world_resource_delta.v1",
                        "supported_action_sets": ["phase1_low_frequency"]
                    })
                    .to_string(),
                },
                ("GET", "/v1/provider/health") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true, "status": "ok"}).to_string(),
                },
                ("POST", "/v1/world-simulator/feedback") => MockHttpResponse {
                    status_code: 200,
                    body: serde_json::json!({"ok": true}).to_string(),
                },
                ("POST", "/v1/world-simulator/agent-chat") => MockHttpResponse {
                    status_code: 500,
                    body: serde_json::json!({"ok": false, "error": "should_not_call"}).to_string(),
                },
                _ => MockHttpResponse {
                    status_code: 404,
                    body: serde_json::json!({"ok": false, "error": "not_found"}).to_string(),
                },
            }
        }
    });
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, base_url);
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(RUNTIME_AGENT_CHAT_ECHO_ENV, "0");
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
    let (public_key, private_key) = test_signer(36);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello phase1 provider".to_string(),
            intent_tick: Some(13),
            intent_seq: Some(36),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        36,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        35,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let error = server
        .handle_agent_chat(request)
        .expect_err("unsupported provider capability must reject before acceptance");
    assert_eq!(error.code, "agent_provider_chat_unsupported");
    assert_eq!(server.world.state().agents[&agent_id].intent, None);
    let recorded = recorded.lock().expect("recorded lock");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].method, "GET");
    assert_eq!(recorded[0].path, "/v1/provider/info");
    assert!(
        !recorded
            .iter()
            .any(|request| request.path == "/v1/world-simulator/agent-chat")
    );
    assert!(server.pending_virtual_events.iter().all(|event| !matches!(
        &event.kind,
        oasis7_simulator::WorldEventKind::AgentSpoke { .. }
    )));
    clear_runtime_provider_env();
}

#[test]
fn runtime_agent_chat_replay_returns_idempotent_ack() {
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(21);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(7),
            intent_seq: Some(5),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        5,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        4,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let first = server
        .handle_agent_chat(request.clone())
        .expect("first request accepted");
    assert_eq!(first.intent_tick, Some(7));
    assert_eq!(first.intent_seq, Some(5));
    assert!(!first.idempotent_replay);
    assert!(first.intent_id.is_some());
    assert!(first.accepted_event_seq.is_some_and(|seq| seq > 0));
    assert_eq!(first.status.as_deref(), Some("accepted"));

    let replay = server
        .handle_agent_chat(request.clone())
        .expect("replay request accepted");
    assert_eq!(replay.agent_id, first.agent_id);
    assert_eq!(replay.accepted_at_tick, first.accepted_at_tick);
    assert_eq!(replay.message_len, first.message_len);
    assert_eq!(replay.player_id, first.player_id);
    assert_eq!(replay.intent_tick, first.intent_tick);
    assert_eq!(replay.intent_seq, first.intent_seq);
    assert_eq!(replay.intent_id, first.intent_id);
    assert_eq!(replay.accepted_event_seq, first.accepted_event_seq);
    assert_eq!(replay.status, first.status);
    assert!(replay.idempotent_replay);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-a")
            .copied(),
        Some(5)
    );

    server.reorg_epoch = server.reorg_epoch.saturating_add(1);
    let stale = server
        .handle_agent_chat(request)
        .expect_err("cached ack must not cross a reorg epoch");
    assert_eq!(stale.code, "auth_invalid");
}

#[test]
fn runtime_agent_chat_acceptance_publishes_durable_primary_intent_handoff() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test holds the runtime LLM env lock while mutating process env.
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
    let (public_key, private_key) = test_signer(92);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        91,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let ack = server
        .handle_agent_chat(signed_agent_chat_request(
            crate::viewer::AgentChatRequest {
                agent_id: agent_id.clone(),
                player_id: Some("player-a".to_string()),
                public_key: None,
                auth: None,
                message: "Prioritize the iron line before expanding.".to_string(),
                intent_tick: Some(92),
                intent_seq: Some(92),
                world_id: None,
                reorg_epoch: None,
                authority_scope: None,
                replaces_intent_id: None,
                canonical_authority: None,
            },
            92,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("accepted player instruction");
    assert!(!ack.idempotent_replay);

    let canonical_intent = server.world.state().agents[agent_id.as_str()]
        .intent
        .clone()
        .expect("accepted chat must journal canonical Agent Intent V2");
    assert_eq!(canonical_intent.status, "accepted");
    assert_eq!(
        canonical_intent.summary,
        "Agent guidance accepted; the Agent will evaluate its next world action."
    );
    assert_eq!(canonical_intent.agent_id, agent_id);
    assert_eq!(canonical_intent.logical_time, ack.accepted_at_tick);
    assert_ne!(canonical_intent.event_seq, 0);

    let gameplay = server
        .compat_snapshot(Some("player-a"))
        .player_gameplay
        .expect("bound player gameplay snapshot");
    let contract = serde_json::to_value(gameplay).expect("serialize gameplay snapshot");
    assert_eq!(
        contract
            .pointer("/primary_intent/status")
            .and_then(serde_json::Value::as_str),
        Some("accepted"),
        "an accepted instruction must publish a durable primary-intent handoff, not only transient feedback"
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/message")
            .and_then(serde_json::Value::as_str),
        Some("Agent guidance accepted; the Agent will evaluate its next world action."),
        "the durable handoff must expose only the canonical player-safe summary"
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/resume_required")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "a newly accepted primary intent must clear any resume-required state"
    );
    assert_eq!(
        contract.pointer("/primary_intent/schema_version"),
        Some(&serde_json::json!(2))
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/intent_id")
            .and_then(serde_json::Value::as_str),
        Some(canonical_intent.intent_id.as_str())
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/agent_id")
            .and_then(serde_json::Value::as_str),
        Some(agent_id.as_str())
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/world_id")
            .and_then(serde_json::Value::as_str),
        Some("live-runtime-minimal")
    );
    assert_eq!(
        contract.pointer("/primary_intent/reorg_epoch"),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        contract.pointer("/primary_intent/logical_time"),
        Some(&serde_json::json!(canonical_intent.logical_time))
    );
    assert_eq!(
        contract.pointer("/primary_intent/updated_at"),
        Some(&serde_json::json!(canonical_intent.updated_at))
    );
    assert_eq!(
        contract.pointer("/primary_intent/source_class"),
        Some(&serde_json::json!("runtime_projection"))
    );
    assert_eq!(
        contract.pointer("/primary_intent/freshness"),
        Some(&serde_json::json!("current"))
    );
    assert_eq!(
        contract.pointer("/primary_intent/control_state"),
        Some(&serde_json::json!("controllable"))
    );
    assert_eq!(
        contract.pointer("/primary_intent/event_seq"),
        Some(&serde_json::json!(canonical_intent.event_seq.to_string()))
    );
}

#[test]
fn runtime_agent_chat_reprioritizes_the_durable_primary_intent() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test holds the runtime LLM env lock while mutating process env.
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
    let (public_key, private_key) = test_signer(93);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        91,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let mut first_request = None;
    for (nonce, message) in [
        (92, "Prioritize the iron line before expanding."),
        (93, "Stabilize power before expanding the iron line."),
    ] {
        let replaces_intent_id = server
            .world
            .state()
            .agents
            .get(agent_id.as_str())
            .and_then(|cell| cell.intent.as_ref())
            .map(|intent| intent.intent_id.clone());
        let request = signed_agent_chat_request(
            crate::viewer::AgentChatRequest {
                canonical_authority: None,
                agent_id: agent_id.clone(),
                player_id: Some("player-a".to_string()),
                public_key: None,
                auth: None,
                message: message.to_string(),
                intent_tick: Some(nonce),
                intent_seq: Some(nonce),
                world_id: None,
                reorg_epoch: None,
                authority_scope: None,
                replaces_intent_id,
            },
            nonce,
            public_key.as_str(),
            private_key.as_str(),
        );
        if nonce == 92 {
            first_request = Some(request.clone());
        }
        server
            .handle_agent_chat(request)
            .expect("accepted player instruction");
    }

    let replacement_replay = server
        .handle_agent_chat(first_request.expect("retain first chat request"))
        .expect("replay of superseded intent should return its durable disposition");
    assert_eq!(replacement_replay.status.as_deref(), Some("superseded"));
    assert!(replacement_replay.idempotent_replay);
    assert_eq!(
        replacement_replay.replaced_by.as_deref(),
        server.world.state().agents[agent_id.as_str()]
            .intent
            .as_ref()
            .map(|intent| intent.intent_id.as_str())
    );

    let gameplay = server
        .compat_snapshot(Some("player-a"))
        .player_gameplay
        .expect("bound player gameplay snapshot");
    let contract = serde_json::to_value(gameplay).expect("serialize gameplay snapshot");
    assert_eq!(
        contract
            .pointer("/primary_intent/status")
            .and_then(serde_json::Value::as_str),
        Some("accepted")
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/message")
            .and_then(serde_json::Value::as_str),
        Some("Agent guidance accepted; the Agent will evaluate its next world action.")
    );
    assert_eq!(
        contract
            .pointer("/primary_intent/resume_required")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );

    let canonical_intent = server.world.state().agents[agent_id.as_str()]
        .intent
        .as_ref()
        .expect("reprioritized chat must replace canonical intent");
    assert_eq!(canonical_intent.status, "accepted");
    assert_eq!(
        canonical_intent.summary,
        "Agent guidance accepted; the Agent will evaluate its next world action."
    );
}

#[test]
fn runtime_intent_projection_distinguishes_missing_intent_from_lost_control() {
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
        .llm_sidecar
        .player_agent_bindings
        .insert("player-missing".to_string(), agent_id.clone());
    server
        .llm_sidecar
        .agent_player_bindings
        .insert(agent_id.clone(), "player-missing".to_string());
    let missing = serde_json::to_value(
        server
            .compat_snapshot(Some("player-missing"))
            .player_gameplay
            .expect("player gameplay snapshot"),
    )
    .expect("serialize missing projection");
    assert!(
        missing.pointer("/primary_intent").is_none(),
        "missing canonical intent must remain missing"
    );

    server.llm_sidecar.player_agent_bindings.insert(
        "player-lost".to_string(),
        "agent-no-longer-present".to_string(),
    );
    server.llm_sidecar.agent_player_bindings.insert(
        "agent-no-longer-present".to_string(),
        "player-lost".to_string(),
    );
    let lost = serde_json::to_value(
        server
            .compat_snapshot(Some("player-lost"))
            .player_gameplay
            .expect("player gameplay snapshot"),
    )
    .expect("serialize lost-control projection");
    assert_eq!(
        lost.pointer("/primary_intent/status")
            .and_then(serde_json::Value::as_str),
        Some("unavailable")
    );
    assert_eq!(
        lost.pointer("/primary_intent/freshness")
            .and_then(serde_json::Value::as_str),
        Some("stale")
    );
    assert_eq!(
        lost.pointer("/primary_intent/control_state")
            .and_then(serde_json::Value::as_str),
        Some("control_lost")
    );
}

#[test]
fn runtime_agent_chat_requires_starter_oc_balance() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test holds the runtime LLM env lock while mutating process env.
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
    let (public_key, private_key) = test_signer(37);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello without OC".to_string(),
            intent_tick: Some(14),
            intent_seq: Some(37),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        37,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        36,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let err = server
        .handle_agent_chat(request)
        .expect_err("starter OC should be required before agent chat");
    assert_eq!(err.code, "starter_oc_required");
    assert_eq!(
        server.llm_sidecar.player_auth_last_nonce.get("player-a"),
        Some(&36)
    );
}

#[test]
fn runtime_agent_chat_allows_zero_balance_after_starter_oc_claim() {
    let _guard = lock_test_llm_env();
    // SAFETY: This test holds the runtime LLM env lock while mutating process env.
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
    let (public_key, private_key) = test_signer(39);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        38,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let starter_oc_request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.to_string(),
            target_agent_id: agent_id.clone(),
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        39,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_gameplay_action(starter_oc_request)
        .expect("starter OC claim accepted");
    server.world.step().expect("apply starter OC claim");
    assert!(
        server
            .world
            .state()
            .starter_oc_claims
            .contains_key(agent_id.as_str())
    );
    server
        .world
        .set_main_token_account_balance(agent_id.as_str(), 0, 0)
        .expect("simulate spent starter OC");

    let chat_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello after spent OC".to_string(),
            intent_tick: Some(40),
            intent_seq: Some(40),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        40,
        public_key.as_str(),
        private_key.as_str(),
    );
    let ack = server
        .handle_agent_chat(chat_request)
        .expect("claimed starter OC should keep chat gate open");
    assert_eq!(ack.agent_id, agent_id);
}
