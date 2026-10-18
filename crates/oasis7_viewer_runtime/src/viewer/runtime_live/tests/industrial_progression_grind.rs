use crate::runtime::{
    FactoryModuleSpec, FactoryProductionFailureDispositionV1, FactoryProductionState,
    FactoryProductionStatus, FactoryState, IndustryStage, MaterialLedgerId, MaterialStack,
    RecipeJobState, WorldState,
};
use crate::viewer::FACTORY_SMELTER_MK1;
use serde_json::{Value, json};

fn small_player_test_factory_spec(factory_id: &str) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Test Smelter MK1".to_string(),
        tier: 2,
        tags: vec!["smelter".to_string(), "thermal".to_string()],
        build_cost: vec![MaterialStack::new("structural_frame", 12)],
        build_time_ticks: 1,
        base_power_draw: 20,
        recipe_slots: 2,
        throughput_bps: 10_000,
        maintenance_per_tick: 1,
    }
}

fn failure_disposition(
    action_id: u64,
    requester_agent_id: &str,
    factory_id: &str,
    recipe_id: &str,
) -> FactoryProductionFailureDispositionV1 {
    FactoryProductionFailureDispositionV1 {
        action_id,
        requester_agent_id: requester_agent_id.to_string(),
        factory_id: factory_id.to_string(),
        recipe_id: recipe_id.to_string(),
        blocker_kind: "product_validation_rejected".to_string(),
        blocker_detail: "product profile rejected the committed output".to_string(),
        disposition_kind: "consumed_lost".to_string(),
        consumed_inputs: vec![MaterialStack::new("iron_ore", 3)],
        lost_inputs: vec![MaterialStack::new("iron_ore", 3)],
        consumed_power: 7,
        lost_power: 7,
        next_action: "inspect_product_validation_and_reschedule".to_string(),
        next_recheck: None,
    }
}

fn failure_disposition_world_state() -> WorldState {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Bootstrap;
    state.factories.insert(
        "factory.target".to_string(),
        FactoryState {
            factory_id: "factory.target".to_string(),
            site_id: "site-target".to_string(),
            builder_agent_id: "agent-a".to_string(),
            spec: small_player_test_factory_spec("factory.target"),
            input_ledger: MaterialLedgerId::site("site-target"),
            output_ledger: MaterialLedgerId::site("site-target"),
            durability_ppm: 1_000_000,
            production: FactoryProductionState {
                status: FactoryProductionStatus::Blocked,
                active_jobs: 0,
                current_job_id: None,
                current_recipe_id: None,
                completed_jobs: 5,
                last_completed_at: Some(20),
                last_completed_recipe_id: Some("recipe.target".to_string()),
                last_blocked_at: Some(21),
                current_blocker_kind: Some("product_validation_rejected".to_string()),
                current_blocker_detail: Some(
                    "product profile rejected the committed output".to_string(),
                ),
                current_blocker_action_id: Some(19),
                ..FactoryProductionState::default()
            },
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 1,
        },
    );
    state.factories.insert(
        "factory.decoy".to_string(),
        FactoryState {
            factory_id: "factory.decoy".to_string(),
            site_id: "site-decoy".to_string(),
            builder_agent_id: "agent-b".to_string(),
            spec: small_player_test_factory_spec("factory.decoy"),
            input_ledger: MaterialLedgerId::site("site-decoy"),
            output_ledger: MaterialLedgerId::site("site-decoy"),
            durability_ppm: 1_000_000,
            production: FactoryProductionState {
                completed_jobs: 1,
                last_completed_at: Some(10),
                last_completed_recipe_id: Some("recipe.decoy".to_string()),
                ..FactoryProductionState::default()
            },
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 1,
        },
    );
    state.factory_production_failure_dispositions.insert(
        17,
        failure_disposition(17, "agent-b", "factory.target", "recipe.foreign-requester"),
    );
    state.factory_production_failure_dispositions.insert(
        18,
        failure_disposition(18, "agent-a", "factory.decoy", "recipe.foreign-factory"),
    );
    state.factory_production_failure_dispositions.insert(
        19,
        failure_disposition(19, "agent-a", "factory.target", "recipe.target"),
    );

    state
}

