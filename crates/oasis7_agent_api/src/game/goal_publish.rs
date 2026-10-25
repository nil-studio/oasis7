//! Public owner goal activation metadata. No private body or fingerprint.
use super::{API_REVISION, DecimalU64};
use serde::{Deserialize, Serialize};
pub const GAME_GOAL_PUBLISH_DOMAIN: &str = "game_goal_publish/v1";
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn reference(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameGoalPublishV1 {
    pub api_rev: u32,
    pub world_id: String,
    pub genesis_digest: String,
    pub task_id: String,
    pub agent_id: String,
    pub owner_public_key: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub expected_task_revision: DecimalU64,
    pub expected_latest_goal_revision: DecimalU64,
    pub goal_revision: DecimalU64,
    pub client_operation_id: String,
    pub prepare_operation_id: String,
    /// Unpredictable salted commitment. Salt, body and body signature are private.
    pub goal_ref: String,
}
impl GameGoalPublishV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_rev != API_REVISION
            || self.owner_generation.0 == 0
            || self.expected_task_revision.0 == 0
            || self.expected_task_revision.0.checked_add(1).is_none()
            || self.expected_latest_goal_revision.0.checked_add(1) != Some(self.goal_revision.0)
            || [
                &self.world_id,
                &self.genesis_digest,
                &self.task_id,
                &self.agent_id,
                &self.owner_binding,
                &self.branch_id,
                &self.client_operation_id,
                &self.prepare_operation_id,
            ]
            .iter()
            .any(|value| !identity(value))
            || !reference(&self.owner_public_key)
            || !reference(&self.goal_ref)
        {
            return Err("invalid public goal publication metadata".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GameGoalReferenceV1 {
    pub goal_revision: DecimalU64,
    pub goal_ref: String,
    pub prepare_operation_id: String,
    pub publish_operation_id: String,
    /// A full commit root cannot be embedded in the state that it hashes.
    pub activation_committed_height: DecimalU64,
}
impl GameGoalReferenceV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.goal_revision.0 == 0
            || self.activation_committed_height.0 == 0
            || !reference(&self.goal_ref)
            || !identity(&self.prepare_operation_id)
            || !identity(&self.publish_operation_id)
        {
            return Err("invalid canonical goal reference".into());
        }
        Ok(())
    }
}
