//! Public executor control and opaque private-context identities.
use super::{API_REVISION, DecimalU64, GameGoalReferenceV1, GameTaskControlV1, GameTaskStatusV1};
use serde::{Deserialize, Serialize};

pub const GAME_EXECUTOR_CONTROL_DOMAIN: &str = "game_executor_control/v1";
fn identity(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn commitment(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(key)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameExecutorBindingV1 {
    pub executor_id: String,
    pub executor_public_key: String,
    pub delegation_generation: DecimalU64,
    pub executor_epoch: DecimalU64,
}
impl GameExecutorBindingV1 {
    pub fn validate(&self) -> Result<(), String> {
        if !identity(&self.executor_id)
            || !key(&self.executor_public_key)
            || self.delegation_generation.0 == 0
            || self.executor_epoch.0 == 0
        {
            return Err("invalid executor binding".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskFenceV1 {
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
    pub owner_public_key: String,
    pub task_id: String,
    pub task_revision: DecimalU64,
    pub task_status: GameTaskStatusV1,
    pub latest_goal_revision: DecimalU64,
    pub active_goal_revision: DecimalU64,
    pub active_goal_reference: Option<GameGoalReferenceV1>,
    pub executor_epoch: DecimalU64,
    pub executor_binding: Option<GameExecutorBindingV1>,
}
impl GameTaskFenceV1 {
    pub fn from_control(c: &GameTaskControlV1) -> Self {
        Self {
            world_id: c.world_id.clone(),
            genesis_digest: c.genesis_digest.clone(),
            branch_id: c.branch_id.clone(),
            reorg_epoch: c.reorg_epoch,
            agent_id: c.agent_id.clone(),
            owner_binding: c.owner_binding.clone(),
            owner_generation: c.owner_generation,
            owner_public_key: c.owner_public_key.clone(),
            task_id: c.task_id.clone(),
            task_revision: c.task_revision,
            task_status: c.task_status.clone(),
            latest_goal_revision: c.latest_goal_revision,
            active_goal_revision: c.active_goal_revision,
            active_goal_reference: c.active_goal_reference.clone(),
            executor_epoch: c.executor_epoch,
            executor_binding: c.executor_binding.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.owner_generation.0 == 0
            || self.task_revision.0 == 0
            || !key(&self.owner_public_key)
            || [
                &self.world_id,
                &self.genesis_digest,
                &self.branch_id,
                &self.agent_id,
                &self.owner_binding,
                &self.task_id,
            ]
            .iter()
            .any(|v| !identity(v))
        {
            return Err("invalid task fence identity".into());
        }
        match &self.active_goal_reference {
            Some(r) => {
                r.validate()?;
                if r.goal_revision != self.active_goal_revision
                    || self.latest_goal_revision.0 < self.active_goal_revision.0
                {
                    return Err("invalid task goal fence".into());
                }
            }
            None if self.active_goal_revision.0 == 0 && self.latest_goal_revision.0 == 0 => (),
            None => return Err("missing task goal fence".into()),
        }
        match &self.executor_binding {
            Some(b) => {
                b.validate()?;
                if b.executor_epoch != self.executor_epoch
                    || self.task_status != GameTaskStatusV1::Active
                {
                    return Err("invalid task executor fence".into());
                }
            }
            None if self.task_status == GameTaskStatusV1::Suspended => (),
            None => return Err("missing task executor fence".into()),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameExecutorOperationV1 {
    Bind {
        executor_id: String,
        executor_public_key: String,
        delegation_generation: DecimalU64,
    },
    Stop,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameExecutorControlV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub fence: GameTaskFenceV1,
    pub operation: GameExecutorOperationV1,
}
impl GameExecutorControlV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.fence.validate()?;
        if self.api_rev != API_REVISION
            || !identity(&self.client_operation_id)
            || self.fence.task_revision.0.checked_add(1).is_none()
            || self.fence.executor_epoch.0.checked_add(1).is_none()
        {
            return Err("invalid executor control identity or overflow".into());
        }
        if let GameExecutorOperationV1::Bind {
            executor_id,
            executor_public_key,
            delegation_generation,
        } = &self.operation
            && (!identity(executor_id)
                || !key(executor_public_key)
                || delegation_generation.0 == 0
                || self.fence.active_goal_reference.is_none())
        {
            return Err("invalid executor bind".into());
        }
        Ok(())
    }
}

/// Explicit native selection; absence preserves the original provider wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskContextAuthorityModeV1 {
    NativeExecutorV1,
}
impl TaskContextAuthorityModeV1 {
    pub fn schema_revision(mode: Option<Self>) -> u32 {
        if mode.is_some() { 2 } else { 1 }
    }
}

/// Only salted commitments and public metadata; never raw private input digests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextIdentityV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_mode: Option<TaskContextAuthorityModeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_authorization_root: Option<String>,
    pub schema_revision: u32,
    pub fence: GameTaskFenceV1,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_commitment: String,
    pub context_commitment: String,
    pub provider_invocation_commitment: String,
    pub valid_until_tick: DecimalU64,
}
/// The public canonical prefix and private preparation share one lease bound.
pub const TASK_CONTEXT_MAX_LEASE_TICKS: u64 = 16;
impl TaskContextIdentityV1 {
    pub fn validate_capture_deadline(&self, captured_tick: u64) -> Result<(), String> {
        self.validate()?;
        let limit = captured_tick
            .checked_add(TASK_CONTEXT_MAX_LEASE_TICKS)
            .ok_or("task context lease overflow")?;
        if self.valid_until_tick.0 < captured_tick || self.valid_until_tick.0 > limit {
            return Err("task context exceeds captured lease".into());
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        self.fence.validate()?;
        if self.schema_revision != TaskContextAuthorityModeV1::schema_revision(self.authority_mode)
            || match (&self.authority_mode, &self.native_authorization_root) {
                (None, None) => false,
                (Some(_), Some(root)) => !commitment(root),
                _ => true,
            }
            || self.retry_seq == 0
            || self.transport_attempt == 0
            || self.transport_attempt > u64::from(u16::MAX)
            || self.fence.task_status != GameTaskStatusV1::Active
            || self.fence.active_goal_reference.is_none()
            || [
                &self.agent_session_id,
                &self.agent_turn_id,
                &self.decision_request_id,
            ]
            .iter()
            .any(|v| !identity(v))
            || [
                &self.request_commitment,
                &self.context_commitment,
                &self.provider_invocation_commitment,
            ]
            .iter()
            .any(|v| !commitment(v))
        {
            return Err("invalid public task context identity".into());
        }
        Ok(())
    }
}
