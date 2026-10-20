use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalInstance {
    pub api_version: String,
    pub action_id: String,
    pub generation: u64,
    pub incarnation: String,
    pub config_digest: String,
    pub supervisor_pid: u32,
    pub runtime_pid: Option<u32>,
    pub started_unix_ms: u128,
    pub state: String,
    pub exit_code: Option<i32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalActionRecord {
    pub api_version: String,
    pub action_id: String,
    pub payload_digest: String,
    pub kind: String,
    pub expected_generation: u64,
    pub target: LocalInstance,
    /// Present only when a package operation starts the exact verified release
    /// binary. Omission preserves all historical local action identities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_artifact: Option<LocalRuntimeArtifactV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalRuntimeArtifactV1 {
    pub api_version: String,
    pub profile: String,
    pub operation_id: String,
    pub operation_request_digest: String,
    pub package_version: String,
    pub commit: String,
    pub runtime_path: std::path::PathBuf,
    pub runtime_sha256: String,
    pub buildinfo_sha256: String,
}
