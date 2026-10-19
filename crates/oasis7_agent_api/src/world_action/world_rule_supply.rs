//! Emergency supply v1 consumes authority-certified minimum-service facts.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyServiceRegistrationV1 {
    pub obligation_id: String,
    pub beneficiary_id: String,
    pub controller_id: String,
    pub region_id: String,
    pub location_id: String,
    pub registered_at: u64,
    pub electricity_per_interval: i64,
    pub public_critical: bool,
    pub recipe_ids: BTreeSet<String>,
    pub source_ref: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyObservationV1 {
    pub window_start: u64,
    pub world_root: String,
    pub obligation_ids: BTreeSet<String>,
    /// Authority-certified normal coverage; excludes all emergency provenance.
    pub normal_coverage: std::collections::BTreeMap<String, i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyEvidenceV1 {
    pub region_id: String,
    pub region_budget_lineage: String,
    pub observations: Vec<EmergencyObservationV1>,
    pub source_ref: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyNeedV1 {
    pub obligation_id: String,
    pub interval_start: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum EmergencySupplyCommandV1 {
    Observe {
        region_id: String,
    },
    RegisterService {
        proposal_key: String,
        registration: EmergencyServiceRegistrationV1,
    },
    PublishEvidence {
        proposal_key: String,
        evidence: EmergencyEvidenceV1,
    },
    Authorize {
        proposal_key: String,
        evidence_id: String,
        reviewer_id: String,
        starts_at: u64,
        expires_at: u64,
        quantity: i64,
    },
    RefreshEvidence {
        authorization_id: String,
        evidence_id: String,
    },
    Request {
        authorization_id: String,
        needs: Vec<EmergencyNeedV1>,
    },
    Freeze {
        authorization_id: String,
    },
    Allocate {
        batch_id: String,
    },
    Deliver {
        batch_id: String,
        need_id: String,
        quantity: i64,
    },
    OpenReview {
        batch_id: String,
        need_id: String,
        reason_ref: String,
    },
    DecideReview {
        proposal_key: String,
        review_id: String,
        cancel_unsettled: bool,
    },
    Close {
        authorization_id: String,
    },
}
impl EmergencySupplyCommandV1 {
    pub fn governance_proposal(&self) -> Option<&str> {
        match self {
            Self::RegisterService { proposal_key, .. }
            | Self::PublishEvidence { proposal_key, .. }
            | Self::Authorize { proposal_key, .. }
            | Self::DecideReview { proposal_key, .. } => Some(proposal_key),
            _ => None,
        }
    }
    pub fn clear_governance_proposal(&mut self) {
        match self {
            Self::RegisterService { proposal_key, .. }
            | Self::PublishEvidence { proposal_key, .. }
            | Self::Authorize { proposal_key, .. }
            | Self::DecideReview { proposal_key, .. } => proposal_key.clear(),
            _ => {}
        }
    }
    pub fn authority_scope(&self) -> String {
        match self {
            Self::Observe { region_id } => region_id.clone(),
            Self::RegisterService { registration, .. } => registration.region_id.clone(),
            Self::PublishEvidence { evidence, .. } => evidence.region_id.clone(),
            Self::Authorize { evidence_id, .. } => evidence_id.clone(),
            Self::RefreshEvidence {
                authorization_id, ..
            }
            | Self::Request {
                authorization_id, ..
            }
            | Self::Freeze { authorization_id }
            | Self::Close { authorization_id } => authorization_id.clone(),
            Self::Allocate { batch_id }
            | Self::Deliver { batch_id, .. }
            | Self::OpenReview { batch_id, .. } => batch_id.clone(),
            Self::DecideReview { review_id, .. } => review_id.clone(),
        }
    }
}
