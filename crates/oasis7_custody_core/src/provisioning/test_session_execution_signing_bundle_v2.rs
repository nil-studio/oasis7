//! Closed public-only Executor stage data. This wire is not a signing grant.
use super::*;
use oasis7_client_api::{
    world_service::{self as ws, task_context::*, task_context_projection::*},
    world_service_wire::TaskSchedulerIntentV1,
};
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionSigningActionV2 {
    Prepare {
        original: Box<TaskContextPrepareV1>,
        observation_nonce: String,
    },
    Prefix {
        original: Box<TaskSchedulerIntentV1>,
    },
    Get {
        original: Box<TaskContextGetV1>,
        observation_nonce: String,
        prepared_projection: Box<WorldContextAttestationV2<TaskContextPreparedProjectionV2>>,
    },
    Cognition {
        original: Box<oasis7_client_api::world_service_wire::CognitionIntentV1>,
    },
}
/// Every field is rebuilt from the actual Account book and pinned public proofs
/// before the restricted Vault call. No private body/envelope is a field here.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSigningBundleV2 {
    pub api_version: u8,
    pub session_id: String,
    pub action_request_id: String,
    pub stage_operation_id: String,
    pub allocation_sequence: DecimalU64,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub approved_input_digest: String,
    pub world: crate::approval::ApprovedWorldV1,
    pub execution_deadline: crate::time_units::UnixMillis,
    pub observation: ws::ReadWorldViewRequest,
    pub fence: oasis7_agent_api::game::GameTaskFenceV1,
    pub action: Box<ExecutionSigningActionV2>,
}
impl core::fmt::Debug for ExecutionSigningBundleV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionSigningBundleV2(<unverified public stage>)")
    }
}
impl ExecutionSigningBundleV2 {
    pub fn digest(&self) -> Result<String> {
        digest(&("oasis7.custody.execution-fixed-signing-bundle.v2", self))
    }
}
