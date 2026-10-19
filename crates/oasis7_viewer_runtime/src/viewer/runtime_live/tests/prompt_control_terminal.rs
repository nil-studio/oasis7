use super::*;

#[test]
fn runtime_prompt_control_rollback_terminal_errors_are_replayable_and_reserved() {
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
        "player-rollback-terminal",
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
    let make_request = |request_id: &str, to_version: u64, nonce: u64| {
        signed_prompt_control_rollback_request(
            crate::viewer::PromptControlRollbackRequest {
                agent_id: agent_id.clone(),
                player_id: "player-rollback-terminal".to_string(),
                expected_version: Some(0),
                updated_by: Some("player-rollback-terminal".to_string()),
                to_version,
                request_id: Some(request_id.to_string()),
                session_epoch: registration.session_epoch,
                binding_epoch: registration.binding_epoch,
                expected_authority_epoch: Some(authority_epoch.clone()),
                ..Default::default()
            },
            nonce,
            public_key.as_str(),
            private_key.as_str(),
        )
    };

    let no_op_request = make_request("rollback-noop", 0, 2);
    let no_op = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: no_op_request.clone(),
            },
            &negotiated,
        )
        .expect_err("rollback to the current baseline must be a terminal no-op");
    assert_eq!(no_op.code, "rollback_noop");
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-rollback-terminal"),
        Some(&2)
    );
    let no_op_replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: no_op_request,
            },
            &negotiated,
        )
        .expect_err("same no-op rollback must replay its terminal receipt");
    assert_eq!(no_op_replay.code, "rollback_noop");
    assert!(no_op_replay.idempotent_replay);

    let target_request = make_request("rollback-target-missing", 999, 3);
    let target_error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: target_request.clone(),
            },
            &negotiated,
        )
        .expect_err("unknown rollback target must be a terminal result");
    assert_eq!(target_error.code, "target_version_not_found");
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-rollback-terminal"),
        Some(&3)
    );
    let target_replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: target_request,
            },
            &negotiated,
        )
        .expect_err("same missing target must replay its terminal receipt");
    assert_eq!(target_replay.code, "target_version_not_found");
    assert!(target_replay.idempotent_replay);

    let conflict_request = make_request("rollback-target-missing", 998, 4);
    let conflict = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: conflict_request,
            },
            &negotiated,
        )
        .expect_err("same rollback request id with another target must conflict");
    assert_eq!(conflict.code, "request_id_conflict");
    assert!(!conflict.idempotent_replay);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-rollback-terminal"),
        Some(&3)
    );
}

#[test]
fn runtime_prompt_control_enqueue_failure_is_a_replayable_terminal_result() {
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
    let (public_key, private_key) = test_signer(51);
    let registration = register_runtime_session(
        &mut server,
        "player-enqueue-failure",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let initial_event_count = server.pending_virtual_events.len();
    server.llm_sidecar.replace_builtin_runner_for_test(
        crate::simulator::AsyncAgentRunner::with_default_capacity(),
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
                player_id: "player-enqueue-failure".to_string(),
                expected_version: Some(0),
                updated_by: Some("player-enqueue-failure".to_string()),
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
    let failed_request = make_request("enqueue-failure", 2, "must-not-apply");
    let first_error = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: failed_request.clone(),
            },
            &negotiated,
        )
        .expect_err("missing native actor must fail at prompt override enqueue");
    assert_eq!(first_error.code, "prompt_override_enqueue_failed");
    assert_eq!(
        first_error.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Blocked)
    );
    assert!(first_error.operation_digest.is_none());
    let first_error_wire = serde_json::to_string(&first_error).expect("serialize hidden error");
    assert!(!first_error_wire.contains("operation_digest"));
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-enqueue-failure"),
        Some(&2)
    );
    assert!(server.llm_sidecar.prompt_profiles.is_empty());
    assert_eq!(server.pending_virtual_events.len(), initial_event_count);

    let replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: failed_request,
            },
            &negotiated,
        )
        .expect_err("same enqueue failure must replay its terminal receipt");
    assert_eq!(replay.code, "prompt_override_enqueue_failed");
    assert!(replay.idempotent_replay);
    assert!(replay.operation_digest.is_none());
    let replay_wire = serde_json::to_string(&replay).expect("serialize hidden replay");
    assert!(!replay_wire.contains("operation_digest"));
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-enqueue-failure"),
        Some(&2)
    );

    let conflict = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: make_request("enqueue-failure", 3, "different-operation"),
            },
            &negotiated,
        )
        .expect_err("same request id with a different enqueue digest must conflict");
    assert_eq!(conflict.code, "request_id_conflict");
    assert!(!conflict.idempotent_replay);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("player-enqueue-failure"),
        Some(&2)
    );
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
