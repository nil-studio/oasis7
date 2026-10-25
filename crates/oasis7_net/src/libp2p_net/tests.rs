use super::peer_manager::{
    PeerManagerHealthIssue, PeerManagerHealthStatus, PeerManagerPeerHealth, PeerManagerPolicy,
};
use super::peer_record::sign_peer_record;
use super::runtime_loop::{peer_requires_active_quarantine, refresh_peer_manager_healths};
use super::transport_paths::{
    PeerReachabilityContract, TransportMuxer, TransportPathKind, TransportSecurity,
    TransportSessionFlavor, active_transport_path_from_endpoint, peer_record_transport_paths,
    select_preferred_transport_path, sync_known_transport_paths,
};
use super::utils::push_bounded_vec;
use super::*;
use crate::util::to_canonical_cbor;
use libp2p::kad::RecordKey;
use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole};
use oasis7_proto::distributed_net::DistributedNetwork as _;
use oasis7_proto::distributed_net::NetworkLane;
use std::time::{Duration, Instant};

#[path = "tests/active_set_candidate_tests.rs"]
mod active_set_candidate_tests;
mod admissible_request_peers_tests;
mod discovery_dial_tests;
mod discovery_observation_flow_tests;
mod discovery_peer_record_failure_tests;
mod discovery_peer_record_tests;
mod discovery_work_budget_tests;
mod peer_health_refresh_tests;
mod peer_manager_observer_tests;
mod peer_record_admission_budget_tests;
mod peer_record_lifecycle_tests;
mod peer_record_receiver_floor_tests;
mod peer_record_receiver_transport_tests;
mod peer_record_tests;
mod peer_record_tombstone_startup_tests;
mod rendezvous_trigger_tests;
mod rendezvous_ttl_tests;
mod request_peer_filter_tests;
mod response_worker_tests;
mod subscribe_ack_tests;
mod transport_path_refresh_tests;

mod transport_retry_tests {
    use super::*;
    include!("transport_retry_tests.rs");
}

pub(super) fn signed_discovery_peer_record(
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

fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for condition: {what}");
}

fn has_missing_peer_record_issue(health: &PeerManagerPeerHealth) -> bool {
    health
        .issues
        .contains(&PeerManagerHealthIssue::MissingPeerRecord)
}

#[test]
fn dht_get_providers_collects_results() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetProviders {
        response: Some(sender),
        providers: HashSet::new(),
        error: None,
    };
    let key_label = "providers".to_string();
    let key = RecordKey::new(&key_label);
    let mut providers = HashSet::new();
    providers.insert(PeerId::random());
    providers.insert(PeerId::random());
    let expected: HashSet<String> = providers.iter().map(|peer| peer.to_string()).collect();
    let result =
        kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers }));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::peer_record_time::DiscoveryClock::default(),
    );
    let records = receiver
        .recv()
        .expect("command response")
        .expect("get providers");
    let actual: HashSet<String> = records
        .into_iter()
        .map(|record| record.provider_id)
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn build_swarm_supports_more_than_default_max_provider_records() {
    let keypair = Keypair::generate_ed25519();
    let mut swarm = super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        true,
        std::time::Duration::from_secs(30),
        super::wire_bytes::init_shared_wire_byte_counters(),
    );

    for idx in 0..1100 {
        let key = RecordKey::new(&format!("provider-key-{idx}"));
        swarm
            .behaviour_mut()
            .kademlia
            .start_providing(key)
            .unwrap_or_else(|err| panic!("start_providing should exceed default 1024 cap: {err}"));
    }
}

