//! Read-only owner-attributed capability facts. No personal stage is persisted.
use crate::events::IndustryStage;
use crate::state::industry_transition::{
    factory_has_canonical_stable_line, industry_stage_for_progress,
};
use crate::state::*;
use crate::types::MaterialLedgerId;
use serde::{Deserialize, Serialize};

pub const CURRENT_INDUSTRIAL_CAPABILITY_GOAL: &str = "current_industrial_capability";

/// Resolved by an authenticated entry point; observing an owner grants no writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameplaySubject {
    Personal { owner_agent_id: String },
    Spectator,
    Unavailable,
}
impl GameplaySubject {
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::Personal { owner_agent_id } => Some(owner_agent_id),
            _ => None,
        }
    }
    pub fn resolve(owner: Option<&str>, personal: bool) -> Self {
        match (owner.filter(|id| !id.trim().is_empty()), personal) {
            (Some(owner), _) => Self::Personal {
                owner_agent_id: owner.into(),
            },
            (None, true) => Self::Unavailable,
            (_, false) => Self::Spectator,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FirstSettlementEvidence {
    pub settlement_job_id: u64,
    pub settled_at: u64,
    pub output_ledger: MaterialLedgerId,
    pub settlement: StarterIndustrialSettlementSummaryV1,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplayCapabilityQuery {
    pub subject: GameplaySubject,
    pub source_tick: u64,
    pub goal_id: String,
    pub world_stage: IndustryStage,
    pub personal_stage: Option<IndustryStage>,
    pub primary_factory_id: Option<String>,
    pub current_recipe_id: Option<String>,
    pub current_stable_line: Option<bool>,
    pub first_settlement: Option<FirstSettlementEvidence>,
    pub recovery: Vec<RecoveryAssessment>,
}
impl GameplayCapabilityQuery {
    pub fn progression_stage(&self) -> IndustryStage {
        match self.subject {
            GameplaySubject::Spectator => self.world_stage,
            _ => self.personal_stage.unwrap_or(IndustryStage::Bootstrap),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAssessment {
    pub kind: String,
    pub status: String,
    pub reason_code: String,
    pub action: Option<serde_json::Value>,
    pub evidence: Option<serde_json::Value>,
    pub recheck: Option<String>,
}
impl WorldState {
    pub fn gameplay_capability_query(
        &self,
        subject: GameplaySubject,
        goal_id: &str,
    ) -> GameplayCapabilityQuery {
        let owner = subject.owner();
        let factory = owner.and_then(|id| primary_personal_factory(self, id));
        let reason = if goal_id != CURRENT_INDUSTRIAL_CAPABILITY_GOAL {
            "unsupported_recovery_goal"
        } else {
            match subject {
                GameplaySubject::Personal { .. } => "unsupported_recovery_action",
                GameplaySubject::Spectator => "spectator_has_no_personal_capability",
                GameplaySubject::Unavailable => "personal_owner_unavailable",
            }
        };
        GameplayCapabilityQuery {
            source_tick: self.time,
            goal_id: goal_id.into(),
            world_stage: self.industry_progress.stage,
            personal_stage: owner.map(|_| personal_stage(self, factory, owner)),
            primary_factory_id: factory.map(|f| f.factory_id.clone()),
            current_recipe_id: factory.and_then(|f| {
                f.production
                    .current_recipe_id
                    .clone()
                    .or_else(|| f.production.last_completed_recipe_id.clone())
            }),
            current_stable_line: owner
                .map(|_| factory.is_some_and(|f| has_current_stable_line(self, f))),
            first_settlement: settled_outcome(self, owner),
            recovery: ["repair", "rebuild", "pivot"]
                .into_iter()
                .map(|kind| RecoveryAssessment {
                    kind: kind.into(),
                    status: "unknown".into(),
                    reason_code: reason.into(),
                    action: None,
                    evidence: None,
                    recheck: None,
                })
                .collect(),
            subject,
        }
    }
}

// The canonical reducer owns the window and identity. Its completed window
// remains historical evidence while a different job is in flight; it cannot
// certify the current candidate on the player's goal card.
pub fn has_current_stable_line(state: &WorldState, factory: &FactoryState) -> bool {
    if matches!(
        factory.production.status,
        FactoryProductionStatus::Blocked | FactoryProductionStatus::Paused
    ) || !factory_has_canonical_stable_line(factory)
    {
        return false;
    }
    current_candidate_matches_completed(state, factory)
}

pub fn current_repeat_count(state: &WorldState, factory: &FactoryState) -> u32 {
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

pub fn primary_personal_factory<'a>(
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
                factory.factory_id == "factory.smelter.mk1",
                production.completed_jobs,
                production.last_completed_at.unwrap_or(0),
            )
        })
}

pub fn personal_stage(
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

/// Project only a recorded settlement for the currently controlled owner.
/// Current balances and mutable recipe profiles cannot reconstruct this fact.
pub fn settled_outcome(
    state: &WorldState,
    controlled_agent_id: Option<&str>,
) -> Option<FirstSettlementEvidence> {
    let milestone = state
        .industry_progress
        .starter_industrial_milestone
        .as_ref()?;
    let settlement = milestone.settlement_summary.as_ref()?;
    let owner = controlled_agent_id?;
    let origin = milestone.committed_recipe_origin.as_ref()?;
    let factory = state.factories.get(&milestone.factory_id)?;
    let site = state.factory_site_authorities.get(&factory.site_id)?;
    if milestone.profile_id != STARTER_INDUSTRIAL_PROFILE_ID
        || milestone.profile_revision != STARTER_INDUSTRIAL_PROFILE_REVISION
        || milestone.factory_id != STARTER_SMELTER_FACTORY_ID
        || milestone.recipe_id != STARTER_SMELTER_RECIPE_ID
        || !origin
            .submission
            .matches_recipe(owner, &milestone.factory_id, &milestone.recipe_id)
        || settlement.requester_agent_id != owner
        || factory.builder_agent_id != owner
        || site.owner_agent_id != owner
        || !site.active
        || factory.output_ledger != milestone.output_ledger
        || milestone.output_ledger != MaterialLedgerId::site(&factory.site_id)
        || !state
            .material_ledgers
            .contains_key(&milestone.output_ledger)
        || settlement.accepted_batches == 0
        || settlement.power_required < 0
        || !settlement
            .produce
            .iter()
            .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
    {
        return None;
    }
    Some(FirstSettlementEvidence {
        settlement_job_id: milestone.settlement_job_id,
        settled_at: milestone.settled_at,
        output_ledger: milestone.output_ledger.clone(),
        settlement: settlement.clone(),
    })
}
