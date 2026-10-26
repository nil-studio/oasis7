//! Closed management HTTP contracts. Principals are derived from native authentication.
use super::{DirectoryEntry, ManagerClosureStatus, ProvisionPlan};
use crate::{DecimalU64, approval::ApprovedWorldV1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProvisionDraftRequestV1 {
    pub api_version: u8,
    pub client_request_id: String,
    pub plan: ProvisionPlan,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftPhase {
    DraftPending,
    DraftInitialized,
    Frozen,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct AccountDraftV1 {
    pub entry: DirectoryEntry,
    pub plan: ProvisionPlan,
    pub world_identity: ApprovedWorldV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProvisionDraftViewV1 {
    pub api_version: u8,
    pub phase: DraftPhase,
    pub entry: DirectoryEntry,
    pub plan: ProvisionPlan,
    pub world_identity: ApprovedWorldV1,
    pub security_epoch: DecimalU64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagerFreezeRequestV1 {
    pub api_version: u8,
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagerClosureViewV1 {
    pub api_version: u8,
    pub status: ManagerClosureStatus,
    pub associated_accounts: DecimalU64,
    pub pending_accounts: Vec<String>,
    pub pending_operations: Vec<String>,
}

/// The public body carries no claimed manager session or authority facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(try_from = "RawProvisionApproveRequestV1")]
pub struct ProvisionApproveRequestV1 {
    pub api_version: u8,
    pub input: crate::approval_admission::ApprovalAdmissionInput,
}
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct RawProvisionApproveRequestV1 {
    api_version: u8,
    input: crate::approval_admission::ApprovalAdmissionInput,
}
impl TryFrom<RawProvisionApproveRequestV1> for ProvisionApproveRequestV1 {
    type Error = &'static str;
    fn try_from(raw: RawProvisionApproveRequestV1) -> Result<Self, Self::Error> {
        let request = Self {
            api_version: raw.api_version,
            input: raw.input,
        };
        request
            .validate()
            .map_err(|_| "invalid provision approval request")?;
        Ok(request)
    }
}
impl ProvisionApproveRequestV1 {
    /// Deadline/source validity is checked by Account state. Historical exact
    /// retries must not accidentally become a fresh authorization window.
    pub fn validate(&self) -> Result<(), super::ProvisionError> {
        if self.api_version != 1
            || !super::types::valid_id(&self.input.operation_id)
            || !matches!(
                &self.input.request.intent,
                crate::approval::CustodyIntentV1::Provision { .. }
            )
        {
            return Err(super::ProvisionError::Invalid);
        }
        Ok(())
    }
    pub fn into_input(
        self,
    ) -> Result<crate::approval_admission::ApprovalAdmissionInput, super::ProvisionError> {
        self.validate()?;
        Ok(self.input)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ProvisionApprovalViewV1 {
    pub api_version: u8,
    pub provision_id: String,
    pub custody_account_id: String,
    pub operation_id: String,
    pub stage: super::ProvisionStage,
}
