use super::durable_peer_record_publisher::LocalPeerRecordPublisher;
use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_record_cache::PeerRecordCache;
use oasis7_proto::network_failure::{FailureCause, FailureOrigin, NetworkFailure};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::gossipsub::{IdentTopic, TopicHash};
use libp2p::kad::{self, Quorum, RecordKey};
use libp2p::request_response;
use libp2p::swarm::Swarm;
use libp2p::{Multiaddr, PeerId};
use oasis7_proto::distributed_net::NetworkRequestContext;

use super::peer_manager::{
    peer_health_is_record_exchange_pending, peer_health_issue_is_record_exchange_pending,
    recompute_peer_manager_healths,
};
use super::peer_manager_active_set::{
    ActivePeerCandidate, ActivePeerSetStats, candidate_transport_admission_status,
    candidate_would_degrade_admitted_peers,
};
use super::traffic_metrics::{record_gossip_outbound, record_request_outbound};
use super::{
    Behaviour, DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES, Keypair, Libp2pReachabilitySnapshot,
    MembershipDirectorySnapshot, NetworkMessage, NetworkRequest, PeerManagerBlockArtifact,
    PeerManagerHealthStatus, PeerManagerPeerHealth, PeerManagerPolicy, PeerRecord, PendingDhtQuery,
    PendingPeerRecordRequest, ProviderRecord, SignedPeerRecord, TransportPath, WorldError,
    WorldHeadAnnounce, classify_network_protocol, classify_network_topic,
    maybe_discover_rendezvous_namespace, maybe_register_rendezvous_namespace,
    maybe_request_cached_discovery_peers, now_ms, publish_configured_peer_record,
    publish_discovery_provider, push_bounded_clone, put_record_query, should_republish,
    start_peer_discovery_query,
};

pub(super) type CommandResponseSender<T> = std::sync::mpsc::Sender<Result<T, WorldError>>;
pub(super) type Handler =
    Arc<dyn Fn(&NetworkRequestContext, &[u8]) -> Result<Vec<u8>, WorldError> + Send + Sync>;
pub(super) type Admission = Arc<dyn Fn(&[u8]) -> Result<(), WorldError> + Send + Sync>;

pub(super) struct HandlerRegistration {
    pub handler: Handler,
    pub admission: Option<Admission>,
    pub authenticated: Option<Arc<super::authenticated_requests::Registration>>,
}

pub(super) enum CommandOutcome {
    Continue,
    Break,
}

pub(super) struct PendingResponse {
    pub protocol: String,
    pub response: CommandResponseSender<Vec<u8>>,
}

pub(super) struct CommandContext<'a> {
    pub event_published: &'a Arc<Mutex<Vec<NetworkMessage>>>,
    pub event_errors: &'a Arc<Mutex<Vec<String>>>,
    pub event_listening_addrs: &'a Arc<Mutex<Vec<Multiaddr>>>,
    pub event_reachability: &'a Arc<Mutex<Libp2pReachabilitySnapshot>>,
    pub event_traffic_metrics: &'a super::SharedLibp2pTrafficMetrics,
    pub keypair: &'a Keypair,
    pub peer_record_template: Option<&'a PeerRecord>,
    pub local_peer_record_publisher: &'a Arc<Mutex<LocalPeerRecordPublisher>>,
    pub local_peer_id: PeerId,
    pub max_published_messages: usize,
    pub max_error_messages: usize,
    pub republish_interval_ms: i64,
    pub discovery_query_cooldown_ms: i64,
    pub allow_loopback_external_addrs_for_testing: bool,
}

pub(super) enum Command {
    Publish {
        topic: String,
        payload: Vec<u8>,
        response: Option<CommandResponseSender<()>>,
    },
    Subscribe {
        topic: String,
        response: CommandResponseSender<()>,
    },
    Dial(Multiaddr),
    Request {
        protocol: String,
        payload: Vec<u8>,
        providers: Vec<String>,
        response: CommandResponseSender<Vec<u8>>,
    },
    RequestToPeer {
        protocol: String,
        payload: Vec<u8>,
        peer: PeerId,
        response: CommandResponseSender<Vec<u8>>,
    },
    RegisterAuthenticatedHandler {
        protocol: String,
        registration: super::authenticated_requests::Registration,
        response: CommandResponseSender<()>,
    },
    RegisterHandler {
        protocol: String,
        handler: Handler,
        admission: Option<Admission>,
        response: CommandResponseSender<()>,
    },
    PublishProvider(String, Option<CommandResponseSender<()>>),
    GetProviders(String, CommandResponseSender<Vec<ProviderRecord>>),
    PutWorldHead {
        key: String,
        payload: Vec<u8>,
        response: CommandResponseSender<()>,
    },
    GetWorldHead(String, CommandResponseSender<Option<WorldHeadAnnounce>>),
    PutMembershipDirectory {
        key: String,
        payload: Vec<u8>,
        response: CommandResponseSender<()>,
    },
    GetMembershipDirectory {
        key: String,
        response: CommandResponseSender<Option<MembershipDirectorySnapshot>>,
    },
    PutPeerRecord {
        key: String,
        payload: Vec<u8>,
        response: CommandResponseSender<()>,
    },
    GetPeerRecord {
        key: String,
        peer_id: PeerId,
        response: CommandResponseSender<Option<SignedPeerRecord>>,
    },
    RefreshPeerDiscovery,
    RepublishProviders,
    Shutdown,
}

