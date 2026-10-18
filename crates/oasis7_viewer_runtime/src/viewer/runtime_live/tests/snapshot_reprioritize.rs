use super::*;

const SNAPSHOT_PLAYER_ID: &str = "player-snapshot";

fn bind_agent_for_snapshot(server: &mut ViewerRuntimeLiveServer, agent_id: &str) {
    server
        .llm_sidecar
        .bind_agent_player(
            agent_id,
            SNAPSHOT_PLAYER_ID,
            Some("snapshot-public-key"),
            false,
        )
        .expect("bind snapshot player to agent");
}

#[test]
fn compat_snapshot_publishes_enabled_reprioritize_for_the_bound_player_agent() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
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
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    bind_agent_for_snapshot(&mut server, agent_id.as_str());

    let gameplay = server
        .compat_snapshot(Some(SNAPSHOT_PLAYER_ID))
        .player_gameplay
        .expect("bound player gameplay snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "reprioritize")
        .expect("bound player must receive the visible reprioritize action");
    assert_eq!(action.protocol_action, "prompt_control.apply");
    assert_eq!(action.target_agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(action.disabled_reason.is_none());

    let contract = serde_json::to_value(&gameplay).expect("serialize gameplay snapshot");
    assert_eq!(
        contract
            .get("can_reprioritize")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "the player-visible action and canonical snapshot availability must agree"
    );
}

#[test]
fn compat_snapshot_explains_when_bound_player_reprioritize_is_unavailable() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
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
    bind_agent_for_snapshot(&mut server, agent_id.as_str());

    let gameplay = server
        .compat_snapshot(Some(SNAPSHOT_PLAYER_ID))
        .player_gameplay
        .expect("bound player gameplay snapshot");
    let action = gameplay
        .available_actions
        .iter()
        .find(|action| action.action_id == "reprioritize")
        .expect("unsupported mode must still explain reprioritize availability");
    assert_eq!(action.target_agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(
        action
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty()),
        "unsupported mode must give the player an actionable disabled reason"
    );
}
