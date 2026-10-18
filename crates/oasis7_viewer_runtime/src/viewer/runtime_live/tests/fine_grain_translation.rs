use super::*;

fn snapshot_after_fine_grain_request(
    server: &mut ViewerRuntimeLiveServer,
    action_id: &str,
) -> crate::simulator::PlayerGameplaySnapshot {
    server.latest_player_gameplay_feedback = Some(crate::simulator::PlayerGameplayRecentFeedback {
        action: format!("gameplay_action:{action_id}"),
        stage: "rejected".to_string(),
        effect: format!("unknown gameplay action `{action_id}` was rejected"),
        intent_summary: Some(format!("submit unsupported action `{action_id}`")),
        target_agent_id: None,
        reason: Some(format!("unknown_gameplay_action: `{action_id}`")),
        hint: Some(format!(
            "correct unsupported action `{action_id}` before retrying"
        )),
        delta_logical_time: 0,
        delta_event_seq: 0,
    });
    server
        .compat_snapshot(Some("fine-grain-player"))
        .player_gameplay
        .expect("runtime-live compatibility snapshot exposes gameplay")
}

#[test]
fn compat_snapshot_projects_only_the_five_frozen_fine_grain_action_ids() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");

    for (action_id, expected_class) in [
        ("mine_resource", "no_safe_replacement"),
        ("place_block_at_smelter_site", "no_safe_replacement"),
        ("jump", "future_embodied_candidate"),
        ("local_physics", "future_embodied_candidate"),
        ("attack", "no_safe_replacement"),
    ] {
        let journal_len_before = server.world.snapshot().journal_len;
        let gameplay = snapshot_after_fine_grain_request(&mut server, action_id);
        let translation = gameplay
            .fine_grain_action_translation
            .as_ref()
            .expect("each frozen fine-grain action ID projects a translation");

        assert!(!translation.requested_granularity.is_empty());
        assert!(!translation.why_fine_action_deferred.is_empty());
        assert!(!translation.closest_playable_goal.is_empty());
        assert!(!translation.player_next_step_hint.is_empty());
        assert_eq!(translation.replacement_value_class, expected_class);
        assert!(
            translation.canonical_replacement_action.is_none(),
            "the default snapshot does not publish an enabled smelter replacement"
        );
        assert_eq!(
            server.world.snapshot().journal_len,
            journal_len_before,
            "advisory projection for {action_id} must not append a runtime journal entry"
        );
    }
}

#[test]
fn compat_snapshot_does_not_translate_typos_case_variants_or_empty_action_ids() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");

    for action_id in ["mine_resorce", "Mine_Resource", ""] {
        let gameplay = snapshot_after_fine_grain_request(&mut server, action_id);
        let raw_marker = format!("gameplay_action:{action_id}");
        let raw_id = if action_id.is_empty() {
            raw_marker.as_str()
        } else {
            action_id
        };
        assert!(
            gameplay.fine_grain_action_translation.is_none(),
            "{action_id:?} must not receive a heuristic fine-grain translation"
        );
        assert!(
            !gameplay
                .accepted_intent_id
                .as_deref()
                .is_some_and(|value| value.contains(raw_marker.as_str())),
            "{action_id:?} must not leak through accepted_intent_id"
        );
        assert!(
            !gameplay
                .intent_summary
                .as_deref()
                .is_some_and(|value| value.contains(raw_marker.as_str())),
            "{action_id:?} must not leak through intent_summary"
        );
        assert!(
            !gameplay
                .recent_feedback
                .as_ref()
                .is_some_and(|feedback| feedback.action.contains(raw_marker.as_str())),
            "{action_id:?} must not leak through recent_feedback.action"
        );
        for value in [
            gameplay.status_reason.as_deref(),
            gameplay.causality_detail.as_deref(),
            gameplay.blocker_kind.as_deref(),
            gameplay.blocker_detail.as_deref(),
            gameplay
                .recent_feedback
                .as_ref()
                .map(|feedback| feedback.effect.as_str()),
            gameplay
                .recent_feedback
                .as_ref()
                .and_then(|feedback| feedback.reason.as_deref()),
            gameplay
                .recent_feedback
                .as_ref()
                .and_then(|feedback| feedback.hint.as_deref()),
        ] {
            assert!(
                !value.is_some_and(|value| value.contains(raw_id)),
                "{action_id:?} must not leak through player-facing snapshot feedback"
            );
        }
    }
}

#[test]
fn compat_snapshot_translates_block_placement_to_an_enabled_published_smelter_action() {
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
    server
        .llm_sidecar
        .bind_agent_player(&agent_id, "fine-grain-player", None, false)
        .expect("bind player");
    let ledger = crate::runtime::MaterialLedgerId::agent(&agent_id);
    for (kind, amount) in [
        ("structural_frame", 12),
        ("heat_coil", 4),
        ("refractory_brick", 6),
    ] {
        server
            .world
            .set_ledger_material_balance(ledger.clone(), kind, amount)
            .expect("seed smelter material");
    }
    let journal_len_before = server.world.snapshot().journal_len;
    let gameplay = snapshot_after_fine_grain_request(&mut server, "place_block_at_smelter_site");
    let translation = gameplay
        .fine_grain_action_translation
        .expect("published enabled smelter action produces translation");
    assert_eq!(translation.replacement_value_class, "replacement_available");
    assert_eq!(
        translation.canonical_replacement_action.as_deref(),
        Some(crate::viewer::ACTION_BUILD_SMELTER_MK1)
    );
    assert_eq!(
        server.world.snapshot().journal_len,
        journal_len_before,
        "translation remains advisory"
    );
}
