use super::*;
use std::sync::{Arc, Mutex};

pub(super) use super::mock_http::{
    MockHttpResponse, RecordedHttpRequest, RuntimeLiveMockHttpServer, provider_context_response,
    spawn_runtime_live_mock_http_server, spawn_runtime_live_mock_http_server_until_drop,
    spawn_runtime_live_mock_http_server_with_provider_probes,
};

#[path = "auth_actions_agent_chat.rs"]
mod agent_chat_tests;
#[path = "auth_actions_authoritative_recovery.rs"]
mod authoritative_recovery_tests;
#[path = "auth_actions_claims.rs"]
mod claim_action_tests;

#[test]
fn runtime_agent_chat_script_mode_requires_llm_mode() {
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
    let err = server
        .handle_agent_chat(crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: None,
            intent_seq: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        })
        .expect_err("script mode should reject chat");
    assert_eq!(err.code, "llm_mode_required");
}

#[test]
fn runtime_gameplay_action_requires_auth() {
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
    let err = server
        .handle_gameplay_action(crate::viewer::GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: agent_id,
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("missing auth should fail");
    assert_eq!(err.code, "auth_proof_required");
}

#[test]
fn runtime_gameplay_action_script_mode_requires_llm_mode() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(87);
    let request = signed_gameplay_action_request(
        crate::viewer::GameplayActionRequest {
            action_id: "build_factory_smelter_mk1".to_string(),
            target_agent_id: agent_id,
            actor_agent_id: None,
            player_id: "player-a".to_string(),
            public_key: None,
            auth: None,
        },
        87,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_gameplay_action(request)
        .expect_err("script mode should reject gameplay actions");
    assert_eq!(err.code, "llm_mode_required");
}

#[test]
fn runtime_step_control_reports_blocked_without_llm_mode() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let (mut writer, client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();

    server
        .apply_control_mode(
            ViewerControl::Step { count: 1 },
            Some(1),
            &mut session,
            &mut writer,
        )
        .expect("control handled");
    writer.flush().expect("flush response");

    let ack =
        read_control_completion_ack(&client, Duration::from_millis(250)).expect("blocked step ack");
    assert_eq!(ack.status, ControlCompletionStatus::Blocked);
    assert_eq!(ack.error_code.as_deref(), Some("llm_mode_required"));
    assert!(
        ack.error_message
            .as_deref()
            .is_some_and(|message| message.contains("--llm"))
    );

    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("--llm"))
    );
}

#[test]
fn runtime_step_control_reports_llm_init_failed_when_provider_unavailable() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
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
    let (mut writer, client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();

    server
        .apply_control_mode(
            ViewerControl::Step { count: 1 },
            Some(7),
            &mut session,
            &mut writer,
        )
        .expect("control handled");
    writer.flush().expect("flush response");

    let ack = read_control_completion_ack(&client, Duration::from_millis(250))
        .expect("blocked init failure ack");
    assert_eq!(ack.status, ControlCompletionStatus::Blocked);
    assert_eq!(ack.error_code.as_deref(), Some("llm_init_failed"));
    assert!(
        ack.error_message
            .as_deref()
            .is_some_and(|message| message.contains("configured and reachable LLM provider"))
    );

    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| { reason.contains("configured and reachable LLM provider") })
    );
}

#[test]
fn runtime_background_play_stops_when_llm_access_is_unavailable() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
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
    let baseline_time = server.world.state().time;
    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    server
        .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
        .expect("play loop handled");

    assert!(
        !session.playing,
        "background play should stop without LLM access"
    );
    assert_eq!(
        server.world.state().time,
        baseline_time,
        "background play must not advance world time without active LLM access"
    );
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.action, "play");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| { reason.contains("configured and reachable LLM provider") })
    );
}

