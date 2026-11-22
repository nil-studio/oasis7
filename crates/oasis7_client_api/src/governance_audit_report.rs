//! Canonical governance audit report shared by operator tools and recovery verification.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceRegistryAuditReport {
    pub world_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audited_manifest_digest: Option<String>,
    pub finality: GovernanceSlotAuditRow,
    pub controllers: Vec<GovernanceSlotAuditRow>,
    pub rollback_authorities: Vec<RollbackAuthorityAuditRow>,
    pub rollback_blockers: Vec<String>,
    pub overall_single_failure_tolerance_pass: bool,
    pub manifest_match_pass: Option<bool>,
    pub overall_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RollbackAuthorityAuditRow {
    pub slot_id: String,
    pub role: String,
    pub configured: bool,
    pub active: bool,
    pub threshold: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_match: Option<bool>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GovernanceSlotAuditRow {
    pub slot_id: String,
    pub threshold: u16,
    pub signer_count: usize,
    pub tolerated_failures: usize,
    pub single_failure_tolerant: bool,
    pub threshold_matches_expectation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_match: Option<bool>,
    pub status: String,
}

pub fn audit_row(
    slot_id: &str,
    threshold: u16,
    signer_count: usize,
    expected_threshold: u16,
    manifest_match: Option<bool>,
) -> GovernanceSlotAuditRow {
    let tolerated_failures = signer_count.saturating_sub(usize::from(threshold));
    let single_failure_tolerant = tolerated_failures >= 1;
    let threshold_matches_expectation = threshold == expected_threshold;
    let status = if !threshold_matches_expectation {
        "threshold_mismatch".to_string()
    } else if !single_failure_tolerant {
        "single_failure_blocks_slot".to_string()
    } else {
        "single_failure_tolerant".to_string()
    };
    GovernanceSlotAuditRow {
        slot_id: slot_id.to_string(),
        threshold,
        signer_count,
        tolerated_failures,
        single_failure_tolerant,
        threshold_matches_expectation,
        manifest_match,
        status,
    }
}
