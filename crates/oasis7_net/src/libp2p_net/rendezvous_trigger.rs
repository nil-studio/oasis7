use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libp2p::PeerId;
use libp2p::kad;
use libp2p::rendezvous;
use libp2p::request_response;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed_dht::PeerRecord;
use oasis7_proto::distributed_dht::SignedPeerRecord;

use super::discovery::{
    PendingPeerRecordRequest, maybe_queue_discovery_peer_record, maybe_request_cached_peer_record,
};
use super::kad_queries::PendingDhtQuery;
use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_record_cache::PeerRecordCache;
use super::swarm_behaviour::Behaviour;
use super::traffic_metrics::SharedLibp2pTrafficMetrics;
use super::utils::push_bounded_clone;

pub(super) const MAX_RENDEZVOUS_TRIGGER_TARGETS: usize = 1024;
const MAX_SERVERS_PER_TARGET: usize = 8;
const TRIGGER_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RendezvousTriggerEvidence {
    pub(super) server: PeerId,
    pub(super) observed_at: Instant,
}

#[derive(Debug, Clone)]
pub(super) struct RendezvousTriggerContext(Arc<RendezvousTriggerInner>);

#[derive(Debug)]
struct RendezvousTriggerInner {
    target: PeerId,
    evidence: Mutex<HashMap<PeerId, Instant>>,
}

impl RendezvousTriggerContext {
    pub(super) fn new(target: PeerId, server: PeerId, now: Instant) -> Self {
        Self(Arc::new(RendezvousTriggerInner {
            target,
            evidence: Mutex::new(HashMap::from([(server, now)])),
        }))
    }

    pub(super) fn target(&self) -> PeerId {
        self.0.target
    }

    pub(super) fn merge(&self, target: PeerId, server: PeerId, now: Instant) {
        if target != self.0.target {
            return;
        }
        let Ok(mut evidence) = self.0.evidence.lock() else {
            return;
        };
        evidence.retain(|_, at| trigger_is_live(*at, now));
        if evidence.contains_key(&server) || evidence.len() >= MAX_SERVERS_PER_TARGET {
            return;
        }
        evidence.insert(server, now);
    }

    pub(super) fn merge_evidence(&self, incoming: &[RendezvousTriggerEvidence], now: Instant) {
        let Ok(mut evidence) = self.0.evidence.lock() else {
            return;
        };
        evidence.retain(|_, at| trigger_is_live(*at, now));
        for item in incoming {
            if !trigger_is_live(item.observed_at, now)
                || evidence.contains_key(&item.server)
                || evidence.len() >= MAX_SERVERS_PER_TARGET
            {
                continue;
            }
            evidence.insert(item.server, item.observed_at);
        }
    }

    pub(super) fn active_evidence(&self, now: Instant) -> Vec<RendezvousTriggerEvidence> {
        let Ok(mut evidence) = self.0.evidence.lock() else {
            return Vec::new();
        };
        evidence.retain(|_, at| trigger_is_live(*at, now));
        let mut active = evidence
            .iter()
            .map(|(server, observed_at)| RendezvousTriggerEvidence {
                server: *server,
                observed_at: *observed_at,
            })
            .collect::<Vec<_>>();
        active.sort_by_key(|item| item.server.to_bytes());
        active
    }

    pub(super) fn clear(&self) {
        if let Ok(mut evidence) = self.0.evidence.lock() {
            evidence.clear();
        }
    }

    pub(super) fn has_live_evidence(&self, now: Instant) -> bool {
        !self.active_evidence(now).is_empty()
    }
}

fn trigger_is_live(observed_at: Instant, now: Instant) -> bool {
    now.checked_duration_since(observed_at)
        .is_some_and(|age| age < TRIGGER_TTL)
}

