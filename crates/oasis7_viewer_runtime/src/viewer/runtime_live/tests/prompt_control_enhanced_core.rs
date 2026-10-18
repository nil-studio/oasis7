use super::*;

#[test]
fn runtime_prompt_control_enhanced_apply_is_idempotent_per_authority_epoch() {
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
    let (public_key, private_key) = test_signer(41);
    let registration = register_runtime_session(
        &mut server,
        "player-enhanced",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let mut request = crate::viewer::PromptControlApplyRequest {
        agent_id: agent_id.clone(),
        player_id: "player-enhanced".to_string(),
        public_key: None,
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(0),
        updated_by: Some("player-enhanced".to_string()),
        system_prompt_override: Some(Some("enhanced system".to_string())),
        short_term_goal_override: None,
        long_term_goal_override: None,
        request_id: Some("enhanced-apply-1".to_string()),
        session_epoch: registration.session_epoch,
        binding_epoch: registration.binding_epoch,
        expected_authority_epoch: Some(authority_epoch.clone()),
    };
    request = signed_prompt_control_apply_request(
        request,
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let first = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: request.clone(),
            },
            &negotiated,
        )
        .expect("enhanced apply");
    assert_eq!(
        first.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(first.request_id.as_deref(), Some("enhanced-apply-1"));
    assert_eq!(
        first.authority_epoch.as_deref(),
        Some(authority_epoch.as_str())
    );
    assert_eq!(first.mutation_count, Some(1));

    let replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect("same signed request replays from result ledger");
    assert!(replay.idempotent_replay);
    assert_eq!(replay.version, first.version);
    assert!(server.pending_virtual_events.iter().any(|event| matches!(
        event.kind,
        oasis7_simulator::WorldEventKind::AgentPromptUpdated { .. }
    )));
}

#[test]
fn runtime_prompt_control_rejects_zero_result_limits_before_bootstrap() {
    let cache_error = match ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_prompt_result_cache_capacity(0),
    ) {
        Err(error) => error,
        Ok(_) => panic!("zero result cache capacity must fail closed"),
    };
    assert!(format!("{cache_error:?}").contains("cache capacity"));

    let receipt_error = match ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_prompt_result_receipt_max_bytes(0),
    ) {
        Err(error) => error,
        Ok(_) => panic!("zero receipt limit must fail closed"),
    };
    assert!(format!("{receipt_error:?}").contains("receipt max bytes"));
}

#[test]
fn runtime_prompt_control_enhanced_requires_negotiated_result_capability() {
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
    let request = crate::viewer::PromptControlApplyRequest {
        agent_id,
        player_id: "player-a".to_string(),
        request_id: Some("capability-required".to_string()),
        ..Default::default()
    };
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: Vec::new(),
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview { request },
            &negotiated,
        )
        .expect_err("enhanced request without capability must fail");
    assert_eq!(error.code, "prompt_control_capability_required");
}

#[test]
fn runtime_prompt_control_selected_capability_rejects_legacy_shaped_requests() {
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
    let (public_key, private_key) = test_signer(47);
    let _registration = register_runtime_session(
        &mut server,
        "player-shape",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let legacy_shaped = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-shape".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-shape".to_string()),
            system_prompt_override: Some(Some("must not apply".to_string())),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: legacy_shaped,
            },
            &negotiated,
        )
        .expect_err("selected enhanced capability must reject legacy shape");
    assert_eq!(error.code, "prompt_control_field_required");
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Rejected)
    );
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-shape"),
        Some(&1)
    );

    let partial = crate::viewer::PromptControlApplyRequest {
        agent_id,
        player_id: "player-shape".to_string(),
        request_id: Some("partial-shape".to_string()),
        expected_version: Some(0),
        ..Default::default()
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview { request: partial },
            &negotiated,
        )
        .expect_err("partial enhanced shape must fail before proof verification");
    assert_eq!(error.code, "prompt_control_field_required");
    assert_eq!(error.request_id.as_deref(), Some("partial-shape"));
}

