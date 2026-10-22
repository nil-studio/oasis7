use super::*;
use crate::libp2p_net::runtime_loop::active_transport_request_peers;

#[test]
fn filter_request_peers_by_health_prefers_non_suspect_peers() {
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();
    let peer_c = PeerId::random();
    let peers = vec![peer_a, peer_b, peer_c];
    let healths = HashMap::from([
        (
            peer_a,
            PeerManagerPeerHealth {
                peer_id: peer_a.to_string(),
                status: PeerManagerHealthStatus::Suspect,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            peer_b,
            PeerManagerPeerHealth {
                peer_id: peer_b.to_string(),
                status: PeerManagerHealthStatus::Active,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            peer_c,
            PeerManagerPeerHealth {
                peer_id: peer_c.to_string(),
                status: PeerManagerHealthStatus::Candidate,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    let filtered = filter_request_peers_by_health(peers, &healths);
    assert_eq!(filtered, vec![peer_b, peer_c, peer_a]);
}

#[test]
fn filter_request_peers_by_health_excludes_all_blocked_peers() {
    let peer_a = PeerId::random();
    let peer_b = PeerId::random();
    let peers = vec![peer_a, peer_b];
    let healths = HashMap::from([
        (
            peer_a,
            PeerManagerPeerHealth {
                peer_id: peer_a.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            peer_b,
            PeerManagerPeerHealth {
                peer_id: peer_b.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: Vec::new(),
                discovery_sources: Vec::new(),
                active_path_kind: None,
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    let filtered = filter_request_peers_by_health(peers, &healths);
    assert!(filtered.is_empty());
}

#[test]
fn filter_request_peers_by_health_keeps_record_exchange_pending_blocked_peer_as_fallback() {
    let active_peer = PeerId::random();
    let soft_blocked_peer = PeerId::random();
    let hard_blocked_peer = PeerId::random();
    let peers = vec![soft_blocked_peer, hard_blocked_peer, active_peer];
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
            soft_blocked_peer,
            PeerManagerPeerHealth {
                peer_id: soft_blocked_peer.to_string(),
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
        ),
        (
            hard_blocked_peer,
            PeerManagerPeerHealth {
                peer_id: hard_blocked_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::RelayBudgetExceeded {
                    relayed_active_peers: 2,
                    active_peer_count: 2,
                    limit_per_mille: 500,
                }],
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    let filtered = filter_request_peers_by_health(peers, &healths);
    assert_eq!(filtered, vec![active_peer, soft_blocked_peer]);
}

#[test]
fn active_transport_request_peers_keeps_record_exchange_pending_bootstrap_peer() {
    let bootstrap_peer = PeerId::random();
    let hard_blocked_peer = PeerId::random();
    let healths = HashMap::from([
        (
            bootstrap_peer,
            PeerManagerPeerHealth {
                peer_id: bootstrap_peer.to_string(),
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
        ),
        (
            hard_blocked_peer,
            PeerManagerPeerHealth {
                peer_id: hard_blocked_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::RelayBudgetExceeded {
                    relayed_active_peers: 2,
                    active_peer_count: 2,
                    limit_per_mille: 500,
                }],
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
    ]);

    assert_eq!(
        active_transport_request_peers(&healths),
        vec![bootstrap_peer]
    );
}

#[test]
fn active_transport_request_peers_returns_empty_without_active_path() {
    let bootstrap_peer = PeerId::random();
    let healths = HashMap::from([(
        bootstrap_peer,
        PeerManagerPeerHealth {
            peer_id: bootstrap_peer.to_string(),
            status: PeerManagerHealthStatus::Blocked,
            issues: vec![
                PeerManagerHealthIssue::MissingPeerRecord,
                PeerManagerHealthIssue::InsufficientActiveDiscoverySources {
                    observed_sources: 1,
                    required_sources: 2,
                },
            ],
            discovery_sources: Vec::new(),
            active_path_kind: None,
            source_operator: None,
            source_asn: None,
        },
    )]);

    assert!(active_transport_request_peers(&healths).is_empty());
}

#[test]
fn request_filtering_keeps_unknown_bootstrap_fallback_after_hard_blocked_capable_peer() {
    let capable_key = Keypair::generate_ed25519();
    let capable_peer = PeerId::from(capable_key.public());
    let bootstrap_peer = PeerId::random();
    let mut discovered = PeerRecordCache::default();
    discovered.insert(
        capable_peer,
        sign_peer_record(
            &PeerRecord {
                peer_id: capable_peer.to_string(),
                node_id: "capable-peer".to_string(),
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
                published_at_ms: crate::libp2p_net::now_ms(),
                ttl_ms: 60_000,
            },
            &capable_key,
        )
        .expect("capable peer record"),
    );
    let healths = HashMap::from([
        (
            capable_peer,
            PeerManagerPeerHealth {
                peer_id: capable_peer.to_string(),
                status: PeerManagerHealthStatus::Blocked,
                issues: vec![PeerManagerHealthIssue::RelayBudgetExceeded {
                    relayed_active_peers: 2,
                    active_peer_count: 2,
                    limit_per_mille: 500,
                }],
                discovery_sources: Vec::new(),
                active_path_kind: Some("relay_reserved".to_string()),
                source_operator: None,
                source_asn: None,
            },
        ),
        (
            bootstrap_peer,
            PeerManagerPeerHealth {
                peer_id: bootstrap_peer.to_string(),
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
        ),
    ]);

    let filtered_by_health =
        filter_request_peers_by_health(vec![capable_peer, bootstrap_peer], &healths);
    let filtered = filter_request_peers_by_lane(
        filtered_by_health,
        "/aw/node/replication/fetch-blob/1.0.0",
        &discovered,
    );

    assert_eq!(filtered, vec![bootstrap_peer]);
}
