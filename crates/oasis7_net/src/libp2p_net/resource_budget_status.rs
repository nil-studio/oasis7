use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use libp2p::PeerId;
use oasis7_proto::p2p_resource_budget::P2pResourceBudgetLimits;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{connection_budget::ConnectionBudget, wire_bytes::SharedLibp2pWireByteCounters};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceBudgetRejection {
    ConnectionTotal,
    PendingTotal,
    PendingInbound,
    PendingOutbound,
    PerPeer,
    HandshakeDeadline,
    BytesIn,
    BytesOut,
    BytesTotal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionBudgetSnapshot {
    pub total: u32,
    pub established: u32,
    pub pending: u32,
    pub pending_inbound_unknown: u32,
    pub pending_outbound: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceByteCounts {
    pub inbound: u64,
    pub outbound: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceByteBudgetSnapshot {
    pub actual_bytes: ResourceByteCounts,
    pub uncertain_charge_bytes: ResourceByteCounts,
    pub charged_bytes: ResourceByteCounts,
    pub reserved_bytes: ResourceByteCounts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBudgetSnapshot {
    pub profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<P2pResourceBudgetLimits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<ConnectionBudgetSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<ResourceByteBudgetSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rejection: Option<ResourceBudgetRejection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_attempt_delay_ms: Option<u64>,
}

impl Default for ResourceBudgetSnapshot {
    fn default() -> Self {
        Self {
            profile: "legacy".into(),
            limits: None,
            instance_id: None,
            connections: None,
            bytes: None,
            last_rejection: None,
            next_attempt_delay_ms: None,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct BudgetDiagnostics(Mutex<Option<(ResourceBudgetRejection, Option<Instant>)>>);

impl BudgetDiagnostics {
    pub(super) fn rejected(&self, reason: ResourceBudgetRejection, retry: Option<Instant>) {
        if let Ok(mut latest) = self.0.lock() {
            *latest = Some((reason, retry));
        }
    }

    pub(super) fn latest(&self) -> (Option<ResourceBudgetRejection>, Option<u64>) {
        match self.0.lock().ok().and_then(|latest| *latest) {
            Some((reason, retry)) => {
                let delay = retry.map(|at| {
                    let remaining: Duration = at.saturating_duration_since(Instant::now());
                    u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX)
                });
                (Some(reason), delay)
            }
            None => (None, None),
        }
    }
}

#[derive(Clone)]
pub(super) struct ActiveResourceBudget {
    pub(super) limits: P2pResourceBudgetLimits,
    pub(super) instance_id: PeerId,
    pub(super) connections: ConnectionBudget,
    pub(super) diagnostics: Arc<BudgetDiagnostics>,
}

impl ActiveResourceBudget {
    pub(super) fn snapshot(&self, wire: &SharedLibp2pWireByteCounters) -> ResourceBudgetSnapshot {
        let (last_rejection, next_attempt_delay_ms) = self.diagnostics.latest();
        ResourceBudgetSnapshot {
            profile: oasis7_proto::p2p_resource_budget::P2P_RESOURCE_BUDGET_V1.into(),
            limits: Some(self.limits),
            instance_id: Some(self.instance_id.to_string()),
            connections: self.connections.snapshot(),
            bytes: wire.window().and_then(|window| window.snapshot()),
            last_rejection,
            next_attempt_delay_ms,
        }
    }
}
