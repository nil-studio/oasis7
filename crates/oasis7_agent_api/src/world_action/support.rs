use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
type LocationId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BodyKernelView {
    pub mass_kg: u64,
    pub radius_cm: u64,
    pub thrust_limit: u64,
    pub cross_section_cm2: u64,
}

pub use oasis7_domain_types::world_resource::ResourceKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ModuleInstallTarget {
    #[default]
    SelfAgent,
    LocationInfrastructure {
        location_id: LocationId,
    },
}

/// Settlement semantics for a persisted economic contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EconomicContractFulfillmentKind {
    #[default]
    AtomicExchange,
    Service,
}

/// Finality certificate bound to consensus height and signer threshold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceFinalityCertificate {
    pub proposal_id: ProposalId,
    pub manifest_hash: String,
    pub consensus_height: u64,
    #[serde(default)]
    pub epoch_id: u64,
    #[serde(default)]
    pub validator_set_hash: String,
    #[serde(default)]
    pub stake_root: String,
    #[serde(default)]
    pub threshold_bps: u16,
    #[serde(default)]
    pub min_unique_signers: u16,
    #[serde(default)]
    pub threshold: u16,
    pub signatures: BTreeMap<String, String>,
}

impl GovernanceFinalityCertificate {
    pub const SIGNATURE_PREFIX_ED25519_V1: &'static str = "govsig:ed25519:v1:";

    #[expect(
        clippy::too_many_arguments,
        reason = "The governance signature preimage is a stable wire contract; grouping arguments would alter callers and signing compatibility."
    )]
    pub fn signing_payload_v1(
        proposal_id: ProposalId,
        manifest_hash: &str,
        consensus_height: u64,
        epoch_id: u64,
        validator_set_hash: &str,
        stake_root: &str,
        threshold_bps: u16,
        min_unique_signers: u16,
        signer_node_id: &str,
    ) -> Vec<u8> {
        format!(
            "govfinal:ed25519:v1|{proposal_id}|{manifest_hash}|{consensus_height}|{epoch_id}|{validator_set_hash}|{stake_root}|{threshold_bps}|{min_unique_signers}|{signer_node_id}"
        )
        .into_bytes()
    }

    pub fn effective_min_unique_signers(&self) -> u16 {
        if self.min_unique_signers > 0 {
            self.min_unique_signers
        } else {
            self.threshold
        }
    }
}

/// Per-node settlement result for one epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeSettlement {
    pub node_id: String,
    pub obligation_met: bool,
    pub compute_score: f64,
    pub storage_score: f64,
    pub uptime_score: f64,
    pub reliability_score: f64,
    pub storage_reward_score: f64,
    pub rewardable_storage_bytes: u64,
    pub penalty_score: f64,
    pub total_score: f64,
    pub main_awarded_points: u64,
    pub storage_awarded_points: u64,
    pub awarded_points: u64,
    pub cumulative_points: u64,
}

/// A full epoch settlement report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpochSettlementReport {
    pub epoch_index: u64,
    pub pool_points: u64,
    pub storage_pool_points: u64,
    pub distributed_points: u64,
    pub storage_distributed_points: u64,
    pub total_distributed_points: u64,
    pub settlements: Vec<NodeSettlement>,
}

/// On-chain mint record derived from node points settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodeRewardMintRecord {
    pub epoch_index: u64,
    pub node_id: String,
    pub source_awarded_points: u64,
    pub minted_power_credits: u64,
    pub settlement_hash: String,
    pub signer_node_id: String,
    pub signature: String,
}