#[test]
fn runtime_background_play_tolerates_transient_llm_failure_after_confirmed_progress() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let request_count = Arc::new(Mutex::new(0_usize));
    let base_url = spawn_runtime_live_mock_http_server(3, {
        let request_count = Arc::clone(&request_count);
        move |request| {
            let count = if request.path == "/v1/world-simulator/decision-context" {
                let mut count = request_count.lock().expect("request count lock");
                *count += 1;
                *count
            } else {
                0
            };
            match (request.method.as_str(), request.path.as_str(), count) {
                ("POST", "/v1/world-simulator/decision-context", 1) => {
                    let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode outer decision request");
                    let response = crate::simulator::DecisionResponse {
                        decision: crate::simulator::ProviderDecision::Act {
                            action_ref: "move_agent".to_string(),
                            action: oasis7_simulator::Action::MoveAgent {
                                agent_id: decoded
                                    .base_decision_request
                                    .observation
                                    .agent_id
                                    .clone(),
                                to: decoded
                                    .base_decision_request
                                    .observation
                                    .observation
                                    .self_state
                                    .location_ref
                                    .clone(),
                            },
                        },
                        module_command: None,
                        provider_error: None,
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: Vec::new(),
                    };
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&provider_context_response(&decoded, response))
                            .expect("encode outer decision response"),
                    }
                }
                ("POST", "/v1/world-simulator/feedback-context", 0) => {
                    let feedback: crate::simulator::FeedbackEnvelopeV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode Runtime feedback");
                    assert_eq!(
                        feedback.status, "committed",
                        "the initial provider action should retain its captured base"
                    );
                    assert!(feedback.runtime_receipt_id.is_some());
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::json!({"ok": true}).to_string(),
                    }
                }
                ("POST", "/v1/world-simulator/decision-context", _) => MockHttpResponse {
                    status_code: 503,
                    body: serde_json::json!({
                        "ok": false,
                        "error": "provider temporarily unavailable"
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
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let test_world_id = format!("live-runtime-{}", WorldScenario::Minimal.as_str());
    let test_finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &test_world_id).to_string();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "main",
                0,
                Some(test_finality_block_hash),
                "verified",
                0,
            ),
    )
    .expect("runtime server");
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");
    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    let progress_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    server
        .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
        .expect("first background play tick advances");
    while server.confirmed_player_gameplay_progress_time.is_none()
        && std::time::Instant::now() < progress_deadline
    {
        server
            .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
            .expect("background play progress polling should be tolerated");
        if server.confirmed_player_gameplay_progress_time.is_none() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let progress_confirmed = server.confirmed_player_gameplay_progress_time.is_some();
    let advanced_time = server.world.state().time;
    let baseline_journal_len = server.world.journal().events.len();

    let mut failure_observed = false;
    let failure_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !failure_observed && std::time::Instant::now() < failure_deadline {
        server
            .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
            .expect("transient provider failure should be tolerated");
        if session.transient_play_failures == 1 {
            failure_observed = true;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    clear_runtime_provider_env();
    drop(_guard);

    assert!(
        progress_confirmed,
        "successful background play should confirm gameplay progress"
    );
    assert!(
        failure_observed,
        "async provider failure should be observed"
    );
    assert!(
        session.playing,
        "background play should remain active during transient failure budget"
    );
    assert_eq!(session.transient_play_failures, 1);
    assert!(
        server.world.state().time == advanced_time,
        "the Runtime base must remain stable while the transient provider response is pending"
    );
    assert!(
        !server.world.journal().events[baseline_journal_len..]
            .iter()
            .any(|event| matches!(
                event.body,
                crate::runtime::WorldEventBody::Domain(
                    crate::runtime::DomainEvent::ActionAccepted { .. }
                ) | crate::runtime::WorldEventBody::EffectQueued(_)
                    | crate::runtime::WorldEventBody::ReceiptAppended(_)
            )),
        "transient provider failure must not create an action, effect, or receipt"
    );
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.action, "play");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("provider temporarily unavailable"))
    );
}

