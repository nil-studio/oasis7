use super::*;

#[test]
fn runtime_prompt_control_script_mode_requires_llm_mode() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
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
    let (public_key, private_key) = test_signer(11);
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect_err("script mode should reject prompt control");
    assert_eq!(err.code, "llm_mode_required");
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
}

#[test]
fn runtime_prompt_control_hosted_public_join_requires_strong_auth() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
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
    let (public_key, private_key) = test_signer(27);
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        27,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect_err("hosted public join should require strong auth");
    assert_eq!(err.code, "strong_auth_required");
    assert!(
        err.message
            .contains("prompt_control requires hosted strong auth")
    );
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
}

#[test]
fn runtime_prompt_control_hosted_public_join_accepts_valid_backend_grant() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let (backend_public_key, backend_private_key) = test_signer(28);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
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
    let (player_public_key, player_private_key) = test_signer(29);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let mut request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: Some("player-a".to_string()),
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: Some(Some("goal".to_string())),
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let grant = crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        agent_id.as_str(),
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(60_000),
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("backend strong-auth grant");
    request.strong_auth_grant = Some(grant);

    let ack = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect("hosted apply with backend grant");
    assert_eq!(ack.version, 1);
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_hosted_public_join_rejects_expired_backend_grant() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let (backend_public_key, backend_private_key) = test_signer(30);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
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
    let (player_public_key, player_private_key) = test_signer(31);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let mut request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(10_000);
    let grant = crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        agent_id.as_str(),
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(1_000),
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("backend strong-auth grant");
    request.strong_auth_grant = Some(grant);

    let err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect_err("expired hosted strong-auth grant must fail");
    assert_eq!(err.code, "strong_auth_grant_invalid");
    assert!(err.message.contains("expired"));
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_hosted_public_join_rejects_replayed_auth_nonce_even_with_valid_grant() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let (backend_public_key, backend_private_key) = test_signer(32);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
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
    let (player_public_key, player_private_key) = test_signer(33);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let grant = crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        agent_id.as_str(),
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(60_000),
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("backend strong-auth grant");
    let build_request = || {
        signed_prompt_control_apply_request(
            crate::viewer::PromptControlApplyRequest {
                agent_id: agent_id.clone(),
                player_id: "player-a".to_string(),
                public_key: None,
                auth: None,
                strong_auth_grant: Some(grant.clone()),
                expected_version: Some(0),
                updated_by: Some("player-a".to_string()),
                system_prompt_override: Some(Some("system".to_string())),
                short_term_goal_override: Some(Some("goal".to_string())),
                long_term_goal_override: None,
                ..Default::default()
            },
            crate::viewer::PromptControlAuthIntent::Apply,
            2,
            player_public_key.as_str(),
            player_private_key.as_str(),
        )
    };

    let first_ack = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: build_request(),
        })
        .expect("first hosted apply should succeed");
    assert_eq!(first_ack.version, 1);

    let replay_err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: build_request(),
        })
        .expect_err("replayed nonce should fail even with valid hosted grant");
    assert_eq!(replay_err.code, "auth_nonce_replay");
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_hosted_public_join_rejects_revoked_session_even_with_valid_grant() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    let (backend_public_key, backend_private_key) = test_signer(34);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV,
            backend_public_key.as_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
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
    let (player_public_key, player_private_key) = test_signer(35);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let grant = crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        agent_id.as_str(),
        issued_at_unix_ms,
        issued_at_unix_ms.saturating_add(60_000),
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("backend strong-auth grant");

    let _ = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-a".to_string(),
                session_pubkey: Some(player_public_key.clone()),
                revoke_reason: "abuse-drill".to_string(),
                revoked_by: Some("qa".to_string()),
            },
        })
        .expect("revoke session");

    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: Some(grant),
            expected_version: Some(0),
            updated_by: Some("player-a".to_string()),
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: Some(Some("goal".to_string())),
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        player_public_key.as_str(),
        player_private_key.as_str(),
    );

    let err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect_err("revoked session should fail even with valid hosted grant");
    assert_eq!(err.code, "session_revoked");
    clear_hosted_strong_auth_env();
}