fn failure_disposition_gameplay_snapshot() -> crate::simulator::PlayerGameplaySnapshot {
    let state = failure_disposition_world_state();
    super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &state,
        Some("agent-a"),
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    )
}

fn gameplay_snapshot_for_failure_state(
    state: &WorldState,
) -> crate::simulator::PlayerGameplaySnapshot {
    super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        state,
        Some("agent-a"),
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
    )
}

#[test]
fn runtime_gameplay_snapshot_flags_grind_only_after_repeating_same_loop_without_new_leverage() {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Bootstrap;
    state.industry_progress.completed_recipe_jobs = 4;
    state.factories.insert(
        FACTORY_SMELTER_MK1.to_string(),
        FactoryState {
            factory_id: FACTORY_SMELTER_MK1.to_string(),
            site_id: "runtime:10:20:0".to_string(),
            builder_agent_id: "agent-1".to_string(),
            spec: small_player_test_factory_spec(FACTORY_SMELTER_MK1),
            input_ledger: MaterialLedgerId::world(),
            output_ledger: MaterialLedgerId::world(),
            durability_ppm: 1_000_000,
            production: FactoryProductionState {
                completed_jobs: 4,
                last_completed_at: Some(12),
                last_completed_recipe_id: Some("recipe.smelter.iron_ingot".to_string()),
                same_recipe_repeat_count: 3,
                ..FactoryProductionState::default()
            },
            location_anchor_revision: None,
            site_authority_revision: None,
            site_location_id: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 1,
        },
    );

    let gameplay = super::super::gameplay_snapshot::build_player_gameplay_snapshot(
        &state, None, true, None, None, None, true, None, false, true, None,
    );

    assert_eq!(
        gameplay.goal_id,
        "post_onboarding.stabilize_first_line_after_output"
    );
    assert_eq!(gameplay.same_loop_repeat_count, 3);
    assert_eq!(gameplay.leverage_class.as_deref(), Some("throughput_only"));
    assert!(gameplay.grind_only_flag);
}

#[test]
fn runtime_gameplay_snapshot_projects_requester_factory_scoped_failure_disposition() {
    let gameplay = failure_disposition_gameplay_snapshot();
    let encoded = serde_json::to_value(&gameplay).expect("serialize gameplay snapshot");
    let projection = encoded
        .get("factory_production_failure_disposition")
        .and_then(Value::as_object)
        .expect("player snapshot must expose the scoped failure disposition");

    assert_eq!(projection.get("action_id"), Some(&json!("19")));
    assert_eq!(
        projection.get("requester_agent_id"),
        Some(&json!("agent-a"))
    );
    assert_eq!(projection.get("factory_id"), Some(&json!("factory.target")));
    assert_eq!(projection.get("recipe_id"), Some(&json!("recipe.target")));
    assert_eq!(
        projection.get("blocker_kind"),
        Some(&json!("product_validation_rejected"))
    );
    assert_eq!(
        projection.get("blocker_detail"),
        Some(&json!("product profile rejected the committed output"))
    );
    assert_eq!(
        projection.get("disposition_kind"),
        Some(&json!("consumed_lost"))
    );
    assert_eq!(
        projection.get("consumed_inputs"),
        Some(&json!([{ "kind": "iron_ore", "amount": 3 }]))
    );
    assert_eq!(
        projection.get("lost_inputs"),
        Some(&json!([{ "kind": "iron_ore", "amount": 3 }]))
    );
    assert_eq!(projection.get("consumed_power"), Some(&json!(7)));
    assert_eq!(projection.get("lost_power"), Some(&json!(7)));
    assert_eq!(
        projection.get("next_action"),
        Some(&json!("inspect_product_validation_and_reschedule"))
    );
    assert_eq!(projection.get("next_recheck"), Some(&Value::Null));
    for generic_field in [
        "repair_available",
        "rebuild_available",
        "pivot_available",
        "wait_resolution_quote",
    ] {
        assert!(
            !projection.contains_key(generic_field),
            "failure disposition must not synthesize generic {generic_field}"
        );
    }
}

