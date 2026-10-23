use super::discovery_budget::{
    MAX_COOLDOWN_ENTRIES, MAX_DISCOVERY_IDS, MAX_DISCOVERY_LIST_BYTES, MAX_PROXY_ATTEMPTS,
};
use super::peer_record_cache::PeerRecordCache;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use libp2p::identity::Keypair;
use libp2p::kad::{self, RecordKey};
use libp2p::rendezvous;
use libp2p::request_response;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};

use crate::error::WorldError;
use crate::util::to_canonical_cbor;
use oasis7_proto::distributed::{
    DistributedErrorCode, ErrorResponse, dht_peer_discovery_key, dht_peer_record_key,
    rendezvous_namespace as distributed_rendezvous_namespace,
};
use oasis7_proto::distributed_dht::{PeerDiscoverySource, PeerRecord, SignedPeerRecord};
use oasis7_proto::distributed_net::NetworkRequest;

use super::kad_queries::PendingDhtQuery;
use super::peer_discovery_observations::{DiscoveryIngress, DiscoveryObservations};
use super::peer_manager::{
    PeerManagerHealthStatus, PeerManagerPolicy, recompute_peer_manager_healths,
};
use super::peer_record::{
    build_configured_peer_record, put_record_query, validate_discovered_peer_record,
};
use super::reachability::Libp2pReachabilitySnapshot;
use super::swarm_behaviour::Behaviour;
use super::traffic_metrics::{SharedLibp2pTrafficMetrics, record_request_outbound};
use super::transport_paths::{
    TransportPath, dial_transport_path, peer_record_transport_paths,
    select_preferred_transport_path, sync_known_transport_paths,
};
use super::utils::push_bounded_string_with_keyed_cooldown;
use super::{HandlerRegistration, push_bounded_clone};

const RR_GET_LOCAL_PEER_RECORD: &str = "/aw/rr/1.0.0/get_local_peer_record";
pub(super) const RR_GET_CACHED_PEER_RECORD: &str = "/aw/rr/1.0.0/get_cached_peer_record";
const RR_GET_CACHED_DISCOVERY_PEERS: &str = "/aw/rr/1.0.0/get_cached_discovery_peers";

pub(super) enum PendingPeerRecordRequest {
    ConnectedPeerRecord {
        peer_id: PeerId,
        target_owned_route_expected: bool,
    },
    CachedPeerRecord {
        ask_peer: PeerId,
        peer_id: PeerId,
        tried_proxies: Vec<PeerId>,
    },
    CachedDiscoveryPeers {
        peer_id: PeerId,
    },
}

impl PendingPeerRecordRequest {
    pub(super) fn protocol_label(&self) -> &'static str {
        match self {
            Self::ConnectedPeerRecord { .. } => RR_GET_LOCAL_PEER_RECORD,
            Self::CachedPeerRecord { .. } => RR_GET_CACHED_PEER_RECORD,
            Self::CachedDiscoveryPeers { .. } => RR_GET_CACHED_DISCOVERY_PEERS,
        }
    }
}

const PEER_RECORD_REQUEST_COOLDOWN_MS: i64 = 60_000;

