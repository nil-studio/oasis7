use super::*;

#[test]
fn runtime_gameplay_action_dispatch_rejection_emits_error_then_rejected_snapshot() {
    const ACTION_ID: &str = "build_factory_smelter_mk1";
    const ERROR_MESSAGE: &str = "gameplay_action requires auth proof";
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
    let request = crate::viewer::GameplayActionRequest {
        action_id: ACTION_ID.to_string(),
        target_agent_id: agent_id.clone(),
        actor_agent_id: None,
        player_id: "player-a".to_string(),
        public_key: None,
        auth: None,
    };
    let mut session = RuntimeLiveSession::new();
    session.subscribed.insert(ViewerStream::Snapshot);
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::GameplayAction { request },
            &mut session,
            &mut writer,
        )
        .expect("synchronous gameplay rejection should remain a protocol response");
    writer.flush().expect("flush gameplay rejection responses");

    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(200));
    assert_eq!(
        responses.len(),
        2,
        "snapshot subscribers should receive rejection context immediately after the error"
    );
    match &responses[0] {
        ViewerResponse::GameplayActionError { error } => {
            assert_eq!(error.code, "auth_proof_required");
            assert_eq!(error.message, ERROR_MESSAGE);
            assert_eq!(error.action_id.as_deref(), Some(ACTION_ID));
            assert_eq!(error.target_agent_id.as_deref(), Some(agent_id.as_str()));
        }
        other => panic!("expected gameplay action error first, got {other:?}"),
    }
    match &responses[1] {
        ViewerResponse::Snapshot { snapshot } => {
            let gameplay = snapshot
                .player_gameplay
                .as_ref()
                .expect("player gameplay snapshot");
            assert_eq!(
                gameplay
                    .recent_feedback
                    .as_ref()
                    .expect("rejected feedback")
                    .stage,
                "rejected"
            );
            assert_eq!(
                gameplay.response_window_class.as_deref(),
                Some("request_rejected")
            );
        }
        other => panic!("expected rejection snapshot second, got {other:?}"),
    }
    assert_eq!(
        session
            .local_gameplay_feedback
            .lock()
            .expect("session feedback")
            .feedback
            .as_ref()
            .map(|feedback| (
                feedback.action.as_str(),
                feedback.stage.as_str(),
                feedback.target_agent_id.as_deref(),
                feedback.reason.as_deref(),
                feedback.hint.as_deref(),
                feedback.delta_logical_time,
                feedback.delta_event_seq,
            )),
        Some((
            "gameplay_action:build_factory_smelter_mk1",
            "rejected",
            Some(agent_id.as_str()),
            Some("auth_proof_required: gameplay_action requires auth proof"),
            Some("correct the rejected request before retrying"),
            0,
            0,
        ))
    );
    assert!(server.latest_player_gameplay_feedback.is_none());
    assert!(server.scoped_gameplay_feedback.is_empty());
    let mut other = RuntimeLiveSession::new();
    assert!(
        server
            .compat_snapshot_for_session(&mut other)
            .player_gameplay
            .expect("public gameplay")
            .recent_feedback
            .is_none()
    );
}

#[test]
fn runtime_gameplay_action_dispatch_rejection_without_snapshot_subscription_emits_only_error() {
    const ACTION_ID: &str = "build_factory_smelter_mk1";
    const ERROR_MESSAGE: &str = "gameplay_action requires auth proof";
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
    let request = crate::viewer::GameplayActionRequest {
        action_id: ACTION_ID.to_string(),
        target_agent_id: agent_id.clone(),
        actor_agent_id: None,
        player_id: "player-a".to_string(),
        public_key: None,
        auth: None,
    };
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(
            ViewerRequest::GameplayAction { request },
            &mut session,
            &mut writer,
        )
        .expect("synchronous gameplay rejection should remain a protocol response");
    writer.flush().expect("flush gameplay rejection response");

    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(200));
    assert_eq!(
        responses.len(),
        1,
        "unsubscribed clients must not receive a snapshot"
    );
    match &responses[0] {
        ViewerResponse::GameplayActionError { error } => {
            assert_eq!(error.code, "auth_proof_required");
            assert_eq!(error.message, ERROR_MESSAGE);
            assert_eq!(error.action_id.as_deref(), Some(ACTION_ID));
            assert_eq!(error.target_agent_id.as_deref(), Some(agent_id.as_str()));
        }
        other => panic!("expected only gameplay action error, got {other:?}"),
    }
    assert!(server.latest_player_gameplay_feedback.is_none());
    assert!(server.scoped_gameplay_feedback.is_empty());
    let local = session
        .local_gameplay_feedback
        .lock()
        .expect("session feedback");
    let feedback = local
        .feedback
        .as_ref()
        .expect("connection retains its own rejection");
    assert_eq!(feedback.action, "gameplay_action:build_factory_smelter_mk1");
    assert_eq!(feedback.stage, "rejected");
    assert_eq!(feedback.target_agent_id.as_deref(), Some(agent_id.as_str()));
}

