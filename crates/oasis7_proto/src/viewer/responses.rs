use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlCompletionStatus {
    Advanced,
    TimeoutNoProgress,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCompletionAck<Time> {
    pub request_id: u64,
    pub status: ControlCompletionStatus,
    pub delta_logical_time: Time,
    pub delta_event_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptControlOperation {
    Apply,
    Rollback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptControlResultStatus {
    Accepted,
    Applied,
    Blocked,
    Rejected,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptControlValueVisibility {
    Hidden,
    MetadataOnly,
    LatestAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptControlApplicationScope {
    None,
    RuntimeInstance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptControlAck<Time> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_epoch: Option<String>,
    pub agent_id: String,
    pub operation: PromptControlOperation,
    pub preview: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PromptControlResultStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    pub version: u64,
    pub updated_at_tick: Time,
    pub applied_fields: Vec<String>,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_visibility: Option<PromptControlValueVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_digest: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent_replay: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled_back_to_version: Option<u64>,
}

impl<Time: Default> PromptControlAck<Time> {
    pub fn default_legacy() -> Self {
        Self {
            request_id: None,
            authority_epoch: None,
            agent_id: String::new(),
            operation: PromptControlOperation::Apply,
            preview: false,
            status: None,
            player_id: None,
            session_epoch: None,
            binding_epoch: None,
            expected_version: None,
            version: 0,
            updated_at_tick: Time::default(),
            applied_fields: Vec::new(),
            digest: String::new(),
            value_visibility: None,
            applied_scope: None,
            persistence_scope: None,
            sync_scope: None,
            reason_code: None,
            next_step: None,
            operation_digest: None,
            idempotent_replay: false,
            mutation_count: None,
            rolled_back_to_version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptControlError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<PromptControlOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PromptControlResultStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_visibility: Option<PromptControlValueVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_scope: Option<PromptControlApplicationScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_digest: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent_replay: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolled_back_to_version: Option<u64>,
}

impl PromptControlError {
    pub fn default_legacy() -> Self {
        Self {
            code: String::new(),
            message: String::new(),
            request_id: None,
            authority_epoch: None,
            operation: None,
            preview: None,
            status: None,
            agent_id: None,
            player_id: None,
            session_epoch: None,
            binding_epoch: None,
            expected_version: None,
            current_version: None,
            version: None,
            digest: None,
            applied_fields: None,
            value_visibility: None,
            applied_scope: None,
            persistence_scope: None,
            sync_scope: None,
            reason_code: None,
            next_step: None,
            operation_digest: None,
            idempotent_replay: false,
            mutation_count: None,
            rolled_back_to_version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentChatAck<Time> {
    pub agent_id: String,
    pub accepted_at_tick: Time,
    pub message_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_tick: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub idempotent_replay: bool,
    /// Durable runtime identity. Older clients may ignore these additive
    /// fields; retries use them instead of reconstructing acceptance from
    /// response arrival time. `accepted_event_seq` is the canonical position.
    /// The fields remain optional so older clients can continue decoding the
    /// acknowledgement while newer clients preserve durable retry identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
    /// Original signed canonical owner nonce, never a Runtime acceptance claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_nonce: Option<u64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentChatError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayActionAck<Time> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consensus_action_payload_hash: Option<String>,
    pub action_id: String,
    pub target_agent_id: String,
    pub player_id: String,
    pub runtime_action_id: u64,
    pub accepted_at_tick: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayActionError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,
}
