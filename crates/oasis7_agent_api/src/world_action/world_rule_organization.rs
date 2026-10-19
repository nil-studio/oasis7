//! Typed intents for the organization continuity v1 contract.
//!
//! These commands describe requests only. The runtime resolves current
//! charter, asset-owner, governance, availability, and remedy authority before
//! committing any state transition.

use super::GovernanceFinalityCertificate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OrganizationContinuityCommandV1 {
    Enable {
        organization_id: String,
        expected_charter_hash: String,
        governance_proposal_key: String,
        designated_successors: Vec<String>,
        neutral_maintainers: Vec<String>,
        appointment_authorities: BTreeSet<String>,
        remedy_adjudicators: BTreeSet<String>,
        residual_share_bps: BTreeMap<String, u16>,
    },
    /// Registers one already accepted external AtomicExchange as an explicit
    /// organization activity source. Its full terms and controller identities
    /// are resolved by the runtime and bound to the governance proposal.
    RegisterActivityContract {
        organization_id: String,
        contract_id: String,
        expected_terms_hash: String,
        governance_proposal_key: String,
    },
    RecordAvailability {
        organization_id: String,
        from_world_tick: u64,
        through_world_tick: u64,
        world_writes_available: bool,
        notice_available: bool,
        remedy_available: bool,
        certificate: GovernanceFinalityCertificate,
    },
    /// Runtime-generated closure for committed ticks without fresh authority
    /// evidence. Only system events may use this command.
    FinalizeAvailabilityUnknown {
        organization_id: String,
        through_world_tick: u64,
    },
    /// Runtime-generated per-tick source-catalog and journal coverage close.
    /// Only action_id=0 events may use this command.
    CloseActivityTick {
        organization_id: String,
        through_world_tick: u64,
        previous_close_hash: Option<String>,
        source_catalog_root: String,
        through_event_id: u64,
        through_event_hash: String,
        close_hash: String,
    },
    OpenDormancyCase {
        organization_id: String,
        expected_manifest_root: String,
        trigger_activity_receipt: Option<String>,
    },
    OpenVoluntaryCase {
        organization_id: String,
        expected_charter_hash: String,
        governance_proposal_key: String,
        kind: OrganizationVoluntaryCaseKindV1,
    },
    AdvancePhase {
        case_id: String,
        expected_revision: u64,
    },
    PublishNotice {
        case_id: String,
        expected_revision: u64,
        recipient_ids: BTreeSet<String>,
        scope_refs: BTreeSet<String>,
        reason_code: String,
        trigger_fact_ref: String,
        evidence_refs: Vec<String>,
        allowed_actions: BTreeSet<String>,
        restricted_actions: BTreeSet<String>,
    },
    SubmitSuccessor {
        case_id: String,
        expected_revision: u64,
        candidate_id: String,
        scope_refs: BTreeSet<String>,
        budget_ref: String,
        qualification_receipts: Vec<String>,
        conflict_disclosures: BTreeSet<String>,
        acceptance_receipt: String,
    },
    CommitHandoff {
        case_id: String,
        expected_revision: u64,
        candidate_id: String,
        scope_refs: BTreeSet<String>,
        governance_proposal_key: String,
    },
    SubmitRemedy {
        case_id: String,
        expected_revision: u64,
        request_id: String,
        kind: OrganizationRemedyKindV1,
        scope_refs: BTreeSet<String>,
        fact_ref: String,
        evidence_refs: Vec<String>,
    },
    ResolveRemedy {
        case_id: String,
        expected_revision: u64,
        request_id: String,
        outcome: OrganizationRemedyOutcomeV1,
        supported_scope_refs: BTreeSet<String>,
        adjudication_ref: String,
        governance_proposal_key: String,
    },
    AdvanceDisposition {
        case_id: String,
        expected_revision: u64,
        governance_proposal_key: String,
    },
    SettleEstateAsset {
        case_id: String,
        expected_revision: u64,
        asset_ref: String,
        expected_asset_version: u64,
        operation: OrganizationEstateOperationV1,
        amount: i64,
        allocations: BTreeMap<String, i64>,
        effect_ref: String,
        governance_proposal_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationVoluntaryCaseKindV1 {
    Dissolution,
    Reorganization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationRemedyKindV1 {
    Reclaim,
    Appeal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationRemedyOutcomeV1 {
    Rejected,
    Withdrawn,
    Supported,
    PartiallySupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationEstateOperationV1 {
    ReturnCustody,
    PaySecuredClaim,
    PayApprovedPreservationCost,
    PayOrdinaryClaim,
    TransferBusiness,
    DistributeResidual,
}

impl OrganizationContinuityCommandV1 {
    pub fn governance_proposal(&self) -> Option<&str> {
        let proposal = match self {
            Self::Enable {
                governance_proposal_key,
                ..
            }
            | Self::OpenVoluntaryCase {
                governance_proposal_key,
                ..
            }
            | Self::RegisterActivityContract {
                governance_proposal_key,
                ..
            }
            | Self::CommitHandoff {
                governance_proposal_key,
                ..
            }
            | Self::ResolveRemedy {
                governance_proposal_key,
                ..
            }
            | Self::AdvanceDisposition {
                governance_proposal_key,
                ..
            }
            | Self::SettleEstateAsset {
                governance_proposal_key,
                ..
            } => Some(governance_proposal_key),
            _ => None,
        }?;
        (!proposal.is_empty()).then_some(proposal)
    }

    pub fn clear_governance_proposal(&mut self) {
        match self {
            Self::Enable {
                governance_proposal_key,
                ..
            }
            | Self::OpenVoluntaryCase {
                governance_proposal_key,
                ..
            }
            | Self::RegisterActivityContract {
                governance_proposal_key,
                ..
            }
            | Self::CommitHandoff {
                governance_proposal_key,
                ..
            }
            | Self::ResolveRemedy {
                governance_proposal_key,
                ..
            }
            | Self::AdvanceDisposition {
                governance_proposal_key,
                ..
            }
            | Self::SettleEstateAsset {
                governance_proposal_key,
                ..
            } => governance_proposal_key.clear(),
            _ => {}
        }
    }

    pub fn authority_scope(&self) -> String {
        match self {
            Self::Enable {
                organization_id, ..
            }
            | Self::RecordAvailability {
                organization_id, ..
            }
            | Self::OpenDormancyCase {
                organization_id, ..
            }
            | Self::OpenVoluntaryCase {
                organization_id, ..
            } => organization_id.clone(),
            Self::RegisterActivityContract {
                organization_id, ..
            }
            | Self::CloseActivityTick {
                organization_id, ..
            } => organization_id.clone(),
            Self::FinalizeAvailabilityUnknown {
                organization_id, ..
            } => organization_id.clone(),
            Self::AdvancePhase { case_id, .. }
            | Self::PublishNotice { case_id, .. }
            | Self::SubmitSuccessor { case_id, .. }
            | Self::CommitHandoff { case_id, .. }
            | Self::SubmitRemedy { case_id, .. }
            | Self::ResolveRemedy { case_id, .. }
            | Self::AdvanceDisposition { case_id, .. } => case_id.clone(),
            Self::SettleEstateAsset { asset_ref, .. } => asset_ref.clone(),
        }
    }

    pub fn is_runtime_observation(&self) -> bool {
        matches!(
            self,
            Self::FinalizeAvailabilityUnknown { .. } | Self::CloseActivityTick { .. }
        )
    }
}
