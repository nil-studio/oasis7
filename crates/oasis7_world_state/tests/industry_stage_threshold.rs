//! Public legacy publication oracle for mathematical industrial-stage thresholds.
use oasis7_world_state::{
    DomainEvent, FactoryState, MaterialLedgerId, WorldState, events::IndustryStage,
};
use serde_json::json;

fn factory(id: &str, jobs: u64, stable: bool) -> FactoryState {
    let mut factory: FactoryState = serde_json::from_value(json!({
        "factory_id":id,"site_id":"site","builder_agent_id":"builder",
        "spec":{"factory_id":id,"display_name":"test","tier":1,"tags":[],
        "build_cost":[],"build_time_ticks":1,"base_power_draw":1,"recipe_slots":1,
        "throughput_bps":1,"maintenance_per_tick":1},"built_at":1
    }))
    .unwrap();
    factory.production.completed_jobs = jobs;
    if stable {
        factory.production.same_recipe_repeat_count = 3;
        factory.production.last_completed_recipe_id = Some("r".into());
        factory.production.last_completed_canonical_snapshot =
            Some(serde_json::from_value(json!({"recipe_id":"r"})).unwrap());
    }
    factory
}

fn recycle(id: &str) -> DomainEvent {
    DomainEvent::FactoryRecycled {
        operator_agent_id: "builder".into(),
        factory_id: id.into(),
        recycle_ledger: MaterialLedgerId::World,
        recovered: vec![],
        durability_ppm: 1_000_000,
    }
}

#[test]
fn recycle_recomputes_mathematical_threshold_after_overflow_and_removal() {
    let mut state = WorldState::default();
    state.gameplay_policy.electricity_tax_bps = 1;
    state
        .factories
        .insert("large".into(), factory("large", u64::MAX, false));
    state
        .factories
        .insert("stable".into(), factory("stable", 1, true));
    state
        .factories
        .insert("zero".into(), factory("zero", 0, false));
    // Real publication computes the remaining total MAX + 1, not a synthetic scalar.
    state.apply_domain_event(&recycle("zero"), 11).unwrap();
    assert_eq!(state.industry_progress.stage, IndustryStage::Governance);
    assert_eq!(state.industry_progress.stage_updated_at, 11);
    assert_eq!(state.factories["large"].production.completed_jobs, u64::MAX);
    state.apply_domain_event(&recycle("large"), 12).unwrap();
    assert_eq!(state.industry_progress.stage, IndustryStage::ScaleOut);
    assert_eq!(state.industry_progress.stage_updated_at, 12);
    assert_eq!(state.factories.len(), 1);
    assert_eq!(state.factories["stable"].production.completed_jobs, 1);
}