#[test]
fn runtime_gameplay_action_rejection_feedback_classifies_operational_and_unknown_hints() {
    let feedback = ViewerRuntimeLiveServer::gameplay_action_rejection_feedback(
        &crate::viewer::GameplayActionError {
            code: "chain_submit_unavailable".to_string(),
            message: "chain gameplay submit transport failed".to_string(),
            action_id: Some("mine".to_string()),
            target_agent_id: Some("agent-1".to_string()),
        },
    );
    assert_eq!(
        feedback.hint.as_deref(),
        Some("restore the chain gameplay submission path, then retry")
    );

    let feedback = ViewerRuntimeLiveServer::gameplay_action_rejection_feedback(
        &crate::viewer::GameplayActionError {
            code: "llm_mode_required".to_string(),
            message: "gameplay actions require LLM mode".to_string(),
            action_id: Some("mine".to_string()),
            target_agent_id: Some("agent-1".to_string()),
        },
    );
    assert_eq!(
        feedback.hint.as_deref(),
        Some("restore or wait for runtime and provider readiness, then retry")
    );

    let feedback = ViewerRuntimeLiveServer::gameplay_action_rejection_feedback(
        &crate::viewer::GameplayActionError {
            code: "future_gameplay_rejection".to_string(),
            message: "future rejection detail".to_string(),
            action_id: Some("mine".to_string()),
            target_agent_id: Some("agent-1".to_string()),
        },
    );
    assert_eq!(
        feedback.hint.as_deref(),
        Some("inspect the rejection details before retrying")
    );
}

fn rejected_feedback_request(
    player: &str,
    agent: &str,
    nonce: u64,
    public: &str,
    private: &str,
) -> ViewerRequest {
    ViewerRequest::GameplayAction {
        request: signed_gameplay_action_request(
            crate::viewer::GameplayActionRequest {
                action_id: "unsupported_feedback_test_action".into(),
                target_agent_id: agent.into(),
                actor_agent_id: None,
                player_id: player.into(),
                public_key: None,
                auth: None,
            },
            nonce,
            public,
            private,
        ),
    }
}

fn session_feedback(
    server: &mut ViewerRuntimeLiveServer,
    session: &mut RuntimeLiveSession,
) -> Option<PlayerGameplayRecentFeedback> {
    server
        .compat_snapshot_for_session(session)
        .player_gameplay
        .expect("gameplay snapshot")
        .recent_feedback
}