#[test]
fn player_gameplay_snapshot_failure_disposition_roundtrips_and_accepts_legacy_omission() {
    let gameplay = failure_disposition_gameplay_snapshot();
    let encoded = serde_json::to_value(&gameplay).expect("serialize gameplay snapshot");
    let mut legacy = encoded.clone();
    legacy
        .as_object_mut()
        .expect("snapshot object")
        .remove("factory_production_failure_disposition");
    let restored_legacy: crate::simulator::PlayerGameplaySnapshot =
        serde_json::from_value(legacy).expect("legacy snapshot without optional disposition");
    let restored_legacy = serde_json::to_value(restored_legacy).expect("serialize legacy snapshot");
    assert!(
        restored_legacy
            .get("factory_production_failure_disposition")
            .is_none(),
        "legacy snapshots must omit the optional failure disposition"
    );

    let mut enriched = encoded;
    enriched["factory_production_failure_disposition"] = json!({
        "action_id": "19",
        "requester_agent_id": "agent-a",
        "factory_id": "factory.target",
        "recipe_id": "recipe.target",
        "blocker_kind": "product_validation_rejected",
        "blocker_detail": "product profile rejected the committed output",
        "disposition_kind": "consumed_lost",
        "consumed_inputs": [{ "kind": "iron_ore", "amount": 3 }],
        "lost_inputs": [{ "kind": "iron_ore", "amount": 3 }],
        "consumed_power": 7,
        "lost_power": 7,
        "next_action": "inspect_product_validation_and_reschedule",
        "next_recheck": null,
    });
    let restored: crate::simulator::PlayerGameplaySnapshot =
        serde_json::from_value(enriched.clone()).expect("decode enriched snapshot");
    let reencoded = serde_json::to_value(restored).expect("re-encode enriched snapshot");
    assert_eq!(
        reencoded.get("factory_production_failure_disposition"),
        enriched.get("factory_production_failure_disposition"),
        "failure disposition must survive snapshot persistence"
    );
}

#[test]
fn runtime_gameplay_snapshot_hides_stale_failure_disposition_when_target_factory_is_running() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");
    target.production.status = FactoryProductionStatus::Running;
    target.production.active_jobs = 1;
    target.production.current_job_id = Some(20);
    target.production.current_recipe_id = Some("recipe.retry".to_string());
    target.production.current_blocker_kind = None;
    target.production.current_blocker_detail = None;

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert!(gameplay.factory_production_failure_disposition.is_none());
    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&19)
    );
}

#[test]
fn runtime_gameplay_snapshot_hides_stale_failure_disposition_after_target_completion() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");
    target.production.status = FactoryProductionStatus::Idle;
    target.production.last_completed_at = Some(22);
    target.production.last_completed_recipe_id = Some("recipe.retry".to_string());
    target.production.current_blocker_kind = None;
    target.production.current_blocker_detail = None;

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert!(gameplay.factory_production_failure_disposition.is_none());
    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&19)
    );
}

#[test]
fn runtime_gameplay_snapshot_keeps_failure_after_sibling_completion_returns_factory_idle() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");

    // Product validation consumed the target slot, then the sibling slot
    // completed. That completion leaves the durable failure record and its
    // current blocker intact while the aggregate factory becomes idle.
    target.production.status = FactoryProductionStatus::Idle;
    target.production.active_jobs = 0;
    target.production.current_job_id = None;
    target.production.current_recipe_id = None;
    target.production.last_completed_at = Some(22);
    target.production.last_completed_recipe_id = Some("recipe.sibling".to_string());

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.action_id.as_str()),
        Some("19"),
        "a sibling completion must not hide the still-current consumed/lost recovery card"
    );
}

#[test]
fn runtime_gameplay_snapshot_keeps_failure_while_sibling_slot_remains_active() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");

    // The failed slot is settled, but a second recipe slot is still running.
    // The aggregate active-job count must not erase the durable disposition.
    target.production.status = FactoryProductionStatus::Blocked;
    target.production.active_jobs = 1;
    target.production.current_job_id = None;
    target.production.current_recipe_id = None;

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.action_id.as_str()),
        Some("19"),
        "an active sibling slot must not hide the failed slot's recovery card"
    );
}