#[test]
fn runtime_background_play_stops_on_non_retryable_provider_error_after_progress() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    let request_count = Arc::new(Mutex::new(0_usize));
    let base_url = spawn_runtime_live_mock_http_server(3, {
        let request_count = Arc::clone(&request_count);
        move |request| {
            let count = if request.path == "/v1/world-simulator/decision-context" {
                let mut count = request_count.lock().expect("request count lock");
                *count += 1;
                *count
            } else {
                0
            };
            match (request.method.as_str(), request.path.as_str(), count) {
                ("POST", "/v1/world-simulator/decision-context", 1) => {
                    let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode outer decision request");
                    let response = crate::simulator::DecisionResponse {
                        decision: crate::simulator::ProviderDecision::Act {
                            action_ref: "speak_to_nearby".to_string(),
                            action: oasis7_simulator::Action::SpeakToNearby {
                                agent_id: decoded
                                    .base_decision_request
                                    .observation
                                    .agent_id
                                    .clone(),
                                message: "runtime-live play ok".to_string(),
                                target_agent_id: None,
                            },
                        },
                        module_command: None,
                        provider_error: None,
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: Vec::new(),
                    };
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&provider_context_response(&decoded, response))
                            .expect("encode outer decision response"),
                    }
                }
                ("POST", "/v1/world-simulator/feedback-context", 0) => {
                    let _feedback: crate::simulator::FeedbackEnvelopeV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode Runtime feedback");
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::json!({"ok": true}).to_string(),
                    }
                }
                ("POST", "/v1/world-simulator/decision-context", _) => {
                    let decoded: crate::simulator::ContinuousAgentRequestContextV1 =
                        serde_json::from_slice(request.body.as_slice())
                            .expect("decode outer decision request");
                    let response = crate::simulator::DecisionResponse {
                        decision: crate::simulator::ProviderDecision::Wait,
                        module_command: None,
                        provider_error: Some(crate::simulator::ProviderErrorEnvelope {
                            code: "provider_unauthorized".to_string(),
                            message: "missing provider token".to_string(),
                            retryable: false,
                        }),
                        diagnostics: crate::simulator::ProviderDiagnostics::default(),
                        trace_payload: crate::simulator::ProviderTraceEnvelope::default(),
                        memory_write_intents: Vec::new(),
                    };
                    MockHttpResponse {
                        status_code: 200,
                        body: serde_json::to_string(&provider_context_response(&decoded, response))
                            .expect("encode outer decision response"),
                    }
                }
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
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let test_world_id = format!("live-runtime-{}", WorldScenario::Minimal.as_str());
    let test_finality_block_hash =
        crate::simulator::h_v1("oasis7.viewer.test.finality-block.v1", &test_world_id).to_string();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding(
                "main",
                0,
                Some(test_finality_block_hash),
                "verified",
                0,
            ),
    )
    .expect("runtime server");
    server
        .world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");
    let (mut writer, _client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();
    session.playing = true;

    server
        .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
        .expect("first background play tick advances");
    let advanced_time = server.world.state().time;
    let baseline_journal_len = server.world.journal().events.len();

    let mut failure_observed = false;
    let failure_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !failure_observed && std::time::Instant::now() < failure_deadline {
        server
            .advance_runtime(&mut session, &mut writer, "play", 1, None, false)
            .expect("non-retryable provider failure should be handled");
        if !session.playing {
            failure_observed = true;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    clear_runtime_provider_env();
    drop(_guard);

    assert!(
        failure_observed,
        "async provider failure should be observed"
    );
    assert!(!session.playing);
    assert_eq!(session.transient_play_failures, 0);
    assert!(
        server.world.state().time > advanced_time,
        "ticks while the provider response was pending must remain visible"
    );
    assert!(
        !server.world.journal().events[baseline_journal_len..]
            .iter()
            .any(|event| matches!(
                event.body,
                crate::runtime::WorldEventBody::Domain(
                    crate::runtime::DomainEvent::ActionAccepted { .. }
                ) | crate::runtime::WorldEventBody::EffectQueued(_)
                    | crate::runtime::WorldEventBody::ReceiptAppended(_)
            )),
        "non-retryable provider failure must not create an action, effect, or receipt"
    );
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.action, "play");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("provider_unauthorized"))
    );
}

#[test]
fn runtime_step_control_surfaces_runtime_failure_as_blocked_ack() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Script),
    )
    .expect("runtime server");
    let missing_agent = "missing-agent".to_string();

    let (mut writer, client) = test_writer_pair();
    let mut session = RuntimeLiveSession::new();

    server
        .block_runtime_control(
            &mut session,
            &mut writer,
            "step",
            "runtime step aborted because world advance failed",
            ViewerRuntimeLiveServerError::Runtime(crate::runtime::WorldError::AgentNotFound {
                agent_id: missing_agent.clone(),
            }),
            Some(19),
            1,
            0,
            true,
        )
        .expect("control handled");
    writer.flush().expect("flush response");

    let ack =
        read_control_completion_ack(&client, Duration::from_millis(250)).expect("blocked step ack");
    assert_eq!(ack.status, ControlCompletionStatus::Blocked);
    assert_eq!(ack.error_code.as_deref(), Some("agent_not_found"));
    assert_eq!(ack.delta_logical_time, 1);
    assert!(
        ack.error_message
            .as_deref()
            .is_some_and(|message| message.contains(missing_agent.as_str()))
    );

    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("blocked feedback recorded");
    assert_eq!(feedback.stage, "blocked");
    assert_eq!(feedback.delta_logical_time, 1);
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains(missing_agent.as_str()))
    );
    assert!(
        feedback
            .hint
            .as_deref()
            .is_some_and(|hint| hint.contains("restore the missing agent"))
    );
}

#[test]
fn runtime_agent_chat_requires_explicit_session_registration() {
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
    let (public_key, private_key) = test_signer(24);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(1),
            intent_seq: Some(2),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_agent_chat(request)
        .expect_err("session register should be required before agent chat");
    assert_eq!(err.code, "session_not_found");
}

#[test]
fn runtime_session_register_rejects_same_player_binding_to_second_agent() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_ids: Vec<_> = server
        .world
        .state()
        .agents
        .keys()
        .take(2)
        .cloned()
        .collect();
    assert!(
        agent_ids.len() >= 2,
        "expected at least two agents in two_bases scenario"
    );
    let (public_key, private_key) = test_signer(25);

    let first_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_ids[0].as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        first_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(first_ack.agent_id.as_deref(), Some(agent_ids[0].as_str()));

    let conflict_request = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: "player-a".to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_ids[1].clone()),
            force_rebind: false,
        },
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: conflict_request,
        })
        .expect_err("same player should not silently rebind to another agent");
    assert_eq!(err.code, "player_bind_failed");
    assert!(err.message.contains("explicit rebind required"));
}

