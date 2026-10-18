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

/// Errors that can occur when merging rule decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleDecisionMergeError {
    ActionIdMismatch {
        expected: ActionId,
        got: ActionId,
    },
    MissingOverride {
        action_id: ActionId,
    },
    ConflictingOverride {
        action_id: ActionId,
    },
    CostOverflow {
        action_id: ActionId,
        kind: ResourceKind,
        current: i64,
        delta: i64,
    },
}

/// Merge decisions for a single action, applying deny > modify > allow.
pub fn merge_rule_decisions<I>(
    action_id: ActionId,
    decisions: I,
) -> Result<RuleDecision, RuleDecisionMergeError>
where
    I: IntoIterator<Item = RuleDecision>,
{
    let mut merged = RuleDecision {
        action_id,
        verdict: RuleVerdict::Allow,
        override_action: None,
        cost: ResourceDelta::default(),
        notes: Vec::new(),
    };
    let mut has_deny = false;

    for decision in decisions {
        if decision.action_id != action_id {
            return Err(RuleDecisionMergeError::ActionIdMismatch {
                expected: action_id,
                got: decision.action_id,
            });
        }

        merged.cost.add_assign(&decision.cost).map_err(|overflow| {
            RuleDecisionMergeError::CostOverflow {
                action_id,
                kind: overflow.kind,
                current: overflow.current,
                delta: overflow.delta,
            }
        })?;
        merged.notes.extend(decision.notes);

        match decision.verdict {
            RuleVerdict::Deny => {
                merged.verdict = RuleVerdict::Deny;
                has_deny = true;
            }
            RuleVerdict::Modify => {
                if has_deny {
                    continue;
                }
                let Some(action) = decision.override_action else {
                    return Err(RuleDecisionMergeError::MissingOverride { action_id });
                };
                match &merged.override_action {
                    Some(existing) if existing != &action => {
                        return Err(RuleDecisionMergeError::ConflictingOverride { action_id });
                    }
                    Some(_) => {}
                    None => merged.override_action = Some(action),
                }
                merged.verdict = RuleVerdict::Modify;
            }
            RuleVerdict::Allow => {}
        }
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_agent_api::world_action::GeoPos;
    use oasis7_agent_api::world_action::ResourceKind;

    fn action() -> Action {
        Action::RegisterAgent {
            agent_id: "agent-1".to_string(),
            pos: GeoPos {
                x_cm: 1,
                y_cm: 2,
                z_cm: 0,
            },
        }
    }

    #[test]
    fn merge_allows_and_costs() {
        let mut cost = ResourceDelta::default();
        cost.entries.insert(ResourceKind::Electricity, -2);
        let decisions = vec![
            RuleDecision {
                action_id: 1,
                verdict: RuleVerdict::Allow,
                override_action: None,
                cost: cost.clone(),
                notes: vec!["ok".to_string()],
            },
            RuleDecision {
                action_id: 1,
                verdict: RuleVerdict::Allow,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: vec!["ok2".to_string()],
            },
        ];

        let merged = merge_rule_decisions(1, decisions).unwrap();
        assert_eq!(merged.verdict, RuleVerdict::Allow);
        assert_eq!(
            merged.cost.entries.get(&ResourceKind::Electricity),
            Some(&-2)
        );
        assert_eq!(merged.override_action, None);
        assert_eq!(merged.notes.len(), 2);
    }

    #[test]
    fn resource_delta_deficits_report_shortfall() {
        let mut balances = BTreeMap::new();
        balances.insert(ResourceKind::Electricity, 3);
        balances.insert(ResourceKind::Data, 5);

        let mut delta = ResourceDelta::default();
        delta.entries.insert(ResourceKind::Electricity, -5);
        delta.entries.insert(ResourceKind::Data, -7);

        let deficits = delta.deficits(&balances);
        assert_eq!(deficits.get(&ResourceKind::Electricity), Some(&2));
        assert_eq!(deficits.get(&ResourceKind::Data), Some(&2));
    }

    #[test]
    fn resource_delta_affordable_ok() {
        let mut balances = BTreeMap::new();
        balances.insert(ResourceKind::Electricity, 10);

        let mut delta = ResourceDelta::default();
        delta.entries.insert(ResourceKind::Electricity, -7);

        assert!(delta.ensure_affordable(&balances).is_ok());
    }

    #[test]
    fn resource_delta_affordable_returns_error() {
        let mut balances = BTreeMap::new();
        balances.insert(ResourceKind::Electricity, 1);

        let mut delta = ResourceDelta::default();
        delta.entries.insert(ResourceKind::Electricity, -4);

        let err = delta.ensure_affordable(&balances).unwrap_err();
        assert_eq!(err.deficits.get(&ResourceKind::Electricity), Some(&3));
    }

    #[test]
    fn resource_delta_add_assign_returns_overflow_error() {
        let mut left = ResourceDelta::default();
        left.entries.insert(ResourceKind::Electricity, i64::MAX);
        let mut right = ResourceDelta::default();
        right.entries.insert(ResourceKind::Electricity, 5);

        let err = left.add_assign(&right).expect_err("overflow");
        assert_eq!(
            err,
            ResourceDeltaOverflowError {
                kind: ResourceKind::Electricity,
                current: i64::MAX,
                delta: 5,
            }
        );
    }

    #[test]
    fn resource_delta_deficits_saturates_abs_of_i64_min() {
        let balances = BTreeMap::new();
        let mut delta = ResourceDelta::default();
        delta.entries.insert(ResourceKind::Electricity, i64::MIN);

        let deficits = delta.deficits(&balances);
        assert_eq!(deficits.get(&ResourceKind::Electricity), Some(&i64::MAX));
    }

    #[test]
    fn merge_deny_overrides_modify() {
        let decisions = vec![
            RuleDecision {
                action_id: 7,
                verdict: RuleVerdict::Modify,
                override_action: Some(action()),
                cost: ResourceDelta::default(),
                notes: Vec::new(),
            },
            RuleDecision {
                action_id: 7,
                verdict: RuleVerdict::Deny,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: Vec::new(),
            },
            RuleDecision {
                action_id: 7,
                verdict: RuleVerdict::Modify,
                override_action: Some(action()),
                cost: ResourceDelta::default(),
                notes: Vec::new(),
            },
        ];

        let merged = merge_rule_decisions(7, decisions).unwrap();
        assert_eq!(merged.verdict, RuleVerdict::Deny);
    }

    #[test]
    fn merge_conflicting_overrides() {
        let mut other = action();
        if let Action::RegisterAgent { ref mut pos, .. } = other {
            pos.x_cm = 9;
        }

        let decisions = vec![
            RuleDecision {
                action_id: 9,
                verdict: RuleVerdict::Modify,
                override_action: Some(action()),
                cost: ResourceDelta::default(),
                notes: Vec::new(),
            },
            RuleDecision {
                action_id: 9,
                verdict: RuleVerdict::Modify,
                override_action: Some(other),
                cost: ResourceDelta::default(),
                notes: Vec::new(),
            },
        ];

        let err = merge_rule_decisions(9, decisions).unwrap_err();
        assert_eq!(
            err,
            RuleDecisionMergeError::ConflictingOverride { action_id: 9 }
        );
    }

    #[test]
    fn merge_missing_override_is_error() {
        let decisions = vec![RuleDecision {
            action_id: 11,
            verdict: RuleVerdict::Modify,
            override_action: None,
            cost: ResourceDelta::default(),
            notes: Vec::new(),
        }];

        let err = merge_rule_decisions(11, decisions).unwrap_err();
        assert_eq!(
            err,
            RuleDecisionMergeError::MissingOverride { action_id: 11 }
        );
    }

    #[test]
    fn merge_rejects_action_id_mismatch() {
        let decisions = vec![RuleDecision {
            action_id: 2,
            verdict: RuleVerdict::Allow,
            override_action: None,
            cost: ResourceDelta::default(),
            notes: Vec::new(),
        }];

        let err = merge_rule_decisions(1, decisions).unwrap_err();
        assert_eq!(
            err,
            RuleDecisionMergeError::ActionIdMismatch {
                expected: 1,
                got: 2
            }
        );
    }
}