#[test]
fn runtime_gameplay_snapshot_ignores_pending_sibling_when_matching_failed_job_is_settled() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");

    // The failed slot is settled while a sibling slot remains pending in the
    // same factory. Freshness must inspect the disposition's failed job ID,
    // not reject every pending job belonging to the factory.
    target.production.status = FactoryProductionStatus::Blocked;
    target.production.active_jobs = 1;
    target.production.current_job_id = None;
    target.production.current_recipe_id = None;
    state.pending_recipe_jobs.insert(
        20,
        RecipeJobState {
            committed_recipe_origin: None,
            job_id: 20,
            requester_agent_id: "agent-a".to_string(),
            factory_id: "factory.target".to_string(),
            recipe_id: "recipe.sibling".to_string(),
            accepted_batches: 1,
            consume: vec![MaterialStack::new("iron_ore", 3)],
            produce: vec![MaterialStack::new("iron_ingot", 1)],
            byproducts: Vec::new(),
            power_required: 7,
            power_owner_agent_id: Some("agent-a".to_string()),
            duration_ticks: 1,
            consume_ledger: MaterialLedgerId::site("site-target"),
            output_ledger: MaterialLedgerId::site("site-target"),
            bottleneck_tags: Vec::new(),
            logistics_route_ids: Vec::new(),
            logistics_path_ids: Vec::new(),
            ready_at: 22,
        },
    );

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.action_id.as_str()),
        Some("19"),
        "a sibling pending job must not hide the settled failed-job recovery card"
    );
}

#[test]
fn runtime_gameplay_snapshot_keeps_target_failure_when_only_unrelated_factory_is_running() {
    let mut state = failure_disposition_world_state();
    let decoy = state
        .factories
        .get_mut("factory.decoy")
        .expect("decoy factory");
    decoy.production.status = FactoryProductionStatus::Running;
    decoy.production.active_jobs = 1;
    decoy.production.current_job_id = Some(20);
    decoy.production.current_recipe_id = Some("recipe.decoy.retry".to_string());

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.action_id.as_str()),
        Some("19")
    );
}

#[test]
fn runtime_gameplay_snapshot_prefers_requester_failure_over_healthy_primary_factory() {
    let mut state = failure_disposition_world_state();
    let mut healthy = state
        .factories
        .get("factory.target")
        .cloned()
        .expect("target factory");
    healthy.factory_id = "factory.healthy".to_string();
    healthy.site_id = "site-healthy".to_string();
    healthy.input_ledger = MaterialLedgerId::site("site-healthy");
    healthy.output_ledger = MaterialLedgerId::site("site-healthy");
    healthy.production = FactoryProductionState {
        status: FactoryProductionStatus::Running,
        active_jobs: 1,
        current_job_id: Some(21),
        current_recipe_id: Some("recipe.healthy".to_string()),
        completed_jobs: 6,
        last_completed_at: Some(22),
        last_completed_recipe_id: Some("recipe.healthy.previous".to_string()),
        ..FactoryProductionState::default()
    };
    state.factories.insert(healthy.factory_id.clone(), healthy);

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.factory_id.as_str()),
        Some("factory.target"),
        "a fresh requester-scoped failure must win before healthy primary-factory fallback"
    );
    assert_eq!(
        gameplay
            .factory_production_failure_disposition
            .as_ref()
            .map(|receipt| receipt.action_id.as_str()),
        Some("19")
    );
}

#[test]
fn runtime_gameplay_snapshot_hides_old_failure_when_target_blocker_changed() {
    let mut state = failure_disposition_world_state();
    let target = state
        .factories
        .get_mut("factory.target")
        .expect("target factory");
    target.production.current_blocker_kind = Some("material_shortage".to_string());
    target.production.current_blocker_detail = Some("iron input exhausted".to_string());

    let gameplay = gameplay_snapshot_for_failure_state(&state);
    assert!(gameplay.factory_production_failure_disposition.is_none());
    assert!(
        state
            .factory_production_failure_dispositions
            .contains_key(&19)
    );
}
