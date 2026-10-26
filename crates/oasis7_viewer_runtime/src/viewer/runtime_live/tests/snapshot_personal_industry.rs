use crate::runtime::{
    FactoryModuleSpec, FactoryProductionState, FactoryState, IndustryStage, MaterialLedgerId,
    WorldState,
};
use crate::simulator::PlayerGameplaySnapshot;

fn factory(owner: &str, id: &str, repeats: u32) -> FactoryState {
    FactoryState {
        factory_id: id.into(),
        site_id: id.into(),
        builder_agent_id: owner.into(),
        spec: FactoryModuleSpec {
            factory_id: id.into(),
            display_name: id.into(),
            tier: 1,
            tags: vec![],
            build_cost: vec![],
            build_time_ticks: 1,
            base_power_draw: 1,
            recipe_slots: 1,
            throughput_bps: 10_000,
            maintenance_per_tick: 0,
        },
        input_ledger: MaterialLedgerId::site(id),
        output_ledger: MaterialLedgerId::site(id),
        durability_ppm: 1_000_000,
        production: FactoryProductionState {
            completed_jobs: repeats.into(),
            last_completed_at: (repeats > 0).then_some(10),
            same_recipe_repeat_count: repeats,
            last_completed_recipe_id: (repeats > 0).then(|| "recipe.iron".into()),
            last_completed_canonical_snapshot: (repeats > 0).then(|| {
                oasis7_world_state::state::FactoryProductionSnapshot {
                    recipe_id: "recipe.iron".into(),
                    ..Default::default()
                }
            }),
            ..Default::default()
        },
        location_anchor_revision: None,
        site_authority_revision: None,
        site_location_id: None,
        construction_power_profile_key: None,
        construction_power_profile_revision: None,
        built_at: 1,
    }
}
fn snapshot(state: &WorldState) -> PlayerGameplaySnapshot {
    crate::viewer::runtime_live::gameplay_snapshot::build_player_gameplay_snapshot(
        state,
        Some("agent-b"),
        true,
        None,
        None,
        None,
        true,
        None,
        false,
        true,
        None,
        true,
    )
}

#[test]
fn personal_industry_progress_does_not_inherit_mature_world_stage() {
    for stage in [IndustryStage::ScaleOut, IndustryStage::Governance] {
        let mut state = WorldState::default();
        state.industry_progress.stage = stage;
        state
            .factories
            .insert("factory.a".into(), factory("agent-a", "factory.a", 6));
        for repeats in [0, 1, 3] {
            state
                .factories
                .insert("factory.b".into(), factory("agent-b", "factory.b", repeats));
            let gameplay = snapshot(&state);
            println!(
                "PERSONAL_INDUSTRY_SNAPSHOT {}",
                serde_json::json!({"world_stage": format!("{stage:?}"), "repeats": repeats, "player_gameplay": gameplay})
            );
            let (goal, percent) = match repeats {
                0 => ("post_onboarding.start_factory_run", 54),
                1 => ("post_onboarding.stabilize_first_line_after_output", 80),
                _ => ("post_onboarding.choose_first_expansion_tradeoff", 92),
            };
            assert_eq!(gameplay.goal_id, goal, "stage={stage:?}, repeats={repeats}");
            assert_eq!(gameplay.progress_percent, percent);
            assert!(
                !gameplay
                    .objective
                    .contains("sustainable industrial capability is online")
            );
            if repeats == 1 {
                assert!(
                    gameplay
                        .objective
                        .contains("One visible output is not enough")
                );
                assert!(gameplay.next_step_hint.contains("Advance 1-2 more times"));
            }
            state
                .factories
                .get_mut("factory.a")
                .unwrap()
                .production
                .completed_jobs += 10;
            assert_eq!(snapshot(&state).goal_id, goal);
            assert_eq!(snapshot(&state).progress_percent, percent);
        }
    }
}

#[test]
fn personal_industry_uses_one_owned_canonical_window() {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Governance;
    state
        .factories
        .insert("factory.a".into(), factory("agent-a", "factory.a", 9));
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 3));
    assert_eq!(snapshot(&state).progress_percent, 92);
    // A different owned factory's one output must not hide the valid stable line.
    let mut other = factory("agent-b", "factory.other", 1);
    other.production.completed_jobs = 100;
    state.factories.insert("factory.other".into(), other);
    // This owner has enough completed work for the next stage, but only
    // factory.b supplies the required stable evidence. The other factory's
    // high historical count cannot substitute for that evidence.
    assert_eq!(
        snapshot(&state).goal_id,
        "post_onboarding.choose_midloop_path"
    );
    assert_eq!(snapshot(&state).progress_percent, 100);
    state.factories.remove("factory.b");
    assert_eq!(snapshot(&state).progress_percent, 80);
    let production = &mut state.factories.get_mut("factory.other").unwrap().production;
    production.same_recipe_repeat_count = 3;
    production.last_completed_recipe_id = Some("recipe.changed".into());
    assert_eq!(
        snapshot(&state).progress_percent,
        80,
        "mismatched canonical evidence"
    );
    state
        .factories
        .get_mut("factory.other")
        .unwrap()
        .production
        .last_completed_canonical_snapshot = None;
    assert_eq!(
        snapshot(&state).progress_percent,
        80,
        "missing canonical evidence"
    );
}

