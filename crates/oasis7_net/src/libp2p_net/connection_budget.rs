//! Shared active-attempt accounting at the Swarm transport and authentication boundaries.
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use libp2p::{
    Multiaddr, PeerId,
    swarm::{
        Config, ConnectionDenied, ConnectionId, TransportAdmission,
        TransportHandshakeDeadlineElapsed,
    },
};

use super::resource_budget::{
    BudgetDiagnostics, ConnectionBudgetSnapshot, ResourceBudgetRejection,
};

#[cfg(test)]
#[path = "connection_budget_tests.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(super) struct ConnectionLimits {
    pub total: u32,
    pub pending: u32,
    pub inbound_unknown: u32,
    pub outbound: u32,
    pub per_peer: u32,
    pub handshake_timeout: Duration,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum AttemptKey {
    Inbound(ConnectionId),
    Outbound(ConnectionId, Multiaddr),
}

impl AttemptKey {
    fn pending_lane(&self) -> usize {
        match self {
            Self::Inbound(_) => 0,
            Self::Outbound(..) => 1,
        }
    }
}

#[derive(Clone, Copy)]
struct Attempt {
    serial: u64,
    peer: Option<PeerId>,
    phase: AttemptPhase,
    deadline: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttemptPhase {
    Pending,
    Authorized,
    Established,
}

#[derive(Default)]
struct ActiveAttempts {
    entries: HashMap<AttemptKey, Attempt>,
    pending: [u32; 2],
    peers: HashMap<PeerId, u32>,
    next_serial: u64,
    failed: bool,
}

impl ActiveAttempts {
    fn for_peer(&self, peer: PeerId) -> u32 {
        self.peers.get(&peer).copied().unwrap_or(0)
    }

    fn release(&mut self, key: &AttemptKey, serial: u64) {
        let Some(attempt) = self
            .entries
            .get(key)
            .filter(|attempt| attempt.serial == serial)
            .copied()
        else {
            return;
        };
        self.entries.remove(key);
        if attempt.phase != AttemptPhase::Established {
            let lane = &mut self.pending[key.pending_lane()];
            if let Some(remaining) = lane.checked_sub(1) {
                *lane = remaining;
            } else {
                self.failed = true;
            }
        }
        if let Some(peer) = attempt.peer {
            match self.for_peer(peer).checked_sub(1) {
                Some(0) => {
                    self.peers.remove(&peer);
                }
                Some(remaining) => {
                    self.peers.insert(peer, remaining);
                }
                None => self.failed = true,
            }
        }
    }
}

struct ConnectionLedger {
    limits: ConnectionLimits,
    state: Mutex<ActiveAttempts>,
    diagnostics: Arc<BudgetDiagnostics>,
}

#[derive(Clone)]
pub(super) struct ConnectionBudget(Arc<ConnectionLedger>);

fn denied(reason: &'static str) -> ConnectionDenied {
    ConnectionDenied::new(io::Error::other(reason))
}

impl ConnectionBudget {
    #[cfg(test)]
    pub(super) fn new(limits: ConnectionLimits) -> io::Result<Self> {
        Self::with_diagnostics(limits, Arc::default())
    }

