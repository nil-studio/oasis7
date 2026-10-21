//! Agent cell representation - wraps agent state with mailbox and activity tracking.

use crate::agent_state::AgentState;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use super::events::DomainEvent;
use super::types::{ActionId, WorldTime};

/// Schema version for the runtime-owned Agent Intent representation.
pub const AGENT_INTENT_V2_SCHEMA_VERSION: u32 = 2;

/// Authority identity supplied by the authenticated transport when a player
/// intent enters the runtime. These fields are part of the durable request
/// digest, but remain optional for legacy callers and snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentIntentAuthorityContext {
    pub intent_tick: Option<u64>,
    pub world_id: Option<String>,
    pub reorg_epoch: Option<u64>,
    pub authority_scope: Option<String>,
    /// Explicit active intent that this request is authorized to replace.
    pub replaces_intent_id: Option<String>,
}

/// The canonical runtime intent currently associated with an agent.
///
/// This deliberately contains only runtime authority fields.  Viewer
/// projection concerns such as world identity, reorg epoch, source class,
/// freshness, and control state are added by the projection layer and must
/// not be persisted in the agent cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIntentV2 {
    pub schema_version: u32,
    pub agent_id: String,
    pub intent_id: String,
    pub kind: String,
    pub summary: String,
    pub target_id: Option<String>,
    /// Effect intent whose committed receipt may complete this intent.
    /// Formal births use `EffectIntent::identity_key()` (including birth era).
    /// A legacy raw reference matches only a legacy birth-None receipt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_intent_id: Option<String>,
    /// Authenticated client authority position, when supplied by the transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_tick: Option<u64>,
    /// Canonical world identity for the authenticated request, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    /// Reorg epoch bound to the authenticated request, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorg_epoch: Option<u64>,
    /// Player-facing authority scope, never raw auth material.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_scope: Option<String>,
    pub status: String,
    pub source: String,
    pub logical_time: WorldTime,
    pub event_seq: u64,
    pub updated_at: WorldTime,
    pub receipt_ref: Option<String>,
    pub reason_code: Option<String>,
    pub reason_summary: Option<String>,
    pub replaced_by: Option<String>,
    /// Stable identity of the authenticated principal that submitted the intent.
    /// Legacy snapshots omit this field and remain readable.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub actor_id: String,
    /// Digest of the authenticated request envelope for durable retry detection.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub request_digest: String,
}

/// The safe, durable disposition returned when an authenticated request is
/// retried against the runtime Intent ledger.
///
/// This intentionally excludes the request summary and other mutable payload
/// fields. Callers use it to decide whether a retry is still accepted, blocked,
/// or terminal without reconstructing an acceptance from a sidecar ACK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIntentReplayDisposition {
    pub intent_id: String,
    pub status: String,
    pub event_seq: u64,
    /// Original runtime position of the intent, retained across terminal
    /// transitions so retries never need to synthesize a wall-clock value.
    #[serde(default)]
    pub logical_time: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

impl From<&AgentIntentV2> for AgentIntentReplayDisposition {
    fn from(intent: &AgentIntentV2) -> Self {
        Self {
            intent_id: intent.intent_id.clone(),
            status: intent.status.clone(),
            event_seq: intent.event_seq,
            logical_time: intent.logical_time,
            receipt_ref: intent.receipt_ref.clone(),
            reason_code: intent.reason_code.clone(),
            reason_summary: intent.reason_summary.clone(),
            replaced_by: intent.replaced_by.clone(),
        }
    }
}

/// The authoritative coarse-grained activity state exposed for an agent.
///
/// A missing activity (`AgentCell::activity == None`) is distinct from an
/// explicitly idle agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityStatus {
    Idle,
    Executing,
    Blocked,
    Waiting,
    Unavailable,
}

/// Versioned activity metadata persisted alongside an agent cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivityV1 {
    pub status: AgentActivityStatus,
    pub updated_at: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<ActionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_summary: Option<String>,
}

impl AgentActivityV1 {
    pub fn idle(updated_at: WorldTime) -> Self {
        Self {
            status: AgentActivityStatus::Idle,
            updated_at,
            operation_kind: None,
            operation_id: None,
            target_id: None,
            reason_code: None,
            reason_summary: None,
        }
    }

    pub fn executing(
        operation_kind: impl Into<String>,
        operation_id: ActionId,
        target_id: impl Into<String>,
        updated_at: WorldTime,
    ) -> Self {
        Self {
            status: AgentActivityStatus::Executing,
            updated_at,
            operation_kind: Some(operation_kind.into()),
            operation_id: Some(operation_id),
            target_id: Some(target_id.into()),
            reason_code: None,
            reason_summary: None,
        }
    }

    pub fn blocked(
        operation_kind: impl Into<String>,
        operation_id: ActionId,
        target_id: impl Into<String>,
        reason_code: impl Into<String>,
        reason_summary: impl Into<String>,
        updated_at: WorldTime,
    ) -> Self {
        Self {
            status: AgentActivityStatus::Blocked,
            updated_at,
            operation_kind: Some(operation_kind.into()),
            operation_id: Some(operation_id),
            target_id: Some(target_id.into()),
            reason_code: Some(reason_code.into()),
            reason_summary: Some(reason_summary.into()),
        }
    }
}

/// A cell that holds an agent's state along with its mailbox and activity tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCell {
    pub state: AgentState,
    pub mailbox: VecDeque<DomainEvent>,
    pub last_active: WorldTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<AgentActivityV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<AgentIntentV2>,
}

impl AgentCell {
    pub fn new(state: AgentState, now: WorldTime) -> Self {
        Self {
            state,
            mailbox: VecDeque::new(),
            last_active: now,
            activity: None,
            intent: None,
        }
    }
}