#[test]
fn runtime_prompt_control_result_capability_is_not_advertised_in_script_mode() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "script-capability-probe".to_string(),
                version: crate::viewer::VIEWER_PROTOCOL_VERSION,
                capabilities: vec![
                    crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string(),
                ],
            },
            &mut session,
            &mut writer,
        )
        .expect("handle script v2 hello");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    let ViewerResponse::HelloAck {
        capabilities,
        authority_epoch,
        ..
    } = responses.first().expect("missing script hello ack")
    else {
        panic!("expected hello ack, got {responses:?}");
    };
    assert!(
        !capabilities
            .iter()
            .any(|capability| capability
                == crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY)
    );
    assert!(authority_epoch.is_none());
    assert!(
        !crate::viewer::protocol::viewer_protocol_supports_prompt_control_result(
            &session.negotiated_protocol
        )
    );
}

#[test]
fn runtime_prompt_control_enhanced_applied_then_stale_is_serialized() {
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
    let (public_key, private_key) = test_signer(42);
    let registration = register_runtime_session(
        &mut server,
        "player-serialized",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let make_request = |request_id: &str, nonce: u64, text: &str| {
        signed_prompt_control_apply_request(
            crate::viewer::PromptControlApplyRequest {
                agent_id: agent_id.clone(),
                player_id: "player-serialized".to_string(),
                expected_version: Some(0),
                updated_by: Some("player-serialized".to_string()),
                system_prompt_override: Some(Some(text.to_string())),
                request_id: Some(request_id.to_string()),
                session_epoch: registration.session_epoch,
                binding_epoch: registration.binding_epoch,
                expected_authority_epoch: Some(authority_epoch.clone()),
                ..Default::default()
            },
            crate::viewer::PromptControlAuthIntent::Apply,
            nonce,
            public_key.as_str(),
            private_key.as_str(),
        )
    };
    let first = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: make_request("serialized-a", 2, "first"),
            },
            &negotiated,
        )
        .expect("first operation applies");
    assert_eq!(
        first.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(
        first.applied_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::RuntimeInstance)
    );
    assert_eq!(
        first.persistence_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::None)
    );
    assert_eq!(
        first.sync_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::None)
    );

    let mut stale_request = make_request("serialized-b", 3, "second");
    stale_request.expected_version = Some(0);
    stale_request.auth = None;
    stale_request = signed_prompt_control_apply_request(
        stale_request,
        crate::viewer::PromptControlAuthIntent::Apply,
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    let stale = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: stale_request.clone(),
            },
            &negotiated,
        )
        .expect_err("second baseline request must become stale");
    assert_eq!(stale.code, "version_conflict");
    assert_eq!(
        stale.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Stale)
    );
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-serialized"),
        Some(&3)
    );
    let stale_replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: stale_request.clone(),
            },
            &negotiated,
        )
        .expect_err("same stale request must replay its terminal result");
    assert_eq!(stale_replay.code, "version_conflict");
    assert!(stale_replay.idempotent_replay);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-serialized"),
        Some(&3)
    );

    let mut conflict_request = stale_request;
    conflict_request.system_prompt_override = Some(Some("different operation".to_string()));
    conflict_request.auth = None;
    conflict_request = signed_prompt_control_apply_request(
        conflict_request,
        crate::viewer::PromptControlAuthIntent::Apply,
        4,
        public_key.as_str(),
        private_key.as_str(),
    );
    let conflict = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: conflict_request,
            },
            &negotiated,
        )
        .expect_err("same request id with a different digest must conflict");
    assert_eq!(conflict.code, "request_id_conflict");
    assert!(!conflict.idempotent_replay);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-serialized"),
        Some(&3)
    );
}

