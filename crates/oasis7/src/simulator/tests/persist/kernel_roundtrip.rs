use super::*;

#[test]
fn kernel_snapshot_roundtrip() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.snapshot();
    let journal = kernel.journal_snapshot();
    let restored = WorldKernel::from_snapshot(snapshot, journal).unwrap();
    assert_eq!(restored.time(), kernel.time());
    assert_eq!(restored.model(), kernel.model());
}

#[test]
fn snapshot_player_gameplay_execution_state_backfills_from_legacy_fields() {
    let kernel = WorldKernel::new();
    let journal_before_translation_roundtrip = kernel.journal_snapshot();
    let mut snapshot = kernel.snapshot();
    snapshot.player_gameplay = Some(PlayerGameplaySnapshot {
        canonical_agent_chat: None,
        recipe_substitution: None,
        stage_id: PlayerGameplayStageId::PostOnboarding,
        stage_status: PlayerGameplayStageStatus::Blocked,
        execution_state: PlayerGameplayExecutionState::Blocked,
        accepted_intent_id: Some("step".to_string()),
        primary_intent: None,
        intent_summary: Some("advance the live world by 1 step(s)".to_string()),
        intent_scope: Some("world_control".to_string()),
        intent_target: None,
        can_reprioritize: false,
        goal_id: "post_onboarding.recover_capability".to_string(),
        goal_kind: PlayerGameplayGoalKind::RecoverCapability,
        goal_title: "Recover sustainable capability".to_string(),
        objective: "Restore the first blocked capability chain.".to_string(),
        progress_detail: "The primary line is blocked.".to_string(),
        progress_percent: 68,
        blocker_kind: Some("material_shortage".to_string()),
        blocker_detail: Some("iron input exhausted at factory-0".to_string()),
        next_step_hint: "Replenish upstream materials and advance again.".to_string(),
        status_reason: Some("iron input exhausted at factory-0".to_string()),
        last_world_change: None,
        causality_kind: Some(PlayerGameplayCausalityKind::WorldConstraint),
        causality_detail: Some("iron input exhausted at factory-0".to_string()),
        branch_hint: None,
        branch_recommendations: Vec::new(),
        resume_anchor: Some(
            "Recover sustainable capability (post_onboarding.recover_capability)".to_string(),
        ),
        primary_blocker: Some("iron input exhausted at factory-0".to_string()),
        response_window_class: Some("stalled_needs_escalation".to_string()),
        stalled_reason: Some("latest command did not create forward progress".to_string()),
        escalation_hint: Some("repair the line, then advance again".to_string()),
        fallback_action_id: None,
        fallback_action_label: None,
        fallback_tradeoff_preview: None,
        no_safe_fallback_reason: None,
        required_next_decision_action_id: None,
        required_next_decision_class: None,
        resume_next_step: Some("Replenish upstream materials and advance again.".to_string()),
        available_actions: vec![PlayerGameplayAction {
            action_id: "build_factory_smelter_mk1".to_string(),
            label: "Build the first smelter".to_string(),
            protocol_action: "build_factory".to_string(),
            target_agent_id: None,
            disabled_reason: None,
        }],
        recent_feedback: Some(PlayerGameplayRecentFeedback {
            action: "step".to_string(),
            stage: "completed_no_progress".to_string(),
            effect: "no visible world delta".to_string(),
            intent_summary: Some("advance the live world by 1 step(s)".to_string()),
            target_agent_id: None,
            reason: Some("latest command did not create forward progress".to_string()),
            hint: Some("repair the line, then advance again".to_string()),
            delta_logical_time: 0,
            delta_event_seq: 0,
        }),
        wait_resolution_quote: None,
        agent_claim: None,
        micro_depot_facilities: vec![MicroDepotPlayerFacilitySnapshot {
            facility_id: "depot-public-snapshot".to_string(),
            owner_claim_id: "claim-public-snapshot".to_string(),
            status: "active".to_string(),
            location_id: "loc-public-snapshot".to_string(),
            service_radius_cm: 250_000,
            inventory_revision: 7,
            available_units_by_kind: [("data".to_string(), 5)].into_iter().collect(),
            throughput_epoch: 11,
            throughput_remaining_units: 13,
            throughput_limit_units_per_epoch: 16,
            supported_resource_kinds: vec!["data".to_string()],
            module_id: "regional.micro_depot".to_string(),
            module_version: "0.2.0".to_string(),
            wasm_hash: "sha256:micro-depot-public-evidence".to_string(),
            upkeep_paid: true,
            last_receipt_id: Some("receipt-micro-depot-public".to_string()),
            last_proposal_hash: Some("sha256:proposal-public".to_string()),
            available_actions: vec!["service_micro_depot_repair".to_string()],
        }],
        small_player_lane_id: None,
        leverage_class: None,
        same_loop_repeat_count: 0,
        grind_only_flag: false,
        major_power_dependency_status: None,
        recovery_path_kind: None,
        recovery_path_detail: None,
        requires_major_power_sponsorship: None,
        repair_available: None,
        rebuild_available: None,
        pivot_available: None,
        validation_unlock_preview: None,
        starter_industrial_feasibility: None,
        factory_production_failure_disposition: None,
        recovery_options: Vec::new(),
        fine_grain_action_translation: None,
    });

    let mut value: serde_json::Value =
        serde_json::from_str(&snapshot.to_json().expect("snapshot to json"))
            .expect("parse snapshot json");
    fine_grain_persistence::assert_fine_grain_translation_roundtrip(
        &value,
        &kernel,
        &journal_before_translation_roundtrip,
    );

    value["player_gameplay"]["branch_recommendations"] = serde_json::json!([{
        "action_id": "build_factory_assembler_mk1",
        "route_label": "Open a second production line",
        "immediate_gain": "Add the assembler capability.",
        "future_beat_changed": "The next beat becomes coordinating complementary factories.",
        "risk_or_lockin": "Commits construction materials before the new line produces value.",
        "next_session_hook": "Return to choose the assembler's first recipe."
    }]);

    let gameplay_object = value
        .get_mut("player_gameplay")
        .and_then(|gameplay| gameplay.as_object_mut())
        .expect("player gameplay object");
    gameplay_object.remove("execution_state");
    gameplay_object.remove("fallback_tradeoff_preview");
    gameplay_object.remove("no_safe_fallback_reason");
    gameplay_object.remove("required_next_decision_action_id");
    gameplay_object.remove("required_next_decision_class");
    gameplay_object.remove("recovery_options");
    gameplay_object.remove("fine_grain_action_translation");

    let migrated = WorldSnapshot::from_json(
        &serde_json::to_string(&value).expect("serialize migrated snapshot"),
    )
    .expect("load legacy player gameplay snapshot");

    let gameplay = migrated
        .player_gameplay
        .as_ref()
        .expect("restored player gameplay");
    assert_eq!(gameplay.stage_status, PlayerGameplayStageStatus::Blocked);
    assert_eq!(
        gameplay.execution_state,
        PlayerGameplayExecutionState::Blocked
    );
    assert_eq!(gameplay.branch_recommendations.len(), 1);
    let legacy_branch_commitment = &gameplay.branch_recommendations[0];
    assert_eq!(
        legacy_branch_commitment.future_beat_changed,
        "The next beat becomes coordinating complementary factories."
    );
    assert!(
        legacy_branch_commitment.future_beats.is_empty(),
        "legacy branch commitments without two-beat data must remain readable"
    );
    assert_eq!(
        gameplay.causality_kind,
        Some(PlayerGameplayCausalityKind::WorldConstraint)
    );
    assert_eq!(
        gameplay.small_player_lane_id.as_deref(),
        Some("unclassified")
    );
    assert_eq!(gameplay.leverage_class.as_deref(), Some("unclassified"));
    assert_eq!(gameplay.same_loop_repeat_count, 0);
    assert!(!gameplay.grind_only_flag);
    assert_eq!(
        gameplay.major_power_dependency_status.as_deref(),
        Some("unverified")
    );
    assert_eq!(gameplay.recovery_path_kind.as_deref(), Some("unverified"));
    assert!(
        gameplay.starter_industrial_feasibility.is_none(),
        "legacy snapshots without the optional starter feasibility field remain readable"
    );
    assert!(gameplay.fallback_tradeoff_preview.is_none());
    assert!(gameplay.no_safe_fallback_reason.is_none());
    assert!(gameplay.required_next_decision_action_id.is_none());
    assert!(gameplay.required_next_decision_class.is_none());
    assert!(gameplay.recovery_options.is_empty());
    fine_grain_persistence::assert_legacy_omits_fine_grain_translation(&migrated);

    fine_grain_persistence::assert_legacy_micro_depot_evidence(gameplay);
}
