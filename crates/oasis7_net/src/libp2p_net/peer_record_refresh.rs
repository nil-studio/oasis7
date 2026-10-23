//! Best-effort refresh of live candidates. This never renews an existing lease.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use libp2p::{PeerId, Swarm, kad, request_response};
use oasis7_proto::distributed_dht::PeerRecord;

use super::discovery::{
    PendingPeerRecordRequest, maybe_request_connected_peer_record, queue_refresh_peer_record,
};
use super::kad_queries::PendingDhtQuery;
use super::peer_record_cache::PeerRecordCache;
use super::swarm_behaviour::Behaviour;
use super::traffic_metrics::SharedLibp2pTrafficMetrics;

pub(super) const REFRESH_BATCH: usize = 4;
pub(super) const REFRESH_RETRY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecordRefreshSchedule {
    pub(super) due: Instant,
    pub(super) retry_not_before: Instant,
}
impl RecordRefreshSchedule {
    pub(super) fn new(deadline: Instant, ttl_ms: i64, sample: u64) -> Self {
        let lead_ms = (ttl_ms as u64 / 5).min(60_000);
        let jitter_ms = sample % (lead_ms / 2 + 1);
        let due = deadline
            .checked_sub(Duration::from_millis(lead_ms + jitter_ms))
            .unwrap_or(deadline);
        Self {
            due,
            retry_not_before: due,
        }
    }
    pub(super) fn eligible_at(&self) -> Instant {
        self.due.max(self.retry_not_before)
    }
}

pub(super) struct RefreshState<'a> {
    pub(super) cache: &'a mut PeerRecordCache,
    pub(super) pending_dht: &'a mut HashMap<kad::QueryId, PendingDhtQuery>,
    pub(super) pending_records: &'a mut HashSet<PeerId>,
    pub(super) pending_requests:
        &'a mut HashMap<request_response::OutboundRequestId, PendingPeerRecordRequest>,
    pub(super) pending_connected: &'a mut HashSet<PeerId>,
    pub(super) connected_cooldowns: &'a mut HashMap<PeerId, i64>,
}

pub(super) fn refresh_due_from_context(
    swarm: &mut Swarm<Behaviour>,
    state: RefreshState<'_>,
    context: &super::runtime_loop::CommandContext<'_>,
    interval_ms: i64,
) {
    refresh_due_records(
        swarm,
        state,
        context.peer_record_template,
        interval_ms,
        context.local_peer_id,
        context.event_traffic_metrics,
    );
}

