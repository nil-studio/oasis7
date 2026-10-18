use super::*;

const SNAPSHOT_PLAYER_ID: &str = "player-snapshot";

#[test]
fn compat_snapshot_serializes_stalled_and_blocked_fallback_tradeoff_previews() {
    let _guard = lock_test_llm_env();
    let mut stalled_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    stalled_server.confirmed_player_gameplay_progress_time =
        Some(stalled_server.world.state().time);
    stalled_server.latest_player_gameplay_feedback =
        Some(crate::simulator::PlayerGameplayRecentFeedback {
            action: "play".to_string(),
            stage: "completed_no_progress".to_string(),
            effect: "no visible world delta: logicalTime +0, eventSeq +0".to_string(),
            intent_summary: None,
            target_agent_id: None,
            reason: Some("latest command did not create forward progress".to_string()),
            hint: Some("inspect blockers before retrying play".to_string()),
            delta_logical_time: 0,
            delta_event_seq: 0,
        });
    let mut blocked_server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");

    for snapshot in [
        stalled_server.compat_snapshot(None),
        blocked_server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID)),
    ] {
        let serialized = serde_json::to_value(&snapshot).expect("compat snapshot serializes");
        let preview = serialized
            .pointer("/player_gameplay/fallback_tradeoff_preview")
            .and_then(serde_json::Value::as_array)
            .expect("stalled or blocked snapshot publishes fallback tradeoff preview");

        assert_eq!(
            preview.len(),
            3,
            "publish exactly the three fallback options"
        );
        assert_eq!(
            preview
                .iter()
                .map(|option| option["value_class"].as_str())
                .collect::<Vec<_>>(),
            vec![Some("safe_wait"), Some("repair_now"), Some("reroute_now")],
            "publish the deterministic wait, repair, reroute comparison order",
        );
        for option in preview {
            assert!(
                option["available"].is_boolean(),
                "option availability is explicit"
            );
            assert!(option["cost"].is_string(), "option cost is player-readable");
            assert!(
                option["progress_kept"].is_string(),
                "option states retained progress"
            );
            assert!(
                option["opportunity_cost"].is_string(),
                "option states its opportunity cost"
            );
            assert!(option["reason"].is_string(), "option states why it applies");
            assert!(
                option["recommended"].is_boolean(),
                "recommendation is explicit"
            );
        }
        assert_eq!(
            preview
                .iter()
                .filter(|option| option["recommended"] == true)
                .count(),
            1,
            "exactly one fallback option is deterministically recommended",
        );
    }
}
