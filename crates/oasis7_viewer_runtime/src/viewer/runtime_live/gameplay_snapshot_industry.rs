use crate::runtime::{FactoryProductionStatus, FactoryState, IndustryStage, WorldState};
use oasis7_world_state::state::{
    FactoryProductionSnapshot,
    industry_transition::{factory_has_canonical_stable_line, industry_stage_for_progress},
};

// The canonical reducer owns the window and identity. Its completed window
// remains historical evidence while a different job is in flight; it cannot
// certify the current candidate on the player's goal card.
pub(super) fn has_current_stable_line(state: &WorldState, factory: &FactoryState) -> bool {
    if matches!(
        factory.production.status,
        FactoryProductionStatus::Blocked | FactoryProductionStatus::Paused
    ) || !factory_has_canonical_stable_line(factory)
    {
        return false;
    }
    current_candidate_matches_completed(state, factory)
}

pub(super) fn current_repeat_count(state: &WorldState, factory: &FactoryState) -> u32 {
    if current_candidate_matches_completed(state, factory) {
        factory.production.same_recipe_repeat_count
    } else {
        0
    }
}

fn current_candidate_matches_completed(state: &WorldState, factory: &FactoryState) -> bool {
    let Some(completed) = factory
        .production
        .last_completed_canonical_snapshot
        .as_ref()
    else {
        return false;
    };
    factory
        .production
        .current_recipe_id
        .as_ref()
        .is_none_or(|recipe| *recipe == completed.recipe_id)
        && state
            .pending_recipe_jobs
            .values()
            .filter(|job| job.factory_id == factory.factory_id)
            .all(|job| FactoryProductionSnapshot::from_recipe_job(job) == *completed)
}

pub(super) fn primary_personal_factory<'a>(
    state: &'a WorldState,
    agent_id: &str,
) -> Option<&'a FactoryState> {
    state
        .factories
        .values()
        .filter(|factory| factory.builder_agent_id == agent_id)
        .max_by_key(|factory| {
            let production = &factory.production;
            (
                has_current_stable_line(state, factory),
                production.completed_jobs > 0 || production.last_completed_at.is_some(),
                production.status != FactoryProductionStatus::Blocked,
                production.status == FactoryProductionStatus::Running,
                factory.factory_id == crate::viewer::FACTORY_SMELTER_MK1,
                production.completed_jobs,
                production.last_completed_at.unwrap_or(0),
            )
        })
}

pub(super) fn personal_stage(
    state: &WorldState,
    factory: Option<&FactoryState>,
    agent_id: Option<&str>,
) -> IndustryStage {
    let Some(agent_id) = agent_id else {
        return IndustryStage::Bootstrap;
    };
    let jobs = state
        .factories
        .values()
        .filter(|factory| factory.builder_agent_id == agent_id)
        .map(|factory| factory.production.completed_jobs)
        .fold(0_u64, u64::saturating_add);
    // The world transit counter has no durable per-player attribution. It
    // cannot stand in for this player's subsequent industrial work.
    industry_stage_for_progress(
        factory.is_some_and(|factory| has_current_stable_line(state, factory)),
        jobs,
        0,
        state.gameplay_policy.electricity_tax_bps > 0 || state.gameplay_policy.data_tax_bps > 0,
    )
}