#[test]
fn runtime_session_register_allows_same_player_rebind_with_force_rebind() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_ids: Vec<_> = server
        .world
        .state()
        .agents
        .keys()
        .take(2)
        .cloned()
        .collect();
    assert!(
        agent_ids.len() >= 2,
        "expected at least two agents in two_bases scenario"
    );
    let (public_key, private_key) = test_signer(26);

    let first_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_ids[0].as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        first_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(first_ack.agent_id.as_deref(), Some(agent_ids[0].as_str()));

    let second_ack = register_runtime_session_with_options(
        &mut server,
        "player-a",
        Some(agent_ids[1].as_str()),
        true,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        second_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(second_ack.agent_id.as_deref(), Some(agent_ids[1].as_str()));
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player("player-a"),
        Some(agent_ids[1].as_str())
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get(agent_ids[0].as_str()),
        None
    );
}

#[test]
fn hosted_registration_grant_survives_bind_rejection_until_force_rebind_succeeds() {
    let _guard = lock_test_llm_env();
    let replay_dir = runtime_live_temp_dir("hosted-grant-force-rebind");
    let replay_ledger = replay_dir.join("registration-replay.json");
    let issuer_private_key = hex::encode([91_u8; 32]);
    let issuer_public_key =
        crate::viewer::derive_hosted_registration_issuer_public_key(issuer_private_key.as_str())
            .expect("derive issuer public key");
    unsafe {
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
            issuer_public_key,
        );
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
            replay_ledger.as_os_str(),
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
    let (old_public_key, old_private_key) = test_signer(92);
    register_runtime_session(
        &mut server,
        "player-old",
        Some(agent_id.as_str()),
        1,
        old_public_key.as_str(),
        old_private_key.as_str(),
    );

    let hosted_player_id = "hosted-player-account-force-rebind";
    let (hosted_public_key, hosted_private_key) = test_signer(93);
    let registration_grant = crate::viewer::issue_hosted_registration_grant(
        hosted_player_id,
        hosted_public_key.as_str(),
        "device-force-rebind",
        "nonce-force-rebind",
        test_now_unix_ms(),
        issuer_private_key.as_str(),
    )
    .expect("issue registration grant");

    let request = |force_rebind, nonce| {
        signed_session_register_request(
            crate::viewer::AuthoritativeSessionRegisterRequest {
                player_id: hosted_player_id.to_string(),
                public_key: None,
                registration_grant: Some(registration_grant.clone()),
                auth: None,
                requested_agent_id: Some(agent_id.clone()),
                force_rebind,
            },
            nonce,
            hosted_public_key.as_str(),
            hosted_private_key.as_str(),
        )
    };

    let first_error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(false, 2),
        })
        .expect_err("initial bind must require explicit rebind");
    assert_eq!(first_error.code, "player_bind_failed");
    assert!(
        !replay_ledger.exists(),
        "failed bind must not consume grant"
    );

    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(true, 3),
        })
        .expect("force rebind should reuse still-valid grant");
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);
    assert_eq!(ack.player_id.as_deref(), Some(hosted_player_id));

    let replay_error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(true, 4),
        })
        .expect_err("successful registration must consume grant");
    assert_eq!(replay_error.code, "auth_invalid");
    assert!(replay_error.message.contains("registration grant replay"));

    unsafe {
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
    let _ = std::fs::remove_dir_all(replay_dir);
}

#[test]
fn runtime_session_register_allows_different_player_rebind_with_force_rebind() {
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
    let (old_public_key, old_private_key) = test_signer(27);
    let (new_public_key, new_private_key) = test_signer(28);

    let first_ack = register_runtime_session(
        &mut server,
        "player-old",
        Some(agent_id.as_str()),
        1,
        old_public_key.as_str(),
        old_private_key.as_str(),
    );
    assert_eq!(
        first_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(first_ack.agent_id.as_deref(), Some(agent_id.as_str()));

    let second_ack = register_runtime_session_with_options(
        &mut server,
        "player-new",
        Some(agent_id.as_str()),
        true,
        2,
        new_public_key.as_str(),
        new_private_key.as_str(),
    );
    assert_eq!(
        second_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(second_ack.agent_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player("player-new"),
        Some(agent_id.as_str())
    );
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player("player-old"),
        None
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get(agent_id.as_str())
            .map(String::as_str),
        Some("player-new")
    );
}
