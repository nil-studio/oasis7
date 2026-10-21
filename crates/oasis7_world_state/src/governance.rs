//! Governance types - proposals, decisions, and events.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::events::DomainEvent;
use super::gameplay_state::GovernanceIdentityStatus;
use super::manifest::{Manifest, ManifestPatch};
use super::types::{ProposalId, WorldTime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceExecutionPolicy {
    pub timelock_ticks: u64,
    pub epoch_length_ticks: u64,
    pub activation_delay_epochs: u64,
    pub emergency_brake_guardian_threshold: u16,
    pub emergency_veto_guardian_threshold: u16,
    pub emergency_brake_max_ticks: u64,
}

impl Default for GovernanceExecutionPolicy {
    fn default() -> Self {
        Self {
            timelock_ticks: 0,
            epoch_length_ticks: 120,
            activation_delay_epochs: 0,
            emergency_brake_guardian_threshold: 2,
            emergency_veto_guardian_threshold: 2,
            emergency_brake_max_ticks: 720,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceIdentityPenaltyStatus {
    #[default]
    Applied,
    Appealed,
    AppealAccepted,
    AppealRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceIdentityPenaltyRecord {
    pub penalty_id: u64,
    pub target_agent_id: String,
    pub evidence_hash: String,
    pub reason: String,
    #[serde(default)]
    pub slash_stake: u64,
    #[serde(default)]
    pub appeal_deadline_tick: WorldTime,
    #[serde(default)]
    pub status: GovernanceIdentityPenaltyStatus,
    #[serde(default)]
    pub identity_status_before: GovernanceIdentityStatus,
    #[serde(default)]
    pub detection_source: String,
    #[serde(default)]
    pub detection_risk_score: i64,
    #[serde(default)]
    pub detection_incident_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence_chain_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appeal_evidence_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_evidence_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appellant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appeal_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at_tick: Option<WorldTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceIdentityPenaltyMonitorStats {
    pub total_penalties: u64,
    pub appealed_penalties: u64,
    pub resolved_appeals: u64,
    pub appeal_accepted_penalties: u64,
    pub high_risk_open_penalties: u64,
    pub false_positive_rate_bps: u16,
}

/// A proposal for manifest changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub author: String,
    pub base_manifest_hash: String,
    pub manifest: Manifest,
    pub patch: Option<ManifestPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_at_tick: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before_tick: Option<WorldTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activate_epoch: Option<u64>,
    #[serde(default)]
    pub timelock_ticks: u64,
    pub status: ProposalStatus,
}

/// The current status of a proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data")]
pub enum ProposalStatus {
    Proposed,
    Shadowed {
        manifest_hash: String,
    },
    Approved {
        manifest_hash: String,
        approver: String,
    },
    Rejected {
        reason: String,
    },
    Applied {
        manifest_hash: String,
    },
}

impl ProposalStatus {
    pub fn label(&self) -> String {
        match self {
            ProposalStatus::Proposed => "proposed".to_string(),
            ProposalStatus::Shadowed { .. } => "shadowed".to_string(),
            ProposalStatus::Approved { .. } => "approved".to_string(),
            ProposalStatus::Rejected { .. } => "rejected".to_string(),
            ProposalStatus::Applied { .. } => "applied".to_string(),
        }
    }
}

/// A decision on a proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", content = "data")]
pub enum ProposalDecision {
    Approve,
    Reject { reason: String },
}

pub use oasis7_agent_api::world_action::GovernanceFinalityCertificate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceFinalityEpochSnapshot {
    pub epoch_id: u64,
    #[serde(default)]
    pub threshold_bps: u16,
    #[serde(default)]
    pub min_unique_signers: u16,
    #[serde(default)]
    pub validator_set_hash: String,
    #[serde(default)]
    pub stake_root: String,
    #[serde(default)]
    pub threshold: u16,
    #[serde(default)]
    pub signer_node_ids: Vec<String>,
    #[serde(default)]
    pub validator_stakes: BTreeMap<String, u64>,
}

impl GovernanceFinalityEpochSnapshot {
    pub fn effective_min_unique_signers(&self) -> u16 {
        if self.min_unique_signers > 0 {
            self.min_unique_signers
        } else {
            self.threshold
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceThresholdSignerPolicy {
    #[serde(default)]
    pub threshold: u16,
    #[serde(default)]
    pub allowed_public_keys: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceFinalitySignerRegistry {
    #[serde(default)]
    pub slot_id: String,
    #[serde(default)]
    pub threshold: u16,
    #[serde(default)]
    pub threshold_bps: u16,
    #[serde(default)]
    pub signer_bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub validator_stakes: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceMainTokenControllerRegistry {
    #[serde(default)]
    pub genesis_controller_account_id: String,
    #[serde(default)]
    pub treasury_bucket_controller_slots: BTreeMap<String, String>,
    #[serde(default)]
    pub restricted_starter_claim_admin_account_ids: BTreeSet<String>,
    #[serde(default)]
    pub controller_signer_policies: BTreeMap<String, GovernanceThresholdSignerPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceValidatorAdmissionStatus {
    #[default]
    Applied,
    ApprovedCandidate,
    ProbationReady,
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GovernanceValidatorAdmissionRecord {
    #[serde(default)]
    pub candidate_id: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub finality_signer_public_key: String,
    #[serde(default = "default_governance_validator_admission_stake")]
    pub stake: u64,
    #[serde(default)]
    pub operator_owner: String,
    #[serde(default)]
    pub public_manifest_hash: String,
    #[serde(default)]
    pub requested_at_epoch: u64,
    #[serde(default)]
    pub last_transition_tick: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_epoch: Option<u64>,
    #[serde(default)]
    pub status: GovernanceValidatorAdmissionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation_reason: Option<String>,
}

fn default_governance_validator_admission_stake() -> u64 {
    100
}

/// Events related to governance actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum GovernanceEvent {
    Proposed {
        proposal_id: ProposalId,
        author: String,
        base_manifest_hash: String,
        manifest: Manifest,
        patch: Option<ManifestPatch>,
    },
    ShadowReport {
        proposal_id: ProposalId,
        manifest_hash: String,
    },
    Approved {
        proposal_id: ProposalId,
        approver: String,
        decision: ProposalDecision,
    },
    Queued {
        proposal_id: ProposalId,
        manifest_hash: String,
        queued_at_tick: WorldTime,
        not_before_tick: WorldTime,
        activate_epoch: u64,
        timelock_ticks: u64,
    },
    Applied {
        proposal_id: ProposalId,
        #[serde(default)]
        manifest_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        consensus_height: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        threshold: Option<u16>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        signer_node_ids: Vec<String>,
    },
    /// Historical signer/finality inputs are journaled at the epoch in which
    /// they were used.  Replay must not depend on a mutable current snapshot.
    FinalityEpochSnapshotSet {
        snapshot: GovernanceFinalityEpochSnapshot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        previous: Option<GovernanceFinalityEpochSnapshot>,
    },
    FinalityEpochSnapshotRemoved {
        epoch_id: u64,
        snapshot: GovernanceFinalityEpochSnapshot,
    },
    EmergencyBrakeActivated {
        initiator: String,
        reason: String,
        active_until_tick: WorldTime,
        threshold: u16,
        signer_node_ids: Vec<String>,
    },
    EmergencyBrakeReleased {
        initiator: String,
        reason: String,
        #[serde(default)]
        threshold: u16,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        signer_node_ids: Vec<String>,
    },
    EmergencyVetoed {
        proposal_id: ProposalId,
        initiator: String,
        reason: String,
        threshold: u16,
        signer_node_ids: Vec<String>,
    },
    IdentityPenaltyApplied {
        penalty_id: u64,
        target_agent_id: String,
        evidence_hash: String,
        initiator: String,
        reason: String,
        slash_stake: u64,
        appeal_deadline_tick: WorldTime,
        threshold: u16,
        signer_node_ids: Vec<String>,
    },
    IdentityPenaltyAppealed {
        penalty_id: u64,
        appellant: String,
        reason: String,
    },
    IdentityPenaltyResolved {
        penalty_id: u64,
        resolver: String,
        accepted: bool,
        reason: String,
    },
    RestrictedStarterClaimAdminRegistryUpdated {
        controller_account_id: String,
        previous_admin_account_ids: Vec<String>,
        next_admin_account_ids: Vec<String>,
    },
    ValidatorAdmissionSubmitted {
        controller_account_id: String,
        candidate_id: String,
        node_id: String,
        finality_signer_public_key: String,
        #[serde(default = "default_governance_validator_admission_stake")]
        stake: u64,
        operator_owner: String,
        public_manifest_hash: String,
        requested_at_epoch: u64,
    },
    ValidatorAdmissionApproved {
        controller_account_id: String,
        candidate_id: String,
        approved_at_epoch: u64,
    },
    ValidatorAdmissionActivated {
        controller_account_id: String,
        candidate_id: String,
        activation_epoch: u64,
    },
    ValidatorAdmissionRevoked {
        controller_account_id: String,
        candidate_id: String,
        node_id: String,
        revoked_at_epoch: u64,
        reason: String,
    },
}

/// Schedule entry for agent activation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSchedule {
    pub agent_id: String,
    pub event: DomainEvent,
}