#[test]
fn runtime_feedback_replay_and_asserted_reconnect_never_authorize_private_reads() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent = server.world.state().agents.keys().next().unwrap().clone();
    let (public, private) = test_signer(218);
    register_runtime_session(
        &mut server,
        "feedback-owner",
        Some(&agent),
        1,
        &public,
        &private,
    );
    let original = rejected_feedback_request("feedback-owner", &agent, 2, &public, &private);
    let mut owner = RuntimeLiveSession::new();
    let mut responses = Vec::new();
    server
        .handle_request(original.clone(), &mut owner, &mut responses)
        .unwrap();
    let expected = session_feedback(&mut server, &mut owner).expect("owner rejection");
    let expected_cache = server.scoped_feedback_for_player(Some("feedback-owner"));
    assert!(expected_cache.is_some());
    assert!(
        !expected
            .reason
            .as_deref()
            .unwrap()
            .contains("auth_nonce_replay")
    );
    assert!(owner.gameplay_feedback_owner.is_some());

    let mut attacker = RuntimeLiveSession::new();
    server
        .handle_request(original, &mut attacker, &mut responses)
        .unwrap();
    assert!(
        attacker.gameplay_feedback_owner.is_none(),
        "replayed proof cannot establish a reader"
    );
    assert!(
        session_feedback(&mut server, &mut attacker)
            .unwrap()
            .reason
            .unwrap()
            .contains("auth_nonce_replay")
    );
    assert_eq!(
        session_feedback(&mut server, &mut owner),
        Some(expected.clone())
    );
    assert_eq!(
        server.scoped_feedback_for_player(Some("feedback-owner")),
        expected_cache
    );

    server
        .handle_request(
            ViewerRequest::AuthoritativeRecovery {
                command: crate::viewer::AuthoritativeRecoveryCommand::ReconnectSync {
                    request: crate::viewer::AuthoritativeReconnectSyncRequest {
                        player_id: "feedback-owner".into(),
                        session_pubkey: None,
                        last_known_log_cursor: None,
                        expected_reorg_epoch: None,
                    },
                },
            },
            &mut attacker,
            &mut responses,
        )
        .unwrap();
    assert_eq!(
        attacker.current_player_id.as_deref(),
        Some("feedback-owner")
    );
    assert!(attacker.gameplay_feedback_owner.is_none());
    assert!(session_feedback(&mut server, &mut attacker).is_none());
    assert!(server.latest_player_gameplay_feedback.is_none());
}

#[test]
fn runtime_feedback_isolated_across_players_and_shared_only_between_authorized_sessions() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent = server.world.state().agents.keys().next().unwrap().clone();
    let (a_public, a_private) = test_signer(219);
    let (b_public, b_private) = test_signer(220);
    register_runtime_session(
        &mut server,
        "feedback-a",
        Some(&agent),
        1,
        &a_public,
        &a_private,
    );
    register_runtime_session(&mut server, "feedback-b", None, 1, &b_public, &b_private);
    let mut a = RuntimeLiveSession::new();
    let mut a_second = RuntimeLiveSession::new();
    let mut b = RuntimeLiveSession::new();
    let mut responses = Vec::new();
    for (player, public, private, nonce, session) in [
        ("feedback-a", &a_public, &a_private, 2, &mut a),
        ("feedback-b", &b_public, &b_private, 2, &mut b),
        ("feedback-a", &a_public, &a_private, 3, &mut a_second),
    ] {
        server
            .handle_request(
                rejected_feedback_request(player, &agent, nonce, public, private),
                session,
                &mut responses,
            )
            .unwrap();
    }
    let a_feedback = session_feedback(&mut server, &mut a).unwrap();
    let b_feedback = session_feedback(&mut server, &mut b).unwrap();
    assert_eq!(
        session_feedback(&mut server, &mut a_second),
        Some(a_feedback.clone())
    );
    assert_ne!(a_feedback.reason, b_feedback.reason);

    let mut invalid = rejected_feedback_request("feedback-a", &agent, 4, &a_public, &a_private);
    let ViewerRequest::GameplayAction { request } = &mut invalid else {
        unreachable!()
    };
    request.auth = None;
    server
        .handle_request(invalid, &mut a, &mut responses)
        .unwrap();
    assert!(
        session_feedback(&mut server, &mut a)
            .unwrap()
            .reason
            .unwrap()
            .contains("auth_proof_required")
    );
    assert_eq!(
        session_feedback(&mut server, &mut a_second),
        Some(a_feedback)
    );
    assert_eq!(
        session_feedback(&mut server, &mut b),
        Some(b_feedback.clone())
    );
    assert!(server.latest_player_gameplay_feedback.is_none());

    server
        .handle_request(
            rejected_feedback_request("feedback-b", &agent, 3, &b_public, &b_private),
            &mut a,
            &mut responses,
        )
        .unwrap();
    assert_eq!(session_feedback(&mut server, &mut a), Some(b_feedback));
    server.record_chain_sync_failure(&ViewerRuntimeLiveServerError::Init(
        "shared sync unavailable".into(),
    ));
    for session in [
        &mut a,
        &mut a_second,
        &mut b,
        &mut RuntimeLiveSession::new(),
    ] {
        let feedback = session_feedback(&mut server, session).unwrap();
        assert_eq!(feedback.action, "chain_sync");
        assert_eq!(feedback.stage, "blocked");
    }
}

