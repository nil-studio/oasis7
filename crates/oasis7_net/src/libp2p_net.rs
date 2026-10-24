use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
mod api;
pub use api::Libp2pNetwork;
mod config;
mod connection_lifecycle;
mod constructor_support;
mod discovery;
mod discovery_budget;
mod discovery_list;
mod drop_support;
pub(crate) mod error_mapping;
mod inbound_dispatch;
mod kad_queries;
mod peer_discovery_observations;
mod peer_manager;
mod peer_manager_active_set;
mod peer_record;
mod peer_record_cache;
mod peer_record_dial_gate;
mod peer_record_protocol_dial;
mod peer_record_refresh;
mod peer_record_store;
mod peer_record_time;
use peer_record_cache::PeerRecordCache;
mod peer_record_republish;
mod reachability;
mod response_budget;
mod response_workers;
mod runtime_loop;
mod runtime_support;
mod swarm_behaviour;
mod swarm_reachability_events;
mod traffic_metrics;
mod transport_paths;
mod utils;
mod wire_bytes;
use crate::error::WorldError;
pub use config::Libp2pNetworkConfig;
use connection_lifecycle::{
    clear_disconnected_peer_state, failover_after_disconnect, log_active_transport_path,
    prune_redundant_peer_connections, record_established_connection,
    refresh_active_path_after_connection_close, refresh_peer_manager_views,
};
use constructor_support::{
    enqueue_initial_bootstrap_dials, schedule_bootstrap_redial,
    schedule_periodic_discovery_refresh, schedule_periodic_republish,
};
use discovery::{
    PendingPeerRecordRequest, handle_peer_record_outbound_failure, handle_peer_record_response,
    handle_rendezvous_discovered, handle_routing_updated, maybe_discover_rendezvous_namespace,
    maybe_queue_discovery_peer_record, maybe_register_rendezvous_namespace,
    maybe_request_cached_discovery_peers, maybe_request_cached_peer_record,
    maybe_request_connected_peer_record, peer_record_matches_target, peer_record_world_id,
    process_discovered_peer_record, publish_discovery_provider, start_peer_discovery_query,
};
pub use discovery_budget::DiscoveryWorkMetricsSnapshot;
use futures::{FutureExt, StreamExt, channel::mpsc};
use inbound_dispatch::dispatch_inbound_request;
use kad_queries::{DhtProgressAction, PendingDhtQuery, handle_dht_progress};
use libp2p::gossipsub::{self, TopicHash};
use libp2p::identity::Keypair;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId};
use libp2p::{kad, relay, rendezvous, request_response};
use oasis7_proto::distributed::WorldHeadAnnounce;
use oasis7_proto::distributed_dht::{
    MembershipDirectorySnapshot, PeerRecord, ProviderRecord, SignedPeerRecord,
};
use oasis7_proto::distributed_net::{
    DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES, NetworkMessage, NetworkRequest,
    classify_network_protocol, classify_network_topic, push_bounded_inbox_message,
};
pub use peer_manager::{
    PeerManagerBlockArtifact, PeerManagerHealthIssue, PeerManagerHealthStatus,
    PeerManagerPeerHealth, PeerManagerPolicy,
};
use peer_record::{publish_configured_peer_record, put_record_query};
use peer_record_republish::{
    LocalPeerRecordRepublisher, log_external_addr_confirmed_and_republish,
    log_external_addr_expired_and_republish,
};
pub use reachability::{
    Libp2pReachabilitySnapshot, LiveAutoNatStatus, LiveHolePunchState, LivePublicPortReachability,
    LiveTransportKind, LiveTransportTransition, LiveTransportTransitionCounters,
};
use reachability::{note_hole_punch_result, note_relay_reservation_accepted, snapshot_clone};
use response_workers::ResponseWorkers;
use runtime_loop::{
    Command, CommandContext, CommandOutcome, CommandResponseSender, CommandStateRefs, Handler,
    HandlerRegistration, PendingResponse, fail_pending_request, handle_command,
};
#[cfg(test)]
use runtime_loop::{
    admitted_active_transport_paths, collect_quarantined_active_peers,
    filter_request_peers_by_health, filter_request_peers_by_lane,
};
use swarm_behaviour::{Behaviour, BehaviourEvent, build_swarm, dial_addr_with_optional_peer_id};
use swarm_reachability_events::{
    handle_autonat_event, handle_expired_listen_addr, handle_external_addr_candidate,
    handle_listener_closed, handle_new_listen_addr,
};
pub use traffic_metrics::{
    Libp2pControlPlaneMetricsSnapshot, Libp2pTrafficMetricsSnapshot,
    TrafficDirectionMetricsSnapshot, TrafficLaneMetricsSnapshot, WireByteDirectionMetricsSnapshot,
    WireByteLaneMetricsSnapshot,
};
use traffic_metrics::{
    SharedLibp2pTrafficMetrics, classify_control_plane_event, init_shared_traffic_metrics,
    record_control_plane_event, record_gossip_inbound, record_request_inbound,
    record_response_inbound, record_response_outbound, snapshot_traffic_metrics,
};
use transport_paths::{
    TransportPath, bootstrap_transport_path_state, retry_transport_path_after_error,
};
use utils::{
    LIFECYCLE_EVENT_ERROR_COOLDOWN_MS, decode_membership_directory, decode_world_head, now_ms,
    push_bounded_clone, push_bounded_string_with_keyed_cooldown, should_republish,
    try_send_command,
};
use wire_bytes::init_shared_wire_byte_counters;
impl Libp2pNetwork {
    pub fn new(config: Libp2pNetworkConfig) -> Self {
        let keypair = config
            .keypair
            .clone()
            .unwrap_or_else(Keypair::generate_ed25519);
        let peer_id = PeerId::from(keypair.public());
        let inbox = Arc::new(Mutex::new(HashMap::<String, Vec<Vec<u8>>>::new()));
        let published = Arc::new(Mutex::new(Vec::new()));
        let listening_addrs = Arc::new(Mutex::new(Vec::new()));
        let connected_peers = Arc::new(Mutex::new(HashSet::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let peer_healths = Arc::new(Mutex::new(HashMap::<String, PeerManagerPeerHealth>::new()));
        let peer_block_artifacts = Arc::new(Mutex::new(
            HashMap::<String, PeerManagerBlockArtifact>::new(),
        ));
        let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
        let traffic_metrics = init_shared_traffic_metrics();
        let wire_byte_counters = init_shared_wire_byte_counters();
        let command_buffer_capacity = config.command_buffer_capacity.max(1);
        let (command_tx, command_rx) = mpsc::channel(command_buffer_capacity);
        let max_published_messages = config.max_published_messages.max(1);
        let max_error_messages = config.max_error_messages.max(1);
        let max_listening_addrs = config.max_listening_addrs.max(1);
        let event_inbox = Arc::clone(&inbox);
        let event_published = Arc::clone(&published);
        let event_listening_addrs = Arc::clone(&listening_addrs);
        let event_connected_peers = Arc::clone(&connected_peers);
        let event_errors = Arc::clone(&errors);
        let event_peer_healths = Arc::clone(&peer_healths);
        let event_peer_block_artifacts = Arc::clone(&peer_block_artifacts);
        let event_reachability = Arc::clone(&reachability);
        let event_traffic_metrics = Arc::clone(&traffic_metrics);
        let event_wire_byte_counters = Arc::clone(&wire_byte_counters);
        let config_clone = config.clone();
        let keypair_clone = keypair.clone();
        let local_peer_id = peer_id;
        let republish_tx = command_tx.clone();
        let discovery_tx = command_tx.clone();
        let bootstrap_redial_tx = command_tx.clone();
        let bootstrap_peers = config.bootstrap_peers.clone();
        let bootstrap_redial_peers = bootstrap_peers.clone();
        let peer_record_template = config.peer_record.clone();
        let enable_rendezvous = config.rendezvous_enabled();
        std::thread::spawn(move || {
            let runtime = runtime_support::new_current_thread_runtime("build libp2p tokio runtime");
            runtime.block_on(async move {
            let mut swarm = build_swarm(
                &keypair_clone,
                enable_rendezvous,
                config_clone.enable_autonat,
                config_clone.request_response_timeout,
                Arc::clone(&event_wire_byte_counters),
            );
            let mut subscriptions = HashSet::new();
            let mut topic_map: HashMap<TopicHash, String> = HashMap::new();
            let mut topic_inbox_limits: HashMap<String, usize> = HashMap::new();
            let mut handlers: HashMap<String, HandlerRegistration> = HashMap::new();
            let (response_workers, mut completed_response_rx, mut fetch_blob_response_budget) =
                ResponseWorkers::new(config_clone.request_response_timeout);
            let mut pre_crypto_admission_budget =
                inbound_dispatch::PreCryptoAdmissionBudget::default();
            let mut pending: HashMap<request_response::OutboundRequestId, PendingResponse> =
                HashMap::new();
            let mut pending_peer_record_requests: HashMap<
                request_response::OutboundRequestId,
                PendingPeerRecordRequest,
            > = HashMap::new();
            let mut pending_dht: HashMap<kad::QueryId, PendingDhtQuery> = HashMap::new();
            let mut peers: Vec<PeerId> = Vec::new();
            let mut provider_keys: HashMap<String, i64> = HashMap::new();
            let dial_policy = swarm.behaviour().discovery_dial_policy();
            dial_policy.set_configured(&config_clone.bootstrap_peers);
            let mut discovered_peer_records = PeerRecordCache::with_dial_policy(swarm.behaviour_mut().kademlia.store_mut().clock().clone(), dial_policy);
            let (mut known_transport_paths, mut failed_transport_path_labels, static_bootstrap_peer_ids) =
                bootstrap_transport_path_state(&config_clone.bootstrap_peers);
            let mut discovery_observations = peer_discovery_observations::DiscoveryObservations::default();
            discovery_observations.set_static_bootstrap_peers(static_bootstrap_peer_ids.clone());
            let mut last_dialed_transport_paths: HashMap<PeerId, TransportPath> = HashMap::new();
            let mut active_transport_paths: HashMap<PeerId, TransportPath> = HashMap::new();
            let mut established_transport_paths_by_connection: HashMap<
                libp2p::swarm::ConnectionId,
                TransportPath,
            > = HashMap::new();
            let mut established_connections_by_peer: HashMap<
                PeerId,
                HashSet<libp2p::swarm::ConnectionId>,
            > = HashMap::new();
            let mut peer_healths_by_id: HashMap<PeerId, PeerManagerPeerHealth> = HashMap::new();
            let mut quarantined_active_peers: HashSet<PeerId> = HashSet::new();
            let mut admitted_active_peers: HashSet<PeerId> = HashSet::new();
            let mut pending_quarantine_disconnects: HashSet<PeerId> = HashSet::new();
            let mut pending_discovery_peer_records: HashSet<PeerId> = HashSet::new();
            let mut pending_connected_peer_records: HashSet<PeerId> = HashSet::new();
            let mut pending_cached_peer_records: HashSet<PeerId> = HashSet::new();
            let mut pending_cached_discovery_peers: HashSet<PeerId> = HashSet::new();
            let mut connected_peer_record_cooldowns: HashMap<PeerId, i64> = HashMap::new();
            let mut cached_peer_record_cooldowns: HashMap<PeerId, i64> = HashMap::new();
            let mut cached_discovery_peer_cooldowns: HashMap<PeerId, i64> = HashMap::new();
            let mut lifecycle_event_errors_at_ms: HashMap<String, i64> = HashMap::new();
            let mut lifecycle_event_errors_last_prune_at_ms = None;
            let mut pending_rendezvous_registers: HashSet<PeerId> = HashSet::new();
            let mut pending_rendezvous_discovers: HashSet<PeerId> = HashSet::new();
            let mut registered_rendezvous_nodes: HashSet<PeerId> = HashSet::new();
            let mut rendezvous_cookies: HashMap<PeerId, rendezvous::Cookie> = HashMap::new();
            let mut peer_record_last_published_at_ms = None;
            let mut peer_discovery_query_last_started_at_ms = None;
            let republish_interval_ms = config_clone.republish_interval_ms;
            let discovery_query_cooldown_ms = config_clone.discovery_query_cooldown_ms;
            constructor_support::listen_on_configured_addresses(&mut swarm, config_clone.listen_addrs, &event_errors, max_error_messages);
            schedule_periodic_republish(republish_tx, republish_interval_ms);
            if peer_record_template.is_some() {
                schedule_periodic_discovery_refresh(discovery_tx, config_clone.discovery_query_interval_ms);
            }
            schedule_bootstrap_redial(
                bootstrap_redial_tx,
                bootstrap_redial_peers,
                config_clone.bootstrap_redial_interval_ms,
            );
                let mut command_rx = command_rx;
                let command_ctx = CommandContext {
                    event_published: &event_published,
                    event_errors: &event_errors,
                    event_listening_addrs: &event_listening_addrs,
                    event_reachability: &event_reachability,
                    event_traffic_metrics: &event_traffic_metrics,
                    keypair: &keypair_clone,
                    peer_record_template: peer_record_template.as_ref(),
                    local_peer_id,
                    max_published_messages,
                    max_error_messages,
                    republish_interval_ms,
                    discovery_query_cooldown_ms,
                    allow_loopback_external_addrs_for_testing: config_clone
                        .allow_loopback_external_addrs_for_testing,
                };
                let peer_record_republisher = LocalPeerRecordRepublisher::from_context(&command_ctx);
                let mut record_expiry_tick = runtime_support::discovery_expiry_tick();
                macro_rules! republish_local_peer_record {
                    () => {
                        peer_record_republisher.republish(
                            &mut swarm,
                            &mut pending_dht,
                            &mut provider_keys,
                            &mut peer_record_last_published_at_ms,
                        );
                    };
                }
                loop {
                    discovery_budget::publish_metrics(&swarm, &event_traffic_metrics);
                    if discovered_peer_records.prune(&active_transport_paths) {
                        (peer_healths_by_id, quarantined_active_peers, admitted_active_peers) = refresh_peer_manager_views(
                            &mut swarm, &discovered_peer_records, &discovery_observations,
                            &active_transport_paths, &admitted_active_peers, &config_clone.peer_manager_policy,
                            &event_peer_healths, &event_peer_block_artifacts, &mut pending_quarantine_disconnects,
                            &event_errors, max_error_messages, &event_reachability,
                        );
                    }
                    transport_paths::prune_expired_record_paths(&mut known_transport_paths, &mut failed_transport_path_labels, &config_clone.bootstrap_peers);
                    futures::select! {
                        () = record_expiry_tick => {
                            peer_record_refresh::refresh_due_from_context(
                                &mut swarm,
                                peer_record_refresh::RefreshState {
                                    cache: &mut discovered_peer_records,
                                    pending_dht: &mut pending_dht,
                                    pending_records: &mut pending_discovery_peer_records,
                                    pending_requests: &mut pending_peer_record_requests,
                                    pending_connected: &mut pending_connected_peer_records,
                                    connected_cooldowns: &mut connected_peer_record_cooldowns,
                                },
                                &command_ctx, config_clone.discovery_query_interval_ms,
                            );
                            record_expiry_tick = runtime_support::discovery_expiry_tick();
                        },
                        completed = completed_response_rx.next().fuse() => if let Some(completed) = completed {
                            response_workers.complete(completed, &mut fetch_blob_response_budget, now_ms(), &event_traffic_metrics, &mut swarm, &event_errors, max_error_messages);
                        },
                        command = command_rx.next().fuse() => {
                            match handle_command(
                                &mut swarm,
                                command,
                                CommandStateRefs {
                                    subscriptions: &mut subscriptions,
                                    topic_map: &mut topic_map,
                                    topic_inbox_limits: &mut topic_inbox_limits,
                                    handlers: &mut handlers,
                                    pending: &mut pending,
                                    pending_peer_record_requests: &mut pending_peer_record_requests,
                                    pending_dht: &mut pending_dht,
                                    peers: &mut peers,
                                    provider_keys: &mut provider_keys,
                                    discovered_peer_records: &discovered_peer_records,
                                    peer_healths_by_id: &peer_healths_by_id,
                                    pending_cached_discovery_peers: &mut pending_cached_discovery_peers,
                                    cached_discovery_peer_cooldowns: &mut cached_discovery_peer_cooldowns,
                                    pending_rendezvous_registers: &mut pending_rendezvous_registers,
                                    pending_rendezvous_discovers: &mut pending_rendezvous_discovers,
                                    registered_rendezvous_nodes: &registered_rendezvous_nodes,
                                    rendezvous_cookies: &rendezvous_cookies,
                                    peer_record_last_published_at_ms: &mut peer_record_last_published_at_ms,
                                    peer_discovery_query_last_started_at_ms: &mut peer_discovery_query_last_started_at_ms,
                                },
                                &command_ctx,
                            ) {
                                CommandOutcome::Continue => {}
                                CommandOutcome::Break => {
                                    break;
                                }
                            }
                        }
                        event = swarm.select_next_some().fuse() => {
                            if let Some(kind) = classify_control_plane_event(&event) {
                                record_control_plane_event(&event_traffic_metrics, kind);
                            }
                            match event {
                                SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                                    let topic = topic_map
                                        .get(&message.topic)
                                        .cloned()
                                        .unwrap_or_else(|| message.topic.as_str().to_string());
                                    let inbox_limit = topic_inbox_limits
                                        .get(topic.as_str())
                                        .copied()
                                        .or_else(|| {
                                            classify_network_topic(topic.as_str())
                                                .map(|lane| lane.default_subscription_inbox_messages())
                                        })
                                        .unwrap_or(DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES);
                                    record_gossip_inbound(
                                        &event_traffic_metrics,
                                        topic.as_str(),
                                        message.data.len(),
                                    );
                                    push_bounded_inbox_message(
                                        &event_inbox,
                                        topic.as_str(),
                                        message.data,
                                        inbox_limit,
                                    );
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(event)) => {
                                    match event {
                                        request_response::Event::Message { message, peer, .. } => {
                                            match message {
                                                request_response::Message::Request { request, channel, .. } => {
                                                    dispatch_inbound_request(request, channel, peer, &handlers, &response_workers, &mut fetch_blob_response_budget, &mut pre_crypto_admission_budget, &event_traffic_metrics, &mut swarm, &event_errors, max_error_messages, peer_record_template.as_ref(), &keypair_clone, &event_listening_addrs, &event_reachability, config_clone.allow_loopback_external_addrs_for_testing, &discovered_peer_records);
                                                }
                                                request_response::Message::Response { request_id, response } => {
                                                    if let Some(kind) = pending_peer_record_requests.remove(&request_id) {
                                                        swarm.behaviour_mut().discovery_budget.finish_request(request_id);
                                                        record_response_inbound(
                                                            &event_traffic_metrics,
                                                            kind.protocol_label(),
                                                            response.payload.len(),
                                                        );
                                                        handle_peer_record_response(
                                                            &mut swarm,
                                                            kind,
                                                            response.payload.as_slice(),
                                                            &mut pending_peer_record_requests,
                                                            &mut pending_dht,
                                                            &mut discovered_peer_records,
                                                                &mut discovery_observations,
                                                            &mut known_transport_paths,
                                                            &mut last_dialed_transport_paths,
                                                            &active_transport_paths,
                                                            peers.as_slice(),
                                                            &event_traffic_metrics,
                                                            &mut failed_transport_path_labels,
                                                            &mut pending_discovery_peer_records,
                                                            &mut cached_peer_record_cooldowns,
                                                            peer_record_template.as_ref(),
                                                            local_peer_id,
                                                            &mut pending_connected_peer_records,
                                                            &mut pending_cached_peer_records,
                                                            &mut pending_cached_discovery_peers,
                                                            max_error_messages,
                                                            &event_errors,
                                                            &config_clone.peer_manager_policy,
                                                            &static_bootstrap_peer_ids,
                                                        );
                                                        (
                                                            peer_healths_by_id,
                                                            quarantined_active_peers,
                                                            admitted_active_peers,
                                                        ) = refresh_peer_manager_views(
                                                            &mut swarm,
                                                            &discovered_peer_records,
                                                            &discovery_observations,
                                                            &active_transport_paths,
                                                            &admitted_active_peers,
                                                            &config_clone.peer_manager_policy,
                                                            &event_peer_healths,
                                                            &event_peer_block_artifacts,
                                                            &mut pending_quarantine_disconnects,
                                                            &event_errors,
                                                            max_error_messages,
                                                                &event_reachability,
                                                        );
                                                    } else if let Some(pending_response) = pending.remove(&request_id) {
                                                        record_response_inbound(
                                                            &event_traffic_metrics,
                                                            pending_response.protocol.as_str(),
                                                            response.payload.len(),
                                                        );
                                                        let _ = pending_response.response.send(Ok(response.payload));
                                                    }
                                                }
                                            }
                                        }
                                        request_response::Event::OutboundFailure { request_id, peer, error, .. } => {
                                            if let Some(kind) = pending_peer_record_requests.remove(&request_id) {
                                                        swarm.behaviour_mut().discovery_budget.finish_request(request_id);
                                                handle_peer_record_outbound_failure(
                                                    &mut swarm,
                                                    kind,
                                                    &mut pending_peer_record_requests,
                                                    &mut pending_connected_peer_records,
                                                    &mut connected_peer_record_cooldowns,
                                                    &mut pending_cached_peer_records,
                                                    &mut cached_peer_record_cooldowns,
                                                    &mut pending_cached_discovery_peers,
                                                    &mut cached_discovery_peer_cooldowns,
                                                    peers.as_slice(),
                                                    &event_traffic_metrics,
                                                    local_peer_id,
                                                );
                                                push_bounded_clone(
                                                    &event_errors,
                                                    format!("libp2p peer record request failed: {error:?}"),
                                                    max_error_messages,
                                                    "lock errors",
                                                );
                                            } else if let Some(sender) = pending.remove(&request_id) {
                                                fail_pending_request(sender, peer, error, &event_errors, max_error_messages);
                                            }
                                        }
                                        request_response::Event::InboundFailure { peer, error, .. } => {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!("libp2p inbound failure from {peer:?}: {error:?}"),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        request_response::Event::ResponseSent { .. } => {}
                                    }
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::Kademlia(event)) => {
                                    match event {
                                        kad::Event::OutboundQueryProgressed { id, result, step, .. } => {
                                            let action = if let Some(pending) = pending_dht.get_mut(&id) {
                                                handle_dht_progress(pending, result, step.last, discovered_peer_records.clock())
                                            } else {
                                                DhtProgressAction::None
                                            };
                                            if step.last {
                                                swarm.behaviour_mut().discovery_budget.finish_dht(id);
                                                pending_dht.remove(&id);
                                            }
                                            match action {
                                                DhtProgressAction::None => {}
                                                DhtProgressAction::DiscoverPeers(found) => {
                                                    for peer_id in found {
                                                        maybe_queue_discovery_peer_record(
                                                            &mut swarm,
                                                            &mut pending_dht,
                                                            &mut pending_discovery_peer_records,
                                                            &discovered_peer_records,
                                                            peer_id,
                                                            local_peer_id,
                                                            peer_record_world_id(
                                                                peer_record_template.as_ref(),
                                                            ),
                                                        );
                                                        maybe_request_cached_peer_record(
                                                            &mut swarm,
                                                            &mut pending_peer_record_requests,
                                                            &mut pending_cached_peer_records,
                                                            &mut cached_peer_record_cooldowns,
                                                            &event_traffic_metrics,
                                                            peers.as_slice(),
                                                            peer_id,
                                                            local_peer_id,
                                                        );
                                                    }
                                                }
                                                DhtProgressAction::DiscoveryError(err) => {
                                                    push_bounded_clone(
                                                        &event_errors,
                                                        format!("libp2p discovery query failed: {err:?}"),
                                                        max_error_messages,
                                                        "lock errors",
                                                    );
                                                }
                                                DhtProgressAction::DiscoverPeerRecord { peer_id, upstream, result } => {
                                                    pending_discovery_peer_records.remove(&peer_id);
                                                    match *result {
                                                        Ok(Some(record)) => {
                                                            if !peer_record_matches_target(&record, peer_id) {
                                                                push_bounded_clone(
                                                                    &event_errors,
                                                                    format!("libp2p discovered peer record target mismatch requested={peer_id} actual={}", record.record.peer_id),
                                                                    max_error_messages,
                                                                    "lock errors",
                                                                );
                                                                maybe_request_cached_peer_record(
                                                                    &mut swarm,
                                                                    &mut pending_peer_record_requests,
                                                                    &mut pending_cached_peer_records,
                                                                    &mut cached_peer_record_cooldowns,
                                                                    &event_traffic_metrics,
                                                                    peers.as_slice(),
                                                                    peer_id,
                                                                    local_peer_id,
                                                                );
                                                            } else if let Err(err) = process_discovered_peer_record(
                                                                &mut swarm,
                                                                &mut discovered_peer_records,
                                                                &mut discovery_observations,
                                                                &mut known_transport_paths,
                                                                &mut last_dialed_transport_paths,
                                                                &active_transport_paths,
                                                                &mut failed_transport_path_labels,
                                                                peer_record_template.as_ref(),
                                                                &config_clone.peer_manager_policy,
                                                                &static_bootstrap_peer_ids,
                                                                record,
                                                                peer_discovery_observations::DiscoveryIngress::Dht { upstream },
                                                            ) {
                                                                push_bounded_clone(
                                                                    &event_errors,
                                                                    format!("libp2p discovered peer record rejected peer={peer_id}: {err:?}"),
                                                                    max_error_messages,
                                                                    "lock errors",
                                                                );
                                                                maybe_request_cached_peer_record(
                                                                    &mut swarm,
                                                                    &mut pending_peer_record_requests,
                                                                    &mut pending_cached_peer_records,
                                                                    &mut cached_peer_record_cooldowns,
                                                                    &event_traffic_metrics,
                                                                    peers.as_slice(),
                                                                    peer_id,
                                                                    local_peer_id,
                                                                );
                                                            } else {
                                                                (
                                                                    peer_healths_by_id,
                                                                    quarantined_active_peers,
                                                                    admitted_active_peers,
                                                                ) = refresh_peer_manager_views(
                                                                    &mut swarm,
                                                                    &discovered_peer_records,
                                                            &discovery_observations,
                                                                    &active_transport_paths,
                                                                    &admitted_active_peers,
                                                                    &config_clone.peer_manager_policy,
                                                                    &event_peer_healths,
                                                                    &event_peer_block_artifacts,
                                                                    &mut pending_quarantine_disconnects,
                                                                    &event_errors,
                                                                    max_error_messages,
                                                                    &event_reachability,
                                                                );
                                                            }
                                                        }
                                                        Ok(None) => {
                                                            maybe_request_cached_peer_record(
                                                                &mut swarm,
                                                                &mut pending_peer_record_requests,
                                                                &mut pending_cached_peer_records,
                                                                &mut cached_peer_record_cooldowns,
                                                                &event_traffic_metrics,
                                                                peers.as_slice(),
                                                                peer_id,
                                                                local_peer_id,
                                                            );
                                                        }
                                                        Err(err) => {
                                                            push_bounded_clone(
                                                                &event_errors,
                                                                format!("libp2p discovered peer record load failed peer={peer_id}: {err:?}"),
                                                                max_error_messages,
                                                                "lock errors",
                                                            );
                                                            maybe_request_cached_peer_record(
                                                                &mut swarm,
                                                                &mut pending_peer_record_requests,
                                                                &mut pending_cached_peer_records,
                                                                &mut cached_peer_record_cooldowns,
                                                                &event_traffic_metrics,
                                                                peers.as_slice(),
                                                                peer_id,
                                                                local_peer_id,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        kad::Event::RoutingUpdated { peer, addresses, .. } => {
                                            handle_routing_updated(
                                                &mut swarm,
                                                &mut pending_dht,
                                                &mut pending_discovery_peer_records,
                                                &discovered_peer_records,
                                                peer,
                                                format!("{addresses:?}"),
                                                local_peer_id,
                                                peer_record_template.as_ref(),
                                                peers.as_slice(),
                                                &mut pending_peer_record_requests,
                                                &mut pending_connected_peer_records,
                                                &mut connected_peer_record_cooldowns,
                                                &event_traffic_metrics,
                                                &event_errors,
                                                &mut lifecycle_event_errors_at_ms,
                                                &mut lifecycle_event_errors_last_prune_at_ms,
                                                max_error_messages,
                                                now_ms(),
                                                LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                            );
                                        }
                                        _ => {}
                                    }
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::Autonat(event)) => {
                                    handle_autonat_event(&event_reachability, &event);
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::RelayClient(event)) => {
                                    match event {
                                        relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, .. } => {
                                            note_relay_reservation_accepted(&event_reachability);
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p relay reservation accepted relay={relay_peer_id} renewal={renewal}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                            republish_local_peer_record!();
                                        }
                                        relay::client::Event::OutboundCircuitEstablished { relay_peer_id, .. } => {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p relay outbound circuit established relay={relay_peer_id}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        relay::client::Event::InboundCircuitEstablished { src_peer_id, .. } => {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p relay inbound circuit established src={src_peer_id}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                    }
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::Dcutr(event)) => {
                                    note_hole_punch_result(&event_reachability, event.result.is_ok());
                                    let outcome = match event.result {
                                        Ok(connection_id) => {
                                            format!(
                                                "libp2p dcutr hole-punch upgraded peer={} connection_id={connection_id}",
                                                event.remote_peer_id
                                            )
                                        }
                                        Err(err) => {
                                            format!(
                                                "libp2p dcutr hole-punch failed peer={}: {err}",
                                                event.remote_peer_id
                                            )
                                        }
                                    };
                                    push_bounded_clone(
                                        &event_errors,
                                        outcome,
                                        max_error_messages,
                                        "lock errors",
                                    );
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::RendezvousClient(event)) => {
                                    match event {
                                        rendezvous::client::Event::Registered { rendezvous_node, .. } => {
                                            pending_rendezvous_registers.remove(&rendezvous_node);
                                            registered_rendezvous_nodes.insert(rendezvous_node);
                                        }
                                        rendezvous::client::Event::RegisterFailed { rendezvous_node, namespace, error } => {
                                            pending_rendezvous_registers.remove(&rendezvous_node);
                                            registered_rendezvous_nodes.remove(&rendezvous_node);
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p rendezvous register rejected peer={rendezvous_node} namespace={namespace}: {error:?}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        rendezvous::client::Event::Discovered {
                                            rendezvous_node,
                                            registrations,
                                            cookie,
                                        } => {
                                            pending_rendezvous_discovers.remove(&rendezvous_node);
                                            rendezvous_cookies.insert(rendezvous_node, cookie);
                                            handle_rendezvous_discovered(
                                                &mut swarm,
                                                rendezvous_node,
                                                registrations,
                                                &mut pending_dht,
                                                &mut pending_peer_record_requests,
                                                &mut pending_discovery_peer_records,
                                                &mut pending_cached_peer_records,
                                                &mut cached_peer_record_cooldowns,
                                                &event_traffic_metrics,
                                                peers.as_slice(),
                                                &discovered_peer_records,
                                                local_peer_id,
                                                peer_record_template.as_ref(),
                                                max_error_messages,
                                                &event_errors,
                                            );
                                        }
                                        rendezvous::client::Event::DiscoverFailed { rendezvous_node, namespace, error } => {
                                            pending_rendezvous_discovers.remove(&rendezvous_node);
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p rendezvous discover rejected peer={rendezvous_node} namespace={namespace:?}: {error:?}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        rendezvous::client::Event::Expired { peer } => {
                                            rendezvous_cookies.remove(&peer);
                                        }
                                    }
                                }
                                SwarmEvent::Behaviour(BehaviourEvent::RendezvousServer(
                                    rendezvous::server::Event::PeerNotRegistered {
                                        peer,
                                        namespace,
                                        error,
                                    },
                                )) => {
                                    push_bounded_clone(
                                        &event_errors,
                                        format!(
                                            "libp2p rendezvous server rejected peer={peer} namespace={namespace}: {error:?}"
                                        ),
                                        max_error_messages,
                                        "lock errors",
                                    );
                                }
                                SwarmEvent::NewExternalAddrCandidate { address } => {
                                    push_bounded_clone(
                                        &event_errors,
                                        handle_external_addr_candidate(&address),
                                        max_error_messages,
                                        "lock errors",
                                    );
                                }
                                SwarmEvent::ExternalAddrConfirmed { address } => {
                                    log_external_addr_confirmed_and_republish(
                                        &event_errors,
                                        max_error_messages,
                                        &event_reachability,
                                        &address,
                                        &peer_record_republisher,
                                        &mut swarm,
                                        &mut pending_dht,
                                        &mut provider_keys,
                                        &mut peer_record_last_published_at_ms,
                                    );
                                }
                                SwarmEvent::ExternalAddrExpired { address } => {
                                    log_external_addr_expired_and_republish(
                                        &event_errors,
                                        max_error_messages,
                                        &event_reachability,
                                        &address,
                                        &peer_record_republisher,
                                        &mut swarm,
                                        &mut pending_dht,
                                        &mut provider_keys,
                                        &mut peer_record_last_published_at_ms,
                                    );
                                }
                                SwarmEvent::NewListenAddr { address, .. } => {
                                    handle_new_listen_addr(
                                        &mut swarm,
                                        &event_listening_addrs,
                                        &event_reachability,
                                        &address,
                                        config_clone.allow_loopback_external_addrs_for_testing,
                                        max_listening_addrs,
                                    );
                                    republish_local_peer_record!();
                                }
                                SwarmEvent::ExpiredListenAddr { address, .. } => {
                                    handle_expired_listen_addr(
                                        &mut swarm,
                                        &event_listening_addrs,
                                        &event_reachability,
                                        &address,
                                        config_clone.allow_loopback_external_addrs_for_testing,
                                    );
                                    republish_local_peer_record!();
                                }
                                SwarmEvent::ListenerClosed { addresses, .. } => {
                                    handle_listener_closed(
                                        &mut swarm,
                                        &event_listening_addrs,
                                        &event_reachability,
                                        addresses.as_slice(),
                                        config_clone.allow_loopback_external_addrs_for_testing,
                                    );
                                    republish_local_peer_record!();
                                }
                                SwarmEvent::ConnectionEstablished {
                                    peer_id,
                                    connection_id,
                                    endpoint,
                                    ..
                                } => {
                                    if !peers.contains(&peer_id) {
                                        peers.push(peer_id);
                                    }
                                    event_connected_peers
                                        .lock()
                                        .expect("lock connected peers")
                                        .insert(peer_id);
                                    let (connection_direction, connection_addr) = match &endpoint {
                                        libp2p::core::connection::ConnectedPoint::Dialer { address, .. } => ("outbound", address),
                                        libp2p::core::connection::ConnectedPoint::Listener { send_back_addr, .. } => ("inbound", send_back_addr),
                                    };
                                    push_bounded_string_with_keyed_cooldown(
                                        &event_errors,
                                        &mut lifecycle_event_errors_at_ms,
                                        &mut lifecycle_event_errors_last_prune_at_ms,
                                        format!("connection-established:{peer_id}"),
                                        format!(
                                            "libp2p connection established peer={peer_id} direction={connection_direction} addr={connection_addr}"
                                        ),
                                        max_error_messages,
                                        "lock errors",
                                        now_ms(),
                                        LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                    );
                                    let (refreshed_active_path, _dialed_addr) =
                                        record_established_connection(
                                            &known_transport_paths,
                                            &mut active_transport_paths,
                                            &mut last_dialed_transport_paths,
                                            &mut failed_transport_path_labels,
                                            &mut established_transport_paths_by_connection,
                                            &mut established_connections_by_peer,
                                            peer_id,
                                            connection_id,
                                            &endpoint,
                                        );
                                    log_active_transport_path(
                                        &event_errors,
                                        &mut lifecycle_event_errors_at_ms,
                                        &mut lifecycle_event_errors_last_prune_at_ms,
                                        peer_id,
                                        refreshed_active_path.as_ref(),
                                        max_error_messages,
                                        now_ms(),
                                        LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                    );
                                    prune_redundant_peer_connections(
                                        &mut swarm,
                                        &established_transport_paths_by_connection,
                                        &established_connections_by_peer,
                                        peer_id,
                                        &event_errors,
                                        &mut lifecycle_event_errors_at_ms,
                                        &mut lifecycle_event_errors_last_prune_at_ms,
                                        max_error_messages,
                                        now_ms(),
                                        LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                    );
                                    maybe_queue_discovery_peer_record(
                                        &mut swarm,
                                        &mut pending_dht,
                                        &mut pending_discovery_peer_records,
                                        &discovered_peer_records,
                                        peer_id,
                                        local_peer_id,
                                        peer_record_world_id(peer_record_template.as_ref()),
                                    );
                                    maybe_request_connected_peer_record(
                                        &mut swarm,
                                        &mut pending_peer_record_requests,
                                        &mut pending_connected_peer_records,
                                        &mut connected_peer_record_cooldowns,
                                        &event_traffic_metrics,
                                        peer_id,
                                        local_peer_id,
                                        peer_record_template.is_some(),
                                    );
                                    maybe_request_cached_discovery_peers(
                                        &mut swarm,
                                        &mut pending_peer_record_requests,
                                        &mut pending_cached_discovery_peers,
                                        &mut cached_discovery_peer_cooldowns,
                                        &event_traffic_metrics,
                                        peer_id,
                                        local_peer_id,
                                    );
                                    if let Some(template) = peer_record_template.as_ref() {
                                        if let Err(err) = maybe_register_rendezvous_namespace(
                                            &mut swarm,
                                            &mut pending_rendezvous_registers,
                                            &registered_rendezvous_nodes,
                                            peer_id,
                                            local_peer_id,
                                            template,
                                        ) {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p rendezvous register failed peer={peer_id}: {err:?}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        if let Err(err) = maybe_discover_rendezvous_namespace(
                                            &mut swarm,
                                            &mut pending_rendezvous_discovers,
                                            &rendezvous_cookies,
                                            peer_id,
                                            local_peer_id,
                                            template,
                                        ) {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p rendezvous discover failed peer={peer_id}: {err:?}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        }
                                        start_peer_discovery_query(
                                            &mut swarm,
                                            &mut pending_dht,
                                            template,
                                            &mut peer_discovery_query_last_started_at_ms,
                                            now_ms(),
                                            discovery_query_cooldown_ms,
                                        );
                                    }
                                    (
                                        peer_healths_by_id,
                                        quarantined_active_peers,
                                        admitted_active_peers,
                                    ) = refresh_peer_manager_views(
                                        &mut swarm,
                                        &discovered_peer_records,
                                                            &discovery_observations,
                                        &active_transport_paths,
                                        &admitted_active_peers,
                                        &config_clone.peer_manager_policy,
                                        &event_peer_healths,
                                        &event_peer_block_artifacts,
                                        &mut pending_quarantine_disconnects,
                                        &event_errors,
                                        max_error_messages,
                                        &event_reachability,
                                    );
                                }
                                SwarmEvent::ConnectionClosed {
                                    peer_id,
                                    connection_id,
                                    num_established,
                                    ..
                                } => {
                                    if num_established > 0 {
                                        let refreshed_active_path =
                                            refresh_active_path_after_connection_close(
                                                &mut active_transport_paths,
                                                &mut established_transport_paths_by_connection,
                                                &mut established_connections_by_peer,
                                                peer_id,
                                                connection_id,
                                            );
                                        push_bounded_string_with_keyed_cooldown(
                                            &event_errors,
                                            &mut lifecycle_event_errors_at_ms,
                                            &mut lifecycle_event_errors_last_prune_at_ms,
                                            format!("connection-closed-partial:{peer_id}"),
                                            format!(
                                                "libp2p connection closed peer={peer_id} num_established={num_established} active_path={}",
                                                refreshed_active_path
                                                    .as_ref()
                                                    .map(|path| path.addr.to_string())
                                                    .unwrap_or_else(|| "none".to_string())
                                            ),
                                            max_error_messages,
                                            "lock errors",
                                            now_ms(),
                                            LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                        );
                                    } else {
                                        let disconnected_path =
                                            active_transport_paths.get(&peer_id).cloned();
                                        established_transport_paths_by_connection
                                            .remove(&connection_id);
                                        established_connections_by_peer.remove(&peer_id);
                                        let quarantined = clear_disconnected_peer_state(
                                            &mut peers,
                                            &mut admitted_active_peers,
                                            &mut quarantined_active_peers,
                                            &mut pending_quarantine_disconnects,
                                            &mut active_transport_paths,
                                            &mut last_dialed_transport_paths,
                                            &mut connected_peer_record_cooldowns,
                                            &mut cached_peer_record_cooldowns,
                                            &mut cached_discovery_peer_cooldowns,
                                            &mut pending_rendezvous_registers,
                                            &mut pending_rendezvous_discovers,
                                            &mut registered_rendezvous_nodes,
                                            &mut rendezvous_cookies,
                                            &event_connected_peers,
                                            peer_id,
                                        );
                                        if quarantined {
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p peer manager quarantine suppresses failover peer={peer_id}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        } else {
                                            failover_after_disconnect(
                                                &mut swarm,
                                                &known_transport_paths,
                                                &mut active_transport_paths,
                                                &mut last_dialed_transport_paths,
                                                &mut failed_transport_path_labels,
                                                &event_errors,
                                                max_error_messages,
                                                peer_id,
                                                disconnected_path,
                                            );
                                        }
                                        push_bounded_string_with_keyed_cooldown(
                                            &event_errors,
                                            &mut lifecycle_event_errors_at_ms,
                                            &mut lifecycle_event_errors_last_prune_at_ms,
                                            format!("connection-closed-final:{peer_id}"),
                                            format!("libp2p connection closed peer={peer_id}"),
                                            max_error_messages,
                                            "lock errors",
                                            now_ms(),
                                            LIFECYCLE_EVENT_ERROR_COOLDOWN_MS,
                                        );
                                    }
                                    (
                                        peer_healths_by_id,
                                        quarantined_active_peers,
                                        admitted_active_peers,
                                    ) = refresh_peer_manager_views(
                                        &mut swarm,
                                        &discovered_peer_records,
                                                            &discovery_observations,
                                        &active_transport_paths,
                                        &admitted_active_peers,
                                        &config_clone.peer_manager_policy,
                                        &event_peer_healths,
                                        &event_peer_block_artifacts,
                                        &mut pending_quarantine_disconnects,
                                        &event_errors,
                                        max_error_messages,
                                        &event_reachability,
                                    );
                                }
                                SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                                    if let Some(peer_id) = peer_id {
                                        if quarantined_active_peers.contains(&peer_id)
                                            || pending_quarantine_disconnects.contains(&peer_id)
                                        {
                                            last_dialed_transport_paths.remove(&peer_id);
                                            push_bounded_clone(
                                                &event_errors,
                                                format!(
                                                    "libp2p peer manager quarantine suppresses retry peer={peer_id}"
                                                ),
                                                max_error_messages,
                                                "lock errors",
                                            );
                                        } else {
                                            match retry_transport_path_after_error(
                                                &mut swarm,
                                                &known_transport_paths,
                                                &mut last_dialed_transport_paths,
                                                &mut failed_transport_path_labels,
                                                peer_id,
                                            ) {
                                                Ok(Some((last_path, next_path))) => {
                                                    push_bounded_clone(
                                                        &event_errors,
                                                        format!(
                                                            "libp2p transport retry peer={peer_id} from={} to={}",
                                                            last_path.addr,
                                                            next_path.addr,
                                                        ),
                                                        max_error_messages,
                                                        "lock errors",
                                                    );
                                                }
                                                Err(retry_err) => {
                                                    push_bounded_clone(
                                                        &event_errors,
                                                        format!(
                                                            "libp2p transport retry dial failed peer={peer_id}: {retry_err:?}"
                                                        ),
                                                        max_error_messages,
                                                        "lock errors",
                                                    );
                                                }
                                                Ok(None) => {}
                                            }
                                        }
                                    }
                                    runtime_support::log_connection_error(&event_errors, max_error_messages,
                                        format!("libp2p outgoing connection error peer={peer_id:?}: {error:?}"));
                                }
                                SwarmEvent::IncomingConnectionError { error, .. } => {
                                    runtime_support::log_connection_error(&event_errors, max_error_messages,
                                        format!("libp2p incoming connection error: {error:?}"));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
        });
        enqueue_initial_bootstrap_dials(command_tx.clone(), bootstrap_peers);
        Self {
            peer_id,
            keypair,
            command_tx: command_tx.clone(),
            inbox,
            published,
            listening_addrs,
            connected_peers,
            errors,
            peer_healths,
            peer_block_artifacts,
            reachability,
            traffic_metrics,
            wire_byte_counters,
            _shutdown_guard: Arc::new(drop_support::ShutdownGuard::new(command_tx)),
        }
    }
}
#[cfg(test)]
mod tests;
