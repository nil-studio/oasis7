//! Read-only owner-attributed capability facts. No personal stage is persisted.
use crate::GameplayPolicyState;
use crate::events::IndustryStage;
use crate::state::industry_transition::{
    factory_has_canonical_stable_line, industry_stage_for_progress,
};
use crate::state::*;
use crate::types::MaterialLedgerId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Borrowed original readonly facts. This carries no admission, permission or
/// supplied verdict; all stage/selection/settlement predicates remain below.
/// Current adapters must join these facts from their actual qualified sources.
pub struct GameplayCapabilityFactsV1<'a> {
    pub(crate) time: u64,
    pub(crate) factories: &'a BTreeMap<String, FactoryState>,
    pub(crate) pending_recipe_jobs: &'a BTreeMap<u64, RecipeJobState>,
    pub(crate) industry_progress: &'a IndustryProgressState,
    pub(crate) gameplay_policy: &'a GameplayPolicyState,
    pub(crate) factory_site_authorities: &'a BTreeMap<String, FactorySiteAuthorityV1>,
    pub(crate) material_ledgers: &'a BTreeMap<MaterialLedgerId, BTreeMap<String,i64>>,
}
impl<'a> GameplayCapabilityFactsV1<'a> {
    pub(crate) fn from_world(state: &'a WorldState) -> Self {
        Self { time:state.time, factories:&state.factories,pending_recipe_jobs:&state.pending_recipe_jobs,industry_progress:&state.industry_progress,gameplay_policy:&state.gameplay_policy,factory_site_authorities:&state.factory_site_authorities,material_ledgers:&state.material_ledgers }
    }
}

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
        gameplay_capability_query_from_facts_v1(&GameplayCapabilityFactsV1::from_world(self),subject,goal_id)
    }
}

pub fn gameplay_capability_query_from_facts_v1(
    facts:&GameplayCapabilityFactsV1<'_>, subject:GameplaySubject, goal_id:&str,
) -> GameplayCapabilityQuery {
    infallible(readonly::gameplay_capability_query_readonly_v1(facts,subject,goal_id))
}

// The canonical reducer owns the window and identity. Its completed window
// remains historical evidence while a different job is in flight; it cannot
// certify the current candidate on the player's goal card.
pub fn has_current_stable_line(state: &WorldState, factory: &FactoryState) -> bool {
    has_current_stable_line_from_facts_v1(&GameplayCapabilityFactsV1::from_world(state),factory)
}
pub fn has_current_stable_line_from_facts_v1(state:&GameplayCapabilityFactsV1<'_>, factory:&FactoryState)->bool {
    infallible(readonly::has_current_stable_line_readonly_v1(state,factory))
}

pub fn current_repeat_count(state: &WorldState, factory: &FactoryState) -> u32 {
    current_repeat_count_from_facts_v1(&GameplayCapabilityFactsV1::from_world(state),factory)
}
pub fn current_repeat_count_from_facts_v1(state:&GameplayCapabilityFactsV1<'_>, factory:&FactoryState)->u32 {
    if current_candidate_matches_completed(state, factory) {
        factory.production.same_recipe_repeat_count
    } else {
        0
    }
}

fn current_candidate_matches_completed(state: &GameplayCapabilityFactsV1<'_>, factory: &FactoryState) -> bool {
    infallible(readonly::current_candidate_matches_completed_readonly_v1(state,factory))
}

pub fn primary_personal_factory<'a>(
    state: &'a WorldState,
    agent_id: &str,
) -> Option<&'a FactoryState> {
    primary_personal_factory_from_facts_v1(&GameplayCapabilityFactsV1::from_world(state),agent_id)
}
pub fn primary_personal_factory_from_facts_v1<'a>(state:&GameplayCapabilityFactsV1<'a>,agent_id:&str)->Option<&'a FactoryState> {
    let key=infallible(readonly::primary_outer_key(state,agent_id))?;
    state.factories.get(&key)
}

pub fn personal_stage(
    state: &WorldState,
    factory: Option<&FactoryState>,
    agent_id: Option<&str>,
) -> IndustryStage {
    personal_stage_from_facts_v1(&GameplayCapabilityFactsV1::from_world(state),factory,agent_id)
}
pub fn personal_stage_from_facts_v1(state:&GameplayCapabilityFactsV1<'_>,factory:Option<&FactoryState>,agent_id:Option<&str>)->IndustryStage {
    infallible(readonly::personal_stage_readonly_v1(state,factory,agent_id))
}

/// Project only a recorded settlement for the currently controlled owner.
/// Current balances and mutable recipe profiles cannot reconstruct this fact.
pub fn settled_outcome(
    state: &WorldState,
    controlled_agent_id: Option<&str>,
) -> Option<FirstSettlementEvidence> {
    settled_outcome_from_facts_v1(&GameplayCapabilityFactsV1::from_world(state),controlled_agent_id)
}
pub fn settled_outcome_from_facts_v1(state:&GameplayCapabilityFactsV1<'_>,controlled_agent_id:Option<&str>)->Option<FirstSettlementEvidence> {
    infallible(readonly::settled_outcome_readonly_v1(state,controlled_agent_id))
}

#[path = "gameplay_capability_readonly.rs"]
mod readonly;
pub use readonly::{GameplayCapabilityReadonlyFactsV1,gameplay_capability_query_readonly_v1,has_current_stable_line_readonly_v1,personal_stage_readonly_v1,settled_outcome_readonly_v1,current_candidate_matches_completed_readonly_v1};
fn infallible<T>(result:Result<T,std::convert::Infallible>)->T {match result{Ok(value)=>value,Err(never)=>match never{}}}