#[test]
fn runtime_collect_data_anonymous_rejection_stays_on_its_connection() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .unwrap();
    let mut sender = RuntimeLiveSession::new();
    let mut other = RuntimeLiveSession::new();
    let mut responses = Vec::new();
    server
        .handle_request(
            ViewerRequest::CollectData {
                command: crate::viewer::CollectDataCommand::Preflight {
                    request: crate::viewer::CollectDataRequest {
                        electricity_cost: 1,
                        data_amount: 1,
                        player_id: "unverified-claim".into(),
                        public_key: None,
                        auth: None,
                    },
                },
            },
            &mut sender,
            &mut responses,
        )
        .unwrap();
    assert!(
        session_feedback(&mut server, &mut sender)
            .unwrap()
            .reason
            .unwrap()
            .contains("auth_proof_required")
    );
    assert!(session_feedback(&mut server, &mut other).is_none());
    assert!(server.scoped_gameplay_feedback.is_empty());
    assert!(server.latest_player_gameplay_feedback.is_none());
}

#[test]
fn fresh_legacy_chat_and_prompt_success_replace_only_the_requesting_connections_feedback() {
    let _guard = lock_test_llm_env();
    for prompt in [false, true] {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm),
        )
        .unwrap();
        let agent = server.world.state().agents.keys().next().unwrap().clone();
        seed_agent_chat_oc(&mut server, &agent);
        let (public, private) = test_signer(217);
        register_runtime_session(
            &mut server,
            "legacy-feedback-owner",
            Some(&agent),
            1,
            &public,
            &private,
        );
        let request = if prompt {
            ViewerRequest::PromptControl {
                command: Box::new(crate::viewer::PromptControlCommand::Apply {
                    request: signed_prompt_control_apply_request(
                        crate::viewer::PromptControlApplyRequest {
                            agent_id: agent.clone(),
                            player_id: "legacy-feedback-owner".into(),
                            expected_version: Some(0),
                            system_prompt_override: Some(Some("system guidance".into())),
                            ..Default::default()
                        },
                        crate::viewer::PromptControlAuthIntent::Apply,
                        2,
                        &public,
                        &private,
                    ),
                }),
            }
        } else {
            ViewerRequest::AgentChat {
                request: signed_agent_chat_request(
                    crate::viewer::AgentChatRequest {
                        agent_id: agent.clone(),
                        player_id: Some("legacy-feedback-owner".into()),
                        public_key: None,
                        auth: None,
                        message: "hello runtime feedback".into(),
                        intent_tick: Some(9),
                        intent_seq: Some(2),
                        world_id: None,
                        reorg_epoch: None,
                        authority_scope: None,
                        replaces_intent_id: None,
                        canonical_authority: None,
                    },
                    2,
                    &public,
                    &private,
                ),
            }
        };
        let mut sender = RuntimeLiveSession::new();
        server.record_session_gameplay_rejection(
            &mut sender,
            &GameplayActionError {
                code: "auth_proof_required".into(),
                message: "earlier anonymous rejection".into(),
                action_id: None,
                target_agent_id: None,
            },
            None,
        );
        let mut output = Vec::new();
        server
            .handle_request(request.clone(), &mut sender, &mut output)
            .unwrap();
        let response: ViewerResponse =
            serde_json::from_slice(output.split(|byte| *byte == b'\n').next().unwrap()).unwrap();
        assert!(
            matches!(
                response,
                ViewerResponse::AgentChatAck { .. } | ViewerResponse::PromptControlAck { .. }
            ),
            "expected fresh success: {response:?}"
        );
        let feedback = session_feedback(&mut server, &mut sender).expect("fresh sender feedback");
        assert_eq!(
            feedback.action,
            if prompt {
                "prompt_control.apply"
            } else {
                "agent_chat"
            }
        );
        let displayed = server
            .compat_snapshot_for_session(&mut sender)
            .player_gameplay
            .unwrap();
        let world_status = server
            .compat_snapshot_with_feedback(sender.current_player_id.as_deref(), None)
            .player_gameplay
            .unwrap();
        assert_eq!(
            displayed.execution_state, world_status.execution_state,
            "guidance receipt must not replace current world execution status"
        );
        assert_eq!(
            displayed.response_window_class,
            world_status.response_window_class
        );
        assert!(
            sender.gameplay_feedback_owner.is_none(),
            "request-local success does not silently expand read scope"
        );
        assert_eq!(
            server
                .llm_sidecar
                .player_auth_last_nonce
                .get("legacy-feedback-owner"),
            Some(&2)
        );
        let mut other = RuntimeLiveSession::new();
        other.current_player_id = Some("legacy-feedback-owner".into());
        assert!(session_feedback(&mut server, &mut other).is_none());
        assert!(server.latest_player_gameplay_feedback.is_none());

        server
            .handle_request(request.clone(), &mut other, &mut output)
            .unwrap();
        assert!(other.gameplay_feedback_owner.is_none());
        assert!(session_feedback(&mut server, &mut other).is_none());
        server
            .handle_request(request, &mut sender, &mut output)
            .unwrap();
        assert!(
            sender.gameplay_feedback_owner.is_none(),
            "replay must not upgrade the connection"
        );
        assert!(
            session_feedback(&mut server, &mut sender).is_none(),
            "replay must not import the player's latest cache record"
        );
    }
}

