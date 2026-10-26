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
    assert_eq!(snapshot(&state).same_loop_repeat_count, 3);
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
