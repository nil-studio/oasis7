use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use libp2p::{Multiaddr, PeerId, kad, request_response};
use serde::{Deserialize, Serialize};

pub(super) const MAX_DISCOVERY_IDS: usize = 128;
pub(super) const MAX_DISCOVERY_LIST_BYTES: usize = 16 * 1024;
pub(super) const MAX_PROXY_ATTEMPTS: usize = 4;
const MAX_INFLIGHT: usize = 32;
const MAX_WINDOW_STARTS: usize = 128;
pub(super) const MAX_COOLDOWN_ENTRIES: usize = 1024;
const WINDOW: Duration = Duration::from_secs(60);
const MAX_RENDEZVOUS_ADDRESSES: usize = 128;
const MAX_RENDEZVOUS_ADDRESS_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum RendezvousOperationKind {
    Register,
    Discover,
}

#[derive(Debug, Clone)]
struct PendingRendezvousOperation {
    kind: RendezvousOperationKind,
    peer_id: PeerId,
    external_addresses_digest: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryWorkMetricsSnapshot {
    pub admitted: u64,
    pub rejected_window: u64,
    pub rejected_inflight: u64,
    pub inflight: usize,
    pub window_starts: usize,
    pub max_window_starts: usize,
    pub window_ms: u64,
    pub max_inflight: usize,
    pub max_proxy_attempts: usize,
    pub max_list_ids: usize,
    pub max_list_bytes: usize,
    pub max_cooldown_entries: usize,
}
impl Default for DiscoveryWorkMetricsSnapshot {
    fn default() -> Self {
        Self {
            admitted: 0,
            rejected_window: 0,
            rejected_inflight: 0,
            inflight: 0,
            window_starts: 0,
            max_window_starts: MAX_WINDOW_STARTS,
            window_ms: 60_000,
            max_inflight: MAX_INFLIGHT,
            max_proxy_attempts: MAX_PROXY_ATTEMPTS,
            max_list_ids: MAX_DISCOVERY_IDS,
            max_list_bytes: MAX_DISCOVERY_LIST_BYTES,
            max_cooldown_entries: MAX_COOLDOWN_ENTRIES,
        }
    }
}

#[derive(Default)]
pub(super) struct DiscoveryWorkBudget {
    starts: VecDeque<Instant>,
    dht: HashSet<kad::QueryId>,
    requests: HashSet<request_response::OutboundRequestId>,
    rendezvous_by_request:
        HashMap<request_response::OutboundRequestId, Vec<PendingRendezvousOperation>>,
    rendezvous_peers: HashSet<(RendezvousOperationKind, PeerId)>,
    registered_rendezvous_addresses: HashMap<PeerId, [u8; 32]>,
    metrics: DiscoveryWorkMetricsSnapshot,
}
impl DiscoveryWorkBudget {
    pub(super) fn admit(&mut self) -> bool {
        self.admit_at(Instant::now())
    }
    fn admit_at(&mut self, now: Instant) -> bool {
        while self
            .starts
            .front()
            .is_some_and(|start| now.saturating_duration_since(*start) >= WINDOW)
        {
            self.starts.pop_front();
        }
        self.metrics.window_starts = self.starts.len();
        if self.metrics.inflight >= MAX_INFLIGHT {
            self.metrics.rejected_inflight = self.metrics.rejected_inflight.saturating_add(1);
            return false;
        }
        if self.starts.len() >= MAX_WINDOW_STARTS {
            self.metrics.rejected_window = self.metrics.rejected_window.saturating_add(1);
            return false;
        }
        self.starts.push_back(now);
        self.metrics.inflight += 1;
        self.metrics.admitted = self.metrics.admitted.saturating_add(1);
        self.metrics.window_starts = self.starts.len();
        true
    }
    pub(super) fn track_dht(&mut self, id: kad::QueryId) {
        self.dht.insert(id);
    }
    pub(super) fn track_request(&mut self, id: request_response::OutboundRequestId) {
        self.requests.insert(id);
    }
    pub(super) fn finish_dht(&mut self, id: kad::QueryId) {
        if self.dht.remove(&id) {
            self.finish_unscheduled();
        }
    }
    pub(super) fn finish_request(&mut self, id: request_response::OutboundRequestId) {
        if self.requests.remove(&id) {
            self.finish_unscheduled();
        }
    }
    pub(super) fn begin_rendezvous(
        &mut self,
        kind: RendezvousOperationKind,
        peer_id: PeerId,
    ) -> bool {
        let key = (kind, peer_id);
        if self.rendezvous_peers.contains(&key) || !self.admit() {
            return false;
        }
        self.rendezvous_peers.insert(key);
        true
    }
    pub(super) fn track_rendezvous(
        &mut self,
        id: request_response::OutboundRequestId,
        kind: RendezvousOperationKind,
        peer_id: PeerId,
        external_addresses_digest: Option<[u8; 32]>,
    ) {
        self.rendezvous_by_request
            .entry(id)
            .or_default()
            .push(PendingRendezvousOperation {
                kind,
                peer_id,
                external_addresses_digest,
            });
    }
    pub(super) fn finish_rendezvous_schedule_failure(
        &mut self,
        kind: RendezvousOperationKind,
        peer_id: PeerId,
    ) {
        if self.rendezvous_peers.remove(&(kind, peer_id)) {
            self.finish_unscheduled();
        }
    }
    pub(super) fn finish_rendezvous(
        &mut self,
        id: request_response::OutboundRequestId,
        kind: RendezvousOperationKind,
        peer_id: PeerId,
        current_external_addresses_digest: Option<[u8; 32]>,
        succeeded: bool,
    ) -> bool {
        let Some(operations) = self.rendezvous_by_request.get_mut(&id) else {
            return false;
        };
        let Some(index) = operations
            .iter()
            .position(|operation| operation.kind == kind && operation.peer_id == peer_id)
        else {
            return false;
        };
        let operation = operations.remove(index);
        if operations.is_empty() {
            self.rendezvous_by_request.remove(&id);
        }
        self.rendezvous_peers.remove(&(kind, peer_id));
        if kind == RendezvousOperationKind::Register {
            match (succeeded, operation.external_addresses_digest) {
                (true, Some(expected)) if Some(expected) == current_external_addresses_digest => {
                    self.registered_rendezvous_addresses
                        .insert(peer_id, expected);
                }
                _ => {
                    self.registered_rendezvous_addresses.remove(&peer_id);
                }
            }
        }
        self.finish_unscheduled();
        true
    }
    pub(super) fn rendezvous_registration_matches(
        &self,
        peer_id: PeerId,
        external_addresses_digest: Option<[u8; 32]>,
    ) -> bool {
        external_addresses_digest.is_some_and(|digest| {
            self.registered_rendezvous_addresses.get(&peer_id) == Some(&digest)
        })
    }
    pub(super) fn forget_rendezvous_registration(&mut self, peer_id: PeerId) {
        self.registered_rendezvous_addresses.remove(&peer_id);
    }
    // Scheduling can fail synchronously; concurrency is returned, rate is never refunded.
    pub(super) fn finish_unscheduled(&mut self) {
        self.metrics.inflight = self.metrics.inflight.saturating_sub(1);
    }
    pub(super) fn snapshot(&self) -> DiscoveryWorkMetricsSnapshot {
        let mut metrics = self.metrics.clone();
        let now = Instant::now();
        metrics.window_starts = self
            .starts
            .iter()
            .filter(|start| now.saturating_duration_since(**start) < WINDOW)
            .count();
        metrics
    }
}

pub(super) fn rendezvous_external_addresses_digest(
    addresses: impl Iterator<Item = Multiaddr>,
) -> Option<[u8; 32]> {
    let mut encoded_addresses = Vec::new();
    let mut total_bytes = 0usize;
    for address in addresses {
        if encoded_addresses.len() >= MAX_RENDEZVOUS_ADDRESSES {
            return None;
        }
        let bytes = address.to_vec();
        total_bytes = total_bytes.checked_add(bytes.len())?;
        if total_bytes > MAX_RENDEZVOUS_ADDRESS_BYTES {
            return None;
        }
        encoded_addresses.push(bytes);
    }
    encoded_addresses.sort();
    encoded_addresses.dedup();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"oasis7-rendezvous-address-set-v1\0");
    hasher.update(&(encoded_addresses.len() as u64).to_be_bytes());
    for address in encoded_addresses {
        hasher.update(&(address.len() as u64).to_be_bytes());
        hasher.update(&address);
    }
    Some(*hasher.finalize().as_bytes())
}