    pub(super) fn with_diagnostics(
        limits: ConnectionLimits,
        diagnostics: Arc<BudgetDiagnostics>,
    ) -> io::Result<Self> {
        if limits.total == 0
            || !(1..=limits.total).contains(&limits.pending)
            || !(1..=limits.pending).contains(&limits.inbound_unknown)
            || !(1..=limits.pending).contains(&limits.outbound)
            || !(1..=limits.total).contains(&limits.per_peer)
            || limits.handshake_timeout.is_zero()
            || Instant::now()
                .checked_add(limits.handshake_timeout)
                .is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid connection budget limits",
            ));
        }
        Ok(Self(Arc::new(ConnectionLedger {
            limits,
            state: Mutex::default(),
            diagnostics,
        })))
    }

    pub(super) fn snapshot(&self) -> Option<ConnectionBudgetSnapshot> {
        let state = self.0.state.lock().ok()?;
        if state.failed {
            return None;
        }
        let total = u32::try_from(state.entries.len()).ok()?;
        let pending = state.pending[0].checked_add(state.pending[1])?;
        Some(ConnectionBudgetSnapshot {
            total,
            established: total.checked_sub(pending)?,
            pending,
            pending_inbound_unknown: state.pending[0],
            pending_outbound: state.pending[1],
        })
    }

    pub(super) fn note_handshake_deadline(&self) {
        self.0
            .diagnostics
            .rejected(ResourceBudgetRejection::HandshakeDeadline, None);
    }

    fn rejected(&self, reason: ResourceBudgetRejection, detail: &'static str) -> ConnectionDenied {
        self.0.diagnostics.rejected(reason, None);
        denied(detail)
    }

    pub(super) fn configure_swarm(&self, config: Config) -> Config {
        let inbound = self.clone();
        let outbound = self.clone();
        config
            .with_timed_inbound_transport_admission(move |connection, _, _| {
                inbound.reserve(AttemptKey::Inbound(connection), None)
            })
            .with_timed_outbound_transport_admission(move |connection, peer, address| {
                outbound.reserve(AttemptKey::Outbound(connection, address.clone()), peer)
            })
    }

    fn reserve(
        &self,
        key: AttemptKey,
        peer: Option<PeerId>,
    ) -> Result<TransportAdmission, ConnectionDenied> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| denied("connection budget unavailable"))?;
        let limits = self.0.limits;
        if state.failed {
            return Err(denied("connection budget unavailable"));
        }
        if state.entries.contains_key(&key) {
            return Err(denied("duplicate connection attempt"));
        }
        if u32::try_from(state.entries.len())
            .map_err(|_| denied("connection budget unavailable"))?
            >= limits.total
        {
            return Err(self.rejected(
                ResourceBudgetRejection::ConnectionTotal,
                "total connection budget exhausted",
            ));
        }
        let pending = state.pending[0]
            .checked_add(state.pending[1])
            .ok_or_else(|| denied("connection budget unavailable"))?;
        if pending >= limits.pending {
            return Err(self.rejected(
                ResourceBudgetRejection::PendingTotal,
                "shared pending connection budget exhausted",
            ));
        }
        let lane = key.pending_lane();
        if state.pending[lane] >= [limits.inbound_unknown, limits.outbound][lane] {
            let reason = [
                ResourceBudgetRejection::PendingInbound,
                ResourceBudgetRejection::PendingOutbound,
            ][lane];
            return Err(self.rejected(reason, "directional pending connection budget exhausted"));
        }
        if peer.is_some_and(|peer| state.for_peer(peer) >= limits.per_peer) {
            return Err(self.rejected(
                ResourceBudgetRejection::PerPeer,
                "peer connection budget exhausted",
            ));
        }
        let deadline = Instant::now()
            .checked_add(limits.handshake_timeout)
            .ok_or_else(|| denied("invalid handshake deadline"))?;
        let serial = state.next_serial;
        state.next_serial = serial
            .checked_add(1)
            .ok_or_else(|| denied("connection attempt sequence exhausted"))?;
        state.pending[lane] += 1;
        if let Some(peer) = peer {
            *state.peers.entry(peer).or_default() += 1;
        }
        state.entries.insert(
            key.clone(),
            Attempt {
                serial,
                peer,
                phase: AttemptPhase::Pending,
                deadline,
            },
        );
        Ok(TransportAdmission::until(
            Box::new(AttemptPermit {
                budget: self.clone(),
                key,
                serial,
            }),
            deadline,
        ))
    }

    pub(super) fn authorize_inbound(
        &self,
        connection: ConnectionId,
        peer: PeerId,
    ) -> Result<ConnectionAdmission, ConnectionDenied> {
        self.authorize(AttemptKey::Inbound(connection), peer)
    }

    pub(super) fn authorize_outbound(
        &self,
        connection: ConnectionId,
        address: &Multiaddr,
        peer: PeerId,
    ) -> Result<ConnectionAdmission, ConnectionDenied> {
        self.authorize(AttemptKey::Outbound(connection, address.clone()), peer)
    }

    fn authorize(
        &self,
        key: AttemptKey,
        peer: PeerId,
    ) -> Result<ConnectionAdmission, ConnectionDenied> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| denied("connection budget unavailable"))?;
        let lane = key.pending_lane();
        if state.failed || state.pending[lane] == 0 {
            return Err(denied("connection budget unavailable"));
        }
        let attempt = state
            .entries
            .get(&key)
            .filter(|attempt| attempt.phase == AttemptPhase::Pending)
            .copied()
            .ok_or_else(|| denied("authenticated connection lacks a pending permit"))?;
        if Instant::now() >= attempt.deadline {
            self.note_handshake_deadline();
            return Err(ConnectionDenied::new(TransportHandshakeDeadlineElapsed));
        }
        let add_peer = match attempt.peer {
            Some(expected) if expected != peer => {
                return Err(denied(
                    "authenticated connection differs from reserved peer",
                ));
            }
            Some(_) => false,
            None => true,
        };
        if add_peer && state.for_peer(peer) >= self.0.limits.per_peer {
            return Err(self.rejected(
                ResourceBudgetRejection::PerPeer,
                "authenticated peer connection budget exhausted",
            ));
        }
        let current = state
            .entries
            .get_mut(&key)
            .ok_or_else(|| denied("connection permit disappeared"))?;
        // Reserve the authenticated peer bucket before child behaviours see the
        // connection. Still hold the pending slot until they all approve it.
        current.peer = Some(peer);
        current.phase = AttemptPhase::Authorized;
        if add_peer {
            *state.peers.entry(peer).or_default() += 1;
        }
        Ok(ConnectionAdmission {
            budget: self.clone(),
            key,
            serial: attempt.serial,
        })
    }
}

pub(super) struct ConnectionAdmission {
    budget: ConnectionBudget,
    key: AttemptKey,
    serial: u64,
}

impl ConnectionAdmission {
    pub(super) fn establish(self) -> Result<(), ConnectionDenied> {
        let mut state = self
            .budget
            .0
            .state
            .lock()
            .map_err(|_| denied("connection budget unavailable"))?;
        let lane = self.key.pending_lane();
        if state.failed || state.pending[lane] == 0 {
            return Err(denied("connection budget unavailable"));
        }
        let attempt = state
            .entries
            .get_mut(&self.key)
            .filter(|attempt| {
                attempt.serial == self.serial && attempt.phase == AttemptPhase::Authorized
            })
            .ok_or_else(|| denied("authorized connection permit disappeared"))?;
        attempt.phase = AttemptPhase::Established;
        state.pending[lane] -= 1;
        Ok(())
    }
}

struct AttemptPermit {
    budget: ConnectionBudget,
    key: AttemptKey,
    serial: u64,
}

impl Drop for AttemptPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.budget.0.state.lock() {
            state.release(&self.key, self.serial);
        }
    }
}