fn select_rendezvous_context(
    contexts: &[RendezvousTriggerContext],
    active_target_count: usize,
    target: PeerId,
    server: PeerId,
    now: Instant,
) -> Option<RendezvousTriggerContext> {
    contexts
        .iter()
        .find(|context| context.has_live_evidence(now))
        .cloned()
        .or_else(|| {
            (active_target_count < MAX_RENDEZVOUS_TRIGGER_TARGETS).then(|| {
                contexts
                    .first()
                    .cloned()
                    .unwrap_or_else(|| RendezvousTriggerContext::new(target, server, now))
            })
        })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Rendezvous result creates DHT and RR queries in one event-loop transaction"
)]
pub(super) fn handle_rendezvous_discovered(
    swarm: &mut Swarm<Behaviour>,
    rendezvous_node: PeerId,
    registrations: Vec<rendezvous::Registration>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_discovery_peer_records: &mut HashSet<PeerId>,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    connected_peers: &[PeerId],
    discovered_peer_records: &PeerRecordCache,
    local_peer_id: PeerId,
    template: Option<&PeerRecord>,
    max_error_messages: usize,
    event_errors: &Arc<Mutex<Vec<String>>>,
) {
    if registrations.is_empty() {
        return;
    }
    push_bounded_clone(
        event_errors,
        format!(
            "libp2p rendezvous discovered registrations via={rendezvous_node} count={}",
            registrations.len()
        ),
        max_error_messages,
        "lock errors",
    );
    let world_id = template
        .map(|record| record.world_id.as_str())
        .unwrap_or_default();
    for registration in registrations {
        let peer_id = registration.record.peer_id();
        if peer_id == local_peer_id {
            continue;
        }

        // A cache hit is not a result of this registration. Do not attach a
        // trigger to any existing query that might later return the same record.
        let trigger = if discovered_peer_records.get(&peer_id).is_some() {
            None
        } else {
            let now = Instant::now();
            let contexts =
                target_rendezvous_contexts(peer_id, pending_dht, pending_peer_record_requests);
            let selected = select_rendezvous_context(
                &contexts,
                active_rendezvous_target_count(pending_dht, pending_peer_record_requests, now),
                peer_id,
                rendezvous_node,
                now,
            );
            if let Some(context) = selected.as_ref() {
                for other in contexts {
                    context.merge_evidence(&other.active_evidence(now), now);
                }
                context.merge(peer_id, rendezvous_node, now);
            }
            selected
        };

        maybe_queue_discovery_peer_record(
            swarm,
            pending_dht,
            pending_discovery_peer_records,
            discovered_peer_records,
            peer_id,
            local_peer_id,
            world_id,
        );
        super::discovery::maybe_request_cached_peer_record(
            swarm,
            pending_peer_record_requests,
            pending_cached_peer_records,
            cached_peer_record_cooldowns,
            traffic_metrics,
            connected_peers,
            peer_id,
            local_peer_id,
        );
        if let Some(trigger) = trigger {
            attach_rendezvous_context(peer_id, &trigger, pending_dht, pending_peer_record_requests);
        }
    }
}

fn target_rendezvous_contexts(
    target: PeerId,
    pending_dht: &HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
) -> Vec<RendezvousTriggerContext> {
    let mut contexts = Vec::new();
    for query in pending_dht.values() {
        if let PendingDhtQuery::DiscoverPeerRecord {
            peer_id,
            rendezvous_triggers: Some(context),
            ..
        } = query
            && *peer_id == target
        {
            contexts.push(context.clone());
        }
    }
    for request in pending_peer_record_requests.values() {
        if let PendingPeerRecordRequest::CachedPeerRecord {
            peer_id,
            rendezvous_triggers: Some(context),
            ..
        } = request
            && *peer_id == target
        {
            contexts.push(context.clone());
        }
    }
    contexts
}

fn active_rendezvous_target_count(
    pending_dht: &HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    now: Instant,
) -> usize {
    let mut targets = HashSet::new();
    for query in pending_dht.values() {
        if let PendingDhtQuery::DiscoverPeerRecord {
            peer_id,
            rendezvous_triggers: Some(context),
            ..
        } = query
            && context.has_live_evidence(now)
        {
            targets.insert(*peer_id);
        }
    }
    for request in pending_peer_record_requests.values() {
        if let PendingPeerRecordRequest::CachedPeerRecord {
            peer_id,
            rendezvous_triggers: Some(context),
            ..
        } = request
            && context.has_live_evidence(now)
        {
            targets.insert(*peer_id);
        }
    }
    targets.len()
}

pub(super) fn attach_rendezvous_context(
    target: PeerId,
    context: &RendezvousTriggerContext,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
) -> bool {
    if context.target() != target {
        return false;
    }
    let mut attached = false;
    for query in pending_dht.values_mut() {
        if let PendingDhtQuery::DiscoverPeerRecord {
            peer_id,
            rendezvous_triggers,
            ..
        } = query
            && *peer_id == target
        {
            *rendezvous_triggers = Some(context.clone());
            attached = true;
        }
    }
    for request in pending_peer_record_requests.values_mut() {
        if let PendingPeerRecordRequest::CachedPeerRecord {
            peer_id,
            rendezvous_triggers,
            ..
        } = request
            && *peer_id == target
        {
            *rendezvous_triggers = Some(context.clone());
            attached = true;
        }
    }
    attached
}