#[test]
fn runtime_prompt_control_hosted_control_loss_precedes_missing_grant() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_hosted_public_join_mode(true),
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
    let rebound_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .find(|candidate| *candidate != &agent_id)
        .cloned()
        .expect("second seed agent");
    let (public_key, private_key) = test_signer(48);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-loss",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-loss".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-loss".to_string()),
            system_prompt_override: Some(Some("lost control".to_string())),
            request_id: Some("hosted-control-loss".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(server.prompt_control_authority.authority_epoch.clone()),
            strong_auth_grant: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let rebound = register_runtime_session_with_options(
        &mut server,
        "player-hosted-loss",
        Some(rebound_agent_id.as_str()),
        true,
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(rebound.binding_epoch, Some(1));
    assert_eq!(
        server
            .prompt_control_authority
            .binding_epoch(agent_id.as_str()),
        registration.binding_epoch.unwrap_or_default() + 1
    );

    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("lost hosted control must be reported before grant status");
    assert_eq!(error.code, "control_lost");
    assert_eq!(error.reason_code.as_deref(), Some("control_lost"));
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert!(error.player_id.is_none());
    assert!(error.binding_epoch.is_none());
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_binding_loss_is_hidden_before_nonce_or_version() {
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
    let (public_key, private_key) = test_signer(43);
    let registration = register_runtime_session(
        &mut server,
        "player-binding-loss",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-binding-loss".to_string(),
            expected_version: Some(999),
            updated_by: Some("player-binding-loss".to_string()),
            system_prompt_override: Some(Some("should-not-apply".to_string())),
            request_id: Some("binding-loss".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let rebound = register_runtime_session_with_options(
        &mut server,
        "player-binding-loss",
        Some(agent_id.as_str()),
        true,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(rebound.binding_epoch, Some(2));
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("stale binding must block before expected version and nonce");
    assert_eq!(error.code, "control_lost");
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert!(error.player_id.is_none());
    assert!(error.binding_epoch.is_none());
    assert!(error.current_version.is_none());
    assert!(error.digest.is_none());
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-binding-loss"),
        Some(&2)
    );
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .map(|p| p.version),
        None
    );
}

#[test]
fn runtime_prompt_control_binding_loss_precedes_authority_and_version_mismatch() {
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
    let (public_key, private_key) = test_signer(49);
    let registration = register_runtime_session(
        &mut server,
        "player-binding-authority-race",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let old_authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-binding-authority-race".to_string(),
            // This must never be reached after binding loss.
            expected_version: Some(999),
            updated_by: Some("player-binding-authority-race".to_string()),
            system_prompt_override: Some(Some("must stay unchanged".to_string())),
            request_id: Some("binding-before-authority-race".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(old_authority_epoch),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );

    let rebound = register_runtime_session_with_options(
        &mut server,
        "player-binding-authority-race",
        Some(agent_id.as_str()),
        true,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        rebound.binding_epoch,
        Some(registration.binding_epoch.unwrap_or_default() + 1)
    );
    // Simulate the same request crossing a runtime-authority fence after the
    // binding was revoked/rebound. Binding loss must still win the redaction
    // ordering, rather than being hidden as result_unknown.
    server.prompt_control_authority.authority_epoch = "authority-after-binding-loss".to_string();

    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("binding loss must precede authority and version mismatch");
    assert_eq!(error.code, "control_lost");
    assert_eq!(error.reason_code.as_deref(), Some("control_lost"));
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert_eq!(error.agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(error.player_id.is_none());
    assert!(error.current_version.is_none());
    assert!(error.digest.is_none());
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-binding-authority-race"),
        Some(&2),
        "rebind registration nonce is retained; rejected prompt must not consume nonce 3"
    );
    assert_eq!(server.prompt_control_authority.result_ledger.len(), 0);
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
}

#[test]
fn runtime_prompt_control_conflicting_rebind_precedes_authority_and_version_mismatch() {
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
    let (player_a_public_key, player_a_private_key) = test_signer(52);
    let player_a_registration = register_runtime_session(
        &mut server,
        "player-conflict-a",
        Some(agent_id.as_str()),
        1,
        player_a_public_key.as_str(),
        player_a_private_key.as_str(),
    );
    let old_authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-conflict-a".to_string(),
            expected_version: Some(999),
            updated_by: Some("player-conflict-a".to_string()),
            system_prompt_override: Some(Some(
                "must not apply after conflicting rebind".to_string(),
            )),
            request_id: Some("conflicting-rebind-before-authority-race".to_string()),
            session_epoch: player_a_registration.session_epoch,
            binding_epoch: player_a_registration.binding_epoch,
            expected_authority_epoch: Some(old_authority_epoch),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        player_a_public_key.as_str(),
        player_a_private_key.as_str(),
    );

    let (player_b_public_key, player_b_private_key) = test_signer(53);
    let player_b_registration = register_runtime_session_with_options(
        &mut server,
        "player-conflict-b",
        Some(agent_id.as_str()),
        true,
        1,
        player_b_public_key.as_str(),
        player_b_private_key.as_str(),
    );
    assert_eq!(
        player_b_registration.binding_epoch,
        Some(player_a_registration.binding_epoch.unwrap_or_default() + 1)
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get(agent_id.as_str()),
        Some(&"player-conflict-b".to_string())
    );
    let pending_events_before_request = server.pending_virtual_events.len();
    server.prompt_control_authority.authority_epoch =
        "authority-after-conflicting-rebind".to_string();

    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("conflicting rebind must precede authority and version mismatch");
    assert_eq!(error.code, "control_lost");
    assert_eq!(error.reason_code.as_deref(), Some("control_lost"));
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert_eq!(error.agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(error.player_id.is_none());
    assert!(error.expected_version.is_none());
    assert!(error.current_version.is_none());
    assert!(error.digest.is_none());
    assert!(error.operation_digest.is_none());
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-conflict-a"),
        Some(&1),
        "conflicting rebind rejection must not consume player A nonce 2"
    );
    assert_eq!(
        server.pending_virtual_events.len(),
        pending_events_before_request
    );
    assert_eq!(server.prompt_control_authority.result_ledger.len(), 0);
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
}

#[test]
fn runtime_prompt_control_session_loss_precedes_authority_and_version_mismatch() {
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
    let (public_key, private_key) = test_signer(50);
    let registration = register_runtime_session(
        &mut server,
        "player-session-authority-race",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let old_authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-session-authority-race".to_string(),
            expected_version: Some(999),
            updated_by: Some("player-session-authority-race".to_string()),
            system_prompt_override: Some(Some("must stay unchanged".to_string())),
            request_id: Some("session-before-authority-race".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(old_authority_epoch),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );

    let (revoke_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-session-authority-race".to_string(),
                session_pubkey: Some(public_key.clone()),
                revoke_reason: "authority-ordering-test".to_string(),
                revoked_by: Some("runtime-test".to_string()),
            },
        })
        .expect("revoke session");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        revoke_ack.status,
        AuthoritativeRecoveryStatus::SessionRevoked
    );
    server.prompt_control_authority.authority_epoch = "authority-after-session-loss".to_string();

    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("session loss must precede authority and version mismatch");
    assert_eq!(error.code, "control_lost");
    assert_eq!(error.reason_code.as_deref(), Some("control_lost"));
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert_eq!(error.agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(error.player_id.is_none());
    assert!(error.current_version.is_none());
    assert!(error.digest.is_none());
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
    assert_eq!(server.prompt_control_authority.result_ledger.len(), 0);
}

#[test]
fn runtime_prompt_control_restart_fences_old_authority_result() {
    let _guard = lock_test_llm_env();
    let mut old_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("old runtime server");
    let agent_id = old_server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(44);
    let registration = register_runtime_session(
        &mut old_server,
        "player-restart",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let old_authority_epoch = old_server.prompt_control_authority.authority_epoch.clone();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-restart".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-restart".to_string()),
            system_prompt_override: Some(Some("old-authority".to_string())),
            request_id: Some("restart-retry".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(old_authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut new_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("new runtime server");
    assert_ne!(
        new_server.prompt_control_authority.authority_epoch,
        old_authority_epoch
    );
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = new_server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("old authority retry must be fenced");
    assert_eq!(error.code, "prompt_control_result_unknown");
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert!(error.agent_id.is_none());
    assert!(error.player_id.is_none());
    assert!(error.operation_digest.is_none());
    assert!(error.expected_version.is_none());
    assert!(new_server.pending_virtual_events.is_empty());
}

#[test]
fn runtime_prompt_control_cache_and_receipt_limits_refuse_before_nonce() {
    let _guard = lock_test_llm_env();
    let mut full_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_prompt_result_cache_capacity(1),
    )
    .expect("runtime server");
    let agent_id = full_server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(45);
    let registration = register_runtime_session(
        &mut full_server,
        "player-cache",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = full_server.prompt_control_authority.authority_epoch.clone();
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let make_request = |request_id: &str, nonce: u64, expected_version: u64| {
        signed_prompt_control_apply_request(
            crate::viewer::PromptControlApplyRequest {
                agent_id: agent_id.clone(),
                player_id: "player-cache".to_string(),
                expected_version: Some(expected_version),
                updated_by: Some("player-cache".to_string()),
                system_prompt_override: Some(Some(request_id.to_string())),
                request_id: Some(request_id.to_string()),
                session_epoch: registration.session_epoch,
                binding_epoch: registration.binding_epoch,
                expected_authority_epoch: Some(authority_epoch.clone()),
                ..Default::default()
            },
            crate::viewer::PromptControlAuthIntent::Apply,
            nonce,
            public_key.as_str(),
            private_key.as_str(),
        )
    };
    full_server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: make_request("cache-first", 2, 0),
            },
            &negotiated,
        )
        .expect("first cache entry");
    let full_error = full_server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: make_request("cache-second", 3, 1),
            },
            &negotiated,
        )
        .expect_err("new request must be blocked when ledger is full");
    assert_eq!(full_error.code, "result_cache_full");
    assert_eq!(
        full_error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        full_server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-cache"),
        Some(&2)
    );
    assert_eq!(
        full_server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .map(|p| p.version),
        Some(1)
    );
    assert_eq!(
        full_error.next_step.as_deref(),
        Some("refresh_authority_and_retry_with_new_request_id")
    );

    let mut receipt_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_prompt_result_receipt_max_bytes(1),
    )
    .expect("runtime server");
    let receipt_agent_id = receipt_server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (receipt_public_key, receipt_private_key) = test_signer(46);
    let receipt_registration = register_runtime_session(
        &mut receipt_server,
        "player-receipt",
        Some(receipt_agent_id.as_str()),
        1,
        receipt_public_key.as_str(),
        receipt_private_key.as_str(),
    );
    let receipt_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: receipt_agent_id.clone(),
            player_id: "player-receipt".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-receipt".to_string()),
            system_prompt_override: Some(Some("oversize".to_string())),
            request_id: Some("receipt-oversize".to_string()),
            session_epoch: receipt_registration.session_epoch,
            binding_epoch: receipt_registration.binding_epoch,
            expected_authority_epoch: Some(
                receipt_server
                    .prompt_control_authority
                    .authority_epoch
                    .clone(),
            ),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        receipt_public_key.as_str(),
        receipt_private_key.as_str(),
    );
    let receipt_error = receipt_server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: receipt_request,
            },
            &negotiated,
        )
        .expect_err("oversize receipt must be refused");
    assert_eq!(receipt_error.code, "result_receipt_too_large");
    assert_eq!(
        receipt_error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Rejected)
    );
    assert_eq!(
        receipt_server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-receipt"),
        Some(&1)
    );
    assert_eq!(
        receipt_server
            .llm_sidecar
            .prompt_profiles
            .get(receipt_agent_id.as_str())
            .map(|p| p.version),
        None
    );
}
