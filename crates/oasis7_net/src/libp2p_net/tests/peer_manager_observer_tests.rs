use super::*;
use crate::libp2p_net::peer_discovery_observations::DiscoveryObservations;
use crate::libp2p_net::peer_manager::recompute_peer_manager_healths;
use crate::proto_dht::{PeerDiscoverySource, PeerReachabilityClass, SignedPeerRecord};

fn sample_record_with_role(
    peer_id: PeerId,
    node_role: PeerNodeRole,
    discovery_sources: Vec<PeerDiscoverySource>,
) -> SignedPeerRecord {
    SignedPeerRecord {
        record: oasis7_proto::distributed_dht::PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: format!("node-{peer_id}"),
            world_id: "world-a".to_string(),
            network_id: "world-a".to_string(),
            node_role: node_role.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: PeerReachabilityClass::Hybrid,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources,
            capability_lanes: node_role.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: crate::libp2p_net::now_ms(),
            ttl_ms: 60_000,
        },
        identity_public_key_protobuf_hex: "abcd".to_string(),
        signature_hex: "beef".to_string(),
        publication: None,
    }
}

fn direct_path(peer_id: PeerId, addr: &str) -> TransportPath {
    active_transport_path_from_endpoint(&HashMap::new(), peer_id, &addr.parse().expect("multiaddr"))
}

#[test]
fn recompute_counts_claimed_observer_light_toward_subnet_share_limits() {
    let peers = [
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
    ];
    let discovery_sources = vec![
        PeerDiscoverySource::StaticBootstrap,
        PeerDiscoverySource::Dht,
    ];
    let discovered = PeerRecordCache::from([
        (
            peers[0],
            sample_record_with_role(
                peers[0],
                PeerNodeRole::FullStorage,
                discovery_sources.clone(),
            ),
        ),
        (
            peers[1],
            sample_record_with_role(
                peers[1],
                PeerNodeRole::FullStorage,
                discovery_sources.clone(),
            ),
        ),
        (
            peers[2],
            sample_record_with_role(
                peers[2],
                PeerNodeRole::FullStorage,
                discovery_sources.clone(),
            ),
        ),
        (
            peers[3],
            sample_record_with_role(
                peers[3],
                PeerNodeRole::ObserverLight,
                discovery_sources.clone(),
            ),
        ),
    ]);
    let active = HashMap::from([
        (
            peers[0],
            direct_path(peers[0], "/ip4/192.168.10.1/udp/4101/quic-v1"),
        ),
        (
            peers[1],
            direct_path(peers[1], "/ip4/192.168.10.2/udp/4102/quic-v1"),
        ),
        (
            peers[2],
            direct_path(peers[2], "/ip4/10.20.30.40/udp/4103/quic-v1"),
        ),
        (
            peers[3],
            direct_path(peers[3], "/ip4/192.168.10.89/udp/4104/quic-v1"),
        ),
    ]);

    let healths = recompute_peer_manager_healths(
        &discovered,
        &DiscoveryObservations::default(),
        &active,
        &PeerManagerPolicy::default(),
    );
    for peer in peers {
        assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Blocked);
    }
    for peer in [peers[0], peers[1], peers[3]] {
        assert!(
            healths[&peer].issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::Ipv4SubnetConcentration {
                    peers_in_bucket: 3,
                    active_peer_count: 4,
                    ..
                }
            )),
            "self-claimed observer roles must not waive subnet budgets"
        );
    }
}

#[test]
fn recompute_does_not_trust_observer_light_claimed_discovery_sources() {
    let provider = PeerId::random();
    let observer = PeerId::random();
    let discovered = PeerRecordCache::from([
        (
            provider,
            sample_record_with_role(
                provider,
                PeerNodeRole::FullStorage,
                vec![PeerDiscoverySource::Dht],
            ),
        ),
        (
            observer,
            sample_record_with_role(
                observer,
                PeerNodeRole::ObserverLight,
                vec![PeerDiscoverySource::Rendezvous],
            ),
        ),
    ]);
    let active = HashMap::from([
        (
            provider,
            direct_path(provider, "/ip4/10.0.0.1/udp/4101/quic-v1"),
        ),
        (
            observer,
            direct_path(observer, "/ip4/10.0.0.89/udp/4104/quic-v1"),
        ),
    ]);
    let policy = PeerManagerPolicy {
        min_peer_discovery_sources: 1,
        min_active_discovery_sources: 2,
        ..PeerManagerPolicy::default()
    };

    let healths = recompute_peer_manager_healths(
        &discovered,
        &DiscoveryObservations::default(),
        &active,
        &policy,
    );
    for peer in [provider, observer] {
        assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Suspect);
        assert!(
            healths[&peer].issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::InsufficientActiveDiscoverySources { .. }
            )),
            "observer-light self claims must not prove independent discovery: {:?}",
            healths[&peer].issues
        );
    }
}

#[test]
fn recompute_still_blocks_provider_subnet_concentration() {
    let peers = [
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
        PeerId::random(),
    ];
    let discovery_sources = vec![
        PeerDiscoverySource::StaticBootstrap,
        PeerDiscoverySource::Dht,
    ];
    let discovered = PeerRecordCache::from(peers.map(|peer| {
        (
            peer,
            sample_record_with_role(peer, PeerNodeRole::FullStorage, discovery_sources.clone()),
        )
    }));
    let active = HashMap::from([
        (
            peers[0],
            direct_path(peers[0], "/ip4/192.168.10.1/udp/4101/quic-v1"),
        ),
        (
            peers[1],
            direct_path(peers[1], "/ip4/192.168.10.2/udp/4102/quic-v1"),
        ),
        (
            peers[2],
            direct_path(peers[2], "/ip4/10.20.30.40/udp/4103/quic-v1"),
        ),
        (
            peers[3],
            direct_path(peers[3], "/ip4/192.168.10.89/udp/4104/quic-v1"),
        ),
    ]);

    let healths = recompute_peer_manager_healths(
        &discovered,
        &DiscoveryObservations::default(),
        &active,
        &PeerManagerPolicy::default(),
    );
    for peer in [peers[0], peers[1], peers[3]] {
        assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Blocked);
        assert!(healths[&peer].issues.iter().any(|issue| matches!(
            issue,
            PeerManagerHealthIssue::Ipv4SubnetConcentration {
                peers_in_bucket: 3,
                active_peer_count: 4,
                ..
            }
        )));
    }
    assert_eq!(healths[&peers[2]].status, PeerManagerHealthStatus::Blocked);
}