pub(super) fn refresh_due_records(
    swarm: &mut Swarm<Behaviour>,
    state: RefreshState<'_>,
    template: Option<&PeerRecord>,
    interval_ms: i64,
    local: PeerId,
    metrics: &SharedLibp2pTrafficMetrics,
) {
    let Some(template) = template.filter(|_| interval_ms > 0) else {
        return;
    };
    for peer in state.cache.take_due_refreshes(Instant::now()) {
        if swarm.is_connected(&peer) {
            maybe_request_connected_peer_record(
                swarm,
                state.pending_requests,
                state.pending_connected,
                state.connected_cooldowns,
                metrics,
                peer,
                local,
                true,
            );
        } else {
            queue_refresh_peer_record(
                swarm,
                state.pending_dht,
                state.pending_records,
                peer,
                local,
                &template.world_id,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    #[test]
    fn jitter_is_stable_bounded_and_capped_for_long_ttl() {
        let deadline = Instant::now() + Duration::from_secs(1_000);
        let earliest = RecordRefreshSchedule::new(deadline, 60_000, 6_000);
        let latest = RecordRefreshSchedule::new(deadline, 60_000, 0);
        assert_eq!(earliest.due, deadline - Duration::from_secs(18));
        assert_eq!(latest.due, deadline - Duration::from_secs(12));
        assert_eq!(
            earliest,
            RecordRefreshSchedule::new(deadline, 60_000, 6_000)
        );
        assert_eq!(
            RecordRefreshSchedule::new(deadline, 3_600_000, 30_000).due,
            deadline - Duration::from_secs(90)
        );
    }

    #[test]
    fn budget_denied_refresh_keeps_original_cache_and_backs_off() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let key = Keypair::generate_ed25519();
        let local = PeerId::from(key.public());
        let remote_key = Keypair::generate_ed25519();
        let remote = PeerId::from(remote_key.public());
        let signed = super::super::tests::signed_discovery_peer_record(
            &remote_key,
            vec![],
            super::super::now_ms() - 50_000,
        );
        let mut cache = PeerRecordCache::default();
        cache.insert(remote, signed.clone());
        let mut swarm = super::super::swarm_behaviour::build_swarm(
            &key,
            false,
            false,
            Duration::from_secs(30),
            super::super::wire_bytes::init_shared_wire_byte_counters(),
        );
        for _ in 0..32 {
            assert!(swarm.behaviour_mut().discovery_budget.admit());
        }
        let mut dht = HashMap::new();
        let mut records = HashSet::new();
        let mut requests = HashMap::new();
        let mut connected = HashSet::new();
        let mut cooldowns = HashMap::new();
        let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        for _ in 0..2 {
            refresh_due_records(
                &mut swarm,
                RefreshState {
                    cache: &mut cache,
                    pending_dht: &mut dht,
                    pending_records: &mut records,
                    pending_requests: &mut requests,
                    pending_connected: &mut connected,
                    connected_cooldowns: &mut cooldowns,
                },
                Some(&signed.record),
                1_000,
                local,
                &metrics,
            );
        }
        assert!(dht.is_empty());
        assert!(records.is_empty());
        assert!(requests.is_empty());
        assert_eq!(cache.get(&remote), Some(&signed));
        assert_eq!(
            swarm
                .behaviour()
                .discovery_budget
                .snapshot()
                .rejected_inflight,
            1
        );
    }

    #[test]
    fn due_live_cached_candidate_starts_refresh_despite_presence_dedup() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let key = Keypair::generate_ed25519();
        let local = PeerId::from(key.public());
        let peer_key = Keypair::generate_ed25519();
        let peer = PeerId::from(peer_key.public());
        let mut signed = super::super::tests::signed_discovery_peer_record(
            &peer_key,
            vec![],
            super::super::now_ms() - 8_500,
        );
        signed.record.ttl_ms = 10_000;
        signed = super::super::peer_record::sign_peer_record(&signed.record, &peer_key).unwrap();
        let template = signed.record.clone();
        let mut cache = PeerRecordCache::default();
        cache.insert(peer, signed.clone());
        let mut swarm = super::super::swarm_behaviour::build_swarm(
            &key,
            false,
            false,
            Duration::from_secs(30),
            super::super::wire_bytes::init_shared_wire_byte_counters(),
        );
        let mut pending_dht = HashMap::new();
        let mut pending_records = HashSet::new();
        let mut pending_requests = HashMap::new();
        let mut pending_connected = HashSet::new();
        let mut connected_cooldowns = HashMap::new();
        let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        for (config_template, interval) in
            [(None, 1_000), (Some(&template), 0), (Some(&template), -1)]
        {
            refresh_due_records(
                &mut swarm,
                RefreshState {
                    cache: &mut cache,
                    pending_dht: &mut pending_dht,
                    pending_records: &mut pending_records,
                    pending_requests: &mut pending_requests,
                    pending_connected: &mut pending_connected,
                    connected_cooldowns: &mut connected_cooldowns,
                },
                config_template,
                interval,
                local,
                &metrics,
            );
            assert!(pending_dht.is_empty());
            assert_eq!(swarm.behaviour().discovery_budget.snapshot().admitted, 0);
        }
        refresh_due_records(
            &mut swarm,
            RefreshState {
                cache: &mut cache,
                pending_dht: &mut pending_dht,
                pending_records: &mut pending_records,
                pending_requests: &mut pending_requests,
                pending_connected: &mut pending_connected,
                connected_cooldowns: &mut connected_cooldowns,
            },
            Some(&template),
            1_000,
            local,
            &metrics,
        );
        assert_eq!(pending_dht.len(), 1);
        assert_eq!(cache.get(&peer), Some(&signed));
    }
}

#[cfg(test)]
#[path = "peer_record_refresh_tcp_tests.rs"]
mod tcp_tests;
