//! Policy system for effect authorization.

use serde::{Deserialize, Serialize};

use super::effect::{EffectIntent, OriginKind};

/// A set of policy rules for effect authorization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PolicySet {
    pub rules: Vec<PolicyRule>,
}

impl PolicySet {
    pub fn decide(&self, intent: &EffectIntent) -> PolicyDecision {
        for rule in &self.rules {
            if rule.when.matches(intent) {
                return rule.decision.clone();
            }
        }
        PolicyDecision::Deny {
            reason: "default_deny".to_string(),
        }
    }

    pub fn allow_all() -> Self {
        Self {
            rules: vec![PolicyRule {
                when: PolicyWhen {
                    effect_kind: None,
                    origin_kind: None,
                    cap_name: None,
                },
                decision: PolicyDecision::Allow,
            }],
        }
    }
}

/// A single policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub when: PolicyWhen,
    pub decision: PolicyDecision,
}

/// Conditions for when a policy rule applies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyWhen {
    pub effect_kind: Option<String>,
    pub origin_kind: Option<OriginKind>,
    pub cap_name: Option<String>,
}

impl PolicyWhen {
    pub fn matches(&self, intent: &EffectIntent) -> bool {
        if let Some(effect_kind) = &self.effect_kind
            && effect_kind != &intent.kind
        {
            return false;
        }
        if let Some(origin_kind) = &self.origin_kind
            && origin_kind != &OriginKind::from_origin(&intent.origin)
        {
            return false;
        }
        if let Some(cap_name) = &self.cap_name
            && cap_name != &intent.cap_ref
        {
            return false;
        }
        true
    }
}

/// The decision made by a policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", content = "data")]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }

    pub fn reason(&self) -> Option<String> {
        match self {
            PolicyDecision::Allow => None,
            PolicyDecision::Deny { reason } => Some(reason.clone()),
        }
    }
}

/// Record of a policy decision for audit purposes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyDecisionRecord {
    pub intent_id: String,
    pub decision: PolicyDecision,
    pub effect_kind: String,
    pub cap_ref: String,
    pub origin_kind: OriginKind,
}

impl PolicyDecisionRecord {
    pub fn from_intent(intent: &EffectIntent, decision: PolicyDecision) -> Self {
        Self {
            intent_id: intent.intent_id.clone(),
            decision,
            effect_kind: intent.kind.clone(),
            cap_ref: intent.cap_ref.clone(),
            origin_kind: OriginKind::from_origin(&intent.origin),
        }
    }
}
