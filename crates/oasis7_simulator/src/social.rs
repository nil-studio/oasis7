use serde::{Deserialize, Serialize};

use super::types::{ResourceKind, ResourceOwner, WorldEventId, WorldTime};

pub(crate) fn default_next_social_fact_id() -> u64 {
    1
}

pub(crate) fn default_next_social_edge_id() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialStake {
    pub kind: ResourceKind,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialFactImpactQuote {
    pub actor_id: String,
    pub action_kind: String,
    pub schema_id: String,
    pub subject_id: Option<String>,
    pub object_id: Option<String>,
    pub claim_summary: String,
    pub confidence_ppm: Option<i64>,
    pub stake_at_risk: i64,
    pub ttl_ticks: Option<u64>,
    pub affected_relationships: Vec<String>,
    pub affected_social_surfaces: Vec<String>,
    pub cooperation_opportunity_delta: String,
    pub blacklist_or_dispute_risk: String,
    pub governance_or_claim_relevance: String,
    pub recommended_social_action: String,
    pub why_this_action_matters: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SocialFactLifecycleState {
    #[default]
    Active,
    Challenged,
    Confirmed,
    Retracted,
    Revoked,
    Expired,
}

impl SocialFactLifecycleState {
    pub fn supports_backing(self) -> bool {
        matches!(self, Self::Active | Self::Confirmed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialAdjudicationDecision {
    Confirm,
    Retract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialChallengeState {
    pub challenger: ResourceOwner,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stake: Option<SocialStake>,
    pub challenged_at_tick: WorldTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialFactState {
    pub fact_id: u64,
    pub actor: ResourceOwner,
    pub schema_id: String,
    pub subject: ResourceOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<ResourceOwner>,
    pub claim: String,
    pub confidence_ppm: i64,
    pub evidence_event_ids: Vec<WorldEventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ticks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_tick: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stake: Option<SocialStake>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge: Option<SocialChallengeState>,
    #[serde(default)]
    pub lifecycle: SocialFactLifecycleState,
    pub created_at_tick: WorldTime,
    pub updated_at_tick: WorldTime,
}

impl SocialFactState {
    pub fn supports_backing(&self) -> bool {
        self.lifecycle.supports_backing()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SocialEdgeLifecycleState {
    #[default]
    Active,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialEdgeState {
    pub edge_id: u64,
    pub declarer: ResourceOwner,
    pub schema_id: String,
    pub relation_kind: String,
    pub from: ResourceOwner,
    pub to: ResourceOwner,
    pub weight_bps: i64,
    pub backing_fact_ids: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ticks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_tick: Option<WorldTime>,
    #[serde(default)]
    pub lifecycle: SocialEdgeLifecycleState,
    pub created_at_tick: WorldTime,
    pub updated_at_tick: WorldTime,
}

impl SocialEdgeState {
    pub fn is_active(&self) -> bool {
        matches!(self.lifecycle, SocialEdgeLifecycleState::Active)
    }
}