pub(super) struct CommandStateRefs<'a> {
    pub subscriptions: &'a mut HashSet<String>,
    pub topic_map: &'a mut HashMap<TopicHash, String>,
    pub topic_inbox_limits: &'a mut HashMap<String, usize>,
    pub handlers: &'a mut HashMap<String, HandlerRegistration>,
    pub pending: &'a mut HashMap<request_response::OutboundRequestId, PendingResponse>,
    pub pending_peer_record_requests:
        &'a mut HashMap<request_response::OutboundRequestId, PendingPeerRecordRequest>,
    pub pending_dht: &'a mut HashMap<kad::QueryId, PendingDhtQuery>,
    pub peers: &'a mut Vec<PeerId>,
    pub provider_keys: &'a mut HashMap<String, i64>,
    pub discovered_peer_records: &'a PeerRecordCache,
    pub peer_healths_by_id: &'a HashMap<PeerId, PeerManagerPeerHealth>,
    pub pending_cached_discovery_peers: &'a mut HashSet<PeerId>,
    pub cached_discovery_peer_cooldowns: &'a mut HashMap<PeerId, i64>,
    pub pending_rendezvous_registers: &'a mut HashSet<PeerId>,
    pub pending_rendezvous_discovers: &'a mut HashSet<PeerId>,
    pub registered_rendezvous_nodes: &'a HashSet<PeerId>,
    pub rendezvous_cookies: &'a HashMap<PeerId, libp2p::rendezvous::Cookie>,
    pub peer_record_last_published_at_ms: &'a mut Option<i64>,
    pub peer_discovery_query_last_started_at_ms: &'a mut Option<i64>,
}

pub(super) fn should_backfill_peer_discovery(
    peers: &[PeerId],
    discovered_peer_records: &PeerRecordCache,
    local_peer_id: PeerId,
) -> bool {
    if peers.is_empty() {
        return true;
    }
    let has_remote_discovered_peer = discovered_peer_records
        .keys()
        .any(|peer_id| *peer_id != local_peer_id);
    let missing_connected_peer_record = peers
        .iter()
        .copied()
        .any(|peer_id| peer_id != local_peer_id && !discovered_peer_records.contains_key(&peer_id));
    !has_remote_discovered_peer || missing_connected_peer_record
}

#[cfg(test)]
pub(super) fn dial_target_is_already_connected(peers: &[PeerId], addr: &Multiaddr) -> bool {
    super::swarm_behaviour::split_peer_id(addr.clone())
        .0
        .map(|peer_id| peers.contains(&peer_id))
        .unwrap_or(false)
}