fn pending_job(id: u64, factory_id: &str) -> crate::runtime::RecipeJobState {
    serde_json::from_value(serde_json::json!({
        "job_id": id, "requester_agent_id": "agent-b", "factory_id": factory_id,
        "recipe_id": "recipe.iron", "accepted_batches": 1,
        "consume": [], "produce": [], "byproducts": [], "power_required": 0,
        "duration_ticks": 1, "consume_ledger": "world", "output_ledger": "world",
        "ready_at": 11
    }))
    .unwrap()
}

#[test]
fn personal_industry_inflight_causal_change_cannot_reuse_completed_window() {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Governance;
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 3));
    let job = pending_job(1, "factory.b");
    // Multiple slots: even if the displayed current job matches, every job
    // on this factory must retain the same canonical candidate identity.
    state.pending_recipe_jobs.insert(1, job.clone());
    state.pending_recipe_jobs.insert(2, job);
    assert_eq!(snapshot(&state).progress_percent, 92);
    for change in ["recipe", "power", "input", "path", "output"] {
        let job = state.pending_recipe_jobs.get_mut(&2).unwrap();
        let original = job.clone();
        match change {
            "recipe" => job.recipe_id = "recipe.changed".into(),
            "power" => job.power_required = 1,
            "input" => job.consume_ledger = MaterialLedgerId::site("other"),
            "path" => job.logistics_path_ids = vec!["path.changed".into()],
            _ => job.produce = vec![crate::runtime::MaterialStack::new("iron_ingot", 1)],
        }
        let gameplay = snapshot(&state);
        assert_eq!(gameplay.progress_percent, 80, "{change}");
        assert_eq!(gameplay.same_loop_repeat_count, 0, "{change}");
        state.pending_recipe_jobs.insert(2, original);
    }
    // Another factory's pending work cannot invalidate this player's line.
    state
        .pending_recipe_jobs
        .insert(3, pending_job(3, "factory.other"));
    assert_eq!(snapshot(&state).progress_percent, 92);
    state
        .factories
        .get_mut("factory.b")
        .unwrap()
        .production
        .current_recipe_id = Some("recipe.changed".into());
    assert_eq!(snapshot(&state).progress_percent, 80);
}

#[test]
fn personal_industry_block_pause_and_recovery_use_canonical_window_reset() {
    use crate::runtime::DomainEvent;
    let mut state = WorldState::default();
    state
        .factories
        .insert("factory.a".into(), factory("agent-a", "factory.a", 9));
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 3));
    state.industry_progress.stage = IndustryStage::Governance;
    state
        .apply_domain_event(
            &DomainEvent::FactoryProductionBlocked {
                action_id: 10,
                requester_agent_id: "agent-b".into(),
                factory_id: "factory.b".into(),
                recipe_id: "recipe.iron".into(),
                blocker_kind: "missing_materials".into(),
                blocker_detail: "iron input is unavailable".into(),
            },
            11,
        )
        .unwrap();
    let blocked = snapshot(&state);
    assert_eq!(blocked.same_loop_repeat_count, 0);
    assert!(blocked.progress_percent < 92);
    assert!(blocked.blocker_kind.is_some());
    state
        .apply_domain_event(
            &DomainEvent::FactoryProductionResumed {
                job_id: 11,
                requester_agent_id: "agent-b".into(),
                factory_id: "factory.b".into(),
                recipe_id: "recipe.iron".into(),
                previous_blocked_at: Some(11),
                previous_blocker_kind: None,
                previous_blocker_detail: None,
            },
            12,
        )
        .unwrap();
    let resumed = snapshot(&state);
    assert_eq!(
        resumed.goal_id,
        "post_onboarding.stabilize_first_line_after_output"
    );
    assert_eq!(resumed.same_loop_repeat_count, 0);
    assert_eq!(resumed.progress_percent, 88);
    assert!(!resumed.objective.contains("stable enough to grow"));
    // Even after recovery, a single new completion remains a candidate.
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 1));
    assert_eq!(snapshot(&state).progress_percent, 80);
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 3));
    assert_eq!(snapshot(&state).progress_percent, 92);
    state
        .apply_domain_event(
            &DomainEvent::FactoryProductionPaused {
                action_id: 12,
                requester_agent_id: "agent-b".into(),
                factory_id: "factory.b".into(),
                reason: "planned maintenance".into(),
            },
            13,
        )
        .unwrap();
    assert_eq!(snapshot(&state).same_loop_repeat_count, 0);
    assert_eq!(snapshot(&state).progress_percent, 80);
}