#[test]
fn runtime_prompt_control_provider_mode_reports_unsupported() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:5841");
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
    let (public_key, private_key) = test_signer(31);
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        31,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect_err("provider mode should reject prompt control");
    assert_eq!(err.code, "agent_provider_prompt_control_unsupported");
    clear_runtime_provider_env();
}

#[test]
fn runtime_prompt_control_apply_updates_snapshot_and_bindings() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
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
    let (public_key, private_key) = test_signer(12);
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let ack = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply { request })
        .expect("llm mode apply");
    assert_eq!(ack.version, 1);
    let feedback = server
        .scoped_feedback_for_player(Some("player-a"))
        .expect("prompt-control feedback recorded");
    assert_eq!(feedback.action, "prompt_control.apply");
    assert_eq!(feedback.stage, "completed_advanced");
    assert_eq!(feedback.target_agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(
        feedback
            .intent_summary
            .as_deref()
            .is_some_and(|summary| summary.contains(agent_id.as_str()))
    );
    let snapshot = server.compat_snapshot(None);
    let profile = snapshot
        .model
        .agent_prompt_profiles
        .get(agent_id.as_str())
        .expect("profile in snapshot");
    assert_eq!(profile.version, 1);
    assert_eq!(
        snapshot
            .model
            .agent_player_bindings
            .get(agent_id.as_str())
            .map(String::as_str),
        Some("player-a")
    );
    assert_eq!(
        snapshot
            .model
            .player_auth_last_nonce
            .get("player-a")
            .copied(),
        Some(2)
    );
}

#[test]
fn runtime_prompt_control_keeps_the_canonical_primary_intent_truth() {
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
    let (public_key, private_key) = test_signer(94);
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );

    let apply_goal = |expected_version, nonce, short_term_goal_override| {
        signed_prompt_control_apply_request(
            crate::viewer::PromptControlApplyRequest {
                agent_id: agent_id.clone(),
                player_id: "player-a".to_string(),
                public_key: None,
                auth: None,
                strong_auth_grant: None,
                expected_version,
                updated_by: None,
                system_prompt_override: None,
                short_term_goal_override,
                long_term_goal_override: None,
                ..Default::default()
            },
            crate::viewer::PromptControlAuthIntent::Apply,
            nonce,
            public_key.as_str(),
            private_key.as_str(),
        )
    };
    let primary_intent = |server: &mut ViewerRuntimeLiveServer| {
        serde_json::to_value(
            server
                .compat_snapshot(Some("player-a"))
                .player_gameplay
                .expect("bound player gameplay snapshot"),
        )
        .expect("serialize gameplay snapshot")
    };

    server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: apply_goal(
                Some(0),
                2,
                Some(Some("Keep the iron line stable.".to_string())),
            ),
        })
        .expect("first goal accepted");
    server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: apply_goal(
                Some(1),
                3,
                Some(Some("Keep the iron line stable.".to_string())),
            ),
        })
        .expect("same goal is an accepted unchanged handoff");
    let unchanged = primary_intent(&mut server);
    assert_eq!(
        unchanged
            .pointer("/primary_intent/status")
            .and_then(serde_json::Value::as_str),
        Some("accepted")
    );
    assert_eq!(
        unchanged
            .pointer("/primary_intent/message")
            .and_then(serde_json::Value::as_str),
        Some("Keep the iron line stable.")
    );

    server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: apply_goal(Some(1), 4, Some(None)),
        })
        .expect("clearing the shared goal requires resume");
    let resume_required = primary_intent(&mut server);
    assert_eq!(
        resume_required
            .pointer("/primary_intent/status")
            .and_then(serde_json::Value::as_str),
        Some("blocked")
    );
    assert_eq!(
        resume_required
            .pointer("/primary_intent/message")
            .and_then(serde_json::Value::as_str),
        Some("Keep the iron line stable.")
    );
    assert_eq!(
        resume_required
            .pointer("/primary_intent/resume_required")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let rejected = server
        .handle_prompt_control(crate::viewer::PromptControlCommand::Apply {
            request: apply_goal(
                Some(999),
                5,
                Some(Some("Expand immediately despite the outage.".to_string())),
            ),
        })
        .expect_err("stale version must reject without replacing the handoff");
    assert_eq!(rejected.code, "version_conflict");
    assert_eq!(primary_intent(&mut server), resume_required);
}
