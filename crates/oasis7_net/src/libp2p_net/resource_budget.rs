use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use libp2p::{
    Multiaddr, PeerId,
    core::transport::TransportError,
    swarm::{DialError, ListenError},
};
use oasis7_proto::p2p_resource_budget::{P2pResourceBudget, P2pResourceBudgetLimits};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::{
    connection_budget::{ConnectionBudget, ConnectionLimits},
    transport_admission::{OutboundAttemptBudgetDenied, TransportHandshakeDeadlineElapsed},
    wire_byte_window::{WireByteLimits, WireByteWindow},
    wire_bytes::{
        Libp2pWireByteCounters, SharedLibp2pWireByteCounters, init_shared_wire_byte_counters,
    },
};
use crate::error::WorldError;

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

fn is_handshake_deadline_error(error: &TransportError<io::Error>) -> bool {
    matches!(
        error,
        TransportError::Other(error)
            if super::transport_admission::has_transport_error_cause::<
                TransportHandshakeDeadlineElapsed,
            >(error)
    )
}

pub(super) fn has_handshake_deadline_error(
    errors: &[(Multiaddr, TransportError<io::Error>)],
) -> bool {
    errors
        .iter()
        .any(|(_, error)| is_handshake_deadline_error(error))
}

pub(super) fn dial_hit_handshake_deadline(error: &DialError) -> bool {
    match error {
        DialError::Transport(errors) => has_handshake_deadline_error(errors),
        DialError::Denied { cause } => cause
            .downcast_ref::<TransportHandshakeDeadlineElapsed>()
            .is_some(),
        _ => false,
    }
}

fn is_outbound_budget_denial_error(error: &TransportError<io::Error>) -> bool {
    matches!(
        error,
        TransportError::Other(error)
            if super::transport_admission::has_transport_error_cause::<
                OutboundAttemptBudgetDenied,
            >(error)
    )
}

pub(super) fn dial_path_hit_handshake_deadline(error: &DialError, address: &Multiaddr) -> bool {
    match error {
        DialError::Transport(errors) => errors
            .iter()
            .any(|(failed, error)| failed == address && is_handshake_deadline_error(error)),
        _ => dial_hit_handshake_deadline(error),
    }
}

pub(super) fn dial_path_hit_outbound_budget_denial(error: &DialError, address: &Multiaddr) -> bool {
    match error {
        DialError::Transport(errors) => errors
            .iter()
            .any(|(failed, error)| failed == address && is_outbound_budget_denial_error(error)),
        DialError::Denied { .. } => true,
        _ => false,
    }
}

pub(super) fn listener_hit_handshake_deadline(error: &ListenError) -> bool {
    match error {
        ListenError::Transport(error) => is_handshake_deadline_error(error),
        ListenError::Denied { cause } => cause
            .downcast_ref::<TransportHandshakeDeadlineElapsed>()
            .is_some(),
        _ => false,
    }
}

pub(super) fn setup_resource_budgets(
    profile: P2pResourceBudget,
) -> Result<(Option<ActiveResourceBudget>, SharedLibp2pWireByteCounters), WorldError> {
    let P2pResourceBudget::SubstreamV1(limits) = profile else {
        return Ok((None, init_shared_wire_byte_counters()));
    };
    let error = |reason: &str| WorldError::NetworkProtocolUnavailable {
        protocol: format!("p2p_resource_budget: {reason}"),
    };
    limits.validate_connection_limits().map_err(error)?;
    let diagnostics = Arc::new(BudgetDiagnostics::default());
    let connections = ConnectionBudget::with_diagnostics(
        ConnectionLimits {
            total: limits.connections_total.get(),
            pending: limits.pending_connections_total.get(),
            inbound_unknown: limits.pending_inbound_unknown.get(),
            outbound: limits.pending_outbound.get(),
            per_peer: limits.connections_per_peer.get(),
            handshake_timeout: limits.handshake_timeout(),
        },
        Arc::clone(&diagnostics),
    )
    .map_err(|err| error(&err.to_string()))?;
    let bytes = WireByteWindow::with_diagnostics(
        WireByteLimits {
            inbound: limits.bytes_in_per_window.get(),
            outbound: limits.bytes_out_per_window.get(),
            total: limits.bytes_total_per_window.get(),
        },
        Arc::clone(&diagnostics),
    )
    .map_err(|err| error(&err.to_string()))?;
    Ok((
        Some(ActiveResourceBudget {
            limits,
            instance_id: PeerId::random(),
            connections,
            diagnostics,
        }),
        Libp2pWireByteCounters::with_window(Arc::new(bytes)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixed_local_cause_is_a_budget_handshake_deadline() {
        let address = "/ip4/127.0.0.1/tcp/1".parse::<Multiaddr>().unwrap();
        let error =
            |cause| DialError::Transport(vec![(address.clone(), TransportError::Other(cause))]);
        assert!(dial_hit_handshake_deadline(&error(io::Error::new(
            io::ErrorKind::TimedOut,
            TransportHandshakeDeadlineElapsed,
        ))));
        assert!(!dial_hit_handshake_deadline(&error(io::Error::new(
            io::ErrorKind::TimedOut,
            "transport handshake deadline expired",
        ))));
    }
}
