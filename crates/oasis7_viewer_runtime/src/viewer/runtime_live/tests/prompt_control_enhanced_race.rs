use super::*;

#[test]
fn runtime_prompt_control_two_key_force_rebind_blocks_old_apply_and_rollback() {
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
    let player_id = "player-two-key-rebind-a";
    let (old_public_key, old_private_key) = test_signer(111);
    let old_registration = register_runtime_session(
        &mut server,
        player_id,
        Some(agent_id.as_str()),
        1,
        old_public_key.as_str(),
        old_private_key.as_str(),
    );
    let authority_epoch = server.prompt_control_authority.authority_epoch.clone();
    let old_binding_epoch = old_registration.binding_epoch;
    let apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: player_id.to_string(),
            expected_version: Some(0),
            updated_by: Some(player_id.to_string()),
            system_prompt_override: Some(Some("must not apply after key rebind".to_string())),
            request_id: Some("two-key-rebind-apply".to_string()),
            session_epoch: old_registration.session_epoch,
            binding_epoch: old_binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Apply,
        2,
        old_public_key.as_str(),
        old_private_key.as_str(),
    );
    let rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.clone(),
            player_id: player_id.to_string(),
            expected_version: Some(0),
            updated_by: Some(player_id.to_string()),
            to_version: 0,
            request_id: Some("two-key-rebind-rollback".to_string()),
            session_epoch: old_registration.session_epoch,
            binding_epoch: old_binding_epoch,
            expected_authority_epoch: Some(authority_epoch),
            ..Default::default()
        },
        3,
        old_public_key.as_str(),
        old_private_key.as_str(),
    );

    let (new_public_key, new_private_key) = test_signer(112);
    let new_registration = register_runtime_session_with_options(
        &mut server,
        "player-two-key-rebind-b",
        Some(agent_id.as_str()),
        true,
        2,
        new_public_key.as_str(),
        new_private_key.as_str(),
    );
    assert_eq!(new_registration.session_epoch, Some(1));
    assert_eq!(
        new_registration.binding_epoch,
        old_binding_epoch.map(|epoch| epoch + 1)
    );

    let state_before_old_requests = (
        server.llm_sidecar.prompt_profiles.clone(),
        server.llm_sidecar.player_auth_last_nonce.clone(),
        server.prompt_control_authority.result_ledger.len(),
        server.pending_virtual_events.len(),
    );
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    for command in [
        crate::viewer::PromptControlCommand::Apply {
            request: apply_request,
        },
        crate::viewer::PromptControlCommand::Rollback {
            request: rollback_request,
        },
    ] {
        let error = server
            .handle_prompt_control_for_protocol(command, &negotiated)
            .expect_err("old session key must lose control after force rebind");
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
        assert!(error.digest.is_none());
        assert!(error.operation_digest.is_none());
    }
    assert_eq!(
        (
            server.llm_sidecar.prompt_profiles.clone(),
            server.llm_sidecar.player_auth_last_nonce.clone(),
            server.prompt_control_authority.result_ledger.len(),
            server.pending_virtual_events.len(),
        ),
        state_before_old_requests,
        "old-key Apply/Rollback must not mutate state, consume a nonce, or cache a result"
    );
}

#[test]
fn runtime_prompt_control_enhanced_rollback_replays_same_digest_and_conflicts_on_new_digest() {
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
    let player_id = "player-rollback-ledger";
    let (public_key, private_key) = test_signer(113);
    let registration = register_runtime_session(
        &mut server,
        player_id,
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
    let apply_request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id: agent_id.clone(),
            player_id: player_id.to_string(),
            expected_version: Some(0),
            updated_by: Some(player_id.to_string()),
            system_prompt_override: Some(Some("rollback-ledger-value".to_string())),
            request_id: Some("rollback-ledger-seed".to_string()),
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
    let applied = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: apply_request,
            },
            &negotiated,
        )
        .expect("seed Apply must apply");
    assert_eq!(
        applied.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(applied.version, 1);

    let rollback_request = signed_prompt_control_rollback_request(
        crate::viewer::PromptControlRollbackRequest {
            agent_id: agent_id.clone(),
            player_id: player_id.to_string(),
            expected_version: Some(1),
            updated_by: Some(player_id.to_string()),
            to_version: 0,
            request_id: Some("rollback-ledger-operation".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(authority_epoch.clone()),
            ..Default::default()
        },
        3,
        public_key.as_str(),
        private_key.as_str(),
    );
    let first_rollback = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: rollback_request.clone(),
            },
            &negotiated,
        )
        .expect("first rollback must apply");
    assert_eq!(
        first_rollback.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(first_rollback.rolled_back_to_version, Some(0));
    assert_eq!(first_rollback.version, 2);

    let replay = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: rollback_request.clone(),
            },
            &negotiated,
        )
        .expect("same rollback request must replay its receipt");
    assert!(replay.idempotent_replay);
    assert_eq!(replay.version, first_rollback.version);
    assert_eq!(replay.digest, first_rollback.digest);
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .map(|profile| profile.version),
        Some(2),
        "same-digest rollback replay must not apply a third mutation"
    );

    let mut conflict_request = rollback_request;
    conflict_request.expected_version = Some(2);
    conflict_request.auth = None;
    let conflict_request = signed_prompt_control_rollback_request(
        conflict_request,
        4,
        public_key.as_str(),
        private_key.as_str(),
    );
    let conflict = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Rollback {
                request: conflict_request,
            },
            &negotiated,
        )
        .expect_err("same request ID with a different rollback digest must conflict");
    assert_eq!(conflict.code, "request_id_conflict");
    assert_eq!(
        conflict.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Rejected)
    );
    assert!(!conflict.idempotent_replay);
    assert_eq!(
        server.llm_sidecar.player_auth_last_nonce.get(player_id),
        Some(&3),
        "request-ID conflict must not consume a fresh nonce"
    );
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .map(|profile| profile.version),
        Some(2)
    );
}

#[test]
fn runtime_prompt_control_enhanced_two_requests_same_baseline_apply_one_and_stale_one() {
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
    let (public_key, private_key) = test_signer(114);
    let registration = register_runtime_session(
        &mut server,
        "player-collision",
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
    let make_request = |request_id: &str, nonce: u64, value: &str| {
        signed_prompt_control_apply_request(
            crate::viewer::PromptControlApplyRequest {
                agent_id: agent_id.clone(),
                player_id: "player-collision".to_string(),
                expected_version: Some(0),
                updated_by: Some("player-collision".to_string()),
                system_prompt_override: Some(Some(value.to_string())),
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
                request: make_request("same-baseline-a", 2, "collision-a"),
            },
            &negotiated,
        )
        .expect("first same-baseline request must apply");
    let second = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Apply {
                request: make_request("same-baseline-b", 3, "collision-b"),
            },
            &negotiated,
        )
        .expect_err("second same-baseline request must be stale");
    assert_eq!(
        first.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Applied)
    );
    assert_eq!(first.mutation_count, Some(1));
    assert_eq!(second.code, "version_conflict");
    assert_eq!(
        second.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Stale)
    );
    assert_eq!(second.current_version, Some(1));
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(agent_id.as_str())
            .and_then(|profile| profile.system_prompt_override.as_deref()),
        Some("collision-a")
    );
}
