use crate::libp2p_net::peer_record_cache::PeerRecordCache;
use super::peer_discovery_observations::{DiscoveryIngress, DiscoveryObservations};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use libp2p::PeerId;
use libp2p::identity::Keypair;
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerRecord};

use super::peer_record::sign_peer_record;

fn signed_discovery_peer_record(
    keypair: &Keypair,
    discovery_sources: Vec<crate::dht::PeerDiscoverySource>,
    published_at_ms: i64,
) -> SignedPeerRecord {
    let peer_id = PeerId::from(keypair.public());
    sign_peer_record(
        &PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: format!("node-{peer_id}"),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec!["/ip4/127.0.0.1/udp/4103/quic-v1".to_string()],
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources,
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms,
            ttl_ms: 60_000,
        },
        keypair,
    )
    .expect("sign discovery peer record")
}

#[test]
fn process_discovered_peer_record_retries_failed_candidate_dial_on_rediscovery() {
    super::runtime_support::run_on_libp2p_test_runtime(|| {
        let mut swarm = super::swarm_behaviour::build_swarm(
            &Keypair::generate_ed25519(),
            false,
            true,
            std::time::Duration::from_secs(30),
            super::wire_bytes::init_shared_wire_byte_counters(),
        );
        let peer_key = Keypair::generate_ed25519();
        let record = signed_discovery_peer_record(
            &peer_key,
            vec![
                crate::dht::PeerDiscoverySource::Dht,
                crate::dht::PeerDiscoverySource::Rendezvous,
            ], crate::libp2p_net::now_ms());
        let peer_id = PeerId::from(peer_key.public());
        let mut discovered_peer_records = PeerRecordCache::default();
        let mut known_transport_paths = HashMap::new();
        let mut last_dialed_transport_paths = HashMap::new();
        let active_transport_paths = HashMap::new();
        let mut failed_transport_path_labels = HashSet::new();

        super::discovery::process_discovered_peer_record(
            &mut swarm,
            &mut discovered_peer_records,
            &mut DiscoveryObservations::default(),
            &mut known_transport_paths,
            &mut last_dialed_transport_paths,
            &active_transport_paths,
            &mut failed_transport_path_labels,
            None,
            &PeerManagerPolicy::default(),
            &HashSet::from([peer_id]),
            record.clone(),
            DiscoveryIngress::Dht {
                upstream: Some(peer_id),
            },
        )
        .expect("first discovery dial");
        assert!(last_dialed_transport_paths.contains_key(&peer_id));

        last_dialed_transport_paths.remove(&peer_id);
        let mut retry_swarm = super::swarm_behaviour::build_swarm(
            &Keypair::generate_ed25519(),
            false,
            true,
            std::time::Duration::from_secs(30),
            super::wire_bytes::init_shared_wire_byte_counters(),
        );

        super::discovery::process_discovered_peer_record(
            &mut retry_swarm,
            &mut discovered_peer_records,
            &mut DiscoveryObservations::default(),
            &mut known_transport_paths,
            &mut last_dialed_transport_paths,
            &active_transport_paths,
            &mut failed_transport_path_labels,
            None,
            &PeerManagerPolicy::default(),
            &HashSet::from([peer_id]),
            record,
            DiscoveryIngress::Dht {
                upstream: Some(peer_id),
            },
        )
        .expect("rediscovery must retry dial");

        assert!(last_dialed_transport_paths.contains_key(&peer_id));
    });
}

#[test]
fn request_with_providers_does_not_fallback_outside_provider_subset() {
    let keypair = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(keypair.public());
    let mut swarm = super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = vec![PeerId::random()];
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
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::traffic_metrics::init_shared_traffic_metrics();
    let provider_outside_subset = PeerId::random();

    let outcome = super::runtime_loop::handle_command(
        &mut swarm,
        Some(Command::Request {
            protocol: "/aw/rr/1.0.0/fetch_blob".to_string(),
            payload: Vec::new(),
            providers: vec![provider_outside_subset.to_string()],
            response: sender,
        }),
        super::runtime_loop::CommandStateRefs {
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
        &super::CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: None,
            local_peer_id,
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 0,
            discovery_query_cooldown_ms: 60_000,
            allow_loopback_external_addrs_for_testing: false,
        },
    );
    assert!(matches!(outcome, super::CommandOutcome::Continue));

    let err = receiver
        .recv()
        .expect("request response")
        .expect_err("provider subset mismatch must fail");
    assert!(matches!(
        err,
        WorldError::NetworkProtocolUnavailable { protocol }
            if protocol.contains("no connected providers")
    ));
}

