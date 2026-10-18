//! Persisted gameplay-layer state models.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use oasis7_agent_api::world_action::ResourceKind;

use super::types::WorldTime;

/// One-time onboarding liquid OC claim used to unlock the first LLM interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StarterOcClaimState {
    pub agent_id: String,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    pub amount: u64,
    pub claimed_at: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_treasury_bucket_id: Option<String>,
}

/// Persisted alliance relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllianceState {
    pub alliance_id: String,
    pub members: Vec<String>,
    pub charter: String,
    pub formed_by_agent_id: String,
    pub formed_at: WorldTime,
}

/// Per-agent consequence payload persisted on war conclusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WarParticipantOutcome {
    pub agent_id: String,
    #[serde(default)]
    pub electricity_delta: i64,
    #[serde(default)]
    pub data_delta: i64,
    #[serde(default)]
    pub reputation_delta: i64,
}

/// Persisted war declaration state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarState {
    pub war_id: String,
    pub initiator_agent_id: String,
    pub aggressor_alliance_id: String,
    pub defender_alliance_id: String,
    pub objective: String,
    pub intensity: u32,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub declared_mobilization_electricity_cost: i64,
    #[serde(default)]
    pub declared_mobilization_data_cost: i64,
    #[serde(default)]
    pub max_duration_ticks: u64,
    #[serde(default)]
    pub aggressor_score: i64,
    #[serde(default)]
    pub defender_score: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concluded_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner_alliance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loser_alliance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_summary: Option<String>,
    #[serde(default)]
    pub participant_outcomes: Vec<WarParticipantOutcome>,
    pub declared_at: WorldTime,
}

/// Lifecycle state for one governance proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceProposalStatus {
    #[default]
    Open,
    Passed,
    Rejected,
}

/// Governance identity status used for anti-sybil controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceIdentityStatus {
    #[default]
    Active,
    Frozen,
    Expelled,
}

/// Persisted governance identity profile (stake + status + warmup).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceIdentityProfileState {
    pub agent_id: String,
    #[serde(default)]
    pub stake_locked: u64,
    #[serde(default)]
    pub warmup_until_tick: WorldTime,
    #[serde(default)]
    pub status: GovernanceIdentityStatus,
    #[serde(default)]
    pub slash_count: u32,
    #[serde(default)]
    pub updated_at: WorldTime,
}

/// Snapshot of one voter's governance identity at proposal open time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceVoteWeightSnapshotState {
    pub agent_id: String,
    #[serde(default)]
    pub reputation_score: i64,
    #[serde(default)]
    pub stake_locked: u64,
    #[serde(default)]
    pub status: GovernanceIdentityStatus,
    #[serde(default)]
    pub vote_weight_cap: u32,
}

pub const GOVERNANCE_IDENTITY_DEFAULT_MAX_VOTE_WEIGHT: u32 = 100;

/// Governance proposal lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceProposalState {
    pub proposal_key: String,
    pub proposer_agent_id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub voting_window_ticks: u64,
    #[serde(default)]
    pub quorum_weight: u64,
    #[serde(default)]
    pub pass_threshold_bps: u16,
    pub opened_at: WorldTime,
    pub closes_at: WorldTime,
    #[serde(default)]
    pub status: GovernanceProposalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finalized_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winning_option: Option<String>,
    #[serde(default)]
    pub winning_weight: u64,
    #[serde(default)]
    pub total_weight_at_finalize: u64,
    #[serde(default)]
    pub snapshot_at_tick: WorldTime,
    #[serde(default)]
    pub vote_weight_snapshot: BTreeMap<String, GovernanceVoteWeightSnapshotState>,
}

/// Persisted ballot for one voter in one governance proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceVoteBallotState {
    pub option: String,
    pub weight: u32,
    pub voted_at: WorldTime,
}

/// Aggregated governance vote state by proposal key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceVoteState {
    pub proposal_key: String,
    #[serde(default)]
    pub votes_by_agent: BTreeMap<String, GovernanceVoteBallotState>,
    #[serde(default)]
    pub tallies: BTreeMap<String, u64>,
    #[serde(default)]
    pub total_weight: u64,
    pub last_updated_at: WorldTime,
}

/// Lifecycle state for one crisis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrisisStatus {
    Active,
    #[default]
    Resolved,
    TimedOut,
}

