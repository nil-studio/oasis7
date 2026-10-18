//! Narrow read metadata extracted after authentication of the original projection.
//! This is neither an execution snapshot nor a replacement serialization for a
//! signed projection: proof checks retain the exact original projection type.
use super::world_service::FormalProjectionCommitHeaderV1;
use oasis7_agent_api::RuntimeBindingV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldServiceProjectionMetadataV1 {
    #[serde(default)]
    pub runtime_binding: Option<RuntimeBindingV1>,
    #[serde(default)]
    pub formal_commit_header: Option<FormalProjectionCommitHeaderV1>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
}
