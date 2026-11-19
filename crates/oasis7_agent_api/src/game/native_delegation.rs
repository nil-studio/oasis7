//! Owner-signed native action grants, distinct from executor key delegation.
use super::{API_REVISION, DecimalU64, GameTaskFenceV1};
use serde::{Deserialize, Serialize};

pub const GAME_NATIVE_DELEGATION_DOMAIN: &str = "game_native_delegation/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameNativeDelegationOperationV1 {
    Install {
        grant_id: String,
        object_id: String,
        action_kinds: Vec<String>,
        period_id: String,
        valid_from_tick: DecimalU64,
        valid_until_tick: DecimalU64,
        limit_units: DecimalU64,
    },
    Revoke {
        grant_id: String,
        expected_revision: DecimalU64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameNativeDelegationControlV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub fence: GameTaskFenceV1,
    pub operation: GameNativeDelegationOperationV1,
}

impl GameNativeDelegationControlV1 {
    pub fn validate(&self) -> Result<(), String> {
        fn identity(value: &str) -> bool {
            !value.trim().is_empty() && value.len() <= 256
        }
        self.fence.validate()?;
        if self.api_rev != API_REVISION || !identity(&self.client_operation_id) {
            return Err("invalid native delegation identity".into());
        }
        let valid = match &self.operation {
            GameNativeDelegationOperationV1::Install {
                grant_id,
                object_id,
                action_kinds,
                period_id,
                valid_from_tick,
                valid_until_tick,
                limit_units,
            } => {
                [grant_id, object_id, period_id]
                    .into_iter()
                    .all(|v| identity(v))
                    && !action_kinds.is_empty()
                    && action_kinds.len() <= 5
                    && action_kinds.iter().all(|kind| {
                        matches!(
                            kind.as_str(),
                            "MoveAgent"
                                | "TransferMaterial"
                                | "BuildFactory"
                                | "ScheduleRecipe"
                                | "WorldRule"
                        )
                    })
                    && action_kinds
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == action_kinds.len()
                    && valid_from_tick.0 <= valid_until_tick.0
                    && limit_units.0 > 0
            }
            GameNativeDelegationOperationV1::Revoke {
                grant_id,
                expected_revision,
            } => {
                identity(grant_id)
                    && expected_revision.0 > 0
                    && expected_revision.0.checked_add(1).is_some()
            }
        };
        if !valid {
            return Err("invalid native delegation scope, budget or revision".into());
        }
        Ok(())
    }
}