/// Persisted crisis lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrisisState {
    pub crisis_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub severity: u32,
    #[serde(default)]
    pub status: CrisisStatus,
    #[serde(default)]
    pub opened_at: WorldTime,
    #[serde(default)]
    pub expires_at: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolver_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    pub impact: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<WorldTime>,
}

/// Persisted meta progression state for one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetaProgressState {
    pub agent_id: String,
    #[serde(default)]
    pub track_points: BTreeMap<String, i64>,
    #[serde(default)]
    pub total_points: i64,
    #[serde(default)]
    pub achievements: Vec<String>,
    #[serde(default)]
    pub unlocked_tiers: BTreeMap<String, Vec<String>>,
    pub last_granted_at: WorldTime,
}

/// Lifecycle state for one economic contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EconomicContractStatus {
    #[default]
    Open,
    Accepted,
    Settled,
    Expired,
}

pub use oasis7_agent_api::world_action::EconomicContractFulfillmentKind;

/// Persisted economic contract state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EconomicContractState {
    pub contract_id: String,
    pub creator_agent_id: String,
    pub counterparty_agent_id: String,
    #[serde(default)]
    pub fulfillment_kind: EconomicContractFulfillmentKind,
    pub settlement_kind: ResourceKind,
    pub settlement_amount: i64,
    pub reputation_stake: i64,
    pub expires_at: WorldTime,
    pub description: String,
    #[serde(default)]
    pub status: EconomicContractStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_success: Option<bool>,
    #[serde(default)]
    pub transfer_amount: i64,
    #[serde(default)]
    pub tax_amount: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settlement_notes: Option<String>,
}

/// Persisted agent claim ownership and upkeep state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentClaimState {
    pub target_agent_id: String,
    pub claim_owner_id: String,
    #[serde(default)]
    pub reputation_tier: u8,
    #[serde(default)]
    pub slot_index: u8,
    #[serde(default)]
    pub activation_fee_amount: u64,
    #[serde(default)]
    pub activation_fee_burn_amount: u64,
    #[serde(default)]
    pub activation_fee_treasury_amount: u64,
    #[serde(default)]
    pub claim_bond_amount: u64,
    #[serde(default)]
    pub locked_bond_amount: u64,
    #[serde(default)]
    pub upfront_restricted_spent_amount: u64,
    #[serde(default)]
    pub upfront_liquid_spent_amount: u64,
    #[serde(default)]
    pub claim_bond_locked_restricted_amount: u64,
    #[serde(default)]
    pub claim_bond_locked_liquid_amount: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_bond_restricted_source_treasury_bucket_id: Option<String>,
    #[serde(default)]
    pub upkeep_per_epoch: u64,
    #[serde(default)]
    pub release_cooldown_epochs: u64,
    #[serde(default)]
    pub grace_epochs: u64,
    #[serde(default)]
    pub idle_warning_epochs: u64,
    #[serde(default)]
    pub forced_idle_reclaim_epochs: u64,
    #[serde(default)]
    pub forced_reclaim_penalty_bps: u16,
    #[serde(default)]
    pub claimed_at_epoch: u64,
    #[serde(default)]
    pub upkeep_paid_through_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delinquent_since_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_deadline_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_requested_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ready_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_warning_emitted_at_epoch: Option<u64>,
}

fn default_policy_max_open_contracts_per_agent() -> u16 {
    4
}

/// Minimal governance policy knobs for gameplay economy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayPolicyState {
    #[serde(default)]
    pub electricity_tax_bps: u16,
    #[serde(default)]
    pub data_tax_bps: u16,
    #[serde(default)]
    pub power_trade_fee_bps: u16,
    #[serde(default = "default_policy_max_open_contracts_per_agent")]
    pub max_open_contracts_per_agent: u16,
    #[serde(default)]
    pub blocked_agents: Vec<String>,
    #[serde(default)]
    pub forbidden_location_ids: Vec<String>,
    #[serde(default)]
    pub updated_at: WorldTime,
}

impl Default for GameplayPolicyState {
    fn default() -> Self {
        Self {
            electricity_tax_bps: 200,
            data_tax_bps: 300,
            power_trade_fee_bps: 0,
            max_open_contracts_per_agent: default_policy_max_open_contracts_per_agent(),
            blocked_agents: Vec::new(),
            forbidden_location_ids: Vec::new(),
            updated_at: 0,
        }
    }
}