#[test]
fn dht_get_world_head_decodes_record() {
    let head = WorldHeadAnnounce {
        world_id: "w1".to_string(),
        height: 9,
        block_hash: "b1".to_string(),
        state_root: "s1".to_string(),
        timestamp_ms: 42,
        signature: "sig".to_string(),
    };
    let payload = to_canonical_cbor(&head).expect("encode head");
    let key_label = "head".to_string();
    let record = kad::Record {
        key: RecordKey::new(&key_label),
        value: payload,
        publisher: None,
        expires: None,
    };
    let peer_record = kad::PeerRecord { peer: None, record };
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetWorldHead {
        response: Some(sender),
        head: None,
        error: None,
    };
    let result = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record)));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::peer_record_time::DiscoveryClock::default(),
    );
    let loaded = receiver
        .recv()
        .expect("command response")
        .expect("get head");
    assert_eq!(loaded, Some(head));
}

#[test]
fn dht_get_membership_directory_decodes_record() {
    let snapshot = MembershipDirectorySnapshot {
        world_id: "w1".to_string(),
        requester_id: "seq-1".to_string(),
        requested_at_ms: 99,
        reason: Some("sync".to_string()),
        validators: vec!["seq-1".to_string(), "seq-2".to_string()],
        quorum_threshold: 2,
        signature_key_id: None,
        signature: None,
    };
    let payload = to_canonical_cbor(&snapshot).expect("encode snapshot");
    let key_label = "membership".to_string();
    let record = kad::Record {
        key: RecordKey::new(&key_label),
        value: payload,
        publisher: None,
        expires: None,
    };
    let peer_record = kad::PeerRecord { peer: None, record };
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetMembershipDirectory {
        response: Some(sender),
        snapshot: None,
        error: None,
    };
    let result = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record)));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::peer_record_time::DiscoveryClock::default(),
    );

    let loaded = receiver
        .recv()
        .expect("command response")
        .expect("get membership");
    assert_eq!(loaded, Some(snapshot));
}

#[test]
fn dht_get_peer_record_decodes_and_verifies_record() {
    let keypair = Keypair::generate_ed25519();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: PeerId::from(keypair.public()).to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec!["/ip4/127.0.0.1/tcp/4102".to_string()],
            hole_punch_addrs: Vec::new(),
            relay_addrs: vec!["/dns4/relay.example/tcp/443".to_string()],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: now_ms(),
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");
    let payload = to_canonical_cbor(&signed).expect("encode peer record");
    let key_label = "peer-record".to_string();
    let record = kad::Record {
        key: RecordKey::new(&key_label),
        value: payload,
        publisher: None,
        expires: None,
    };
    let peer_record = kad::PeerRecord { peer: None, record };
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetPeerRecord {
        peer_id: PeerId::from(keypair.public()),
        response: Some(sender),
        record: None,
        error: None,
    };
    let result = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(peer_record)));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::peer_record_time::DiscoveryClock::default(),
    );

    let loaded = receiver
        .recv()
        .expect("command response")
        .expect("get peer record");
    assert_eq!(loaded, Some(signed));
}

#[test]
fn republish_interval_gate() {
    assert!(!should_republish(100, 150, 100));
    assert!(should_republish(100, 200, 100));
    assert!(should_republish(100, 201, 100));
    assert!(!should_republish(100, 200, 0));
}

#[test]
fn push_bounded_vec_keeps_recent_window() {
    let mut values = vec![1_u64, 2, 3];
    push_bounded_vec(&mut values, 4, 3);
    assert_eq!(values, vec![2, 3, 4]);

    push_bounded_vec(&mut values, 5, 1);
    assert_eq!(values, vec![5]);
}

