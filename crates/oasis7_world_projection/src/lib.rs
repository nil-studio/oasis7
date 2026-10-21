//! Authorized observation values and concrete codecs; no live World or read authority.
use oasis7_agent_api::RuntimeBindingV1;
pub use oasis7_client_api::world_service_feedback::{
    WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord,
};
use oasis7_world_state::{WorldEvent, WorldState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldServiceProjection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_rule_status: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formal_commit_header:
        Option<oasis7_client_api::world_service::FormalProjectionCommitHeaderV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_agent_owner: Option<oasis7_proto::viewer::CanonicalAgentOwnerFenceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_agent_chat: Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_task_control: Option<oasis7_agent_api::game::GameTaskControlV1>,
    /// Native action/object electricity grants, distinct from WASM commands.
    /// Only the authenticated Agent scope receives these read models.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_delegation_authorizations:
        Vec<oasis7_agent_api::world_quote::AgentDelegationAuthorizationV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_history: Option<WorldServiceFeedbackHistory>,
    pub state: WorldState,
    pub events: Vec<WorldEvent>,
    pub runtime_binding: Option<RuntimeBindingV1>,
    pub agent_context: Option<WorldServiceAgentContext>,
    pub scheduler_wakes: Vec<oasis7_world_state::SchedulerWakeV1>,
    pub continuations: Vec<oasis7_world_state::AgentContinuation>,
    pub cognition_leases: Vec<oasis7_world_state::CognitionLeaseV1>,
    pub continuation_contexts: std::collections::BTreeMap<String, WorldServiceContinuationContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldServiceContinuationContext {
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub policy_revision: u64,
    pub precondition_summary: String,
    pub precondition_digest: String,
}

pub use oasis7_agent_api::world_context::WorldServiceAgentContext;

pub mod codec;
pub mod formal_read;

/// Independent Game Client evidence specialized for the authoritative value schema.
pub type VerifiedWorldView =
    oasis7_game_client::verified_world_view::VerifiedWorldView<WorldServiceProjection>;

#[cfg(test)]
mod codec_tests;
