use serde::{Deserialize, Serialize};

pub const AGENCY_CONTROL_REQUEST_TYPE: &str = "agency_control_request";
pub const AGENCY_CONTROL_RESPONSE_TYPE: &str = "agency_control_response";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgencyControlOperation {
    Inspect,
    CorrectMemory {
        correction_id: String,
        target_memory_id: String,
        expected_revision: u64,
        replacement_summary: String,
    },
    InstallDelegation {
        grant_id: String,
        object_id: String,
        action_kinds: Vec<String>,
        period_id: String,
        valid_from_tick: u64,
        valid_until_tick: u64,
        limit_units: u64,
    },
    RevokeDelegation {
        grant_id: String,
        expected_revision: u64,
    },
    OverridePendingIntent {
        control_id: String,
        intent_id: String,
        request_digest: String,
        grant_id: String,
        expected_grant_revision: u64,
    },
    InterruptPendingIntent {
        control_id: String,
        intent_id: String,
        request_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grant_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_revision: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgencyControlRequest {
    #[serde(rename = "type")]
    pub frame_type: String,
    pub request_id: String,
    pub player_id: String,
    pub public_key: String,
    pub agent_id: String,
    pub world_id: String,
    pub reorg_epoch: u64,
    pub command: AgencyControlOperation,
    pub auth: Option<crate::viewer::PlayerAuthProof>,
}
