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
