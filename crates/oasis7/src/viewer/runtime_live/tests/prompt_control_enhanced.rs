use super::*;

#[path = "prompt_control_enhanced_core.rs"]
mod prompt_control_enhanced_core;
#[path = "prompt_control_enhanced_hosted.rs"]
mod prompt_control_enhanced_hosted;
#[path = "prompt_control_enhanced_preview.rs"]
mod prompt_control_enhanced_preview_tests;
#[path = "prompt_control_enhanced_race.rs"]
mod prompt_control_enhanced_race;

fn configure_hosted_local_mock_provider_env(contract: &str) {
    // SAFETY: Callers hold the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, contract);
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
}

fn hosted_local_mock_runtime_server() -> ViewerRuntimeLiveServer {
    ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_hosted_public_join_mode(true)
            .with_test_cognition_runtime_binding(
                "hosted-local-mock-branch",
                0,
                Some(
                    crate::simulator::h_v1(
                        "oasis7.viewer.test.finality-block.v1",
                        &"live-runtime-minimal",
                    )
                    .to_string(),
                ),
                "verified",
                0,
            ),
    )
    .expect("Hosted local-mock runtime server")
}

#[cfg(not(target_arch = "wasm32"))]
fn prime_hosted_local_mock_provider_context(server: &mut ViewerRuntimeLiveServer, agent_id: &str) {
    let world_id = server
        .world
        .current_cognition_runtime_binding()
        .expect("Hosted local-mock Runtime binding")
        .world_id
        .clone();
    server
        .llm_sidecar
        .prepare_provider_backed_test_context(
            &mut server.world,
            &WorldConfig::default(),
            world_id.as_str(),
            agent_id,
        )
        .expect("prepare proof-bearing Hosted local-mock provider context");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_provider_requires_runtime_context() {
    let _llm_guard = lock_test_llm_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.enable_hosted_local_mock_test_lane();
    sidecar
        .replace_provider_backed_runner_for_test("hosted-local-mock-agent")
        .expect("ProviderBacked test actor");
    let error = sidecar
        .apply_prompt_profile_to_driver(&oasis7_simulator::AgentPromptProfile {
            agent_id: "hosted-local-mock-agent".to_string(),
            short_term_goal_override: Some("must not project".to_string()),
            ..Default::default()
        })
        .expect_err("missing Runtime context must deny ProviderBacked prompt control");
    assert!(error.contains("without an admitted Hosted local-mock Runtime context"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_provider_rejects_rotated_runtime_binding() {
    let _llm_guard = lock_test_llm_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
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

    let mut rotated_binding = server
        .world
        .current_cognition_runtime_binding()
        .expect("current Runtime binding")
        .clone();
    rotated_binding.branch_id.push_str("-rotated");
    server
        .llm_sidecar
        .set_provider_test_binding(rotated_binding);
    let error = server
        .llm_sidecar
        .apply_prompt_profile_to_driver(&oasis7_simulator::AgentPromptProfile {
            agent_id,
            short_term_goal_override: Some("must not cross Runtime heads".to_string()),
            ..Default::default()
        })
        .expect_err("a context bound to the prior Runtime head must be denied");
    assert!(error.contains("without an admitted Hosted local-mock Runtime context"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_provider_refreshes_after_runtime_rebind() {
    let _llm_guard = lock_test_llm_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
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
        .expect("install initial proof-bearing local-mock capability context");
    prime_hosted_local_mock_provider_context(&mut server, agent_id.as_str());

    let old_binding = server
        .world
        .current_cognition_runtime_binding()
        .expect("initial Runtime binding")
        .clone();
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode Runtime scheduler policy"),
        8,
    );
    let next_reorg_epoch = old_binding.reorg_epoch.saturating_add(1);
    server
        .world
        .invalidate_cognition_for_reorg(next_reorg_epoch)
        .expect("authorize Runtime binding rotation");
    server
        .world
        .bind_cognition_runtime(
            old_binding.world_id.clone(),
            old_binding.branch_id.clone(),
            old_binding.finality_epoch,
            old_binding
                .finality_block_hash
                .as_ref()
                .map(ToString::to_string),
            old_binding.finality_status.clone(),
            next_reorg_epoch,
        )
        .expect("bind the new authoritative Runtime head");
    server
        .world
        .install_test_provider_capability_fixture(agent_id.as_str())
        .expect("install proof-bearing context for the new Runtime head");

    // The sidecar still carries the old lineage marker. Refreshing through
    // the Runtime-owned preparation path must fence the old context, adopt
    // the new authoritative binding, and rebuild a usable provider context.
    server
        .llm_sidecar
        .refresh_provider_backed_test_context(
            &mut server.world,
            &WorldConfig::default(),
            old_binding.world_id.as_str(),
        )
        .expect("refresh provider context after Runtime rebind");
    let current_binding = server
        .world
        .current_cognition_runtime_binding()
        .expect("current Runtime binding");
    assert_eq!(
        server
            .llm_sidecar
            .provider_test_context_binding(agent_id.as_str()),
        Some(current_binding.clone()),
        "refresh must rebuild context instead of retaining the prior Runtime head"
    );
    server
        .llm_sidecar
        .apply_prompt_profile_to_driver(&oasis7_simulator::AgentPromptProfile {
            agent_id,
            short_term_goal_override: Some("new-runtime-head-goal".to_string()),
            ..Default::default()
        })
        .expect("refreshed current-head context must admit prompt control");
    assert_eq!(
        server.llm_sidecar.provider_test_binding(),
        Some(current_binding.clone())
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn runtime_prompt_control_hosted_local_mock_provider_requires_initialized_runner() {
    let _llm_guard = lock_test_llm_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.enable_hosted_local_mock_test_lane();
    let error = sidecar
        .apply_prompt_profile_to_driver(&oasis7_simulator::AgentPromptProfile {
            agent_id: "hosted-local-mock-agent".to_string(),
            short_term_goal_override: Some("must not acknowledge without a driver".to_string()),
            ..Default::default()
        })
        .expect_err("Hosted Apply must fail closed before runner initialization");
    assert_eq!(error, "llm runner is not initialized");
}

fn signed_prompt_control_rollback_request(
    mut request: crate::viewer::PromptControlRollbackRequest,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::PromptControlRollbackRequest {
    request.public_key = Some(public_key_hex.to_string());
    request.auth = Some(
        crate::viewer::sign_prompt_control_rollback_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign rollback prompt auth"),
    );
    request
}

#[test]
fn runtime_prompt_control_hosted_local_mock_serialized_apply_returns_ack() {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let (backend_public_key, backend_private_key) = test_signer(92);
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
    let (public_key, private_key) = test_signer(93);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-local-mock-wire",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "hosted-local-mock-wire".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: vec![
                    crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string(),
                ],
            },
            &mut session,
            &mut writer,
        )
        .expect("serialized-path HelloV2");
    let hello = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    assert!(matches!(
        hello.first(),
        Some(ViewerResponse::HelloAck { capabilities, .. })
            if capabilities.iter().any(|capability| capability
                == crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY)
    ));

    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let mut request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-wire".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-local-mock-wire".to_string()),
            system_prompt_override: Some(Some("wire-apply".to_string())),
            request_id: Some("hosted-local-mock-wire-apply".to_string()),
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
    request.strong_auth_grant = Some(
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            "prompt_control_apply",
            "player-hosted-local-mock-wire",
            public_key.as_str(),
            request.agent_id.as_str(),
            issued_at_unix_ms,
            issued_at_unix_ms.saturating_add(60_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("wire strong-auth grant"),
    );
    let wire_request = ViewerRequest::PromptControl {
        command: Box::new(crate::viewer::PromptControlCommand::Apply { request }),
    };
    let decoded_request = serde_json::from_str::<ViewerRequest>(
        &serde_json::to_string(&wire_request).expect("serialize prompt-control request"),
    )
    .expect("deserialize prompt-control request");
    server
        .handle_request(decoded_request, &mut session, &mut writer)
        .expect("serialized Hosted local-mock Apply");
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    match responses.as_slice() {
        [ViewerResponse::PromptControlAck { ack }] => {
            assert_eq!(
                ack.status,
                Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
            );
            assert_eq!(ack.version, 1);
        }
        other => panic!("expected serialized prompt-control ack, got {other:?}"),
    }

    let mut rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id,
            player_id: "player-hosted-local-mock-wire".to_string(),
            expected_version: Some(1),
            updated_by: Some("player-hosted-local-mock-wire".to_string()),
            to_version: 0,
            request_id: Some("hosted-local-mock-wire-rollback".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(server.prompt_control_authority.authority_epoch.clone()),
            ..Default::default()
        },
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    rollback_request.strong_auth_grant = Some(
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            "prompt_control_rollback",
            "player-hosted-local-mock-wire",
            public_key.as_str(),
            rollback_request.agent_id.as_str(),
            issued_at_unix_ms,
            issued_at_unix_ms.saturating_add(60_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("wire rollback strong-auth grant"),
    );
    let rollback_wire_request = ViewerRequest::PromptControl {
        command: Box::new(crate::viewer::PromptControlCommand::Rollback {
            request: rollback_request,
        }),
    };
    let decoded_rollback_request = serde_json::from_str::<ViewerRequest>(
        &serde_json::to_string(&rollback_wire_request).expect("serialize rollback request"),
    )
    .expect("deserialize rollback request");
    server
        .handle_request(decoded_rollback_request, &mut session, &mut writer)
        .expect("serialized Hosted local-mock Rollback");
    let rollback_responses =
        read_available_runtime_live_responses(&peer, Duration::from_millis(25));
    match rollback_responses.as_slice() {
        [ViewerResponse::PromptControlAck { ack }] => {
            assert_eq!(
                ack.status,
                Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
            );
            assert_eq!(ack.version, 2);
            assert_eq!(ack.rolled_back_to_version, Some(0));
        }
        other => panic!("expected serialized rollback ack, got {other:?}"),
    }
}

#[test]
fn runtime_prompt_control_hosted_local_mock_blocks_apply_and_rollback_when_context_preparation_fails()
 {
    let _llm_guard = lock_test_llm_env();
    let _strong_auth_guard = lock_test_hosted_strong_auth_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    let (backend_public_key, backend_private_key) = test_signer(94);
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
    prime_hosted_local_mock_provider_context(&mut server, agent_id.as_str());
    let old_context_binding = server
        .llm_sidecar
        .provider_test_context_binding(agent_id.as_str())
        .expect("seed proof-bearing provider context");

    let (public_key, private_key) = test_signer(95);
    let registration = register_runtime_session(
        &mut server,
        "player-hosted-local-mock-prep-failure",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let issued_at_unix_ms = test_now_unix_ms().saturating_sub(1_000);
    let strong_auth_grant = |operation: &str| {
        crate::viewer::sign_hosted_prompt_control_strong_auth_grant(
            operation,
            "player-hosted-local-mock-prep-failure",
            public_key.as_str(),
            agent_id.as_str(),
            issued_at_unix_ms,
            issued_at_unix_ms.saturating_add(60_000),
            backend_public_key.as_str(),
            backend_private_key.as_str(),
        )
        .expect("sign Hosted local-mock strong-auth grant")
    };

    let mut apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-prep-failure".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-hosted-local-mock-prep-failure".to_string()),
            system_prompt_override: Some(Some("must not apply from stale context".to_string())),
            request_id: Some("hosted-local-mock-prep-failure-apply".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    apply_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_apply"));

    let mut rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.clone(),
            player_id: "player-hosted-local-mock-prep-failure".to_string(),
            expected_version: Some(1),
            updated_by: Some("player-hosted-local-mock-prep-failure".to_string()),
            to_version: 0,
            request_id: Some("hosted-local-mock-prep-failure-rollback".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    rollback_request.strong_auth_grant = Some(strong_auth_grant("prompt_control_rollback"));

    // Deliberately make the authoritative world/config tuple invalid after a valid
    // provider context exists. The old context must not be used as a fallback.
    server.config.world_id = "mismatched-world-for-prep-failure".to_string();
    let mut session = RuntimeLiveSession::new();
    session.negotiated_protocol = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
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
            .expect("prep failure should be returned as a PromptControlError");
        let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(25));
        match responses.as_slice() {
            [ViewerResponse::PromptControlError { error }] => {
                assert_eq!(error.code, "prompt_control_runtime_context_unavailable");
                assert_eq!(
                    error.status,
                    Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
                );
                assert_eq!(
                    error.value_visibility,
                    Some(crate::viewer::protocol::PromptControlValueVisibility::Hidden)
                );
            }
            other => panic!("expected blocked prep error, got {other:?}"),
        }
    }

    assert_eq!(
        server
            .llm_sidecar
            .provider_test_context_binding(agent_id.as_str()),
        Some(old_context_binding),
        "failed preparation must not authorize or replace the old context"
    );
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .map(|profile| profile.version),
        None,
        "blocked Apply/Rollback must not mutate the prompt profile"
    );
}

#[test]
fn runtime_prompt_control_hosted_local_mock_missing_context_and_wrong_tuple_fail_closed() {
    let _llm_guard = lock_test_llm_env();
    configure_hosted_local_mock_provider_env("wrong_contract");
    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    assert!(
        !sidecar.supports_prompt_control_result(),
        "wrong provider tuple must not enable PromptControl"
    );

    clear_runtime_provider_env();
    configure_hosted_local_mock_provider_env("worldsim_provider_v1");
    assert!(
        !RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm).supports_prompt_control_result(),
        "valid tuple without an installed Hosted Runtime fixture must remain disabled"
    );
    let mut empty_world = crate::runtime_host::world::new();
    let fixture_error = crate::viewer::runtime_live::control_plane::install_hosted_local_mock_test_capability_fixtures(
        &mut empty_world,
        true,
    )
    .expect_err("Hosted local-mock fixture installation must fail without a seeded Agent");
    assert!(fixture_error.contains("Runtime-seeded Agent"));
}

#[test]
fn runtime_prompt_control_production_provider_backed_handler_remains_denied() {
    let _llm_guard = lock_test_llm_env();
    // SAFETY: This test/setup code mutates process environment while holding
    // the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
    }
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("production provider-backed runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview {
                request: crate::viewer::PromptControlApplyRequest {
                    agent_id,
                    player_id: "production-provider-player".to_string(),
                    expected_version: Some(0),
                    updated_by: Some("production-provider-player".to_string()),
                    request_id: Some("production-provider-preview".to_string()),
                    session_epoch: Some(1),
                    binding_epoch: Some(0),
                    expected_authority_epoch: Some(
                        server.prompt_control_authority.authority_epoch.clone(),
                    ),
                    ..Default::default()
                },
            },
            &negotiated,
        )
        .expect_err("production provider-backed handler must remain denied");
    assert_eq!(error.code, "agent_provider_prompt_control_unsupported");
}