#[test]
fn personal_industry_midloop_requires_owned_work_not_shared_transits() {
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Governance;
    state.gameplay_policy.electricity_tax_bps = 100;
    state.industry_progress.completed_material_transits = 30;
    state
        .factories
        .insert("factory.a".into(), factory("agent-a", "factory.a", 60));
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 3));
    assert_eq!(snapshot(&state).progress_percent, 92);
    state
        .factories
        .insert("factory.b".into(), factory("agent-b", "factory.b", 6));
    let gameplay = snapshot(&state);
    assert_eq!(gameplay.goal_id, "post_onboarding.choose_midloop_path");
    assert_eq!(gameplay.progress_percent, 100);
    assert!(
        gameplay
            .progress_detail
            .contains("subsequent completed work")
    );
    assert!(!gameplay.progress_detail.contains("tradeoff is behind you"));
    state.factories.remove("factory.b");
    assert!(snapshot(&state).progress_percent < 92);
}

#[test]
fn capability_query_resolves_subject_and_separates_world_from_current_owner() {
    use oasis7_world_state::gameplay_capability::GameplaySubject;
    let mut state = WorldState::default();
    state.industry_progress.stage = IndustryStage::Governance;
    state
        .factories
        .insert("a".into(), factory("agent-a", "a", 6));
    state
        .factories
        .insert("b".into(), factory("agent-b", "b", 1));
    let query = |state: &WorldState, subject| {
        state.gameplay_capability_query(subject, "current_industrial_capability")
    };
    let personal = || GameplaySubject::resolve(Some("agent-b"), true);
    let first = query(&state, personal());
    assert_eq!(first.world_stage, IndustryStage::Governance);
    assert_eq!(first.personal_stage, Some(IndustryStage::Bootstrap));
    assert_eq!(first.current_stable_line, Some(false));
    assert!(first.first_settlement.is_none()); // no canonical settlement evidence
    assert_eq!(
        snapshot(&state).gameplay_capability.unwrap(),
        serde_json::to_value(&first).unwrap()
    );
    for subject in [
        GameplaySubject::Spectator,
        GameplaySubject::resolve(None, true),
    ] {
        let result = query(&state, subject);
        assert!(result.personal_stage.is_none());
        assert!(result.current_stable_line.is_none());
        assert!(result.first_settlement.is_none());
        assert!(
            result
                .recovery
                .iter()
                .all(|item| item.status == "unknown" && item.action.is_none())
        );
    }
    state
        .factories
        .insert("b".into(), factory("agent-b", "b", 6));
    assert_eq!(
        query(&state, personal()).personal_stage,
        Some(IndustryStage::Governance)
    );
    state
        .factories
        .get_mut("b")
        .unwrap()
        .production
        .current_recipe_id = Some("changed".into());
    assert_eq!(query(&state, personal()).current_stable_line, Some(false));
    state
        .factories
        .get_mut("b")
        .unwrap()
        .production
        .current_recipe_id = None;
    state.factories.get_mut("b").unwrap().production.status =
        crate::runtime::FactoryProductionStatus::Paused;
    assert_eq!(
        query(&state, personal()).personal_stage,
        Some(IndustryStage::Bootstrap)
    );
}

#[test]
fn capability_query_roundtrips_and_legacy_absence_remains_unknown() {
    let gameplay = snapshot(&WorldState::default());
    let mut encoded = serde_json::to_value(&gameplay).unwrap();
    let restored: PlayerGameplaySnapshot = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored.gameplay_capability, gameplay.gameplay_capability);
    encoded
        .as_object_mut()
        .unwrap()
        .remove("gameplay_capability");
    let legacy: PlayerGameplaySnapshot = serde_json::from_value(encoded).unwrap();
    assert!(legacy.gameplay_capability.is_none());
    assert!(legacy.repair_available.is_none());
    assert!(legacy.rebuild_available.is_none());
    assert!(legacy.pivot_available.is_none());
}
