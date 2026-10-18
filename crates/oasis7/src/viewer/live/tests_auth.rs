use super::*;

#[test]
fn prompt_control_preview_reports_fields_and_next_version() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, private_key) = test_signer(11);

    let ack = world
        .prompt_control_preview(signed_prompt_control_apply_request(
            PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
                player_id: "player-a".to_string(),
                public_key: None,
                auth: None,
                strong_auth_grant: None,
                expected_version: Some(0),
                updated_by: None,
                system_prompt_override: Some(Some("系统提示".to_string())),
                short_term_goal_override: None,
                long_term_goal_override: None,
                ..Default::default()
            },
            PromptControlAuthIntent::Preview,
            1,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("preview ack");

    assert!(ack.preview);
    assert_eq!(ack.version, 1);
    assert_eq!(ack.operation, PromptControlOperation::Apply);
    assert_eq!(
        ack.applied_fields,
        vec!["system_prompt_override".to_string()]
    );
    assert!(!ack.digest.is_empty());
}

#[test]
fn prompt_control_apply_requires_llm_mode() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, private_key) = test_signer(12);

    let err = world
        .prompt_control_apply(signed_prompt_control_apply_request(
            PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
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
            PromptControlAuthIntent::Apply,
            2,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("script mode should reject apply");

    assert_eq!(err.code, "llm_mode_required");
    assert!(world.kernel.model().agent_prompt_profiles.is_empty());
    assert!(world.kernel.model().agent_player_bindings.is_empty());
    assert!(!world.kernel.journal().iter().any(|event| matches!(
        event.kind,
        oasis7_simulator::WorldEventKind::AgentPromptUpdated { .. }
    )));
}

#[test]
fn prompt_profile_version_lookup_reads_from_journal() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");

    let mut profile = AgentPromptProfile::for_agent("agent-0");
    profile.system_prompt_override = Some("v1".to_string());
    profile.version = 1;
    profile.updated_at_tick = world.kernel.time();
    profile.updated_by = "test".to_string();
    world.kernel.apply_agent_prompt_profile_update(
        profile.clone(),
        PromptUpdateOperation::Apply,
        vec!["system_prompt_override".to_string()],
        "digest-1".to_string(),
        None,
    );

    let loaded = world
        .lookup_prompt_profile_version("agent-0", 1)
        .expect("profile v1");
    assert_eq!(loaded.system_prompt_override.as_deref(), Some("v1"));
    assert_eq!(loaded.version, 1);
}

#[test]
fn prompt_control_preview_requires_non_empty_player_id() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");

    let err = world
        .prompt_control_preview(PromptControlApplyRequest {
            agent_id: "agent-0".to_string(),
            player_id: "   ".to_string(),
            public_key: None,
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        })
        .expect_err("empty player id should be rejected");

    assert_eq!(err.code, "player_id_required");
}

#[test]
fn prompt_control_preview_requires_auth_proof() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, _) = test_signer(21);

    let err = world
        .prompt_control_preview(PromptControlApplyRequest {
            agent_id: "agent-0".to_string(),
            player_id: "player-a".to_string(),
            public_key: Some(public_key),
            auth: None,
            strong_auth_grant: None,
            expected_version: Some(0),
            updated_by: None,
            system_prompt_override: Some(Some("system".to_string())),
            short_term_goal_override: None,
            long_term_goal_override: None,
            ..Default::default()
        })
        .expect_err("missing proof should be rejected");

    assert_eq!(err.code, "auth_proof_required");
}

#[test]
fn prompt_control_preview_rejects_tampered_auth_signature() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, private_key) = test_signer(22);

    let request = signed_prompt_control_apply_request(
        PromptControlApplyRequest {
            agent_id: "agent-0".to_string(),
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
        PromptControlAuthIntent::Preview,
        7,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut tampered = request.clone();
    tampered.system_prompt_override = Some(Some("tampered".to_string()));

    let err = world
        .prompt_control_preview(tampered)
        .expect_err("tampered payload should be rejected");
    assert_eq!(err.code, "auth_signature_invalid");
}

#[test]
fn prompt_control_preview_rejects_replayed_nonce() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, private_key) = test_signer(23);
    let request = signed_prompt_control_apply_request(
        PromptControlApplyRequest {
            agent_id: "agent-0".to_string(),
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
        PromptControlAuthIntent::Preview,
        8,
        public_key.as_str(),
        private_key.as_str(),
    );

    let first = world
        .prompt_control_preview(request.clone())
        .expect("first request accepted");
    assert!(first.preview);

    let replay = world
        .prompt_control_preview(request)
        .expect_err("replay request should be rejected");
    assert_eq!(replay.code, "auth_nonce_replay");
}

