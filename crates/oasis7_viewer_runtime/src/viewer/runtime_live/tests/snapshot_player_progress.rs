use super::*;

#[test]
fn player_snapshot_background_progress_does_not_complete_first_session() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.confirm_player_gameplay_progress();
    server.set_latest_player_gameplay_feedback(
        ViewerRuntimeLiveServer::make_player_gameplay_feedback(
            "step",
            "completed_advanced",
            "background tick",
            None,
            None,
            None,
            None,
            1,
            1,
        ),
    );
    for bound in [false, true] {
        if bound {
            let agent_id = server.world.state().agents.keys().next().unwrap().clone();
            bind_agent_for_snapshot(&mut server, &agent_id);
        }
        let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
        let gameplay = snapshot.player_gameplay.expect("player gameplay");
        assert_eq!(
            gameplay.stage_id,
            crate::simulator::PlayerGameplayStageId::FirstSessionLoop,
            "background world ticks are not this player's completed loop (bound={bound})"
        );
        assert_eq!(gameplay.progress_percent, 0);
    }
}

#[test]
fn player_snapshot_does_not_inherit_another_players_factory_progress() {
    let _guard = lock_test_llm_env();
    let (mut server, agent_id, public_key, private_key) =
        super::super::industrial_progression::setup_runtime_industrial_gameplay_session(94);
    super::super::industrial_progression::build_first_smelter_via_gameplay_action(
        &mut server,
        &agent_id,
        &public_key,
        &private_key,
        94,
    );
    let owner = server
        .compat_snapshot(Some("player-a"))
        .player_gameplay
        .unwrap();
    assert_eq!(
        owner.stage_id,
        crate::simulator::PlayerGameplayStageId::PostOnboarding
    );
    let newcomer = server
        .compat_snapshot(Some("new-player"))
        .player_gameplay
        .unwrap();
    assert_eq!(
        newcomer.stage_id,
        crate::simulator::PlayerGameplayStageId::FirstSessionLoop
    );
    assert_eq!(newcomer.progress_percent, 0);
}

fn player_snapshot_from_intent(
    intent: crate::runtime_host::archive::PlayerGameplayPrimaryIntent,
) -> crate::simulator::PlayerGameplaySnapshot {
    let feedback = ViewerRuntimeLiveServer::make_player_gameplay_feedback(
        "step",
        "completed_advanced",
        "shared background tick",
        None,
        None,
        None,
        None,
        1,
        1,
    );
    crate::viewer::runtime_live::gameplay_snapshot::build_player_gameplay_snapshot(
        &crate::runtime::WorldState::default(),
        Some("agent-a"),
        true,
        Some(&feedback),
        Some(intent),
        None,
        true,
        None,
        false,
        false,
        None,
        true,
    )
}

#[test]
fn player_snapshot_completion_requires_current_owner_bound_receipt() {
    let base = serde_json::json!({
        "status": "completed", "intent_id": "intent-a", "agent_id": "agent-a",
        "source_class": "runtime_projection", "freshness": "current",
        "receipt_ref": {"intent_id": "intent-a", "world_id": "world-a",
            "reorg_epoch": 0, "logical_time": 1, "event_seq": "2",
            "receipt_id": "world-event:1"}
    });
    for (field, value) in [
        ("status", serde_json::json!("accepted")),
        ("status", serde_json::json!("blocked")),
        (
            "source_class",
            serde_json::json!("prompt_control_compatibility"),
        ),
        ("freshness", serde_json::json!("stale")),
        ("agent_id", serde_json::json!("agent-b")),
        ("receipt_ref", serde_json::Value::Null),
        ("receipt_ref", serde_json::json!("world-event:1")),
        (
            "receipt_ref",
            serde_json::json!({"intent_id": "other", "receipt_id": "world-event:1"}),
        ),
    ] {
        let mut input = base.clone();
        input[field] = value;
        let intent = serde_json::from_value(input).unwrap();
        let snapshot = player_snapshot_from_intent(intent);
        assert_eq!(
            snapshot.stage_id,
            crate::simulator::PlayerGameplayStageId::FirstSessionLoop,
            "{field} must not turn shared delta into this player's completion"
        );
        assert_eq!(snapshot.progress_percent, 0);
    }
    // Reconstructing the same canonical DTO after reload preserves the result;
    // no Viewer process-local completion flag is needed.
    let encoded = serde_json::to_string(&base).unwrap();
    for _ in 0..2 {
        let intent = serde_json::from_str(&encoded).unwrap();
        let snapshot = player_snapshot_from_intent(intent);
        assert_eq!(
            snapshot.stage_id,
            crate::simulator::PlayerGameplayStageId::PostOnboarding
        );
        assert_eq!(snapshot.progress_percent, 20);
        assert!(
            snapshot
                .starter_industrial_feasibility
                .unwrap()
                .settled_outcome
                .is_none(),
            "a completed action receipt does not claim industrial production completion"
        );
    }
}

#[test]
fn player_snapshot_ignores_unattributed_material_transit_counters() {
    let mut state = crate::runtime::WorldState::default();
    state.industry_progress.completed_material_transits = 3;
    for player_scoped in [true, false] {
        let snapshot =
            crate::viewer::runtime_live::gameplay_snapshot::build_player_gameplay_snapshot(
                &state,
                None,
                false,
                None,
                None,
                None,
                true,
                None,
                false,
                true,
                None,
                player_scoped,
            );
        assert_eq!(
            snapshot.stage_id,
            if player_scoped {
                crate::simulator::PlayerGameplayStageId::FirstSessionLoop
            } else {
                crate::simulator::PlayerGameplayStageId::PostOnboarding
            }
        );
        assert_eq!(
            snapshot.progress_percent,
            if player_scoped { 0 } else { 38 }
        );
    }
}