#[test]
fn refresh_peer_discovery_respects_republish_and_query_budgets() {
    let keypair = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(keypair.public());
    let template =
        signed_discovery_peer_record(&keypair, vec![crate::dht::PeerDiscoverySource::Dht], crate::libp2p_net::now_ms())
            .record;
    let provider_key =
        oasis7_proto::distributed::dht_peer_discovery_key(template.world_id.as_str());
    let mut swarm = super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::wire_bytes::init_shared_wire_byte_counters(),
    );
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
    let now = super::now_ms();
    let mut peer_record_last_published_at_ms = Some(now);
    let mut peer_discovery_query_last_started_at_ms = Some(now);
    provider_keys.insert(provider_key.clone(), now);
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::traffic_metrics::init_shared_traffic_metrics();

    let outcome = super::runtime_loop::handle_command(
        &mut swarm,
        Some(Command::RefreshPeerDiscovery),
        super::runtime_loop::CommandStateRefs {
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
        &super::CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: Some(&template),
            local_peer_id,
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 300_000,
            discovery_query_cooldown_ms: 60_000,
            allow_loopback_external_addrs_for_testing: false,
        },
    );
    assert!(matches!(outcome, super::CommandOutcome::Continue));
    assert!(pending_dht.is_empty());
    assert_eq!(peer_record_last_published_at_ms, Some(now));
    assert_eq!(peer_discovery_query_last_started_at_ms, Some(now));
    assert_eq!(provider_keys.get(&provider_key), Some(&now));

    peer_record_last_published_at_ms = Some(0);
    peer_discovery_query_last_started_at_ms = Some(0);
    provider_keys.insert(provider_key.clone(), 0);

    let outcome = super::runtime_loop::handle_command(
        &mut swarm,
        Some(Command::RefreshPeerDiscovery),
        super::runtime_loop::CommandStateRefs {
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
        &super::CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: Some(&template),
            local_peer_id,
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 300_000,
            discovery_query_cooldown_ms: 60_000,
            allow_loopback_external_addrs_for_testing: false,
        },
    );
    assert!(matches!(outcome, super::CommandOutcome::Continue));
    assert_eq!(pending_dht.len(), 2);
    assert!(
        pending_dht
            .values()
            .any(|query| matches!(query, PendingDhtQuery::PutPeerRecord { .. }))
    );
    assert!(
        pending_dht
            .values()
            .any(|query| matches!(query, PendingDhtQuery::DiscoverPeers { .. }))
    );
    assert!(peer_record_last_published_at_ms.is_some_and(|last| last > 0));
    assert!(peer_discovery_query_last_started_at_ms.is_some_and(|last| last > 0));
    assert!(
        provider_keys
            .get(&provider_key)
            .is_some_and(|last| *last > 0)
    );
}

#[test]
fn refresh_peer_discovery_skips_backfill_when_connected_peers_are_already_known() {
    let keypair = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(keypair.public());
    let remote_keypair = Keypair::generate_ed25519();
    let remote_peer_id = PeerId::from(remote_keypair.public());
    let template =
        signed_discovery_peer_record(&keypair, vec![crate::dht::PeerDiscoverySource::Dht], crate::libp2p_net::now_ms())
            .record;
    let provider_key =
        oasis7_proto::distributed::dht_peer_discovery_key(template.world_id.as_str());
    let mut swarm = super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut subscriptions = HashSet::new();
    let mut topic_map = HashMap::new();
    let mut topic_inbox_limits = HashMap::new();
    let mut handlers = HashMap::new();
    let mut pending = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut peers = vec![remote_peer_id];
    let mut provider_keys = HashMap::from([(provider_key, 0)]);
    let discovered_peer_records = PeerRecordCache::from([(
        remote_peer_id,
        signed_discovery_peer_record(
            &remote_keypair,
            vec![crate::dht::PeerDiscoverySource::Dht], crate::libp2p_net::now_ms()),
    )]);
    let peer_healths_by_id = HashMap::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let mut pending_rendezvous_registers = HashSet::new();
    let mut pending_rendezvous_discovers = HashSet::new();
    let registered_rendezvous_nodes = HashSet::new();
    let rendezvous_cookies = HashMap::new();
    let mut peer_record_last_published_at_ms = Some(0);
    let mut peer_discovery_query_last_started_at_ms = Some(0);
    let event_published = Arc::new(Mutex::new(Vec::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_listening_addrs = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let event_traffic_metrics = super::traffic_metrics::init_shared_traffic_metrics();

    let outcome = super::runtime_loop::handle_command(
        &mut swarm,
        Some(Command::RefreshPeerDiscovery),
        super::runtime_loop::CommandStateRefs {
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
        &super::CommandContext {
            event_published: &event_published,
            event_errors: &event_errors,
            event_listening_addrs: &event_listening_addrs,
            event_reachability: &event_reachability,
            event_traffic_metrics: &event_traffic_metrics,
            keypair: &keypair,
            peer_record_template: Some(&template),
            local_peer_id,
            max_published_messages: 8,
            max_error_messages: 8,
            republish_interval_ms: 300_000,
            discovery_query_cooldown_ms: 60_000,
            allow_loopback_external_addrs_for_testing: false,
        },
    );

    assert!(matches!(outcome, super::CommandOutcome::Continue));
    assert_eq!(pending_dht.len(), 1);
    assert!(
        pending_dht
            .values()
            .all(|query| matches!(query, PendingDhtQuery::PutPeerRecord { .. }))
    );
    assert!(pending_peer_record_requests.is_empty());
    assert_eq!(peer_discovery_query_last_started_at_ms, Some(0));
}

#[test]
fn dial_target_is_already_connected_only_for_matching_peer_id_multiaddr() {
    let connected_peer_id = PeerId::random();
    let connected_addr = format!("/ip4/127.0.0.1/tcp/4101/p2p/{connected_peer_id}")
        .parse()
        .expect("connected addr");
    let other_addr = format!("/ip4/127.0.0.1/tcp/4102/p2p/{}", PeerId::random())
        .parse()
        .expect("other addr");
    let bare_addr = "/ip4/127.0.0.1/tcp/4103".parse().expect("bare addr");

    assert!(super::runtime_loop::dial_target_is_already_connected(
        &[connected_peer_id],
        &connected_addr,
    ));
    assert!(!super::runtime_loop::dial_target_is_already_connected(
        &[connected_peer_id],
        &other_addr,
    ));
    assert!(!super::runtime_loop::dial_target_is_already_connected(
        &[connected_peer_id],
        &bare_addr,
    ));
}
