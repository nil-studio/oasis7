#[test]
fn empty_entity_guard_marks_gameplay_snapshot_blocked() {
    let mut gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &crate::runtime::WorldState::default(),
        None,
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );
    super::super::gameplay_snapshot::apply_runtime_snapshot_empty_entities_blocker(
        &mut gameplay,
        true,
        true,
    );
    assert_eq!(
        gameplay.stage_status,
        crate::simulator::PlayerGameplayStageStatus::Blocked
    );
    assert_eq!(
        gameplay.blocker_kind.as_deref(),
        Some("runtime_snapshot_empty_entities")
    );
    assert!(
        gameplay
            .blocker_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("no agents/locations"))
    );
    let request_snapshot_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.protocol_action == "request_snapshot")
        .expect("request_snapshot action should be available");
    assert_eq!(request_snapshot_action.protocol_action, "request_snapshot");
    let first_agent_claim_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("first-agent claim action should remain available");
    assert_eq!(
        first_agent_claim_action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(first_agent_claim_action.disabled_reason.is_none());
    assert!(
        gameplay
            .available_actions
            .iter()
            .filter(|action| {
                action.protocol_action != "request_snapshot"
                    && action.action_id != crate::viewer::ACTION_CLAIM_FIRST_AGENT
            })
            .all(|action| action.disabled_reason.is_some())
    );
}

#[test]
fn empty_runtime_snapshot_publishes_first_agent_claim_action() {
    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &crate::runtime::WorldState::default(),
        None,
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("first-agent claim action");
    assert_eq!(action.protocol_action, "gameplay_action.submit");
    assert_eq!(
        action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(action.disabled_reason.is_none());
}

#[test]
fn formal_release_empty_runtime_snapshot_publishes_first_agent_claim_action() {
    let _guard = super::super::canonical_runtime_provider_env_lock()
        .lock()
        .expect("env lock");
    let mut server = super::super::ViewerRuntimeLiveServer::new(
        super::super::ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_decision_mode(super::super::ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");

    let snapshot = server.compat_snapshot(Some("new-player"));
    let gameplay = snapshot
        .player_gameplay
        .as_ref()
        .expect("player gameplay snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("first-agent claim action");
    assert_eq!(action.protocol_action, "gameplay_action.submit");
    assert_eq!(
        action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(action.disabled_reason.is_none());
}

#[test]
fn stale_starter_binding_without_runtime_agent_keeps_first_agent_claim_action() {
    let _guard = super::super::canonical_runtime_provider_env_lock()
        .lock()
        .expect("env lock");
    let mut server = super::super::ViewerRuntimeLiveServer::new(
        super::super::ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_decision_mode(super::super::ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server
        .llm_sidecar
        .bind_agent_player(
            crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID,
            "stale-player",
            None,
            false,
        )
        .expect("seed stale binding");
    server.world = crate::runtime::World::new_production_hardened();
    assert!(server.world.state().agents.is_empty());

    let snapshot = server.compat_snapshot(Some("new-player"));
    let gameplay = snapshot
        .player_gameplay
        .as_ref()
        .expect("player gameplay snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("first-agent claim action");
    assert_eq!(
        action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(action.disabled_reason.is_none());
}

#[test]
fn existing_world_without_bound_agent_publishes_first_agent_claim_action() {
    let mut world = crate::runtime::World::new_production_hardened();
    world.submit_action(crate::runtime::Action::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: crate::viewer::runtime_live::test_first_agent_spawn_pos()
            .expect("formal release starter spawn"),
    });
    world.step().expect("register existing non-starter agent");
    assert!(!world.state().agents.is_empty());
    assert!(
        !world
            .state()
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID),
        "minimal world should not already occupy the starter claim target"
    );

    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        None,
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("fresh account should be able to claim its first agent in an existing world");
    assert_eq!(action.protocol_action, "gameplay_action.submit");
    assert_eq!(
        action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(action.disabled_reason.is_none());
}

#[test]
fn existing_world_with_unbound_starter_target_publishes_first_agent_claim_action() {
    let mut world = crate::runtime::World::new_production_hardened();
    world.submit_action(crate::runtime::Action::RegisterAgent {
        agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
        pos: crate::viewer::runtime_live::test_first_agent_spawn_pos()
            .expect("formal release starter spawn"),
    });
    world.step().expect("register unbound starter claim target");
    assert!(
        world
            .state()
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );

    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        None,
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("fresh account should be able to claim an existing unbound starter agent");
    assert_eq!(action.protocol_action, "gameplay_action.submit");
    assert_eq!(
        action.target_agent_id.as_deref(),
        Some(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );
    assert!(action.disabled_reason.is_none());
}

#[test]
fn existing_world_with_bound_starter_claim_target_does_not_publish_duplicate_first_agent_claim() {
    let mut world = crate::runtime::World::new_production_hardened();
    world.submit_action(crate::runtime::Action::RegisterAgent {
        agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID.to_string(),
        pos: crate::viewer::runtime_live::test_first_agent_spawn_pos()
            .expect("formal release starter spawn"),
    });
    world.step().expect("register starter claim target");
    assert!(
        world
            .state()
            .agents
            .contains_key(crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
    );

    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        world.state(),
        None,
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        false,
        None,
    );
    assert!(
        gameplay
            .available_actions
            .iter()
            .all(|action| action.action_id != crate::viewer::ACTION_CLAIM_FIRST_AGENT),
        "do not publish a duplicate first-agent claim when the target id already exists"
    );
}

#[test]
fn runtime_sync_blocker_preserves_empty_world_first_agent_claim() {
    let feedback = crate::simulator::PlayerGameplayRecentFeedback {
        action: "chain_sync".to_string(),
        stage: "blocked".to_string(),
        effect: "committed runtime sync failed before the viewer could observe new world state"
            .to_string(),
        intent_summary: None,
        target_agent_id: None,
        reason: Some("simulated missing persistence".to_string()),
        hint: Some("wait for execution world persistence".to_string()),
        delta_logical_time: 0,
        delta_event_seq: 0,
    };
    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &crate::runtime::WorldState::default(),
        None,
        false,
        Some(&feedback),
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    );

    assert_eq!(
        gameplay.blocker_kind.as_deref(),
        Some("runtime_sync_unavailable")
    );
    let first_agent_claim_action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == crate::viewer::ACTION_CLAIM_FIRST_AGENT)
        .expect("first-agent claim action should remain available");
    assert!(first_agent_claim_action.disabled_reason.is_none());
}