fn current_request_peers(
    peers: &[PeerId],
    swarm: &Swarm<Behaviour>,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> Vec<PeerId> {
    let mut current = peers.to_vec();
    current.extend(swarm.connected_peers().copied());
    let current = super::api::dedup_sorted_peers(current);
    if !current.is_empty() {
        return current;
    }
    active_transport_request_peers(peer_healths)
}

pub(super) fn active_transport_request_peers(
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> Vec<PeerId> {
    let peers = peer_healths
        .iter()
        .filter(|(_, health)| health.active_path_kind.is_some())
        .map(|(peer_id, _)| *peer_id)
        .collect::<Vec<_>>();
    filter_request_peers_by_health(super::api::dedup_sorted_peers(peers), peer_healths)
}

fn request_peer_is_connected_or_active(
    peer: PeerId,
    peers: &[PeerId],
    swarm: &Swarm<Behaviour>,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> bool {
    peers.contains(&peer)
        || swarm.is_connected(&peer)
        || active_transport_request_peers(peer_healths).contains(&peer)
}

pub(super) fn filter_request_peers_by_lane(
    peers: Vec<PeerId>,
    protocol: &str,
    discovered_peer_records: &PeerRecordCache,
) -> Vec<PeerId> {
    let Some(lane) = classify_network_protocol(protocol) else {
        return peers;
    };
    let mut capable_record_peers = Vec::new();
    let mut unknown_record_peers = Vec::new();
    for peer_id in peers.iter().copied() {
        match discovered_peer_records.session_record(&peer_id) {
            Some(record) if record.record.supports_lane(lane) => capable_record_peers.push(peer_id),
            Some(_) => {}
            None => unknown_record_peers.push(peer_id),
        }
    }
    if !capable_record_peers.is_empty() {
        capable_record_peers
    } else if !unknown_record_peers.is_empty() {
        unknown_record_peers
    } else {
        Vec::new()
    }
}

pub(super) fn filter_request_peers_by_health(
    peers: Vec<PeerId>,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> Vec<PeerId> {
    let mut ranked: Vec<(usize, PeerId)> = peers
        .iter()
        .copied()
        .enumerate()
        .map(|(idx, peer_id)| {
            let rank: usize = match peer_healths.get(&peer_id).map(|health| health.status) {
                Some(PeerManagerHealthStatus::Active) => 0,
                Some(PeerManagerHealthStatus::Candidate) | None => 1,
                Some(PeerManagerHealthStatus::Suspect) => 2,
                Some(PeerManagerHealthStatus::Blocked) => 3,
            };
            (idx, (rank, peer_id))
        })
        .map(|(idx, (rank, peer_id))| (rank.saturating_mul(10_000).saturating_add(idx), peer_id))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .iter()
        .filter(|(_, peer_id)| {
            !peer_healths
                .get(peer_id)
                .is_some_and(peer_health_is_hard_request_blocked)
        })
        .map(|(_, peer_id)| *peer_id)
        .collect()
}

fn peer_health_is_hard_request_blocked(health: &PeerManagerPeerHealth) -> bool {
    matches!(health.status, PeerManagerHealthStatus::Blocked)
        && (health.issues.is_empty()
            || !health
                .issues
                .iter()
                .all(peer_health_issue_is_record_exchange_pending))
}

pub(super) fn peer_requires_active_quarantine(
    peer_id: PeerId,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> bool {
    let Some(health) = peer_healths.get(&peer_id) else {
        return false;
    };
    match health.status {
        PeerManagerHealthStatus::Suspect => !peer_health_is_record_exchange_pending(health),
        PeerManagerHealthStatus::Blocked => !health
            .issues
            .iter()
            .all(peer_health_issue_is_record_exchange_pending),
        PeerManagerHealthStatus::Active | PeerManagerHealthStatus::Candidate => false,
    }
}

#[cfg(test)]
pub(super) fn collect_quarantined_active_peers(
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> Vec<PeerId> {
    active_transport_paths
        .keys()
        .copied()
        .filter(|peer_id| peer_requires_active_quarantine(*peer_id, peer_healths))
        .collect()
}

#[cfg(test)]
pub(super) fn admitted_active_transport_paths(
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    peer_healths: &HashMap<PeerId, PeerManagerPeerHealth>,
) -> HashMap<PeerId, TransportPath> {
    active_transport_paths
        .iter()
        .filter(|(peer_id, _)| {
            matches!(
                peer_healths.get(*peer_id).map(|health| health.status),
                Some(PeerManagerHealthStatus::Active)
            )
        })
        .map(|(peer_id, path)| (*peer_id, path.clone()))
        .collect()
}

fn active_transport_paths_for_peers(
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    peer_ids: &HashSet<PeerId>,
) -> HashMap<PeerId, TransportPath> {
    active_transport_paths
        .iter()
        .filter(|(peer_id, _)| peer_ids.contains(peer_id))
        .map(|(peer_id, path)| (*peer_id, path.clone()))
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "Peer-manager refresh keeps mutable state references explicit to preserve quarantine ordering"
)]
pub(super) fn refresh_peer_manager_healths(
    discovered_peer_records: &PeerRecordCache,
    discovery_observations: &DiscoveryObservations,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    previously_admitted_active_peers: &HashSet<PeerId>,
    peer_manager_policy: &PeerManagerPolicy,
    event_peer_healths: &Arc<Mutex<HashMap<String, PeerManagerPeerHealth>>>,
    event_block_artifacts: &Arc<Mutex<HashMap<String, PeerManagerBlockArtifact>>>,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
) -> (
    HashMap<PeerId, PeerManagerPeerHealth>,
    HashSet<PeerId>,
    HashSet<PeerId>,
) {
    let raw_healths = recompute_peer_manager_healths(
        discovered_peer_records,
        discovery_observations,
        active_transport_paths,
        peer_manager_policy,
    );
    let mut quarantined_active_peers = HashSet::new();
    let mut admitted_active_peers: HashSet<PeerId> = previously_admitted_active_peers
        .iter()
        .copied()
        .filter(|peer_id| active_transport_paths.contains_key(peer_id))
        .collect();
    let admitted_baseline_paths =
        active_transport_paths_for_peers(active_transport_paths, &admitted_active_peers);
    let admitted_baseline_healths = recompute_peer_manager_healths(
        discovered_peer_records,
        discovery_observations,
        &admitted_baseline_paths,
        peer_manager_policy,
    );
    for peer_id in admitted_active_peers.clone() {
        if peer_requires_active_quarantine(peer_id, &admitted_baseline_healths) {
            admitted_active_peers.remove(&peer_id);
            quarantined_active_peers.insert(peer_id);
        }
    }
    let mut admitted_paths =
        active_transport_paths_for_peers(active_transport_paths, &admitted_active_peers);
    let mut admitted_active_set_stats = ActivePeerSetStats::new(
        discovered_peer_records,
        discovery_observations,
        &admitted_paths,
    );
    let mut pending_active_peers: Vec<PeerId> = active_transport_paths
        .keys()
        .copied()
        .filter(|peer_id| !admitted_active_peers.contains(peer_id))
        .collect();
    pending_active_peers.sort_unstable();
    for peer_id in pending_active_peers {
        if quarantined_active_peers.contains(&peer_id) {
            continue;
        }
        let Some(path) = active_transport_paths.get(&peer_id) else {
            continue;
        };
        let record = discovered_peer_records.get(&peer_id);
        // A connected identity without a record still consumes transport and
        // unknown-domain budgets; missing metadata is not an admission exemption.
        let candidate = match record {
            Some(record) => {
                ActivePeerCandidate::from_record_and_path(record, discovery_observations, path)
            }
            None => ActivePeerCandidate::from_unverified_path(path),
        };
        let candidate_status = candidate_transport_admission_status(
            &candidate,
            &admitted_active_set_stats,
            peer_manager_policy,
        );
        let peer_is_active = matches!(candidate_status, PeerManagerHealthStatus::Active);
        let degrades_admitted_peer = candidate_would_degrade_admitted_peers(
            &candidate,
            &admitted_active_set_stats,
            peer_manager_policy,
        );
        if peer_is_active && !degrades_admitted_peer {
            admitted_paths.insert(peer_id, path.clone());
            admitted_active_peers.insert(peer_id);
            if let Some(record) = record {
                admitted_active_set_stats.add_admitted_peer(record, discovery_observations, path);
            } else {
                admitted_active_set_stats.add_unverified_peer(path);
            }
        } else if matches!(
            candidate_status,
            PeerManagerHealthStatus::Suspect | PeerManagerHealthStatus::Blocked
        ) {
            quarantined_active_peers.insert(peer_id);
        }
    }
    let mut healths = if admitted_paths.len() == active_transport_paths.len() {
        raw_healths.clone()
    } else {
        recompute_peer_manager_healths(
            discovered_peer_records,
            discovery_observations,
            &admitted_paths,
            peer_manager_policy,
        )
    };
    for peer_id in active_transport_paths.keys().copied() {
        if admitted_paths.contains_key(&peer_id) {
            continue;
        }
        if let Some(raw_health) = raw_healths.get(&peer_id) {
            healths.insert(peer_id, raw_health.clone());
        }
    }
    {
        let mut guard = event_peer_healths.lock().expect("lock peer healths");
        let previous = guard.clone();
        let latest: HashMap<String, PeerManagerPeerHealth> = healths
            .values()
            .cloned()
            .map(|health| (health.peer_id.clone(), health))
            .collect();
        for (peer_id, health) in &latest {
            let old = previous.get(peer_id);
            let transitioned_to_suspect = matches!(
                old.map(|entry| entry.status),
                None | Some(PeerManagerHealthStatus::Active | PeerManagerHealthStatus::Candidate)
            ) && matches!(
                health.status,
                PeerManagerHealthStatus::Suspect | PeerManagerHealthStatus::Blocked
            );
            if transitioned_to_suspect {
                push_bounded_clone(
                    event_errors,
                    format!(
                        "libp2p peer manager suspect peer={} issues={:?}",
                        peer_id, health.issues
                    ),
                    max_error_messages,
                    "lock errors",
                );
            }
        }
        *guard = latest;
    }
    {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        let mut guard = event_block_artifacts
            .lock()
            .expect("lock peer block artifacts");
        for health in healths.values() {
            let existing = guard.get_mut(&health.peer_id);
            match (existing, health.status) {
                (Some(artifact), PeerManagerHealthStatus::Blocked) => {
                    artifact.status = health.status;
                    artifact.issues = health.issues.clone();
                    artifact.active_path_kind = health.active_path_kind.clone();
                    artifact.source_operator = health.source_operator.clone();
                    artifact.source_asn = health.source_asn.clone();
                    artifact.last_blocked_at_ms = now_ms;
                    artifact.last_cleared_at_ms = None;
                }
                (Some(artifact), _) => {
                    artifact.status = health.status;
                    artifact.issues = health.issues.clone();
                    artifact.active_path_kind = health.active_path_kind.clone();
                    artifact.source_operator = health.source_operator.clone();
                    artifact.source_asn = health.source_asn.clone();
                    if artifact.last_cleared_at_ms.is_none() {
                        artifact.last_cleared_at_ms = Some(now_ms);
                    }
                }
                (None, PeerManagerHealthStatus::Blocked) => {
                    guard.insert(
                        health.peer_id.clone(),
                        PeerManagerBlockArtifact {
                            peer_id: health.peer_id.clone(),
                            status: health.status,
                            issues: health.issues.clone(),
                            active_path_kind: health.active_path_kind.clone(),
                            source_operator: health.source_operator.clone(),
                            source_asn: health.source_asn.clone(),
                            first_blocked_at_ms: now_ms,
                            last_blocked_at_ms: now_ms,
                            last_cleared_at_ms: None,
                        },
                    );
                }
                (None, _) => {}
            }
        }
    }
    (healths, quarantined_active_peers, admitted_active_peers)
}

pub(super) fn enforce_peer_manager_quarantine(
    swarm: &mut Swarm<Behaviour>,
    quarantined_active_peers: &HashSet<PeerId>,
    pending_quarantine_disconnects: &mut HashSet<PeerId>,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
) {
    pending_quarantine_disconnects.retain(|peer_id| quarantined_active_peers.contains(peer_id));
    for peer_id in quarantined_active_peers.iter().copied() {
        if !pending_quarantine_disconnects.insert(peer_id) {
            continue;
        }
        if swarm.disconnect_peer_id(peer_id).is_ok() {
            push_bounded_clone(
                event_errors,
                format!("libp2p peer manager quarantine disconnect peer={peer_id}"),
                max_error_messages,
                "lock errors",
            );
        } else {
            pending_quarantine_disconnects.remove(&peer_id);
        }
    }
}

pub(super) fn handle_command(
    swarm: &mut Swarm<Behaviour>,
    command: Option<Command>,
    state: CommandStateRefs<'_>,
    ctx: &CommandContext<'_>,
) -> CommandOutcome {
    let CommandStateRefs {
        subscriptions,
        topic_map,
        topic_inbox_limits,
        handlers,
        pending,
        pending_peer_record_requests,
        pending_dht,
        peers,
        provider_keys,
        discovered_peer_records,
        peer_healths_by_id,
        pending_cached_discovery_peers,
        cached_discovery_peer_cooldowns,
        pending_rendezvous_registers,
        pending_rendezvous_discovers,
        registered_rendezvous_nodes,
        rendezvous_cookies,
        peer_record_last_published_at_ms,
        peer_discovery_query_last_started_at_ms,
    } = state;

    match command {
        Some(Command::Publish {
            topic,
            payload,
            response,
        }) => {
            let payload_len = payload.len();
            let message = NetworkMessage {
                topic: topic.clone(),
                payload: payload.clone(),
            };
            let topic_handle = IdentTopic::new(&topic);
            match swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic_handle, payload)
            {
                Ok(_) => {
                    push_bounded_clone(
                        ctx.event_published,
                        message,
                        ctx.max_published_messages,
                        "lock published",
                    );
                    record_gossip_outbound(ctx.event_traffic_metrics, topic.as_str(), payload_len);
                    if let Some(response) = response {
                        let _ = response.send(Ok(()));
                    }
                }
                Err(err) => {
                    let error = WorldError::NetworkProtocolUnavailable {
                        protocol: format!("libp2p publish failed topic={topic}: {err}"),
                    }
                    .with_network_failure(NetworkFailure::observed(
                        FailureOrigin::Publish,
                        FailureCause::Unknown,
                        None,
                    ));
                    push_bounded_clone(
                        ctx.event_errors,
                        format!("libp2p publish failed topic={topic}: {err}"),
                        ctx.max_error_messages,
                        "lock errors",
                    );
                    if let Some(response) = response {
                        let _ = response.send(Err(error));
                    }
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::Subscribe { topic, response }) => {
            if subscriptions.insert(topic.clone()) {
                let topic_handle = IdentTopic::new(topic.clone());
                match swarm.behaviour_mut().gossipsub.subscribe(&topic_handle) {
                    Ok(_) => {
                        let inbox_limit = classify_network_topic(topic.as_str())
                            .map(|lane| lane.default_subscription_inbox_messages())
                            .unwrap_or(DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES);
                        topic_inbox_limits.insert(topic.clone(), inbox_limit);
                        topic_map.insert(topic_handle.hash(), topic);
                        let _ = response.send(Ok(()));
                    }
                    Err(err) => {
                        subscriptions.remove(&topic);
                        let error = WorldError::NetworkProtocolUnavailable {
                            protocol: format!("libp2p subscribe failed topic={topic}: {err}"),
                        };
                        push_bounded_clone(
                            ctx.event_errors,
                            format!("libp2p subscribe failed topic={topic}: {err}"),
                            ctx.max_error_messages,
                            "lock errors",
                        );
                        let _ = response.send(Err(error));
                    }
                }
            } else {
                let _ = response.send(Ok(()));
            }
            CommandOutcome::Continue
        }
        Some(Command::Dial(addr)) => {
            if let (Some(peer_id), _) = super::swarm_behaviour::split_peer_id(addr.clone())
                && (peers.contains(&peer_id) || swarm.is_connected(&peer_id))
            {
                return CommandOutcome::Continue;
            }
            if let Err(err) = super::dial_addr_with_optional_peer_id(swarm, addr) {
                push_bounded_clone(
                    ctx.event_errors,
                    format!("libp2p dial failed: {err}"),
                    ctx.max_error_messages,
                    "lock errors",
                );
            }
            CommandOutcome::Continue
        }
        Some(Command::Request {
            protocol,
            payload,
            providers,
            response,
        }) => {
            let connected_request_peers =
                current_request_peers(peers.as_slice(), swarm, peer_healths_by_id);
            if connected_request_peers.is_empty() {
                if providers.is_empty() {
                    if let Some(handler) = handlers.get(&protocol) {
                        let invoke = || {
                            let context = NetworkRequestContext::new(
                                std::time::Instant::now() + std::time::Duration::from_secs(30),
                                Arc::new(std::sync::atomic::AtomicBool::new(false)),
                            );
                            (handler.handler)(&context, &payload)
                        };
                        let result = if let Some(admission) = handler.admission.as_ref() {
                            admission(&payload).and_then(|()| invoke())
                        } else {
                            invoke()
                        };
                        let _ = response.send(result);
                    } else {
                        let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                            protocol: protocol.clone(),
                        }
                        .with_network_failure(NetworkFailure::observed(
                            FailureOrigin::LocalTransport,
                            FailureCause::MissingHandler,
                            Some(&protocol),
                        ))));
                    }
                } else {
                    let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                        protocol: format!("no connected providers for protocol {protocol}"),
                    }
                    .with_network_failure(NetworkFailure::observed(
                        FailureOrigin::ProviderRouting,
                        FailureCause::ConnectionGap,
                        Some(&protocol),
                    ))));
                }
                return CommandOutcome::Continue;
            }
            let using_provider_subset = !providers.is_empty();
            let mut candidate_peers: Vec<PeerId> = Vec::new();
            if using_provider_subset {
                for provider in providers {
                    if let Ok(peer_id) = provider.parse::<PeerId>()
                        && connected_request_peers.contains(&peer_id)
                    {
                        candidate_peers.push(peer_id);
                    }
                }
                if candidate_peers.is_empty() {
                    let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                        protocol: format!("no connected providers for protocol {protocol}"),
                    }
                    .with_network_failure(NetworkFailure::observed(
                        FailureOrigin::ProviderRouting,
                        FailureCause::ConnectionGap,
                        Some(&protocol),
                    ))));
                    return CommandOutcome::Continue;
                }
            } else {
                candidate_peers = connected_request_peers;
            }
            candidate_peers = filter_request_peers_by_health(candidate_peers, peer_healths_by_id);
            candidate_peers = filter_request_peers_by_lane(
                candidate_peers,
                protocol.as_str(),
                discovered_peer_records,
            );
            if candidate_peers.is_empty() {
                let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                    protocol: if using_provider_subset {
                        format!("no healthy connected providers for protocol {protocol}")
                    } else {
                        format!("no healthy provider for protocol {protocol}")
                    },
                }
                .with_network_failure(NetworkFailure::observed(
                    FailureOrigin::ProviderRouting,
                    FailureCause::ConnectionGap,
                    Some(&protocol),
                ))));
                return CommandOutcome::Continue;
            }
            let peer = candidate_peers[0];
            let payload_len = payload.len();
            let pending_protocol = protocol.clone();
            record_request_outbound(ctx.event_traffic_metrics, protocol.as_str(), payload_len);
            let request = NetworkRequest { protocol, payload };
            let request_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer, request);
            pending.insert(
                request_id,
                PendingResponse {
                    protocol: pending_protocol,
                    response,
                },
            );
            CommandOutcome::Continue
        }
        Some(Command::RequestToPeer {
            protocol,
            payload,
            peer,
            response,
        }) => {
            if !request_peer_is_connected_or_active(
                peer,
                peers.as_slice(),
                swarm,
                peer_healths_by_id,
            ) {
                let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                    protocol: format!("peer {peer} is not connected for protocol {protocol}"),
                }
                .with_network_failure(NetworkFailure::observed(
                    FailureOrigin::LocalTransport,
                    FailureCause::ConnectionGap,
                    Some(&protocol),
                ))));
                return CommandOutcome::Continue;
            }
            let payload_len = payload.len();
            let pending_protocol = protocol.clone();
            record_request_outbound(ctx.event_traffic_metrics, protocol.as_str(), payload_len);
            let request_id = swarm
                .behaviour_mut()
                .request_response
                .send_request(&peer, NetworkRequest { protocol, payload });
            pending.insert(
                request_id,
                PendingResponse {
                    protocol: pending_protocol,
                    response,
                },
            );
            CommandOutcome::Continue
        }
        Some(Command::RegisterAuthenticatedHandler {
            protocol,
            registration,
            response,
        }) => {
            let result = super::authenticated_requests::install(handlers, protocol, registration);
            let _ = response.send(result);
            CommandOutcome::Continue
        }
        Some(Command::RegisterHandler {
            protocol,
            handler,
            admission,
            response,
        }) => {
            if super::authenticated_requests::reserved(&protocol) {
                let _ = response.send(Err(super::authenticated_requests::rejected(
                    "reserved authenticated namespace",
                )));
                return CommandOutcome::Continue;
            }
            handlers.insert(
                protocol,
                HandlerRegistration {
                    handler,
                    admission,
                    authenticated: None,
                },
            );
            let _ = response.send(Ok(()));
            CommandOutcome::Continue
        }
        Some(Command::PublishProvider(key, response)) => {
            let dht_key = RecordKey::new(&key);
            match swarm.behaviour_mut().kademlia.start_providing(dht_key) {
                Ok(query_id) => {
                    provider_keys.insert(key, now_ms());
                    pending_dht.insert(query_id, PendingDhtQuery::PublishProvider { response });
                }
                Err(err) => {
                    if let Some(response) = response {
                        let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                            protocol: format!("kad start_providing failed: {err}"),
                        }));
                    }
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::GetProviders(key, response)) => {
            let dht_key = RecordKey::new(&key);
            let query_id = swarm.behaviour_mut().kademlia.get_providers(dht_key);
            pending_dht.insert(
                query_id,
                PendingDhtQuery::GetProviders {
                    response: Some(response),
                    providers: HashSet::new(),
                    error: None,
                },
            );
            CommandOutcome::Continue
        }
        Some(Command::PutWorldHead {
            key,
            payload,
            response,
        }) => {
            let dht_key = RecordKey::new(&key);
            let record = kad::Record {
                key: dht_key,
                value: payload,
                publisher: None,
                expires: None,
            };
            match swarm
                .behaviour_mut()
                .kademlia
                .put_record(record, Quorum::One)
            {
                Ok(query_id) => {
                    pending_dht.insert(
                        query_id,
                        PendingDhtQuery::PutWorldHead {
                            response: Some(response),
                        },
                    );
                }
                Err(err) => {
                    let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                        protocol: format!("kad put_record failed: {err}"),
                    }));
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::GetWorldHead(key, response)) => {
            let dht_key = RecordKey::new(&key);
            let query_id = swarm.behaviour_mut().kademlia.get_record(dht_key);
            pending_dht.insert(
                query_id,
                PendingDhtQuery::GetWorldHead {
                    response: Some(response),
                    head: None,
                    error: None,
                },
            );
            CommandOutcome::Continue
        }
        Some(Command::PutMembershipDirectory {
            key,
            payload,
            response,
        }) => {
            let dht_key = RecordKey::new(&key);
            let record = kad::Record {
                key: dht_key,
                value: payload,
                publisher: None,
                expires: None,
            };
            match swarm
                .behaviour_mut()
                .kademlia
                .put_record(record, Quorum::One)
            {
                Ok(query_id) => {
                    pending_dht.insert(
                        query_id,
                        PendingDhtQuery::PutMembershipDirectory {
                            response: Some(response),
                        },
                    );
                }
                Err(err) => {
                    let _ = response.send(Err(WorldError::NetworkProtocolUnavailable {
                        protocol: format!("kad put_record failed: {err}"),
                    }));
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::GetMembershipDirectory { key, response }) => {
            let dht_key = RecordKey::new(&key);
            let query_id = swarm.behaviour_mut().kademlia.get_record(dht_key);
            pending_dht.insert(
                query_id,
                PendingDhtQuery::GetMembershipDirectory {
                    response: Some(response),
                    snapshot: None,
                    error: None,
                },
            );
            CommandOutcome::Continue
        }
        Some(Command::PutPeerRecord {
            key,
            payload,
            response,
        }) => {
            match put_record_query(swarm, key, payload) {
                Ok(query_id) => {
                    pending_dht.insert(
                        query_id,
                        PendingDhtQuery::PutPeerRecord {
                            response: Some(response),
                        },
                    );
                }
                Err(err) => {
                    let _ = response.send(Err(err));
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::GetPeerRecord {
            key,
            peer_id,
            response,
        }) => {
            let dht_key = RecordKey::new(&key);
            let query_id = swarm.behaviour_mut().kademlia.get_record(dht_key);
            pending_dht.insert(
                query_id,
                PendingDhtQuery::get_peer_record(peer_id, response),
            );
            CommandOutcome::Continue
        }
        Some(Command::RefreshPeerDiscovery) => {
            if let Some(template) = ctx.peer_record_template {
                let now = now_ms();
                let should_refresh_peer_record = peer_record_last_published_at_ms
                    .map(|last_ms| {
                        ctx.republish_interval_ms > 0
                            && should_republish(last_ms, now, ctx.republish_interval_ms)
                    })
                    .unwrap_or(true);
                if should_refresh_peer_record
                    && publish_configured_peer_record(
                        swarm,
                        pending_dht,
                        ctx.keypair,
                        template,
                        ctx.event_listening_addrs,
                        ctx.event_reachability,
                        ctx.allow_loopback_external_addrs_for_testing,
                        ctx.local_peer_record_publisher,
                        None,
                    )
                    .is_ok()
                {
                    *peer_record_last_published_at_ms = Some(now);
                }
                let provider_key =
                    oasis7_proto::distributed::dht_peer_discovery_key(template.world_id.as_str());
                let should_refresh_discovery_provider = provider_keys
                    .get(&provider_key)
                    .copied()
                    .map(|last_ms| {
                        ctx.republish_interval_ms > 0
                            && should_republish(last_ms, now, ctx.republish_interval_ms)
                    })
                    .unwrap_or(true);
                if should_refresh_discovery_provider {
                    publish_discovery_provider(swarm, provider_keys, template.world_id.as_str());
                }
                let connected_peers = peers.clone();
                let should_backfill_discovery = should_backfill_peer_discovery(
                    connected_peers.as_slice(),
                    discovered_peer_records,
                    ctx.local_peer_id,
                );
                if should_backfill_discovery {
                    start_peer_discovery_query(
                        swarm,
                        pending_dht,
                        template,
                        peer_discovery_query_last_started_at_ms,
                        now,
                        ctx.discovery_query_cooldown_ms,
                    );
                }
                for peer_id in connected_peers {
                    if should_backfill_discovery {
                        maybe_request_cached_discovery_peers(
                            swarm,
                            pending_peer_record_requests,
                            pending_cached_discovery_peers,
                            cached_discovery_peer_cooldowns,
                            ctx.event_traffic_metrics,
                            peer_id,
                            ctx.local_peer_id,
                        );
                    }
                    if let Err(err) = maybe_register_rendezvous_namespace(
                        swarm,
                        pending_rendezvous_registers,
                        registered_rendezvous_nodes,
                        peer_id,
                        ctx.local_peer_id,
                        template,
                    ) {
                        push_bounded_clone(
                            ctx.event_errors,
                            format!("libp2p rendezvous register failed peer={peer_id}: {err:?}"),
                            ctx.max_error_messages,
                            "lock errors",
                        );
                    }
                    if let Err(err) = maybe_discover_rendezvous_namespace(
                        swarm,
                        pending_rendezvous_discovers,
                        rendezvous_cookies,
                        peer_id,
                        ctx.local_peer_id,
                        template,
                    ) {
                        push_bounded_clone(
                            ctx.event_errors,
                            format!("libp2p rendezvous discover failed peer={peer_id}: {err:?}"),
                            ctx.max_error_messages,
                            "lock errors",
                        );
                    }
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::RepublishProviders) => {
            if ctx.republish_interval_ms > 0 {
                let now = now_ms();
                let keys: Vec<String> = provider_keys
                    .iter()
                    .filter_map(|(key, last_publish)| {
                        if should_republish(*last_publish, now, ctx.republish_interval_ms) {
                            Some(key.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                for key in keys {
                    let dht_key = RecordKey::new(&key);
                    if swarm
                        .behaviour_mut()
                        .kademlia
                        .start_providing(dht_key)
                        .is_ok()
                    {
                        provider_keys.insert(key, now);
                    }
                }
                if let Some(template) = ctx.peer_record_template
                    && peer_record_last_published_at_ms
                        .map(|last_ms| should_republish(last_ms, now, ctx.republish_interval_ms))
                        .unwrap_or(true)
                    && publish_configured_peer_record(
                        swarm,
                        pending_dht,
                        ctx.keypair,
                        template,
                        ctx.event_listening_addrs,
                        ctx.event_reachability,
                        ctx.allow_loopback_external_addrs_for_testing,
                        ctx.local_peer_record_publisher,
                        None,
                    )
                    .is_ok()
                {
                    publish_discovery_provider(swarm, provider_keys, template.world_id.as_str());
                    *peer_record_last_published_at_ms = Some(now);
                }
            }
            CommandOutcome::Continue
        }
        Some(Command::Shutdown) | None => CommandOutcome::Break,
    }
}