pub(super) fn peer_record_world_id(template: Option<&PeerRecord>) -> &str {
    template
        .map(|record| record.world_id.as_str())
        .unwrap_or_default()
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn log_routing_updated(
    event_errors: &Arc<Mutex<Vec<String>>>,
    lifecycle_event_errors_at_ms: &mut HashMap<String, i64>,
    lifecycle_event_errors_last_prune_at_ms: &mut Option<i64>,
    max_error_messages: usize,
    now_ms: i64,
    cooldown_ms: i64,
    peer: PeerId,
    addresses_debug: String,
) {
    push_bounded_string_with_keyed_cooldown(
        event_errors,
        lifecycle_event_errors_at_ms,
        lifecycle_event_errors_last_prune_at_ms,
        format!("routing-updated:{peer}"),
        format!("libp2p routing updated peer={peer} addrs={addresses_debug}"),
        max_error_messages,
        "lock errors",
        now_ms,
        cooldown_ms,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_routing_updated(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_discovery_peer_records: &mut HashSet<PeerId>,
    discovered_peer_records: &PeerRecordCache,
    peer: PeerId,
    addresses_debug: String,
    local_peer_id: PeerId,
    peer_record_template: Option<&PeerRecord>,
    peers: &[PeerId],
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_connected_peer_records: &mut HashSet<PeerId>,
    connected_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    event_traffic_metrics: &SharedLibp2pTrafficMetrics,
    event_errors: &Arc<Mutex<Vec<String>>>,
    lifecycle_event_errors_at_ms: &mut HashMap<String, i64>,
    lifecycle_event_errors_last_prune_at_ms: &mut Option<i64>,
    max_error_messages: usize,
    now_ms: i64,
    cooldown_ms: i64,
) {
    log_routing_updated(
        event_errors,
        lifecycle_event_errors_at_ms,
        lifecycle_event_errors_last_prune_at_ms,
        max_error_messages,
        now_ms,
        cooldown_ms,
        peer,
        addresses_debug,
    );
    maybe_queue_discovery_peer_record(
        swarm,
        pending_dht,
        pending_discovery_peer_records,
        discovered_peer_records,
        peer,
        local_peer_id,
        peer_record_world_id(peer_record_template),
    );
    if peers.contains(&peer) || swarm.is_connected(&peer) {
        maybe_request_connected_peer_record(
            swarm,
            pending_peer_record_requests,
            pending_connected_peer_records,
            connected_peer_record_cooldowns,
            event_traffic_metrics,
            peer,
            local_peer_id,
            peer_record_template.is_some(),
        );
    }
}

fn peer_record_request_in_cooldown(
    cooldowns: &mut HashMap<PeerId, i64>,
    peer_id: PeerId,
    now_ms: i64,
) -> bool {
    cooldowns.retain(|_, retry_at| *retry_at > now_ms);
    if !cooldowns.contains_key(&peer_id) && cooldowns.len() >= MAX_COOLDOWN_ENTRIES {
        return true;
    }
    match cooldowns.get(&peer_id).copied() {
        Some(retry_at_ms) if retry_at_ms > now_ms => true,
        Some(_) => {
            cooldowns.remove(&peer_id);
            false
        }
        None => false,
    }
}

fn note_peer_record_request_cooldown(
    cooldowns: &mut HashMap<PeerId, i64>,
    peer_id: PeerId,
    now_ms: i64,
) {
    cooldowns.insert(
        peer_id,
        now_ms.saturating_add(PEER_RECORD_REQUEST_COOLDOWN_MS),
    );
}

pub(super) fn start_peer_discovery_query(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    template: &PeerRecord,
    last_started_at_ms: &mut Option<i64>,
    now_ms: i64,
    cooldown_ms: i64,
) -> bool {
    if pending_dht
        .values()
        .any(|query| matches!(query, PendingDhtQuery::DiscoverPeers { .. }))
    {
        return false;
    }
    if last_started_at_ms
        .map(|last_ms| cooldown_ms > 0 && !super::should_republish(last_ms, now_ms, cooldown_ms))
        .unwrap_or(false)
    {
        return false;
    }
    if !swarm.behaviour_mut().discovery_budget.admit() {
        return false;
    }
    let key = dht_peer_discovery_key(template.world_id.as_str());
    let query_id = swarm
        .behaviour_mut()
        .kademlia
        .get_providers(RecordKey::new(&key));
    swarm.behaviour_mut().discovery_budget.track_dht(query_id);
    pending_dht.insert(
        query_id,
        PendingDhtQuery::DiscoverPeers {
            peers: HashSet::new(),
            error: None,
        },
    );
    *last_started_at_ms = Some(now_ms);
    true
}

pub(super) fn publish_discovery_provider(
    swarm: &mut Swarm<Behaviour>,
    provider_keys: &mut HashMap<String, i64>,
    world_id: &str,
) {
    let key = dht_peer_discovery_key(world_id);
    if swarm
        .behaviour_mut()
        .kademlia
        .start_providing(RecordKey::new(&key))
        .is_ok()
    {
        provider_keys.insert(key, super::now_ms());
    }
}

pub(super) fn maybe_register_rendezvous_namespace(
    swarm: &mut Swarm<Behaviour>,
    pending_rendezvous_registers: &mut HashSet<PeerId>,
    registered_rendezvous_nodes: &HashSet<PeerId>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &PeerRecord,
) -> Result<bool, WorldError> {
    if !peer_record_enables_rendezvous(template) {
        return Ok(false);
    }
    if peer_id == local_peer_id
        || pending_rendezvous_registers.contains(&peer_id)
        || registered_rendezvous_nodes.contains(&peer_id)
    {
        return Ok(false);
    }
    let namespace = rendezvous_namespace(template)?;
    swarm
        .behaviour_mut()
        .rendezvous_client
        .as_mut()
        .expect("rendezvous client enabled when peer record opts in")
        .register(namespace, peer_id, None)
        .map_err(|err| WorldError::NetworkProtocolUnavailable {
            protocol: format!("rendezvous register failed: {err}"),
        })?;
    pending_rendezvous_registers.insert(peer_id);
    Ok(true)
}

pub(super) fn maybe_discover_rendezvous_namespace(
    swarm: &mut Swarm<Behaviour>,
    pending_rendezvous_discovers: &mut HashSet<PeerId>,
    rendezvous_cookies: &HashMap<PeerId, rendezvous::Cookie>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &PeerRecord,
) -> Result<bool, WorldError> {
    if !peer_record_enables_rendezvous(template) {
        return Ok(false);
    }
    if peer_id == local_peer_id || pending_rendezvous_discovers.contains(&peer_id) {
        return Ok(false);
    }
    let namespace = rendezvous_namespace(template)?;
    let cookie = rendezvous_cookies.get(&peer_id).cloned();
    swarm
        .behaviour_mut()
        .rendezvous_client
        .as_mut()
        .expect("rendezvous client enabled when peer record opts in")
        .discover(Some(namespace), cookie, None, peer_id);
    pending_rendezvous_discovers.insert(peer_id);
    Ok(true)
}

pub(super) fn peer_record_enables_rendezvous(template: &PeerRecord) -> bool {
    template
        .discovery_sources
        .iter()
        .any(|source| matches!(source, PeerDiscoverySource::Rendezvous))
}

pub(super) fn maybe_queue_discovery_peer_record(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    pending_discovery_peer_records: &mut HashSet<PeerId>,
    discovered_peer_records: &PeerRecordCache,
    peer_id: PeerId,
    local_peer_id: PeerId,
    world_id: &str,
) {
    if world_id.trim().is_empty()
        || peer_id == local_peer_id
        || pending_discovery_peer_records.contains(&peer_id)
        || discovered_peer_records.contains_key(&peer_id)
    {
        return;
    }
    if !swarm.behaviour_mut().discovery_budget.admit() {
        return;
    }
    let key = dht_peer_record_key(world_id, peer_id.to_string().as_str());
    let query_id = swarm
        .behaviour_mut()
        .kademlia
        .get_record(RecordKey::new(&key));
    pending_discovery_peer_records.insert(peer_id);
    swarm.behaviour_mut().discovery_budget.track_dht(query_id);
    pending_dht.insert(
        query_id,
        PendingDhtQuery::DiscoverPeerRecord {
            peer_id,
            upstream: None,
            record: None,
            error: None,
        },
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn maybe_request_connected_peer_record(
    swarm: &mut Swarm<Behaviour>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_connected_peer_records: &mut HashSet<PeerId>,
    connected_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    peer_id: PeerId,
    local_peer_id: PeerId,
    target_owned_route_expected: bool,
) -> bool {
    let now_ms = super::now_ms();
    if peer_id == local_peer_id
        || pending_connected_peer_records.contains(&peer_id)
        || peer_record_request_in_cooldown(connected_peer_record_cooldowns, peer_id, now_ms)
    {
        return false;
    }
    if !swarm.behaviour_mut().discovery_budget.admit() {
        return false;
    }
    record_request_outbound(traffic_metrics, RR_GET_LOCAL_PEER_RECORD, 0);
    let request_id = swarm.behaviour_mut().request_response.send_request(
        &peer_id,
        NetworkRequest {
            protocol: RR_GET_LOCAL_PEER_RECORD.to_string(),
            payload: Vec::new(),
        },
    );
    pending_connected_peer_records.insert(peer_id);
    note_peer_record_request_cooldown(connected_peer_record_cooldowns, peer_id, now_ms);
    swarm
        .behaviour_mut()
        .discovery_budget
        .track_request(request_id);
    pending_peer_record_requests.insert(
        request_id,
        PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id,
            target_owned_route_expected,
        },
    );
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
fn request_cached_peer_record_via(
    swarm: &mut Swarm<Behaviour>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    ask_peer: PeerId,
    peer_id: PeerId,
    local_peer_id: PeerId,
    mut tried_proxies: Vec<PeerId>,
) -> bool {
    if peer_id == local_peer_id
        || ask_peer == local_peer_id
        || pending_cached_peer_records.contains(&peer_id)
    {
        return false;
    }
    let now_ms = super::now_ms();
    cached_peer_record_cooldowns.retain(|_, until| *until > now_ms);
    if (tried_proxies.is_empty()
        && peer_record_request_in_cooldown(cached_peer_record_cooldowns, peer_id, now_ms))
        || tried_proxies.len() >= MAX_PROXY_ATTEMPTS
        || tried_proxies.contains(&ask_peer)
        || (!cached_peer_record_cooldowns.contains_key(&peer_id)
            && cached_peer_record_cooldowns.len() >= MAX_COOLDOWN_ENTRIES)
        || !swarm.behaviour_mut().discovery_budget.admit()
    {
        return false;
    }
    if !tried_proxies.contains(&ask_peer) {
        tried_proxies.push(ask_peer);
    }
    record_request_outbound(
        traffic_metrics,
        RR_GET_CACHED_PEER_RECORD,
        peer_id.to_string().len(),
    );
    let request_id = swarm.behaviour_mut().request_response.send_request(
        &ask_peer,
        NetworkRequest {
            protocol: RR_GET_CACHED_PEER_RECORD.to_string(),
            payload: peer_id.to_string().into_bytes(),
        },
    );
    pending_cached_peer_records.insert(peer_id);
    note_peer_record_request_cooldown(cached_peer_record_cooldowns, peer_id, super::now_ms());
    swarm
        .behaviour_mut()
        .discovery_budget
        .track_request(request_id);
    pending_peer_record_requests.insert(
        request_id,
        PendingPeerRecordRequest::CachedPeerRecord {
            ask_peer,
            peer_id,
            tried_proxies,
        },
    );
    true
}

fn select_cached_peer_record_proxy(
    connected_peers: &[PeerId],
    peer_id: PeerId,
    local_peer_id: PeerId,
    excluded_peers: &[PeerId],
) -> Option<PeerId> {
    connected_peers.iter().copied().find(|candidate| {
        *candidate != peer_id && *candidate != local_peer_id && !excluded_peers.contains(candidate)
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn maybe_request_cached_peer_record(
    swarm: &mut Swarm<Behaviour>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    connected_peers: &[PeerId],
    peer_id: PeerId,
    local_peer_id: PeerId,
) -> bool {
    let now_ms = super::now_ms();
    if peer_id == local_peer_id
        || pending_cached_peer_records.contains(&peer_id)
        || peer_record_request_in_cooldown(cached_peer_record_cooldowns, peer_id, now_ms)
    {
        return false;
    }
    let Some(ask_peer) =
        select_cached_peer_record_proxy(connected_peers, peer_id, local_peer_id, &[])
    else {
        return false;
    };
    request_cached_peer_record_via(
        swarm,
        pending_peer_record_requests,
        pending_cached_peer_records,
        cached_peer_record_cooldowns,
        traffic_metrics,
        ask_peer,
        peer_id,
        local_peer_id,
        Vec::new(),
    )
}

pub(super) fn maybe_request_cached_discovery_peers(
    swarm: &mut Swarm<Behaviour>,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_cached_discovery_peers: &mut HashSet<PeerId>,
    cached_discovery_peer_cooldowns: &mut HashMap<PeerId, i64>,
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    peer_id: PeerId,
    local_peer_id: PeerId,
) -> bool {
    let now_ms = super::now_ms();
    if peer_id == local_peer_id
        || pending_cached_discovery_peers.contains(&peer_id)
        || peer_record_request_in_cooldown(cached_discovery_peer_cooldowns, peer_id, now_ms)
    {
        return false;
    }
    if !swarm.behaviour_mut().discovery_budget.admit() {
        return false;
    }
    record_request_outbound(traffic_metrics, RR_GET_CACHED_DISCOVERY_PEERS, 0);
    let request_id = swarm.behaviour_mut().request_response.send_request(
        &peer_id,
        NetworkRequest {
            protocol: RR_GET_CACHED_DISCOVERY_PEERS.to_string(),
            payload: Vec::new(),
        },
    );
    pending_cached_discovery_peers.insert(peer_id);
    note_peer_record_request_cooldown(cached_discovery_peer_cooldowns, peer_id, now_ms);
    swarm
        .behaviour_mut()
        .discovery_budget
        .track_request(request_id);
    pending_peer_record_requests.insert(
        request_id,
        PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id },
    );
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
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
        maybe_queue_discovery_peer_record(
            swarm,
            pending_dht,
            pending_discovery_peer_records,
            discovered_peer_records,
            peer_id,
            local_peer_id,
            world_id,
        );
        maybe_request_cached_peer_record(
            swarm,
            pending_peer_record_requests,
            pending_cached_peer_records,
            cached_peer_record_cooldowns,
            traffic_metrics,
            connected_peers,
            peer_id,
            local_peer_id,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn process_discovered_peer_record(
    swarm: &mut Swarm<Behaviour>,
    discovered_peer_records: &mut PeerRecordCache,
    discovery_observations: &mut DiscoveryObservations,
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    template: Option<&PeerRecord>,
    peer_manager_policy: &PeerManagerPolicy,
    static_bootstrap_peer_ids: &HashSet<PeerId>,
    record: SignedPeerRecord,
    ingress: DiscoveryIngress,
) -> Result<(), WorldError> {
    validate_discovered_peer_record(&record, template)?;
    let peer_id = record.record.peer_id.parse::<PeerId>().map_err(|_| {
        WorldError::NetworkProtocolUnavailable {
            protocol: "peer record peer_id must be valid".to_string(),
        }
    })?;
    let allow_single_source_bootstrap_dial = static_bootstrap_peer_ids.contains(&peer_id);
    let transport_paths = peer_record_transport_paths(&record)?;
    let lease = discovered_peer_records.prepare(peer_id, &record)?;
    discovery_observations.record(&record, ingress, super::now_ms())?;
    discovery_observations.set_static_bootstrap_peers(static_bootstrap_peer_ids.clone());
    discovered_peer_records.install(peer_id, record, lease.clone());
    let transport_paths: Vec<_> = transport_paths
        .into_iter()
        .map(|mut path| {
            path.record_lease = Some(lease.clone());
            path
        })
        .collect();
    sync_known_transport_paths(
        known_transport_paths,
        failed_transport_path_labels,
        peer_id,
        transport_paths.clone(),
    );
    let peer_healths = recompute_peer_manager_healths(
        discovered_peer_records,
        discovery_observations,
        active_transport_paths,
        peer_manager_policy,
    );
    let peer_status = peer_healths
        .get(&peer_id)
        .map(|health| health.status)
        .unwrap_or(PeerManagerHealthStatus::Candidate);
    if let Some(preferred_path) =
        select_preferred_transport_path(transport_paths.as_slice(), failed_transport_path_labels)
    {
        let already_dialing_preferred = last_dialed_transport_paths
            .get(&peer_id)
            .map(|path| path.label() == preferred_path.label())
            .unwrap_or(false);
        let should_dial = (!matches!(peer_status, PeerManagerHealthStatus::Blocked)
            && !matches!(peer_status, PeerManagerHealthStatus::Suspect))
            || (matches!(peer_status, PeerManagerHealthStatus::Suspect)
                && allow_single_source_bootstrap_dial);
        let should_dial = should_dial
            && active_transport_paths
                .get(&peer_id)
                .map(|active| preferred_path.preference_rank() < active.preference_rank())
                .unwrap_or(true);
        if should_dial && !already_dialing_preferred {
            let _ = dial_transport_path(swarm, last_dialed_transport_paths, preferred_path.clone());
        }
    }
    Ok(())
}

pub(super) fn peer_record_matches_target(
    record: &SignedPeerRecord,
    requested_peer_id: PeerId,
) -> bool {
    record
        .record
        .peer_id
        .parse::<PeerId>()
        .map(|record_peer_id| record_peer_id == requested_peer_id)
        .unwrap_or(false)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn handle_request_response_request(
    request: &NetworkRequest,
    handlers: &HashMap<String, HandlerRegistration>,
    peer_record_template: Option<&PeerRecord>,
    keypair: &Keypair,
    listening_addrs: &Arc<Mutex<Vec<Multiaddr>>>,
    reachability: &Arc<Mutex<Libp2pReachabilitySnapshot>>,
    allow_loopback_external_addrs_for_testing: bool,
    discovered_peer_records: &PeerRecordCache,
) -> Result<Vec<u8>, WorldError> {
    match request.protocol.as_str() {
        RR_GET_LOCAL_PEER_RECORD => {
            let Some(template) = peer_record_template else {
                return Err(WorldError::NetworkProtocolUnavailable {
                    protocol: RR_GET_LOCAL_PEER_RECORD.to_string(),
                });
            };
            let record = build_configured_peer_record(
                keypair,
                template,
                listening_addrs,
                reachability,
                allow_loopback_external_addrs_for_testing,
            )?;
            discovered_peer_records.clock().check(&record.record)?;
            to_canonical_cbor(&record)
        }
        RR_GET_CACHED_PEER_RECORD => {
            let peer_id = String::from_utf8(request.payload.clone())
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "cached peer record payload must be utf-8".to_string(),
                })?
                .parse::<PeerId>()
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "cached peer record peer_id must be valid".to_string(),
                })?;
            let record = discovered_peer_records.get(&peer_id).ok_or_else(|| {
                WorldError::NetworkRequestFailed {
                    code: DistributedErrorCode::ErrNotFound,
                    message: format!("cached peer record not found for peer={peer_id}"),
                    retryable: true,
                }
            })?;
            to_canonical_cbor(record)
        }
        RR_GET_CACHED_DISCOVERY_PEERS => {
            let mut peers: Vec<String> = discovered_peer_records
                .values()
                .filter(|record| {
                    peer_record_template
                        .map(|template| {
                            record.record.world_id == template.world_id
                                && record.record.network_id == template.network_id
                        })
                        .unwrap_or(true)
                })
                .map(|record| record.record.peer_id.clone())
                .collect();
            peers.sort();
            peers.dedup();
            peers.truncate(MAX_DISCOVERY_IDS);
            to_canonical_cbor(&peers)
        }
        _ => {
            if let Some(handler) = handlers.get(&request.protocol) {
                if let Some(admission) = handler.admission.as_ref() {
                    admission(&request.payload)?;
                }
                let context = oasis7_proto::distributed_net::NetworkRequestContext::new(
                    std::time::Instant::now() + std::time::Duration::from_secs(30),
                    Arc::new(std::sync::atomic::AtomicBool::new(false)),
                );
                (handler.handler)(&context, &request.payload)
            } else {
                Err(WorldError::NetworkProtocolUnavailable {
                    protocol: request.protocol.clone(),
                })
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn handle_peer_record_response(
    swarm: &mut Swarm<Behaviour>,
    kind: PendingPeerRecordRequest,
    payload: &[u8],
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    discovered_peer_records: &mut PeerRecordCache,
    discovery_observations: &mut DiscoveryObservations,
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    connected_peers: &[PeerId],
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    failed_transport_path_labels: &mut HashSet<String>,
    pending_discovery_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    peer_record_template: Option<&PeerRecord>,
    local_peer_id: PeerId,
    pending_connected_peer_records: &mut HashSet<PeerId>,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    pending_cached_discovery_peers: &mut HashSet<PeerId>,
    max_error_messages: usize,
    event_errors: &Arc<Mutex<Vec<String>>>,
    peer_manager_policy: &PeerManagerPolicy,
    static_bootstrap_peer_ids: &HashSet<PeerId>,
) {
    clear_pending_peer_record_request(
        &kind,
        pending_connected_peer_records,
        pending_cached_peer_records,
        pending_cached_discovery_peers,
    );
    let requested_peer_id = match &kind {
        PendingPeerRecordRequest::ConnectedPeerRecord { peer_id, .. }
        | PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id } => *peer_id,
        PendingPeerRecordRequest::CachedPeerRecord { peer_id, .. } => *peer_id,
    };
    let response_via_peer_id = match &kind {
        PendingPeerRecordRequest::ConnectedPeerRecord { .. } => None,
        PendingPeerRecordRequest::CachedPeerRecord { ask_peer, .. }
        | PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id: ask_peer } => Some(*ask_peer),
    };
    if let PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id } = &kind {
        match decode_cached_discovery_peers_response(payload) {
            Ok(peer_ids) => {
                for discovered_peer_id in peer_ids {
                    maybe_queue_discovery_peer_record(
                        swarm,
                        pending_dht,
                        pending_discovery_peer_records,
                        discovered_peer_records,
                        discovered_peer_id,
                        local_peer_id,
                        peer_record_world_id(peer_record_template),
                    );
                    let _ = request_cached_peer_record_via(
                        swarm,
                        pending_peer_record_requests,
                        pending_cached_peer_records,
                        cached_peer_record_cooldowns,
                        traffic_metrics,
                        *peer_id,
                        discovered_peer_id,
                        local_peer_id,
                        Vec::new(),
                    );
                }
            }
            Err(err) => {
                push_bounded_clone(
                    event_errors,
                    format!(
                        "libp2p cached discovery peers decode failed peer={requested_peer_id}{}: {err:?}",
                        response_via_peer_id
                            .map(|peer_id| format!(" via={peer_id}"))
                            .unwrap_or_default()
                    ),
                    max_error_messages,
                    "lock errors",
                );
            }
        }
        return;
    }
    match decode_optional_peer_record_response(payload) {
        Ok(Some(record)) => {
            if !peer_record_matches_target(&record, requested_peer_id) {
                push_bounded_clone(
                    event_errors,
                    format!(
                        "libp2p peer record response target mismatch requested={requested_peer_id} actual={}",
                        record.record.peer_id
                    ),
                    max_error_messages,
                    "lock errors",
                );
            } else if let Err(err) = process_discovered_peer_record(
                swarm,
                discovered_peer_records,
                discovery_observations,
                known_transport_paths,
                last_dialed_transport_paths,
                active_transport_paths,
                failed_transport_path_labels,
                peer_record_template,
                peer_manager_policy,
                static_bootstrap_peer_ids,
                record.clone(),
                match &kind {
                    PendingPeerRecordRequest::ConnectedPeerRecord { peer_id, .. } => {
                        DiscoveryIngress::Direct { peer: *peer_id }
                    }
                    PendingPeerRecordRequest::CachedPeerRecord { ask_peer, .. } => {
                        DiscoveryIngress::PeerExchange {
                            upstream: *ask_peer,
                        }
                    }
                    PendingPeerRecordRequest::CachedDiscoveryPeers { .. } => {
                        unreachable!("ID-only response handled above")
                    }
                },
            ) {
                push_bounded_clone(
                    event_errors,
                    format!(
                        "libp2p peer record response rejected peer={requested_peer_id}{}: {err:?}",
                        response_via_peer_id
                            .map(|peer_id| format!(" via={peer_id}"))
                            .unwrap_or_default()
                    ),
                    max_error_messages,
                    "lock errors",
                );
            } else {
                let _ = republish_cached_peer_record(swarm, pending_dht, &record);
            }
        }
        Ok(None) => {
            if let PendingPeerRecordRequest::CachedPeerRecord {
                ask_peer: _,
                peer_id,
                tried_proxies,
            } = kind
                && let Some(next_ask_peer) = select_cached_peer_record_proxy(
                    connected_peers,
                    peer_id,
                    local_peer_id,
                    tried_proxies.as_slice(),
                )
            {
                let _ = request_cached_peer_record_via(
                    swarm,
                    pending_peer_record_requests,
                    pending_cached_peer_records,
                    cached_peer_record_cooldowns,
                    traffic_metrics,
                    next_ask_peer,
                    peer_id,
                    local_peer_id,
                    tried_proxies,
                );
            }
        }
        Err(err) => {
            push_bounded_clone(
                event_errors,
                format!(
                    "libp2p peer record response decode failed peer={requested_peer_id}{}: {err:?}",
                    response_via_peer_id
                        .map(|peer_id| format!(" via={peer_id}"))
                        .unwrap_or_default()
                ),
                max_error_messages,
                "lock errors",
            );
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Discovery state is kept explicit to preserve peer-record and routing event ordering"
)]
pub(super) fn handle_peer_record_outbound_failure(
    swarm: &mut Swarm<Behaviour>,
    kind: PendingPeerRecordRequest,
    pending_peer_record_requests: &mut HashMap<
        request_response::OutboundRequestId,
        PendingPeerRecordRequest,
    >,
    pending_connected_peer_records: &mut HashSet<PeerId>,
    _connected_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    cached_peer_record_cooldowns: &mut HashMap<PeerId, i64>,
    pending_cached_discovery_peers: &mut HashSet<PeerId>,
    _cached_discovery_peer_cooldowns: &mut HashMap<PeerId, i64>,
    connected_peers: &[PeerId],
    traffic_metrics: &SharedLibp2pTrafficMetrics,
    local_peer_id: PeerId,
) {
    clear_pending_peer_record_request(
        &kind,
        pending_connected_peer_records,
        pending_cached_peer_records,
        pending_cached_discovery_peers,
    );
    match kind {
        PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id,
            target_owned_route_expected,
        } => {
            // Preserve cooldown after failure.
            // Failure must not reset the target cooldown.
            if !target_owned_route_expected {
                let _ = maybe_request_cached_peer_record(
                    swarm,
                    pending_peer_record_requests,
                    pending_cached_peer_records,
                    cached_peer_record_cooldowns,
                    traffic_metrics,
                    connected_peers,
                    peer_id,
                    local_peer_id,
                );
            }
        }
        PendingPeerRecordRequest::CachedPeerRecord {
            peer_id,
            tried_proxies,
            ..
        } => {
            // Failure must not reset the target cooldown.
            if let Some(next_ask_peer) = select_cached_peer_record_proxy(
                connected_peers,
                peer_id,
                local_peer_id,
                tried_proxies.as_slice(),
            ) {
                let _ = request_cached_peer_record_via(
                    swarm,
                    pending_peer_record_requests,
                    pending_cached_peer_records,
                    cached_peer_record_cooldowns,
                    traffic_metrics,
                    next_ask_peer,
                    peer_id,
                    local_peer_id,
                    tried_proxies,
                );
            }
        }
        PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id: _ } => {
            // Preserve cooldown after failure.
        }
    }
}

pub(super) fn clear_pending_peer_record_request(
    kind: &PendingPeerRecordRequest,
    pending_connected_peer_records: &mut HashSet<PeerId>,
    pending_cached_peer_records: &mut HashSet<PeerId>,
    pending_cached_discovery_peers: &mut HashSet<PeerId>,
) {
    match kind {
        PendingPeerRecordRequest::ConnectedPeerRecord { peer_id, .. } => {
            pending_connected_peer_records.remove(peer_id);
        }
        PendingPeerRecordRequest::CachedPeerRecord { peer_id, .. } => {
            pending_cached_peer_records.remove(peer_id);
        }
        PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id } => {
            pending_cached_discovery_peers.remove(peer_id);
        }
    }
}

pub(super) fn republish_cached_peer_record(
    swarm: &mut Swarm<Behaviour>,
    pending_dht: &mut HashMap<kad::QueryId, PendingDhtQuery>,
    record: &SignedPeerRecord,
) -> Result<(), WorldError> {
    let key = dht_peer_record_key(
        record.record.world_id.as_str(),
        record.record.peer_id.as_str(),
    );
    let payload = to_canonical_cbor(record)?;
    if !swarm.behaviour_mut().discovery_budget.admit() {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "discovery work budget exhausted".into(),
        });
    }
    let query_id = match put_record_query(swarm, key, payload) {
        Ok(id) => id,
        Err(error) => {
            swarm.behaviour_mut().discovery_budget.finish_unscheduled();
            return Err(error);
        }
    };
    swarm.behaviour_mut().discovery_budget.track_dht(query_id);
    pending_dht.insert(query_id, PendingDhtQuery::PutPeerRecord { response: None });
    Ok(())
}

pub(super) fn decode_optional_peer_record_response(
    payload: &[u8],
) -> Result<Option<SignedPeerRecord>, WorldError> {
    if payload.len() > super::peer_record::MAX_SIGNED_PEER_RECORD_BYTES {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "peer record payload capacity exceeded".into(),
        });
    }
    if let Ok(error) = serde_cbor::from_slice::<ErrorResponse>(payload) {
        if error.code == DistributedErrorCode::ErrNotFound {
            return Ok(None);
        }
        return Err(WorldError::NetworkRequestFailed {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
        });
    }
    Ok(Some(serde_cbor::from_slice(payload)?))
}

pub(super) fn decode_cached_discovery_peers_response(
    payload: &[u8],
) -> Result<Vec<PeerId>, WorldError> {
    if payload.len() > MAX_DISCOVERY_LIST_BYTES {
        return Err(WorldError::NetworkProtocolUnavailable {
            protocol: "cached discovery list byte limit exceeded".into(),
        });
    }
    let peer_ids = super::discovery_list::decode(payload)?;
    let mut decoded = Vec::with_capacity(peer_ids.len());
    for peer_id in peer_ids {
        let peer_id =
            peer_id
                .parse::<PeerId>()
                .map_err(|_| WorldError::NetworkProtocolUnavailable {
                    protocol: "cached discovery peer_id must be valid".to_string(),
                })?;
        if !decoded.contains(&peer_id) {
            decoded.push(peer_id);
        }
    }
    Ok(decoded)
}

pub(super) fn rendezvous_namespace(
    template: &PeerRecord,
) -> Result<rendezvous::Namespace, WorldError> {
    rendezvous::Namespace::new(distributed_rendezvous_namespace(
        template.world_id.as_str(),
        template.network_id.as_str(),
    ))
    .map_err(|_| WorldError::NetworkProtocolUnavailable {
        protocol: "rendezvous namespace must be <= 255 bytes".to_string(),
    })
}

#[cfg(test)]
#[path = "discovery_budget_tests.rs"]
mod budget_tests;