#[test]
fn prompt_control_preview_rejects_unbound_player_when_agent_already_bound() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (public_key, private_key) = test_signer(13);

    let bind_event = world
        .kernel
        .bind_agent_player("agent-0", "player-a", None)
        .expect("bind ok");
    assert!(bind_event.is_some());

    let err = world
        .prompt_control_preview(signed_prompt_control_apply_request(
            PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
                player_id: "player-b".to_string(),
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
            PromptControlAuthIntent::Preview,
            3,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("mismatched player should be rejected");

    assert_eq!(err.code, "agent_control_forbidden");
    assert_eq!(
        world.kernel.player_binding_for_agent("agent-0"),
        Some("player-a")
    );
}

#[test]
fn prompt_control_preview_requires_matching_public_key_when_agent_is_key_bound() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");
    let (bound_public_key, bound_private_key) = test_signer(14);
    let (wrong_public_key, wrong_private_key) = test_signer(15);

    let bind_event = world
        .kernel
        .bind_agent_player("agent-0", "player-a", Some(bound_public_key.as_str()))
        .expect("bind ok");
    assert!(bind_event.is_some());

    let missing_key = world
        .prompt_control_preview(PromptControlApplyRequest {
            agent_id: "agent-0".to_string(),
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
        })
        .expect_err("missing public key should be rejected");
    assert_eq!(missing_key.code, "auth_proof_required");

    let wrong_key = world
        .prompt_control_preview(signed_prompt_control_apply_request(
            PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
                player_id: "player-a".to_string(),
                public_key: Some(wrong_public_key.clone()),
                auth: None,
                strong_auth_grant: None,
                expected_version: Some(0),
                updated_by: None,
                system_prompt_override: Some(Some("system".to_string())),
                short_term_goal_override: None,
                long_term_goal_override: None,
                ..Default::default()
            },
            PromptControlAuthIntent::Preview,
            4,
            wrong_public_key.as_str(),
            wrong_private_key.as_str(),
        ))
        .expect_err("mismatched public key should be rejected");
    assert_eq!(wrong_key.code, "agent_control_forbidden");

    let ack = world
        .prompt_control_preview(signed_prompt_control_apply_request(
            PromptControlApplyRequest {
                agent_id: "agent-0".to_string(),
                player_id: "player-a".to_string(),
                public_key: Some(bound_public_key.clone()),
                auth: None,
                strong_auth_grant: None,
                expected_version: Some(0),
                updated_by: None,
                system_prompt_override: Some(Some("system".to_string())),
                short_term_goal_override: None,
                long_term_goal_override: None,
                ..Default::default()
            },
            PromptControlAuthIntent::Preview,
            5,
            bound_public_key.as_str(),
            bound_private_key.as_str(),
        ))
        .expect("matching public key should pass");
    assert!(ack.preview);
}

#[test]
fn agent_chat_requires_player_id() {
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Script).expect("init ok");

    let err = world
        .agent_chat(AgentChatRequest {
            agent_id: "agent-0".to_string(),
            message: "hello".to_string(),
            player_id: None,
            public_key: None,
            auth: None,
            intent_tick: None,
            intent_seq: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        })
        .expect_err("missing player_id should be rejected");

    assert_eq!(err.code, "player_id_required");
}

#[test]
fn agent_chat_rejects_replayed_nonce() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Llm).expect("init ok");
    let (public_key, private_key) = test_signer(24);
    let request = signed_agent_chat_request(
        AgentChatRequest {
            agent_id: "agent-0".to_string(),
            message: "hello".to_string(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            intent_tick: None,
            intent_seq: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        9,
        public_key.as_str(),
        private_key.as_str(),
    );

    let first = world
        .agent_chat(request.clone())
        .expect("first request accepted");
    assert_eq!(first.player_id.as_deref(), Some("player-a"));

    let replay = world
        .agent_chat(request)
        .expect_err("replay request should be rejected");
    assert_eq!(replay.code, "auth_nonce_replay");
}

#[test]
fn agent_chat_upgrades_compat_player_binding_with_public_key() {
    let _env_guard = set_test_llm_env();
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::Minimal, &config);
    let mut world = LiveWorld::new(config, init, ViewerLiveDecisionMode::Llm).expect("init ok");
    let (public_key, private_key) = test_signer(16);

    let bind_event = world
        .kernel
        .bind_agent_player("agent-0", "player-a", None)
        .expect("legacy bind ok");
    assert!(bind_event.is_some());
    assert_eq!(world.kernel.public_key_binding_for_agent("agent-0"), None);

    let ack = world
        .agent_chat(signed_agent_chat_request(
            AgentChatRequest {
                agent_id: "agent-0".to_string(),
                message: "hello".to_string(),
                player_id: Some("player-a".to_string()),
                public_key: Some(public_key.clone()),
                auth: None,
                intent_tick: None,
                intent_seq: None,
                world_id: None,
                reorg_epoch: None,
                authority_scope: None,
                replaces_intent_id: None,
                canonical_authority: None,
            },
            6,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("chat should be accepted");

    assert_eq!(ack.player_id.as_deref(), Some("player-a"));
    assert_eq!(
        world.kernel.public_key_binding_for_agent("agent-0"),
        Some(public_key.as_str())
    );
}