#[test]
fn runtime_queued_gameplay_feedback_follows_its_real_action_success_and_rejection() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .unwrap();
    let agent = server.world.state().agents.keys().next().unwrap().clone();
    let (public, private) = test_signer(216);
    register_runtime_session(
        &mut server,
        "runtime-result-owner",
        Some(&agent),
        1,
        &public,
        &private,
    );
    let mut sender = RuntimeLiveSession::new();
    let mut output = Vec::new();
    // Keep the existing LLM admission context while advancing deterministic
    // script ticks, so this test exercises the real advance path without I/O.
    server.config.decision_mode = ViewerLiveDecisionMode::Script;
    for (nonce, expected) in [(2, "completed_advanced"), (3, "rejected")] {
        let request = signed_gameplay_action_request(
            crate::viewer::GameplayActionRequest {
                action_id: crate::viewer::ACTION_CLAIM_STARTER_OC.into(),
                target_agent_id: agent.clone(),
                actor_agent_id: None,
                player_id: "runtime-result-owner".into(),
                public_key: None,
                auth: None,
            },
            nonce,
            &public,
            &private,
        );
        output.clear();
        server
            .handle_request(
                ViewerRequest::GameplayAction { request },
                &mut sender,
                &mut output,
            )
            .unwrap();
        let first: ViewerResponse =
            serde_json::from_slice(output.split(|byte| *byte == b'\n').next().unwrap()).unwrap();
        let ViewerResponse::GameplayActionAck { ack } = first else {
            panic!("expected queued action ack: {first:?}")
        };
        assert_eq!(
            session_feedback(&mut server, &mut sender).unwrap().stage,
            "queued"
        );
        server
            .advance_runtime(&mut sender, &mut output, "step", 1, None, true)
            .unwrap();
        let result = session_feedback(&mut server, &mut sender).unwrap();
        assert_eq!(result.stage, expected);
        assert!(
            server.world.journal().events.iter().any(|event| {
                event.caused_by == Some(crate::runtime::CausedBy::Action(ack.runtime_action_id))
                    && matches!(
                        &event.body,
                        RuntimeWorldEventBody::Domain(
                            RuntimeDomainEvent::StarterOcClaimed { .. }
                                | RuntimeDomainEvent::ActionRejected { .. }
                        )
                    )
            }),
            "feedback must be backed by this exact Runtime action"
        );
        let mut anonymous = RuntimeLiveSession::new();
        assert!(session_feedback(&mut server, &mut anonymous).is_none());
    }
}
