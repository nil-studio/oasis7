use crate::libp2p_net::durable_peer_record_publisher::LocalPeerRecordPublisher;
use crate::libp2p_net::peer_record_cache::PeerRecordCache;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, identity::Keypair};

use super::super::runtime_loop::{
    Command, CommandContext, CommandOutcome, CommandStateRefs, handle_command,
};
use super::super::{
    DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES, Libp2pReachabilitySnapshot, PeerManagerHealthIssue,
    PeerManagerHealthStatus, PeerManagerPeerHealth,
};

#[test]
fn handle_command_subscribe_acknowledges_success() {
    let keypair = Keypair::generate_ed25519();
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = Vec::new();
    let mut provider_keys = HashMap::new();
    let discovered_peer_records = PeerRecordCache::default();
    let peer_healths_by_id = HashMap::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let mut pending_rendezvous_registers = HashSet::new();
    let mut pending_rendezvous_discovers = HashSet::new();
    let registered_rendezvous_nodes = HashSet::new();
    let rendezvous_cookies = HashMap::new();
    let mut peer_record_last_published_at_ms = None;
    let mut peer_discovery_query_last_started_at_ms = None;
    let (sender, receiver) = std::sync::mpsc::channel();

    let outcome = handle_command(
        &mut swarm,
        Some(Command::Subscribe {
            topic: "aw.handle-command".to_string(),
            response: sender,
        }),
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
        &CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: None,
            local_peer_record_publisher: &Arc::new(Mutex::new(LocalPeerRecordPublisher::Legacy)),
            local_peer_id: PeerId::from(keypair.public()),
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 1_000,
            discovery_query_cooldown_ms: 1_000,
            allow_loopback_external_addrs_for_testing: true,
        },
    );

    assert!(matches!(outcome, CommandOutcome::Continue));
    receiver
        .recv()
        .expect("command response")
        .expect("subscribe ack");
    assert!(subscriptions.contains("aw.handle-command"));
    assert_eq!(topic_map.len(), 1);
    assert_eq!(
        topic_inbox_limits.get("aw.handle-command").copied(),
        Some(DEFAULT_SUBSCRIPTION_INBOX_MAX_MESSAGES)
    );
}

async fn connect_loopback_swarms(
    listener: &mut libp2p::Swarm<super::super::Behaviour>,
    dialer: &mut libp2p::Swarm<super::super::Behaviour>,
) {
    listener
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().expect("listen addr"))
        .expect("listen");
    let listen_addr: Multiaddr = loop {
        match listener.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. }
                if address.to_string().contains("127.0.0.1") =>
            {
                break address;
            }
            _ => {}
        }
    };
    let listener_peer = *listener.local_peer_id();
    let dial_addr = listen_addr.with(libp2p::multiaddr::Protocol::P2p(listener_peer));
    super::super::swarm_behaviour::dial_addr_with_optional_peer_id(dialer, dial_addr)
        .expect("dial listener");

    loop {
        futures::select! {
            _ = listener.select_next_some().fuse() => {},
            _ = dialer.select_next_some().fuse() => {},
        }
        if dialer.is_connected(&listener_peer) {
            return;
        }
    }
}