pub(super) fn publish_metrics(
    swarm: &libp2p::Swarm<super::swarm_behaviour::Behaviour>,
    metrics: &super::traffic_metrics::SharedLibp2pTrafficMetrics,
) {
    if let Ok(mut metrics) = metrics.lock() {
        metrics.discovery_work = swarm.behaviour().discovery_budget.snapshot();
    }
}

#[cfg(test)]
#[path = "tests/rendezvous_budget_lifecycle_tests.rs"]
mod rendezvous_budget_lifecycle_tests;

#[cfg(test)]
#[path = "tests/rendezvous_budget_callsite_tests.rs"]
mod rendezvous_budget_callsite_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sliding_window_survives_terminal_release_and_source_switching() {
        let now = Instant::now();
        let mut budget = DiscoveryWorkBudget::default();
        for _ in 0..MAX_DISCOVERY_IDS {
            assert!(budget.admit_at(now));
            budget.finish_unscheduled();
        }
        assert!(!budget.admit_at(now + WINDOW - Duration::from_nanos(1)));
        assert!(budget.admit_at(now + WINDOW));
        assert_eq!(budget.snapshot().admitted, 129);
    }
    #[test]
    fn idle_snapshot_reports_expired_window_without_new_admission() {
        let mut budget = DiscoveryWorkBudget::default();
        let old = Instant::now()
            .checked_sub(WINDOW + Duration::from_secs(1))
            .unwrap();
        assert!(budget.admit_at(old));
        budget.finish_unscheduled();
        assert_eq!(budget.metrics.window_starts, 1);
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.window_starts, 0);
        assert_eq!(snapshot.admitted, 1);
        assert_eq!(snapshot.inflight, 0);
        assert_eq!(
            budget.starts.len(),
            1,
            "reading status does not schedule work"
        );
    }
    #[test]
    fn concurrency_is_shared_and_release_never_refunds_rate() {
        let mut budget = DiscoveryWorkBudget::default();
        let now = Instant::now();
        for _ in 0..MAX_INFLIGHT {
            assert!(budget.admit_at(now));
        }
        assert!(!budget.admit_at(now));
        budget.finish_unscheduled();
        assert!(budget.admit_at(now));
        assert_eq!(budget.snapshot().window_starts, 33);
    }
}
