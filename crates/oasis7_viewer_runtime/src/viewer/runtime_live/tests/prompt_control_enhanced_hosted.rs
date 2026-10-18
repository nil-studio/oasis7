use super::*;

#[cfg(not(target_arch = "wasm32"))]
fn hosted_local_mock_chain_empty_runtime_server() -> (ViewerRuntimeLiveServer, TestChainStatusServer)
{
    let execution_world_dir = runtime_live_temp_dir("prompt_control_hosted_auth_boundary");
    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    let server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_hosted_public_join_mode(true)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("empty chain-linked Hosted local-mock runtime server");
    assert!(server.world.state().agents.is_empty());
    assert!(server.world.capability_invocation_contexts().is_empty());
    assert!(server.hosted_local_mock_test_lane_active);
    assert!(server.llm_sidecar.supports_prompt_control_result());
    (server, chain_status)
}

#[cfg(not(target_arch = "wasm32"))]
fn hosted_prompt_control_runtime_state(
    server: &ViewerRuntimeLiveServer,
    agent_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "capability_contexts": server.world.capability_invocation_contexts(),
        "provider_contexts_empty": server.llm_sidecar.provider_contexts_empty(),
        "lease": server.llm_sidecar.provider_cognition_lease(agent_id),
        "lease_agents_in_flight": server
            .llm_sidecar
            .provider_cognition_lease_agents_in_flight(&server.world),
        "feedback_outbox": server
            .world
            .runtime_feedback_outbox()
            .expect("read runtime feedback outbox"),
        "wait_recovery_agent": server.llm_sidecar.provider_wait_recovery_agent(),
        "wait_recovery_requires_attention": server
            .llm_sidecar
            .provider_wait_recovery_requires_attention(),
        "cognition_continuations": server.world.cognition_continuations(),
        "prompt_profiles": server.llm_sidecar.prompt_profiles,
        "result_ledger_len": server.prompt_control_authority.result_ledger.len(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn hosted_prompt_control_full_mutation_fingerprint(
    server: &ViewerRuntimeLiveServer,
    agent_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "runtime_state": hosted_prompt_control_runtime_state(server, agent_id),
        "agent_player_bindings": server.llm_sidecar.agent_player_bindings,
        "player_auth_last_nonce": server.llm_sidecar.player_auth_last_nonce,
        "binding_epoch_by_agent": server.prompt_control_authority.binding_epoch_by_agent,
        "pending_virtual_events": format!("{:?}", server.pending_virtual_events),
        "session_revoke_metadata": format!("{:?}", server.session_revoke_metadata),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn hosted_prompt_control_negotiated_protocol() -> crate::viewer::protocol::NegotiatedViewerProtocol
{
    crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn tamper_prompt_control_signature(auth: &mut crate::viewer::protocol::PlayerAuthProof) {
    auth.signature.push('x');
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_chain_linked_bad_signature_apply_and_rollback_do_not_prepare_runtime_state()
 {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");

    let (mut server, _chain_status) = hosted_local_mock_chain_empty_runtime_server();
    let agent_id = "deferred-chain-agent";
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let (public_key, private_key) = test_signer(101);
    let mut apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.to_string(),
            player_id: "player-hosted-bad-signature".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-bad-signature".to_string()),
            system_prompt_override: Some(Some("must not prepare".to_string())),
            request_id: Some("hosted-bad-signature-apply".to_string()),
            session_epoch: Some(1),
            binding_epoch: Some(0),
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    tamper_prompt_control_signature(apply_request.auth.as_mut().expect("Apply auth proof"));

    let mut rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.to_string(),
            player_id: "player-hosted-bad-signature".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-bad-signature".to_string()),
            to_version: 0,
            request_id: Some("hosted-bad-signature-rollback".to_string()),
            session_epoch: Some(1),
            binding_epoch: Some(0),
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    tamper_prompt_control_signature(rollback_request.auth.as_mut().expect("Rollback auth proof"));

    let state_before = hosted_prompt_control_runtime_state(&server, agent_id);
    let mut session = RuntimeLiveSession::new();
    session.negotiated_protocol = hosted_prompt_control_negotiated_protocol();
    let (mut writer, peer) = test_writer_pair();

    for command in [
        crate::viewer::PromptControlCommand::Apply {
            request: apply_request,
        },
        crate::viewer::PromptControlCommand::Rollback {
            request: rollback_request,
        },
    ] {
        server
            .handle_request(
                ViewerRequest::PromptControl {
                    command: Box::new(command),
                },
                &mut session,
                &mut writer,
            )
            .expect("bad signature should return a PromptControlError response");
        let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
        assert!(matches!(
            responses.as_slice(),
            [ViewerResponse::PromptControlError { error }] if error.code == "auth_invalid"
        ));
        assert_eq!(
            hosted_prompt_control_runtime_state(&server, agent_id),
            state_before,
            "bad signature must not prepare or settle Hosted local-mock runtime state"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_chain_linked_revoked_apply_and_rollback_do_not_prepare_runtime_state()
 {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");

    let (mut server, _chain_status) = hosted_local_mock_chain_empty_runtime_server();
    let agent_id = "deferred-chain-agent";
    let (public_key, private_key) = test_signer(102);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-revoked",
        None,
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-hosted-revoked".to_string(),
                session_pubkey: Some(public_key.clone()),
                revoke_reason: "prompt-control-revoked-no-prep-test".to_string(),
                revoked_by: Some("runtime-test".to_string()),
            },
        })
        .expect("revoke session");

    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.to_string(),
            player_id: "player-hosted-revoked".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-revoked".to_string()),
            system_prompt_override: Some(Some("must not prepare after revoke".to_string())),
            request_id: Some("hosted-revoked-apply".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: Some(0),
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.to_string(),
            player_id: "player-hosted-revoked".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-revoked".to_string()),
            to_version: 0,
            request_id: Some("hosted-revoked-rollback".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: Some(0),
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        3,
        public_key.as_str(),
        private_key.as_str(),
    );

    let state_before = hosted_prompt_control_runtime_state(&server, agent_id);
    let mut session = RuntimeLiveSession::new();
    session.negotiated_protocol = hosted_prompt_control_negotiated_protocol();
    let (mut writer, peer) = test_writer_pair();

    for command in [
        crate::viewer::PromptControlCommand::Apply {
            request: apply_request,
        },
        crate::viewer::PromptControlCommand::Rollback {
            request: rollback_request,
        },
    ] {
        server
            .handle_request(
                ViewerRequest::PromptControl {
                    command: Box::new(command),
                },
                &mut session,
                &mut writer,
            )
            .expect("revoked request should return a PromptControlError response");
        let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
        assert!(matches!(
            responses.as_slice(),
            [ViewerResponse::PromptControlError { error }] if error.code == "control_lost"
        ));
        assert_eq!(
            hosted_prompt_control_runtime_state(&server, agent_id),
            state_before,
            "revoked PromptControl must not prepare or settle Hosted local-mock runtime state"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_expired_grant_has_full_no_mutation_fingerprint() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let (backend_public_key, backend_private_key) = test_signer(115);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = hosted_local_mock_runtime_server();
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(116);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-expired-fingerprint",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let mut request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-expired-fingerprint".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-expired-fingerprint".to_string()),
            system_prompt_override: Some(Some("must not prepare after expiry".to_string())),
            request_id: Some("hosted-expired-fingerprint".to_string()),
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
    let now = test_now_unix_ms();
    request.strong_auth_grant = Some(
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            "prompt_control_apply",
            "player-hosted-expired-fingerprint",
            public_key.as_str(),
            agent_id.as_str(),
            now.saturating_sub(10_000),
            now.saturating_sub(1_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("sign expired Hosted strong-auth grant"),
    );

    let before = hosted_prompt_control_full_mutation_fingerprint(&server, agent_id.as_str());
    let negotiated = hosted_prompt_control_negotiated_protocol();
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply { request },
            &negotiated,
        )
        .expect_err("expired Hosted grant must be rejected before preparation");
    assert_eq!(error.code, "auth_invalid");
    assert_eq!(error.reason_code.as_deref(), Some("auth_invalid"));
    assert_eq!(
        error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert_eq!(
        error.value_visibility,
        Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
    );
    assert_eq!(
        hosted_prompt_control_full_mutation_fingerprint(&server, agent_id.as_str()),
        before,
        "expired grant must not consume nonce, install Hosted fixtures, enqueue events, or cache a result"
    );
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_hosted_local_mock_invalid_auth_does_not_prepare_runtime_state() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");

    let mut server = hosted_local_mock_runtime_server();
    // Model the chain-linked Hosted local-mock lane after authoritative world
    // sync. This is the state in which PromptControl's lazy preparation path
    // is active for a headed client.
    server.hosted_local_mock_test_lane_active = true;
    server.llm_sidecar.enable_hosted_local_mock_test_lane();
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let capability_contexts_before = server.world.capability_invocation_contexts().clone();
    let provider_contexts_empty_before = server.llm_sidecar.provider_contexts_empty();
    assert!(provider_contexts_empty_before);
    let profile_before = server.llm_sidecar.prompt_profiles.clone();
    let ledger_before = server.prompt_control_authority.result_ledger.len();

    let mut session = RuntimeLiveSession::new();
    session.negotiated_protocol = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::PromptControl {
                command: Box::new(crate::viewer::PromptControlCommand::Apply {
                    request: crate::viewer::PromptControlApplyRequest {
                        agent_id: agent_id.to_string(),
                        player_id: "player-hosted-invalid-auth".to_string(),
                        expected_version: Some(0),
                        updated_by: Some("player-hosted-invalid-auth".to_string()),
                        system_prompt_override: Some(Some("must not apply".to_string())),
                        request_id: Some("hosted-invalid-auth-no-prep".to_string()),
                        session_epoch: Some(1),
                        binding_epoch: Some(0),
                        expected_authority_epoch: Some(
                            server.prompt_control_authority.authority_epoch.clone(),
                        ),
                        ..Default::default()
                    },
                }),
            },
            &mut session,
            &mut writer,
        )
        .expect("invalid auth should return a PromptControlError response");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    match responses.as_slice() {
        [ViewerResponse::PromptControlError { error }] => {
            assert_eq!(error.code, "auth_required", "unexpected error: {error:?}");
        }
        other => panic!("expected auth error, got {other:?}"),
    }
    drop(_strong_auth_guard);
    drop(_llm_guard);

    assert_eq!(
        server.world.capability_invocation_contexts(),
        &capability_contexts_before,
        "invalid PromptControl must not install Hosted local-mock fixtures before auth"
    );
    assert_eq!(server.llm_sidecar.prompt_profiles, profile_before);
    assert_eq!(
        server.llm_sidecar.provider_contexts_empty(),
        provider_contexts_empty_before,
        "invalid PromptControl must not prepare a provider context before auth"
    );
    assert_eq!(
        server.prompt_control_authority.result_ledger.len(),
        ledger_before
    );
}

#[test]
fn runtime_prompt_control_hosted_local_mock_auth_lost_does_not_prepare_runtime_state() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");

    let mut server = hosted_local_mock_runtime_server();
    // Model the chain-linked Hosted local-mock lane after authoritative world
    // sync. This is the state in which PromptControl's lazy preparation path
    // is active for a headed client.
    server.hosted_local_mock_test_lane_active = true;
    server.llm_sidecar.enable_hosted_local_mock_test_lane();
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(96);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-auth-lost",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let (_, emit_snapshot_after_revoke) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-hosted-auth-lost".to_string(),
                session_pubkey: Some(public_key.clone()),
                revoke_reason: "prompt-control-auth-loss-test".to_string(),
                revoked_by: Some("runtime-test".to_string()),
            },
        })
        .expect("revoke session");
    assert!(!emit_snapshot_after_revoke);

    let capability_contexts_before = server.world.capability_invocation_contexts().clone();
    let provider_contexts_empty_before = server.llm_sidecar.provider_contexts_empty();
    assert!(provider_contexts_empty_before);
    let profile_before = server.llm_sidecar.prompt_profiles.clone();
    let ledger_before = server.prompt_control_authority.result_ledger.len();
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.to_string(),
            player_id: "player-hosted-auth-lost".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-auth-lost".to_string()),
            system_prompt_override: Some(Some("must not apply after revoke".to_string())),
            request_id: Some("hosted-auth-lost-no-prep".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(server.prompt_control_authority.authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );

    let mut session = RuntimeLiveSession::new();
    session.negotiated_protocol = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::PromptControl {
                command: Box::new(crate::viewer::PromptControlCommand::Apply { request }),
            },
            &mut session,
            &mut writer,
        )
        .expect("auth-lost request should return a PromptControlError response");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    assert!(matches!(
        responses.as_slice(),
        [ViewerResponse::PromptControlError { error }] if error.code == "control_lost"
    ));
    drop(_strong_auth_guard);
    drop(_llm_guard);

    assert_eq!(
        server.world.capability_invocation_contexts(),
        &capability_contexts_before,
        "auth-lost PromptControl must not install Hosted local-mock fixtures before auth"
    );
    assert_eq!(server.llm_sidecar.prompt_profiles, profile_before);
    assert_eq!(
        server.llm_sidecar.provider_contexts_empty(),
        provider_contexts_empty_before,
        "auth-lost PromptControl must not prepare a provider context before auth"
    );
    assert_eq!(
        server.prompt_control_authority.result_ledger.len(),
        ledger_before
    );
}

#[test]
fn runtime_prompt_control_hosted_local_mock_preview_apply_and_rollback_use_result_capability() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let (backend_public_key, backend_private_key) = test_signer(90);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = hosted_local_mock_runtime_server();
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
        .install_test_provider_capability_fixture(agent_id.as_str())
        .expect("install proof-bearing local-mock capability context");
    prime_hosted_local_mock_provider_context(&mut server, agent_id.as_str());
    assert!(!server.world.capability_invocation_contexts().is_empty());

    let (public_key, private_key) = test_signer(91);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-local-mock-paths",
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
    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let strong_auth_grant = |operation: &str| {
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            operation,
            "player-hosted-local-mock-paths",
            public_key.as_str(),
            agent_id.as_str(),
            issued_at_unix_ms,
            issued_at_unix_ms.saturating_add(60_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("sign Hosted local-mock strong-auth grant")
    };

    let mut preview_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-paths".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-local-mock-paths".to_string()),
            system_prompt_override: Some(Some("preview-local-mock".to_string())),
            request_id: Some("hosted-local-mock-preview".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Preview,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    preview_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_preview"));
    let preview = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview {
                request: preview_request,
            },
            &negotiated,
        )
        .expect("Hosted local-mock Preview request must reach enhanced handler");
    assert_eq!(
        preview.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Accepted)
    );

    let mut apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-paths".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-local-mock-paths".to_string()),
            system_prompt_override: Some(Some("apply-local-mock".to_string())),
            short_term_goal_override: Some(Some("apply-local-mock-goal".to_string())),
            request_id: Some("hosted-local-mock-apply".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    apply_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_apply"));
    let apply = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: apply_request,
            },
            &negotiated,
        )
        .expect("Hosted local-mock Apply request must reach enhanced handler");
    assert_eq!(
        apply.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(apply.version, 1);
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .and_then(|profile| profile.short_term_goal_override.as_deref()),
        Some("apply-local-mock-goal")
    );
    prime_hosted_local_mock_provider_context(&mut server, agent_id.as_str());
    assert_eq!(
        server
            .llm_sidecar
            .provider_test_goal_summary(agent_id.as_str())
            .as_deref(),
        Some("apply-local-mock-goal")
    );

    let mut rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-paths".to_string(),
            expected_version: Some(1),
            updated_by: Some("player-hosted-local-mock-paths".to_string()),
            to_version: 0,
            request_id: Some("hosted-local-mock-rollback".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        4,
        public_key.as_str(),
        private_key.as_str(),
    );
    rollback_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_rollback"));
    let rollback = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: rollback_request,
            },
            &negotiated,
        )
        .expect("Hosted local-mock Rollback request must reach enhanced handler");
    assert_eq!(
        rollback.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(rollback.rolled_back_to_version, Some(0));
}
