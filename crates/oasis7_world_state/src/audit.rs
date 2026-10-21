//! Audit types for event filtering and tracking.

use serde::{Deserialize, Serialize};

use super::events::CausedBy;
use super::types::{WorldEventId, WorldTime};
use super::world_event::WorldEvent;

/// Kinds of events for audit purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventKind {
    Domain,
    EffectQueued,
    ReceiptAppended,
    CapabilityAuthorization,
    PolicyDecision,
    RuleDecision,
    ActionOverridden,
    Governance,
    ModuleEvent,
    ModuleCallFailed,
    ModuleEmitted,
    ModuleStateUpdated,
    ModuleRuntimeCharged,
    ProductValidationDeliveryCursorUpdated,
    SnapshotCreated,
    ManifestUpdated,
    RollbackApplied,
}

/// The cause type for audit filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCausedBy {
    Action,
    Effect,
}

/// Filter criteria for audit events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AuditFilter {
    pub kinds: Option<Vec<AuditEventKind>>,
    pub from_time: Option<WorldTime>,
    pub to_time: Option<WorldTime>,
    pub from_event_id: Option<WorldEventId>,
    pub to_event_id: Option<WorldEventId>,
    pub caused_by: Option<AuditCausedBy>,
}

impl AuditFilter {
    pub fn matches(&self, event: &WorldEvent) -> bool {
        if let Some(kinds) = &self.kinds
            && !kinds.contains(&event.audit_kind())
        {
            return false;
        }
        if let Some(from_time) = self.from_time
            && event.time < from_time
        {
            return false;
        }
        if let Some(to_time) = self.to_time
            && event.time > to_time
        {
            return false;
        }
        if let Some(from_event_id) = self.from_event_id
            && event.id < from_event_id
        {
            return false;
        }
        if let Some(to_event_id) = self.to_event_id
            && event.id > to_event_id
        {
            return false;
        }
        if let Some(cause) = &self.caused_by {
            match cause {
                AuditCausedBy::Action => {
                    if !matches!(event.caused_by, Some(CausedBy::Action(_))) {
                        return false;
                    }
                }
                AuditCausedBy::Effect => {
                    if !matches!(event.caused_by, Some(CausedBy::Effect { .. })) {
                        return false;
                    }
                }
            }
        }
        true
    }
}
