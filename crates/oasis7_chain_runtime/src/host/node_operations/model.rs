use super::super::node_lifecycle::LocalInstance;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Kind {
    Start,
    Stop,
    Restart,
    MigrateDataDirectory,
    MaintenanceStop,
    PermanentExit,
    ProtectedSignerMigration,
    RemoveCriticalReplica,
    PackageUpgrade,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub api_version: String,
    pub operation_id: String,
    pub world_id: String,
    pub chain_id: String,
    pub kind: Kind,
    pub config: PathBuf,
    pub expected_generation: u64,
    pub expected_config_digest: String,
    pub expected_instance: Option<LocalInstance>,
    /// Absent on historical v1 requests. The omission is retained when
    /// serialized so their original request digest remains readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_target: Option<ManagedTargetV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<DataDirectoryMigrationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<OperationAuthorizationV1>,
    /// The Linux package deployment contract is a separate local-only profile.
    /// Omission preserves the serialized identity of historical v1 requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_upgrade: Option<super::super::node_lifecycle::package::PackageUpgradeV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ManagedTargetV1 {
    LinuxSshSystemd(LinuxSshSystemdTargetV1),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinuxSshSystemdTargetV1 {
    pub host: String,
    pub username: String,
    pub port: u16,
    pub identity_file: PathBuf,
    pub known_hosts_file: PathBuf,
    pub unit: String,
    pub node_id: String,
    pub remote_config: PathBuf,
    pub remote_data_dir: PathBuf,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    30_000
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DataDirectoryMigrationV1 {
    pub source: PathBuf,
    pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OperationAuthorizationV1 {
    HealthyDurableSet {
        authorization: Box<super::healthy_durable_set::HealthyDurableSetAuthorizationV1>,
    },
    FormalRecoveryBootstrap {
        formal_live_config_file: PathBuf,
    },
    ProtectedSignerTransfer {
        formal_live_config_file: PathBuf,
        target_signer_public_key: String,
        /// Exact process owner observed before this handoff is admitted.
        /// Omitted only by historical requests that were never executable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_predecessor_instance:
            Option<Box<super::super::formal_live_runtime::FormalRuntimeInstanceV1>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProtectedSignerHandoffReceiptV1 {
    pub api_version: String,
    pub operation_id: String,
    pub request_digest: String,
    pub predecessor_instance: super::super::formal_live_runtime::FormalRuntimeInstanceV1,
    pub successor_instance: super::super::formal_live_runtime::FormalRuntimeInstanceV1,
    pub predecessor_stopped: bool,
    pub active_authority_id: String,
    pub active_authority_epoch: u64,
    pub profile_version: u32,
    pub qualified_position: u64,
    pub qualified_record_id: String,
    pub after_state_root: String,
    pub primary_checkpoint_digest: String,
    pub replica_checkpoint_digest: String,
    pub qualified_evidence_digest: String,
    pub recovery_permission_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub request: Request,
    pub request_digest: String,
    pub revision: u64,
    pub state: String,
    pub stage: String,
    pub dispatched: bool,
    pub cancel_requested: bool,
    #[serde(default)]
    pub recovered: bool,
    pub blocker: Option<String>,
    pub results: Vec<super::super::node_lifecycle::LocalActionRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remote_results: Vec<super::provider::RemoteActionReceiptV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_attempts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_receipt:
        Option<super::super::node_lifecycle::migration::DataDirectoryMigrationReceiptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_capacity_plan:
        Option<super::super::node_lifecycle::migration::DataDirectoryMigrationCapacityPlanV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_handoff_plan: Option<super::protected_handoff::ProtectedSignerHandoffPlanV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_handoff: Option<ProtectedSignerHandoffReceiptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_durable_set_plan: Option<super::healthy_durable_set::HealthyDurableSetPlanV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_durable_set_progress:
        Option<super::healthy_durable_set::HealthyDurableSetProgressV1>,
    // Preserve existing v1 operation-store records written before the shared
    // result envelope was added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_result: Option<super::result::OperationResultV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_upgrade_progress:
        Option<super::super::node_lifecycle::package::PackageUpgradeProgressV1>,
}
impl Record {
    pub fn terminal(&self) -> bool {
        matches!(self.state.as_str(), "succeeded" | "cancelled" | "failed")
    }
    pub fn action_id(&self, step: &str) -> String {
        format!("op:{}:{}", self.request.operation_id, step)
    }
}