#[test]
fn handle_command_request_uses_swarm_connections_when_runtime_peers_are_empty() {
    let listener_keypair = Keypair::generate_ed25519();
    let dialer_keypair = Keypair::generate_ed25519();
    let mut listener = super::super::swarm_behaviour::build_swarm(
        &listener_keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut dialer = super::super::swarm_behaviour::build_swarm(
        &dialer_keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build libp2p test tokio runtime");
    runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(10),
            connect_loopback_swarms(&mut listener, &mut dialer),
        )
        .await
        .expect("connect swarms");
    });
    assert!(dialer.is_connected(listener.local_peer_id()));

    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = Vec::new();
    let mut provider_keys = HashMap::new();
    let discovered_peer_records = PeerRecordCache::default();
    let peer_healths_by_id = HashMap::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let mut pending_rendezvous_registers = HashSet::new();
    let mut pending_rendezvous_discovers = HashSet::new();
    let registered_rendezvous_nodes = HashSet::new();
    let rendezvous_cookies = HashMap::new();
    let mut peer_record_last_published_at_ms = None;
    let mut peer_discovery_query_last_started_at_ms = None;
    let (sender, receiver) = std::sync::mpsc::channel();

    let outcome = handle_command(
        &mut dialer,
        Some(Command::Request {
            protocol: "/aw/test/runtime-empty-request/1.0.0".to_string(),
            payload: b"ping".to_vec(),
            providers: Vec::new(),
            response: sender,
        }),
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
        &CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &dialer_keypair,
            peer_record_template: None,
            local_peer_record_publisher: &Arc::new(Mutex::new(LocalPeerRecordPublisher::Legacy)),
            local_peer_id: PeerId::from(dialer_keypair.public()),
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 1_000,
            discovery_query_cooldown_ms: 1_000,
            allow_loopback_external_addrs_for_testing: true,
        },
    );

    assert!(matches!(outcome, CommandOutcome::Continue));
    assert!(peers.is_empty(), "test must keep runtime peers empty");
    assert_eq!(
        pending.len(),
        1,
        "request should be sent through swarm connection"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn handle_command_request_uses_active_transport_peer_when_snapshots_are_empty() {
    let keypair = Keypair::generate_ed25519();
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let active_peer = PeerId::random();
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = Vec::new();
    let mut provider_keys = HashMap::new();
    let discovered_peer_records = PeerRecordCache::default();
    let peer_healths_by_id = HashMap::from([(
        active_peer,
        PeerManagerPeerHealth {
            peer_id: active_peer.to_string(),
            status: PeerManagerHealthStatus::Blocked,
            issues: vec![
                PeerManagerHealthIssue::MissingPeerRecord,
                PeerManagerHealthIssue::InsufficientActiveDiscoverySources {
                    observed_sources: 1,
                    required_sources: 2,
                },
            ],
            discovery_sources: Vec::new(),
            active_path_kind: Some("direct".to_string()),
            source_operator: None,
            source_asn: None,
        },
    )]);
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let mut pending_rendezvous_registers = HashSet::new();
    let mut pending_rendezvous_discovers = HashSet::new();
    let registered_rendezvous_nodes = HashSet::new();
    let rendezvous_cookies = HashMap::new();
    let mut peer_record_last_published_at_ms = None;
    let mut peer_discovery_query_last_started_at_ms = None;
    let (sender, receiver) = std::sync::mpsc::channel();

    let outcome = handle_command(
        &mut swarm,
        Some(Command::Request {
            protocol: "/aw/test/active-transport-request/1.0.0".to_string(),
            payload: b"ping".to_vec(),
            providers: vec![active_peer.to_string()],
            response: sender,
        }),
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
        &CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: None,
            local_peer_record_publisher: &Arc::new(Mutex::new(LocalPeerRecordPublisher::Legacy)),
            local_peer_id: PeerId::from(keypair.public()),
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 1_000,
            discovery_query_cooldown_ms: 1_000,
            allow_loopback_external_addrs_for_testing: true,
        },
    );

    assert!(matches!(outcome, CommandOutcome::Continue));
    assert!(peers.is_empty(), "test must keep runtime peers empty");
    assert!(
        !swarm.is_connected(&active_peer),
        "test must rely on active transport health fallback"
    );
    assert_eq!(
        pending.len(),
        1,
        "provider request should be sent through active transport fallback"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}

#[test]
fn handle_command_request_to_peer_accepts_active_transport_peer() {
    let keypair = Keypair::generate_ed25519();
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let active_peer = PeerId::random();
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = Vec::new();
    let mut provider_keys = HashMap::new();
    let discovered_peer_records = PeerRecordCache::default();
    let peer_healths_by_id = HashMap::from([(
        active_peer,
        PeerManagerPeerHealth {
            peer_id: active_peer.to_string(),
            status: PeerManagerHealthStatus::Blocked,
            issues: vec![
                PeerManagerHealthIssue::MissingPeerRecord,
                PeerManagerHealthIssue::InsufficientActiveDiscoverySources {
                    observed_sources: 1,
                    required_sources: 2,
                },
            ],
            discovery_sources: Vec::new(),
            active_path_kind: Some("direct".to_string()),
            source_operator: None,
            source_asn: None,
        },
    )]);
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let mut pending_rendezvous_registers = HashSet::new();
    let mut pending_rendezvous_discovers = HashSet::new();
    let registered_rendezvous_nodes = HashSet::new();
    let rendezvous_cookies = HashMap::new();
    let mut peer_record_last_published_at_ms = None;
    let mut peer_discovery_query_last_started_at_ms = None;
    let (sender, receiver) = std::sync::mpsc::channel();

    let outcome = handle_command(
        &mut swarm,
        Some(Command::RequestToPeer {
            protocol: "/aw/test/active-transport-request-to-peer/1.0.0".to_string(),
            payload: b"ping".to_vec(),
            peer: active_peer,
            response: sender,
        }),
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
        &CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: None,
            local_peer_record_publisher: &Arc::new(Mutex::new(LocalPeerRecordPublisher::Legacy)),
            local_peer_id: PeerId::from(keypair.public()),
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 1_000,
            discovery_query_cooldown_ms: 1_000,
            allow_loopback_external_addrs_for_testing: true,
        },
    );

    assert!(matches!(outcome, CommandOutcome::Continue));
    assert!(peers.is_empty(), "test must keep runtime peers empty");
    assert!(
        !swarm.is_connected(&active_peer),
        "test must rely on active transport health fallback"
    );
    assert_eq!(
        pending.len(),
        1,
        "direct request should be sent through active transport fallback"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
}
