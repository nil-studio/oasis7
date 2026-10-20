//! Rule decision structures and merge logic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use oasis7_agent_api::world_action::ResourceKind;

use super::events::Action;
use super::modules::ModuleSubscriptionStage;
use super::types::ActionId;

/// Resource changes produced by rule evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResourceDelta {
    pub entries: BTreeMap<ResourceKind, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDeltaOverflowError {
    pub kind: ResourceKind,
    pub current: i64,
    pub delta: i64,
}

impl ResourceDelta {
    pub fn add_assign(&mut self, other: &ResourceDelta) -> Result<(), ResourceDeltaOverflowError> {
        for (key, value) in &other.entries {
            let current = self.entries.get(key).copied().unwrap_or(0);
            let next = current
                .checked_add(*value)
                .ok_or(ResourceDeltaOverflowError {
                    kind: *key,
                    current,
                    delta: *value,
                })?;
            self.entries.insert(*key, next);
        }
        Ok(())
    }

    pub fn deficits(&self, balances: &BTreeMap<ResourceKind, i64>) -> BTreeMap<ResourceKind, i64> {
        let mut deficits = BTreeMap::new();
        for (kind, delta) in &self.entries {
            if *delta >= 0 {
                continue;
            }
            let available = balances.get(kind).copied().unwrap_or(0);
            let remaining = available.saturating_add(*delta);
            if remaining < 0 {
                deficits.insert(*kind, remaining.saturating_abs());
            }
        }
        deficits
    }

    pub fn ensure_affordable(
        &self,
        balances: &BTreeMap<ResourceKind, i64>,
    ) -> Result<(), ResourceBalanceError> {
        let deficits = self.deficits(balances);
        if deficits.is_empty() {
            Ok(())
        } else {
            Err(ResourceBalanceError { deficits })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBalanceError {
    pub deficits: BTreeMap<ResourceKind, i64>,
}

/// Verdicts that rule modules can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleVerdict {
    Allow,
    Deny,
    Modify,
}

/// Rule decision emitted by rule modules during action evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleDecision {
    pub action_id: ActionId,
    pub verdict: RuleVerdict,
    #[serde(default)]
    pub override_action: Option<Action>,
    #[serde(default)]
    pub cost: ResourceDelta,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Audit record of a rule decision for a specific module call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleDecisionRecord {
    pub action_id: ActionId,
    pub module_id: String,
    pub stage: ModuleSubscriptionStage,
    pub verdict: RuleVerdict,
    #[serde(default)]
    pub override_action: Option<Action>,
    #[serde(default)]
    pub cost: ResourceDelta,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Audit record emitted when a rule module overrides an action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionOverrideRecord {
    pub action_id: ActionId,
    pub original_action: Action,
    pub override_action: Action,
}