pub(super) fn clear_if_not_attached_to_fallback(
    target: PeerId,
    context: Option<&RendezvousTriggerContext>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
) {
    if let Some(context) = context
        && !attach_rendezvous_context(target, context, pending_dht, pending_peer_record_requests)
    {
        context.clear();
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "DHT miss queues the RR fallback and transfers the same bounded trigger context"
)]
pub(super) fn queue_cached_record_fallback(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    connected_peers: &[PeerId],
    target: PeerId,
    local_peer_id: PeerId,
    context: Option<&RendezvousTriggerContext>,
) {
    maybe_request_cached_peer_record(
        swarm,
        pending_peer_record_requests,
        pending_cached_peer_records,
        cached_peer_record_cooldowns,
        traffic_metrics,
        connected_peers,
        target,
        local_peer_id,
    );
    clear_if_not_attached_to_fallback(target, context, pending_dht, pending_peer_record_requests);
}

pub(super) fn context_for_dht_result(
    context: Option<RendezvousTriggerContext>,
    upstream: Option<PeerId>,
    is_not_found: bool,
) -> Option<RendezvousTriggerContext> {
    if upstream.is_some() || is_not_found {
        context
    } else {
        if let Some(context) = context {
            context.clear();
        }
        None
    }
}

pub(super) fn active_attempt_evidence(
    record: &SignedPeerRecord,
    actual_source: Option<PeerId>,
    context: Option<&RendezvousTriggerContext>,
) -> Vec<RendezvousTriggerEvidence> {
    if actual_source.is_none()
        || record.publication.as_ref().is_some_and(|publication| {
            publication.state
                == oasis7_proto::distributed_dht::PeerRecordPublicationStateV2::Tombstone
        })
    {
        return Vec::new();
    }
    let Some(target) = record.record.peer_id.parse::<PeerId>().ok() else {
        return Vec::new();
    };
    context
        .filter(|context| context.target() == target)
        .map(|context| context.active_evidence(Instant::now()))
        .unwrap_or_default()
}

pub(super) fn linked_observation_diagnostic(
    record: &SignedPeerRecord,
    actual_ingress: &str,
    observations: &DiscoveryObservations,
    attempt_evidence: &[RendezvousTriggerEvidence],
) -> Option<String> {
    if attempt_evidence.is_empty() {
        return None;
    }
    let observed_servers = observations.rendezvous_trigger_servers(record, Instant::now());
    let mut servers = attempt_evidence
        .iter()
        .map(|evidence| evidence.server)
        .filter(|server| observed_servers.contains(server))
        .collect::<Vec<_>>();
    servers.sort_by_key(|peer| peer.to_bytes());
    servers.dedup();
    if servers.is_empty() {
        return None;
    }
    let payload = serde_cbor::to_vec(record).ok()?;
    let server_ids = servers
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "rendezvous-trigger-observation peer={} payload_blake3={} ingress={} servers={}",
        record.record.peer_id,
        blake3::hash(&payload).to_hex(),
        actual_ingress,
        server_ids,
    ))
}

pub(super) fn record_linked_observation_diagnostic(
    record: &SignedPeerRecord,
    actual_ingress: &str,
    observations: &DiscoveryObservations,
    attempt_evidence: &[RendezvousTriggerEvidence],
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
) {
    if let Some(diagnostic) =
        linked_observation_diagnostic(record, actual_ingress, observations, attempt_evidence)
    {
        push_bounded_clone(event_errors, diagnostic, max_error_messages, "lock errors");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_does_not_refresh_expiry_and_limit_is_bounded() {
        let now = Instant::now();
        let target = PeerId::random();
        let first = PeerId::random();
        let trigger = RendezvousTriggerContext::new(target, first, now);
        trigger.merge(target, first, now + Duration::from_secs(59));
        assert_eq!(
            trigger.active_evidence(now + Duration::from_secs(59)).len(),
            1
        );
        assert!(
            trigger
                .active_evidence(now + Duration::from_secs(60))
                .is_empty()
        );

        let trigger = RendezvousTriggerContext::new(target, first, now);
        for _ in 0..MAX_SERVERS_PER_TARGET {
            trigger.merge(target, PeerId::random(), now);
        }
        assert_eq!(trigger.active_evidence(now).len(), MAX_SERVERS_PER_TARGET);
    }

    #[test]
    fn stale_target_cannot_reactivate_past_global_limit() {
        let now = Instant::now();
        let target = PeerId::random();
        let server = PeerId::random();
        let stale = RendezvousTriggerContext::new(target, PeerId::random(), now - TRIGGER_TTL);
        assert!(
            select_rendezvous_context(
                std::slice::from_ref(&stale),
                MAX_RENDEZVOUS_TRIGGER_TARGETS,
                target,
                server,
                now,
            )
            .is_none()
        );
        let selected = select_rendezvous_context(
            &[stale],
            MAX_RENDEZVOUS_TRIGGER_TARGETS - 1,
            target,
            server,
            now,
        )
        .expect("expired pending target can be refreshed under the global bound");
        selected.merge(target, server, now);
        assert_eq!(selected.active_evidence(now).len(), 1);
    }
}
