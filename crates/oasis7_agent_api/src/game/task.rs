//! Owner control bootstrap. These records grant no action or executor authority.
use super::{API_REVISION, DecimalU64};
use serde::{Deserialize, Serialize};

pub const GAME_TASK_CREATE_DOMAIN: &str = "game_task_create";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskCreateV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
}
impl GameTaskCreateV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_rev != API_REVISION
            || self.owner_generation.0 == 0
            || [
                &self.client_operation_id,
                &self.world_id,
                &self.genesis_digest,
                &self.branch_id,
                &self.agent_id,
                &self.owner_binding,
            ]
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 256)
        {
            return Err("invalid Game task create identity".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum GameTaskStatusV1 {
    Suspended,
    Active,
}

/// Initial durable control metadata; this is not a complete TaskSnapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameTaskControlV1 {
    pub schema_revision: u32,
    pub task_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
    pub owner_public_key: String,
    pub task_revision: DecimalU64,
    pub task_status: GameTaskStatusV1,
    pub latest_goal_revision: DecimalU64,
    pub active_goal_revision: DecimalU64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_goal_reference: Option<super::GameGoalReferenceV1>,
    pub executor_epoch: DecimalU64,
    pub executor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executor_binding: Option<super::GameExecutorBindingV1>,
    pub created_operation_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_create_strict_decimal_codec_and_fields() {
        let valid = serde_json::json!({"api_rev":1,"client_operation_id":"create","world_id":"w","genesis_digest":"g","branch_id":"main","reorg_epoch":"0","agent_id":"a","owner_binding":"o","owner_generation":"1"});
        let request: GameTaskCreateV1 = serde_json::from_value(valid.clone()).unwrap();
        request.validate().unwrap();
        for invalid in [
            serde_json::json!(1),
            serde_json::json!("01"),
            serde_json::json!("-1"),
        ] {
            let mut value = valid.clone();
            value["owner_generation"] = invalid;
            assert!(serde_json::from_value::<GameTaskCreateV1>(value).is_err());
        }
        let mut value = valid;
        value["extra"] = true.into();
        assert!(serde_json::from_value::<GameTaskCreateV1>(value).is_err());
    }
}
