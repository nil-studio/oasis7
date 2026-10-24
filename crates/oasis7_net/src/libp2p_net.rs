use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
mod api;
pub use api::Libp2pNetwork;
mod config;
mod connection_lifecycle;
mod construction;
mod constructor_support;
mod discovery;
mod discovery_budget;
mod discovery_list;
mod drop_support;
mod durable_peer_record_publisher;
use durable_peer_record_publisher::LocalPeerRecordPublisher;
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
#[cfg(test)]
mod tests;
