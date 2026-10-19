use super::*;

#[test]
fn runtime_prompt_control_changed_preview_requires_explicit_apply_confirmation() {
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
        "player-preview-confirm",
        Some(agent_id.as_str()),
        1,
        public_key.as_str(),
        private_key.as_str(),
    );
    let request = signed_prompt_control_apply_request(
        crate::viewer::PromptControlApplyRequest {
            agent_id,
            player_id: "player-preview-confirm".to_string(),
            expected_version: Some(0),
            updated_by: Some("player-preview-confirm".to_string()),
            system_prompt_override: Some(Some("preview-only".to_string())),
            request_id: Some("preview-confirmation".to_string()),
            session_epoch: registration.session_epoch,
            binding_epoch: registration.binding_epoch,
            expected_authority_epoch: Some(server.prompt_control_authority.authority_epoch.clone()),
            ..Default::default()
        },
        crate::viewer::PromptControlAuthIntent::Preview,
        2,
        public_key.as_str(),
        private_key.as_str(),
    );
    let negotiated = crate::viewer::protocol::NegotiatedViewerProtocol {
        version: crate::viewer::VIEWER_PROTOCOL_VERSION,
        capabilities: vec![crate::viewer::protocol::PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
    };
    let preview = server
        .handle_prompt_control_for_protocol(
            crate::viewer::PromptControlCommand::Preview { request },
            &negotiated,
        )
        .expect("changed preview is accepted without mutation");
    assert_eq!(
        preview.status,
        Some(crate::viewer::protocol::PromptControlResultStatus::Accepted)
    );
    assert_eq!(
        preview.applied_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::None)
    );
    assert_eq!(
        preview.persistence_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::None)
    );
    assert_eq!(
        preview.sync_scope,
        Some(crate::viewer::protocol::PromptControlApplicationScope::None)
    );
    assert_eq!(preview.mutation_count, Some(0));
    assert_eq!(preview.reason_code.as_deref(), Some("preview_only"));
    assert_eq!(preview.next_step.as_deref(), Some("confirm_apply"));
    assert_eq!(
        server
            .llm_sidecar
            .prompt_profiles
            .get(preview.agent_id.as_str())
            .map(|profile| profile.version),
        None
    );
}