#[test]
fn peer_requires_active_quarantine_skips_missing_peer_record_block() {
    let active_peer = PeerId::random();
    let suspect_peer = PeerId::random();
    let blocked_peer = PeerId::random();
    let missing_record_peer = PeerId::random();
    let healths = HashMap::from([
        (
            active_peer,
            PeerManagerPeerHealth {
                peer_id: active_peer.to_string(),
                status: PeerManagerHealthStatus::Active,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            suspect_peer,
            PeerManagerPeerHealth {
                peer_id: suspect_peer.to_string(),
                status: PeerManagerHealthStatus::Suspect,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            blocked_peer,
            PeerManagerPeerHealth {
                peer_id: blocked_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::RelayBudgetExceeded {
                    relayed_active_peers: 3,
                    active_peer_count: 4,
                    limit_per_mille: 500,
                }],
                discovery_sources: Vec::new(),
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            missing_record_peer,
            PeerManagerPeerHealth {
                peer_id: missing_record_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::MissingPeerRecord],
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    assert!(!peer_requires_active_quarantine(active_peer, &healths));
    assert!(peer_requires_active_quarantine(suspect_peer, &healths));
    assert!(peer_requires_active_quarantine(blocked_peer, &healths));
    assert!(!peer_requires_active_quarantine(
        missing_record_peer,
        &healths
    ));
}

#[test]
fn collect_quarantined_active_peers_only_returns_quarantined_active_set() {
    let active_peer = PeerId::random();
    let suspect_peer = PeerId::random();
    let blocked_peer = PeerId::random();
    let inactive_blocked_peer = PeerId::random();
    let mut active_transport_paths = HashMap::new();
    active_transport_paths.insert(
        active_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            active_peer,
            &"/ip4/127.0.0.1/udp/4103/quic-v1"
                .parse()
                .expect("active endpoint"),
        ),
    );
    active_transport_paths.insert(
        suspect_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            suspect_peer,
            &format!(
                "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
                PeerId::random()
            )
            .parse()
            .expect("suspect endpoint"),
        ),
    );
    active_transport_paths.insert(
        blocked_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            blocked_peer,
            &"/ip4/127.0.0.2/udp/4104/quic-v1"
                .parse()
                .expect("blocked endpoint"),
        ),
    );
    let healths = HashMap::from([
        (
            active_peer,
            PeerManagerPeerHealth {
                peer_id: active_peer.to_string(),
                status: PeerManagerHealthStatus::Active,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            suspect_peer,
            PeerManagerPeerHealth {
                peer_id: suspect_peer.to_string(),
                status: PeerManagerHealthStatus::Suspect,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            blocked_peer,
            PeerManagerPeerHealth {
                peer_id: blocked_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::RelayBudgetExceeded {
                    relayed_active_peers: 3,
                    active_peer_count: 4,
                    limit_per_mille: 500,
                }],
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            inactive_blocked_peer,
            PeerManagerPeerHealth {
                peer_id: inactive_blocked_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    let mut quarantined = collect_quarantined_active_peers(&active_transport_paths, &healths);
    quarantined.sort_unstable_by_key(|peer| peer.to_string());

    let mut expected = vec![blocked_peer, suspect_peer];
    expected.sort_unstable_by_key(|peer| peer.to_string());
    assert_eq!(quarantined, expected);
}

#[test]
fn admitted_active_transport_paths_excludes_non_active_peers_from_health_recompute() {
    let active_peer = PeerId::random();
    let suspect_peer = PeerId::random();
    let missing_record_peer = PeerId::random();
    let mut active_transport_paths = HashMap::new();
    active_transport_paths.insert(
        active_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            active_peer,
            &"/ip4/127.0.0.1/udp/4103/quic-v1"
                .parse()
                .expect("active endpoint"),
        ),
    );
    active_transport_paths.insert(
        suspect_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            suspect_peer,
            &"/ip4/127.0.0.2/udp/4104/quic-v1"
                .parse()
                .expect("suspect endpoint"),
        ),
    );
    active_transport_paths.insert(
        missing_record_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            missing_record_peer,
            &"/ip4/127.0.0.3/udp/4105/quic-v1"
                .parse()
                .expect("missing record endpoint"),
        ),
    );
    let healths = HashMap::from([
        (
            active_peer,
            PeerManagerPeerHealth {
                peer_id: active_peer.to_string(),
                status: PeerManagerHealthStatus::Active,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            suspect_peer,
            PeerManagerPeerHealth {
                peer_id: suspect_peer.to_string(),
                status: PeerManagerHealthStatus::Suspect,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            missing_record_peer,
            PeerManagerPeerHealth {
                peer_id: missing_record_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::MissingPeerRecord],
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    let admitted = admitted_active_transport_paths(&active_transport_paths, &healths);
    let admitted_peers: HashSet<PeerId> = admitted.keys().copied().collect();
    assert_eq!(admitted_peers, HashSet::from([active_peer]));
}

#[test]
fn request_uses_swarm_connected_peers_when_snapshot_is_empty() {
    let deadline = || Instant::now() + Duration::from_secs(10);
    let listener = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen")],
        allow_loopback_external_addrs_for_testing: true,
        peer_manager_policy: PeerManagerPolicy {
            min_active_discovery_sources: 0,
            min_peer_discovery_sources: 0,
            ..PeerManagerPolicy::default()
        },
        ..Libp2pNetworkConfig::default()
    });
    wait_until("listener addr", deadline(), || {
        !listener.listening_addrs().is_empty()
    });
    listener
        .register_handler(
            "/aw/test/snapshot-empty-request/1.0.0",
            Box::new(|payload| {
                let mut response = payload.to_vec();
                response.extend_from_slice(b"-ok");
                Ok(response)
            }),
        )
        .expect("register handler");

    let dial_addr = listener
        .listening_addrs()
        .into_iter()
        .find(|addr| addr.to_string().contains("127.0.0.1"))
        .expect("listener addr")
        .with(libp2p::multiaddr::Protocol::P2p(listener.peer_id()));
    let dialer = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen")],
        bootstrap_peers: vec![dial_addr],
        allow_loopback_external_addrs_for_testing: true,
        peer_manager_policy: PeerManagerPolicy {
            min_active_discovery_sources: 0,
            min_peer_discovery_sources: 0,
            ..PeerManagerPolicy::default()
        },
        ..Libp2pNetworkConfig::default()
    });
    wait_until("dialer connected", deadline(), || {
        dialer.connected_peers().contains(&listener.peer_id())
    });
    dialer
        .connected_peers
        .lock()
        .expect("lock connected peers")
        .clear();

    wait_until("request uses swarm connection", deadline(), || match dialer
        .request("/aw/test/snapshot-empty-request/1.0.0", b"ping")
    {
        Ok(response) => response == b"ping-ok".to_vec(),
        Err(WorldError::NetworkProtocolUnavailable { .. }) => false,
        Err(err) => panic!("unexpected request error: {err:?}"),
    });

    wait_until(
        "provider request uses swarm connection",
        deadline(),
        || match dialer.request_with_providers(
            "/aw/test/snapshot-empty-request/1.0.0",
            b"provider-ping",
            &[listener.peer_id().to_string()],
        ) {
            Ok(response) => response == b"provider-ping-ok".to_vec(),
            Err(WorldError::NetworkProtocolUnavailable { .. }) => false,
            Err(err) => panic!("unexpected provider request error: {err:?}"),
        },
    );
}

#[test]
fn try_send_command_reports_queue_disconnect() {
    let (sender, receiver) = mpsc::channel(1);
    drop(receiver);
    let (response, _response_rx) = std::sync::mpsc::channel();
    let err = try_send_command(
        &sender,
        Command::Subscribe {
            topic: "topic-a".to_string(),
            response,
        },
    )
    .expect_err("send must fail once receiver is dropped");
    assert!(matches!(
        err,
        WorldError::NetworkProtocolUnavailable { ref protocol }
            if protocol.contains("disconnected")
    ));
}

#[test]
fn filter_request_peers_by_lane_prefers_capable_peer_records() {
    let blob_peer_key = Keypair::generate_ed25519();
    let sync_only_peer_key = Keypair::generate_ed25519();
    let blob_peer_id = PeerId::from(blob_peer_key.public());
    let sync_only_peer_id = PeerId::from(sync_only_peer_key.public());
    let mut discovered = PeerRecordCache::default();
    discovered.insert(
        blob_peer_id,
        sign_peer_record(
            &PeerRecord {
                peer_id: blob_peer_id.to_string(),
                node_id: "blob-peer".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::FullStorage.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Hybrid,
                reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
                capability_lanes: vec![NetworkLane::BlobState, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: now_ms(),
                ttl_ms: 60_000,
            },
            &blob_peer_key,
        )
        .expect("blob peer record"),
    );
    discovered.insert(
        sync_only_peer_id,
        sign_peer_record(
            &PeerRecord {
                peer_id: sync_only_peer_id.to_string(),
                node_id: "sync-peer".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Private,
                reachability_class: crate::dht::PeerReachabilityClass::Private,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
                capability_lanes: vec![NetworkLane::Sync, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: now_ms(),
                ttl_ms: 60_000,
            },
            &sync_only_peer_key,
        )
        .expect("sync peer record"),
    );

    let filtered = filter_request_peers_by_lane(
        vec![sync_only_peer_id, blob_peer_id],
        "/aw/node/replication/fetch-blob/1.0.0",
        &discovered,
    );

    assert_eq!(filtered, vec![blob_peer_id]);
}

#[test]
fn filter_request_peers_by_lane_excludes_unknown_peers_when_capable_record_exists() {
    let blob_peer_key = Keypair::generate_ed25519();
    let unknown_peer_key = Keypair::generate_ed25519();
    let blob_peer_id = PeerId::from(blob_peer_key.public());
    let unknown_peer_id = PeerId::from(unknown_peer_key.public());
    let mut discovered = PeerRecordCache::default();
    discovered.insert(
        blob_peer_id,
        sign_peer_record(
            &PeerRecord {
                peer_id: blob_peer_id.to_string(),
                node_id: "blob-peer".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::FullStorage.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Hybrid,
                reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
                capability_lanes: vec![NetworkLane::BlobState, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: now_ms(),
                ttl_ms: 60_000,
            },
            &blob_peer_key,
        )
        .expect("blob peer record"),
    );

    let filtered = filter_request_peers_by_lane(
        vec![unknown_peer_id, blob_peer_id],
        "/aw/node/replication/fetch-blob/1.0.0",
        &discovered,
    );

    assert_eq!(filtered, vec![blob_peer_id]);
}

#[test]
fn filter_request_peers_by_lane_falls_back_to_unknown_peers_when_no_records_match() {
    let sync_only_peer_key = Keypair::generate_ed25519();
    let unknown_peer_key = Keypair::generate_ed25519();
    let sync_only_peer_id = PeerId::from(sync_only_peer_key.public());
    let unknown_peer_id = PeerId::from(unknown_peer_key.public());
    let mut discovered = PeerRecordCache::default();
    discovered.insert(
        sync_only_peer_id,
        sign_peer_record(
            &PeerRecord {
                peer_id: sync_only_peer_id.to_string(),
                node_id: "sync-peer".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Private,
                reachability_class: crate::dht::PeerReachabilityClass::Private,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
                capability_lanes: vec![NetworkLane::Sync, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: now_ms(),
                ttl_ms: 60_000,
            },
            &sync_only_peer_key,
        )
        .expect("sync peer record"),
    );

    let filtered = filter_request_peers_by_lane(
        vec![sync_only_peer_id, unknown_peer_id],
        "/aw/node/replication/fetch-blob/1.0.0",
        &discovered,
    );

    assert_eq!(filtered, vec![unknown_peer_id]);
}

#[test]
fn filter_request_peers_by_lane_returns_empty_when_all_known_peers_lack_capability() {
    let sync_only_peer_key = Keypair::generate_ed25519();
    let sync_only_peer_id = PeerId::from(sync_only_peer_key.public());
    let mut discovered = PeerRecordCache::default();
    discovered.insert(
        sync_only_peer_id,
        sign_peer_record(
            &PeerRecord {
                peer_id: sync_only_peer_id.to_string(),
                node_id: "sync-peer".to_string(),
                world_id: "world-a".to_string(),
                network_id: "network-a".to_string(),
                node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Private,
                reachability_class: crate::dht::PeerReachabilityClass::Private,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
                capability_lanes: vec![NetworkLane::Sync, NetworkLane::Control],
                source_operator: None,
                source_asn: None,
                published_at_ms: now_ms(),
                ttl_ms: 60_000,
            },
            &sync_only_peer_key,
        )
        .expect("sync peer record"),
    );

    let filtered = filter_request_peers_by_lane(
        vec![sync_only_peer_id],
        "/aw/node/replication/fetch-blob/1.0.0",
        &discovered,
    );

    assert!(filtered.is_empty());
}

#[test]
fn peer_record_transport_paths_rank_direct_before_hole_punch_before_relay() {
    let keypair = Keypair::generate_ed25519();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: PeerId::from(keypair.public()).to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec![
                "/ip4/127.0.0.1/tcp/4102".to_string(),
                "/ip4/127.0.0.1/udp/4103/quic-v1".to_string(),
                "/ip4/127.0.0.1/tcp/4102".to_string(),
            ],
            hole_punch_addrs: vec!["/ip4/127.0.0.1/udp/5103/quic-v1".to_string()],
            relay_addrs: vec!["/dns4/relay.example/tcp/443/p2p-circuit".to_string()],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 77,
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");

    let paths = peer_record_transport_paths(&signed).expect("transport paths");
    assert_eq!(paths.len(), 4);
    assert_eq!(paths[0].kind, TransportPathKind::Direct);
    assert_eq!(paths[0].flavor, TransportSessionFlavor::Quic);
    assert_eq!(paths[0].security, TransportSecurity::QuicTls);
    assert_eq!(paths[0].muxer, TransportMuxer::Quic);
    assert_eq!(paths[1].kind, TransportPathKind::Direct);
    assert_eq!(paths[1].flavor, TransportSessionFlavor::TcpNoiseYamux);
    assert_eq!(paths[1].security, TransportSecurity::Noise);
    assert_eq!(paths[1].muxer, TransportMuxer::Yamux);
    assert_eq!(paths[2].kind, TransportPathKind::HolePunched);
    assert_eq!(paths[2].flavor, TransportSessionFlavor::Quic);
    assert_eq!(paths[3].kind, TransportPathKind::RelayReserved);
    assert_eq!(paths[3].flavor, TransportSessionFlavor::RelayTunnel);
    assert!(paths[0].addr.to_string().contains("/p2p/"));
}

#[test]
fn preferred_transport_path_skips_direct_and_falls_back_to_hole_punch_before_relay() {
    let keypair = Keypair::generate_ed25519();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: PeerId::from(keypair.public()).to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec![
                "/ip4/127.0.0.1/udp/4103/quic-v1".to_string(),
                "/ip4/127.0.0.1/tcp/4102".to_string(),
            ],
            hole_punch_addrs: vec!["/ip4/127.0.0.1/udp/5103/quic-v1".to_string()],
            relay_addrs: vec!["/dns4/relay.example/tcp/443/p2p-circuit".to_string()],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 77,
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");

    let paths = peer_record_transport_paths(&signed).expect("transport paths");
    let failed: HashSet<String> = [paths[0].label(), paths[1].label()].into_iter().collect();
    let selected =
        select_preferred_transport_path(paths.as_slice(), &failed).expect("fallback path");
    assert_eq!(selected.kind, TransportPathKind::HolePunched);
    assert_eq!(selected.flavor, TransportSessionFlavor::Quic);
}

#[test]
fn sync_known_transport_paths_removes_stale_failed_labels() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec![
                "/ip4/127.0.0.1/udp/4103/quic-v1".to_string(),
                "/ip4/127.0.0.1/tcp/4102".to_string(),
            ],
            hole_punch_addrs: vec!["/ip4/127.0.0.1/udp/5103/quic-v1".to_string()],
            relay_addrs: vec!["/dns4/relay.example/tcp/443/p2p-circuit".to_string()],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 77,
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");
    let initial_paths = peer_record_transport_paths(&signed).expect("transport paths");

    let mut known = HashMap::new();
    let mut failed: HashSet<String> = [initial_paths[3].label()].into_iter().collect();
    sync_known_transport_paths(&mut known, &mut failed, peer_id, initial_paths.clone());
    sync_known_transport_paths(
        &mut known,
        &mut failed,
        peer_id,
        initial_paths[..3].to_vec(),
    );
    assert!(failed.is_empty());
}

#[test]
fn active_transport_path_from_endpoint_infers_quic_and_relay_semantics() {
    let peer_id = PeerId::random();

    let quic_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        peer_id,
        &"/ip4/127.0.0.1/udp/4103/quic-v1"
            .parse()
            .expect("quic endpoint"),
    );
    assert_eq!(quic_path.kind, TransportPathKind::Direct);
    assert_eq!(quic_path.flavor, TransportSessionFlavor::Quic);
    assert_eq!(quic_path.security, TransportSecurity::QuicTls);
    assert_eq!(quic_path.muxer, TransportMuxer::Quic);

    let relay_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        peer_id,
        &format!(
            "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
            PeerId::random()
        )
        .parse()
        .expect("relay endpoint"),
    );
    assert_eq!(relay_path.kind, TransportPathKind::RelayReserved);
    assert_eq!(relay_path.flavor, TransportSessionFlavor::RelayTunnel);
    assert_eq!(relay_path.security, TransportSecurity::Noise);
    assert_eq!(relay_path.muxer, TransportMuxer::Yamux);
}

#[test]
fn active_transport_path_from_endpoint_keeps_hole_punch_kind_when_known() {
    let peer_id = PeerId::random();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: Vec::new(),
            hole_punch_addrs: vec!["/ip4/127.0.0.1/udp/5103/quic-v1".to_string()],
            relay_addrs: Vec::new(),
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 77,
            ttl_ms: 60_000,
        },
        &Keypair::generate_ed25519(),
    )
    .expect("sign peer record");
    let mut known = HashMap::new();
    known.insert(
        peer_id,
        peer_record_transport_paths(&signed).expect("transport paths"),
    );
    let hole_punched_path = active_transport_path_from_endpoint(
        &known,
        peer_id,
        &"/ip4/127.0.0.1/udp/5103/quic-v1"
            .parse()
            .expect("hole-punch endpoint"),
    );
    assert_eq!(hole_punched_path.kind, TransportPathKind::HolePunched);
    assert_eq!(hole_punched_path.flavor, TransportSessionFlavor::Quic);
}

#[test]
fn active_transport_path_from_endpoint_maps_inbound_ephemeral_tcp_port_to_known_direct_path() {
    let peer_id = PeerId::random();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec!["/ip4/39.104.205.67/tcp/5612".to_string()],
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 77,
            ttl_ms: 60_000,
        },
        &Keypair::generate_ed25519(),
    )
    .expect("sign peer record");
    let mut known = HashMap::new();
    known.insert(
        peer_id,
        peer_record_transport_paths(&signed).expect("transport paths"),
    );

    let mapped_path = active_transport_path_from_endpoint(
        &known,
        peer_id,
        &format!("/ip4/39.104.205.67/tcp/60900/p2p/{peer_id}")
            .parse()
            .expect("listener endpoint"),
    );

    assert_eq!(
        mapped_path.addr.to_string(),
        format!("/ip4/39.104.205.67/tcp/5612/p2p/{peer_id}")
    );
    assert_eq!(mapped_path.kind, TransportPathKind::Direct);
    assert_eq!(mapped_path.flavor, TransportSessionFlavor::TcpNoiseYamux);
}
