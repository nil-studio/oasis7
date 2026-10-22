use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;

use libp2p::PeerId;
use libp2p::multiaddr::Protocol;
use oasis7_proto::distributed_dht::{PeerDiscoverySource, SignedPeerRecord};

use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_manager_active_set::ActivePeerSetStats;
use super::transport_paths::{TransportPath, TransportPathKind};

pub use crate::peer_manager_policy::PeerManagerPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PeerManagerHealthStatus {
    Active,
    Candidate,
    Suspect,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerManagerHealthIssue {
    MissingPeerRecord,
    DiscoveryIndependenceUnverified,
    SingleSourceDiscovery {
        observed_sources: usize,
        required_sources: usize,
    },
    InsufficientActiveDiscoverySources {
        observed_sources: usize,
        required_sources: usize,
    },
    Ipv4SubnetConcentration {
        subnet: String,
        peers_in_bucket: usize,
        active_peer_count: usize,
        limit_per_mille: u16,
    },
    Ipv4SubnetActivePeerLimit {
        subnet: String,
        peers_in_bucket: usize,
        active_peer_count: usize,
        max_active_peers: usize,
    },
    RelayDomainConcentration {
        relay_domain: String,
        peers_in_bucket: usize,
        active_peer_count: usize,
        limit_per_mille: u16,
    },
    OperatorConcentration {
        source_operator: String,
        peers_in_bucket: usize,
        active_peer_count: usize,
        limit_per_mille: u16,
    },
    AsnConcentration {
        source_asn: String,
        peers_in_bucket: usize,
        active_peer_count: usize,
        limit_per_mille: u16,
    },
    RelayBudgetExceeded {
        relayed_active_peers: usize,
        active_peer_count: usize,
        limit_per_mille: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerManagerPeerHealth {
    pub peer_id: String,
    pub status: PeerManagerHealthStatus,
    pub issues: Vec<PeerManagerHealthIssue>,
    pub discovery_sources: Vec<PeerDiscoverySource>,
    pub active_path_kind: Option<String>,
    pub source_operator: Option<String>,
    pub source_asn: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerManagerBlockArtifact {
    pub peer_id: String,
    pub status: PeerManagerHealthStatus,
    pub issues: Vec<PeerManagerHealthIssue>,
    pub active_path_kind: Option<String>,
    pub source_operator: Option<String>,
    pub source_asn: Option<String>,
    pub first_blocked_at_ms: i64,
    pub last_blocked_at_ms: i64,
    pub last_cleared_at_ms: Option<i64>,
}

pub(super) fn recompute_peer_manager_healths(
    discovered_peer_records: &HashMap<PeerId, SignedPeerRecord>,
    observations: &DiscoveryObservations,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    policy: &PeerManagerPolicy,
) -> HashMap<PeerId, PeerManagerPeerHealth> {
    let active_set_stats = ActivePeerSetStats::new(
        discovered_peer_records,
        observations,
        active_transport_paths,
    );
    let insufficient_active_discovery_sources = active_set_stats.active_peer_count > 0
        && active_set_stats.active_discovery_sources.len() < policy.min_active_discovery_sources;
    let relay_budget_exceeded = exceeds_share_limit(
        active_set_stats.relayed_active_peers,
        active_set_stats.active_peer_count,
        policy.max_relayed_active_peer_share_per_mille,
    );

    let peer_ids: HashSet<PeerId> = discovered_peer_records
        .keys()
        .copied()
        .chain(active_transport_paths.keys().copied())
        .collect();
    let mut healths = HashMap::new();

    for peer_id in peer_ids {
        let record = discovered_peer_records.get(&peer_id);
        let active_path = active_transport_paths.get(&peer_id);
        let mut issues = Vec::new();
        let mut hard_block = false;
        let mut soft_block = false;
        let discovery_sources = record
            .map(|record| observations.observed_sources(record))
            .unwrap_or_default();
        // Claims remain available in the signed record, never as attributed topology.
        let source_operator = None;
        let source_asn = None;

        if record.is_none() {
            issues.push(PeerManagerHealthIssue::MissingPeerRecord);
            if active_path.is_some() {
                soft_block = true;
            } else {
                hard_block = true;
            }
        } else if record.is_some_and(|record| {
            observations.trusted_source_labels(record).len() < policy.min_peer_discovery_sources
        }) {
            issues.push(PeerManagerHealthIssue::SingleSourceDiscovery {
                observed_sources: record
                    .map(|record| observations.trusted_source_labels(record).len())
                    .unwrap_or(0),
                required_sources: policy.min_peer_discovery_sources,
            });
        }

        if record.is_some_and(|record| !observations.is_static_bootstrap(record)) {
            issues.push(PeerManagerHealthIssue::DiscoveryIndependenceUnverified);
        }

        if let Some(active_path) = active_path {
            if insufficient_active_discovery_sources {
                issues.push(PeerManagerHealthIssue::InsufficientActiveDiscoverySources {
                    observed_sources: active_set_stats.active_discovery_sources.len(),
                    required_sources: policy.min_active_discovery_sources,
                });
            }
            if let Some(bucket) = ipv4_subnet_bucket(active_path) {
                let bucket_count = active_set_stats
                    .ipv4_subnet_counts
                    .get(bucket.as_str())
                    .copied()
                    .unwrap_or(0);
                if let Some(limit) = policy.max_ipv4_subnet_active_peers {
                    if bucket_count > limit {
                        issues.push(PeerManagerHealthIssue::Ipv4SubnetActivePeerLimit {
                            subnet: bucket,
                            peers_in_bucket: bucket_count,
                            active_peer_count: active_set_stats.active_peer_count,
                            max_active_peers: limit,
                        });
                        hard_block = true;
                    }
                } else if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && meets_or_exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.block_ipv4_subnet_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::Ipv4SubnetConcentration {
                        subnet: bucket,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.block_ipv4_subnet_share_per_mille,
                    });
                    hard_block = true;
                } else if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.max_ipv4_subnet_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::Ipv4SubnetConcentration {
                        subnet: bucket,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.max_ipv4_subnet_share_per_mille,
                    });
                }
            }
            if matches!(active_path.kind, TransportPathKind::RelayReserved) {
                if relay_budget_exceeded {
                    issues.push(PeerManagerHealthIssue::RelayBudgetExceeded {
                        relayed_active_peers: active_set_stats.relayed_active_peers,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.max_relayed_active_peer_share_per_mille,
                    });
                }
                if let Some(domain) = relay_domain(active_path) {
                    let bucket_count = active_set_stats
                        .relay_domain_counts
                        .get(domain.as_str())
                        .copied()
                        .unwrap_or(0);
                    if policy.applies_share_limits(active_set_stats.active_peer_count)
                        && bucket_count >= 2
                        && meets_or_exceeds_share_limit(
                            bucket_count,
                            active_set_stats.active_peer_count,
                            policy.block_relay_domain_share_per_mille,
                        )
                    {
                        issues.push(PeerManagerHealthIssue::RelayDomainConcentration {
                            relay_domain: domain,
                            peers_in_bucket: bucket_count,
                            active_peer_count: active_set_stats.active_peer_count,
                            limit_per_mille: policy.block_relay_domain_share_per_mille,
                        });
                        hard_block = true;
                    } else if policy.applies_share_limits(active_set_stats.active_peer_count)
                        && bucket_count >= 2
                        && exceeds_share_limit(
                            bucket_count,
                            active_set_stats.active_peer_count,
                            policy.max_relay_domain_share_per_mille,
                        )
                    {
                        issues.push(PeerManagerHealthIssue::RelayDomainConcentration {
                            relay_domain: domain,
                            peers_in_bucket: bucket_count,
                            active_peer_count: active_set_stats.active_peer_count,
                            limit_per_mille: policy.max_relay_domain_share_per_mille,
                        });
                    }
                }
            }
            {
                let source_operator = "unknown".to_string();
                let bucket_count = active_set_stats
                    .operator_counts
                    .get(source_operator.as_str())
                    .copied()
                    .unwrap_or(0);
                if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && meets_or_exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.block_operator_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::OperatorConcentration {
                        source_operator,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.block_operator_share_per_mille,
                    });
                    hard_block = true;
                } else if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.max_operator_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::OperatorConcentration {
                        source_operator,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.max_operator_share_per_mille,
                    });
                }
            }
            {
                let source_asn = "unknown".to_string();
                let bucket_count = active_set_stats
                    .asn_counts
                    .get(source_asn.as_str())
                    .copied()
                    .unwrap_or(0);
                if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && meets_or_exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.block_asn_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::AsnConcentration {
                        source_asn,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.block_asn_share_per_mille,
                    });
                    hard_block = true;
                } else if policy.applies_share_limits(active_set_stats.active_peer_count)
                    && bucket_count >= 2
                    && exceeds_share_limit(
                        bucket_count,
                        active_set_stats.active_peer_count,
                        policy.max_asn_share_per_mille,
                    )
                {
                    issues.push(PeerManagerHealthIssue::AsnConcentration {
                        source_asn,
                        peers_in_bucket: bucket_count,
                        active_peer_count: active_set_stats.active_peer_count,
                        limit_per_mille: policy.max_asn_share_per_mille,
                    });
                }
            }
        }

        let status = if hard_block || soft_block {
            PeerManagerHealthStatus::Blocked
        } else if !issues.is_empty() {
            PeerManagerHealthStatus::Suspect
        } else if active_path.is_some() {
            PeerManagerHealthStatus::Active
        } else {
            PeerManagerHealthStatus::Candidate
        };

        healths.insert(
            peer_id,
            PeerManagerPeerHealth {
                peer_id: peer_id.to_string(),
                status,
                issues,
                discovery_sources,
                active_path_kind: active_path.map(|path| path.kind_label().to_string()),
                source_operator,
                source_asn,
            },
        );
    }

    healths
}

pub(super) fn peer_health_is_record_exchange_pending(health: &PeerManagerPeerHealth) -> bool {
    !health.issues.is_empty()
        && health
            .issues
            .iter()
            .all(peer_health_issue_is_record_exchange_pending)
}

pub(super) fn peer_health_issue_is_record_exchange_pending(issue: &PeerManagerHealthIssue) -> bool {
    matches!(
        issue,
        PeerManagerHealthIssue::MissingPeerRecord
            | PeerManagerHealthIssue::DiscoveryIndependenceUnverified
            | PeerManagerHealthIssue::InsufficientActiveDiscoverySources { .. }
            | PeerManagerHealthIssue::SingleSourceDiscovery { .. }
    )
}

pub(super) fn exceeds_share_limit(count: usize, total: usize, limit_per_mille: u16) -> bool {
    if total == 0 {
        return false;
    }
    count.saturating_mul(1000) > total.saturating_mul(limit_per_mille as usize)
}

pub(super) fn meets_or_exceeds_share_limit(
    count: usize,
    total: usize,
    limit_per_mille: u16,
) -> bool {
    if total == 0 {
        return false;
    }
    count.saturating_mul(1000) >= total.saturating_mul(limit_per_mille as usize)
}

pub(super) fn ipv4_subnet_bucket(path: &TransportPath) -> Option<String> {
    path.addr.iter().find_map(|protocol| match protocol {
        Protocol::Ip4(ip) if !ip.is_loopback() => Some(ipv4_bucket(ip)),
        _ => None,
    })
}

pub(super) fn relay_domain(path: &TransportPath) -> Option<String> {
    path.addr.iter().find_map(|protocol| match protocol {
        Protocol::Dns(domain) | Protocol::Dns4(domain) | Protocol::Dns6(domain) => {
            Some(domain.to_string())
        }
        Protocol::Ip4(ip) => Some(ip.to_string()),
        Protocol::Ip6(ip) => Some(ip.to_string()),
        _ => None,
    })
}

fn ipv4_bucket(ip: Ipv4Addr) -> String {
    let octets = ip.octets();
    format!("{}.{}.{}", octets[0], octets[1], octets[2])
}

#[cfg(test)]
pub(super) fn discovery_source_label(source: PeerDiscoverySource) -> &'static str {
    match source {
        PeerDiscoverySource::StaticBootstrap => "static_bootstrap",
        PeerDiscoverySource::Dht => "dht",
        PeerDiscoverySource::Rendezvous => "rendezvous",
        PeerDiscoverySource::PeerExchange => "peer_exchange",
        PeerDiscoverySource::Manual => "manual",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto_dht::{
        PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass,
    };
    use libp2p::Multiaddr;

    use super::super::transport_paths::{
        TransportMuxer, TransportSecurity, TransportSessionFlavor,
    };

    fn sample_record(
        peer_id: PeerId,
        discovery_sources: Vec<PeerDiscoverySource>,
    ) -> SignedPeerRecord {
        SignedPeerRecord {
            record: oasis7_proto::distributed_dht::PeerRecord {
                peer_id: peer_id.to_string(),
                node_id: format!("node-{peer_id}"),
                world_id: "world-a".to_string(),
                network_id: "world-a".to_string(),
                node_role: PeerNodeRole::FullStorage.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Hybrid,
                reachability_class: PeerReachabilityClass::Hybrid,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources,
                capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
                source_operator: None,
                source_asn: None,
                published_at_ms: 0,
                ttl_ms: 60_000,
            },
            identity_public_key_protobuf_hex: "abcd".to_string(),
            signature_hex: "beef".to_string(),
        }
    }

    fn transport_path(peer_id: PeerId, addr: &str, kind: TransportPathKind) -> TransportPath {
        let addr: Multiaddr = addr.parse().expect("multiaddr");
        TransportPath {
            peer_id,
            addr,
            kind,
            flavor: match kind {
                TransportPathKind::Direct => TransportSessionFlavor::Quic,
                TransportPathKind::HolePunched => TransportSessionFlavor::TcpNoiseYamux,
                TransportPathKind::RelayReserved => TransportSessionFlavor::RelayTunnel,
            },
            security: match kind {
                TransportPathKind::Direct | TransportPathKind::HolePunched => {
                    TransportSecurity::QuicTls
                }
                TransportPathKind::RelayReserved => TransportSecurity::Noise,
            },
            muxer: match kind {
                TransportPathKind::Direct | TransportPathKind::HolePunched => TransportMuxer::Quic,
                TransportPathKind::RelayReserved => TransportMuxer::Yamux,
            },
        }
    }

    #[test]
    fn recompute_marks_single_source_active_set_as_suspect() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let discovered = HashMap::from([
            (
                peer_a,
                sample_record(peer_a, vec![PeerDiscoverySource::StaticBootstrap]),
            ),
            (
                peer_b,
                sample_record(peer_b, vec![PeerDiscoverySource::StaticBootstrap]),
            ),
        ]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/10.0.0.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/10.0.1.1/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &PeerManagerPolicy::default(),
        );
        for peer in [peer_a, peer_b] {
            let health = healths.get(&peer).expect("health");
            assert_eq!(health.status, PeerManagerHealthStatus::Suspect);
            assert!(health.issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::SingleSourceDiscovery { .. }
            )));
            assert!(health.issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::InsufficientActiveDiscoverySources { .. }
            )));
        }
    }

    #[test]
    fn recompute_marks_subnet_concentration() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let discovery_sources = vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ];
        let discovered = HashMap::from([
            (peer_a, sample_record(peer_a, discovery_sources.clone())),
            (peer_b, sample_record(peer_b, discovery_sources.clone())),
            (peer_c, sample_record(peer_c, discovery_sources)),
        ]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/192.168.10.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/192.168.10.2/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_c,
                transport_path(
                    peer_c,
                    "/ip4/10.20.30.40/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let policy = PeerManagerPolicy {
            min_active_peers_for_share_limits: 0,
            ..PeerManagerPolicy::default()
        };
        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &policy,
        );
        assert!(healths[&peer_a].issues.iter().any(|issue| matches!(
            issue,
            PeerManagerHealthIssue::Ipv4SubnetConcentration { subnet, .. } if subnet == "192.168.10"
        )));
        assert!(healths[&peer_b].issues.iter().any(|issue| matches!(
            issue,
            PeerManagerHealthIssue::Ipv4SubnetConcentration { subnet, .. } if subnet == "192.168.10"
        )));
        assert_eq!(healths[&peer_c].status, PeerManagerHealthStatus::Blocked);
    }

    #[test]
    fn recompute_defers_share_limits_for_small_static_topology() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let discovery_sources = vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ];
        let discovered = HashMap::from([
            (peer_a, sample_record(peer_a, discovery_sources.clone())),
            (peer_b, sample_record(peer_b, discovery_sources.clone())),
            (peer_c, sample_record(peer_c, discovery_sources)),
        ]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/192.168.10.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/192.168.10.2/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_c,
                transport_path(
                    peer_c,
                    "/ip4/10.20.30.40/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &PeerManagerPolicy::default(),
        );
        for peer in [peer_a, peer_b, peer_c] {
            assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Suspect);
        }
    }

    #[test]
    fn recompute_count_limit_blocks_all_peers_when_four_share_same_ipv4_subnet() {
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
        let discovered =
            HashMap::from(peers.map(|peer| (peer, sample_record(peer, discovery_sources.clone()))));
        let active_three = HashMap::from([
            (
                peers[0],
                transport_path(
                    peers[0],
                    "/ip4/192.168.10.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peers[1],
                transport_path(
                    peers[1],
                    "/ip4/192.168.10.2/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peers[2],
                transport_path(
                    peers[2],
                    "/ip4/192.168.10.3/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);
        let active_four = HashMap::from([
            (
                peers[0],
                transport_path(
                    peers[0],
                    "/ip4/192.168.10.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peers[1],
                transport_path(
                    peers[1],
                    "/ip4/192.168.10.2/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peers[2],
                transport_path(
                    peers[2],
                    "/ip4/192.168.10.3/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peers[3],
                transport_path(
                    peers[3],
                    "/ip4/192.168.10.4/udp/4104/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);
        let policy = PeerManagerPolicy {
            max_ipv4_subnet_active_peers: Some(3),
            ..PeerManagerPolicy::default()
        };

        let healths_three = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active_three,
            &policy,
        );
        for peer in peers.iter().take(3) {
            assert_eq!(healths_three[peer].status, PeerManagerHealthStatus::Suspect);
        }

        let healths_four = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active_four,
            &policy,
        );
        for peer in peers {
            assert_eq!(healths_four[&peer].status, PeerManagerHealthStatus::Blocked);
            assert!(healths_four[&peer].issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::Ipv4SubnetActivePeerLimit {
                    max_active_peers: 3,
                    ..
                }
            )));
        }
    }

    #[test]
    fn recompute_ignores_loopback_subnet_concentration() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let discovered = HashMap::from([
            (
                peer_a,
                sample_record(
                    peer_a,
                    vec![
                        PeerDiscoverySource::StaticBootstrap,
                        PeerDiscoverySource::Dht,
                    ],
                ),
            ),
            (
                peer_b,
                sample_record(
                    peer_b,
                    vec![
                        PeerDiscoverySource::StaticBootstrap,
                        PeerDiscoverySource::Dht,
                    ],
                ),
            ),
        ]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/127.0.0.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/127.0.0.1/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let policy = PeerManagerPolicy {
            min_active_peers_for_share_limits: 0,
            ..PeerManagerPolicy::default()
        };
        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &policy,
        );
        for peer in [peer_a, peer_b] {
            assert!(
                !healths[&peer].issues.iter().any(|issue| matches!(
                    issue,
                    PeerManagerHealthIssue::Ipv4SubnetConcentration { .. }
                )),
                "loopback peers should not trip ipv4 subnet concentration: {:?}",
                healths[&peer].issues
            );
        }
    }

    #[test]
    fn recompute_marks_relay_budget_and_domain_concentration() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let discovery_sources = vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ];
        let discovered = HashMap::from([
            (peer_a, sample_record(peer_a, discovery_sources.clone())),
            (peer_b, sample_record(peer_b, discovery_sources.clone())),
            (peer_c, sample_record(peer_c, discovery_sources)),
        ]);
        let relay_peer_a = PeerId::random();
        let relay_peer_b = PeerId::random();
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    format!("/dns4/relay-a.example/tcp/443/p2p/{relay_peer_a}/p2p-circuit")
                        .as_str(),
                    TransportPathKind::RelayReserved,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    format!("/dns4/relay-a.example/tcp/443/p2p/{relay_peer_b}/p2p-circuit")
                        .as_str(),
                    TransportPathKind::RelayReserved,
                ),
            ),
            (
                peer_c,
                transport_path(
                    peer_c,
                    "/ip4/10.20.30.40/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let policy = PeerManagerPolicy {
            min_active_peers_for_share_limits: 0,
            ..PeerManagerPolicy::default()
        };
        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &policy,
        );
        for peer in [peer_a, peer_b] {
            let health = &healths[&peer];
            assert_eq!(health.status, PeerManagerHealthStatus::Blocked);
            assert!(
                health.issues.iter().any(|issue| matches!(
                    issue,
                    PeerManagerHealthIssue::RelayBudgetExceeded { .. }
                ))
            );
            assert!(health.issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::RelayDomainConcentration { relay_domain, .. } if relay_domain == "relay-a.example"
            )));
        }
        assert_eq!(healths[&peer_c].status, PeerManagerHealthStatus::Blocked);
    }

    #[test]
    fn recompute_keeps_unobserved_remote_claims_degraded() {
        let peer = PeerId::random();
        let discovered = HashMap::from([(
            peer,
            sample_record(
                peer,
                vec![
                    PeerDiscoverySource::StaticBootstrap,
                    PeerDiscoverySource::Dht,
                ],
            ),
        )]);

        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &HashMap::new(),
            &PeerManagerPolicy::default(),
        );
        let health = healths.get(&peer).expect("health");
        assert_eq!(health.status, PeerManagerHealthStatus::Suspect);
        assert!(
            health
                .issues
                .contains(&PeerManagerHealthIssue::DiscoveryIndependenceUnverified)
        );
    }

    #[test]
    fn recompute_marks_single_source_discovered_peer_as_suspect_before_activation() {
        let peer = PeerId::random();
        let discovered = HashMap::from([(
            peer,
            sample_record(peer, vec![PeerDiscoverySource::StaticBootstrap]),
        )]);

        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &HashMap::new(),
            &PeerManagerPolicy::default(),
        );
        let health = healths.get(&peer).expect("health");
        assert_eq!(health.status, PeerManagerHealthStatus::Suspect);
        assert!(
            health
                .issues
                .iter()
                .any(|issue| matches!(issue, PeerManagerHealthIssue::SingleSourceDiscovery { .. }))
        );
    }

    #[test]
    fn recompute_marks_operator_and_asn_concentration_as_blocked() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let discovery_sources = vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ];
        let mut record_a = sample_record(peer_a, discovery_sources.clone());
        record_a.record.source_operator = Some("operator-a".to_string());
        record_a.record.source_asn = Some("AS64500".to_string());
        let mut record_b = sample_record(peer_b, discovery_sources.clone());
        record_b.record.source_operator = Some("operator-a".to_string());
        record_b.record.source_asn = Some("AS64500".to_string());
        let mut record_c = sample_record(peer_c, discovery_sources);
        record_c.record.source_operator = Some("operator-b".to_string());
        record_c.record.source_asn = Some("AS64501".to_string());
        let discovered =
            HashMap::from([(peer_a, record_a), (peer_b, record_b), (peer_c, record_c)]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/10.0.0.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/10.1.0.1/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_c,
                transport_path(
                    peer_c,
                    "/ip4/10.2.0.1/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let policy = PeerManagerPolicy {
            min_active_peers_for_share_limits: 0,
            ..PeerManagerPolicy::default()
        };
        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &policy,
        );
        for peer in [peer_a, peer_b] {
            let health = &healths[&peer];
            assert_eq!(health.status, PeerManagerHealthStatus::Blocked);
            assert_eq!(health.source_operator, None);
            assert_eq!(health.source_asn, None);
            assert!(health.issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::OperatorConcentration { source_operator, .. }
                    if source_operator == "unknown"
            )));
            assert!(health.issues.iter().any(|issue| matches!(
                issue,
                PeerManagerHealthIssue::AsnConcentration { source_asn, .. }
                    if source_asn == "unknown"
            )));
        }
        assert_eq!(healths[&peer_c].status, PeerManagerHealthStatus::Blocked);
    }

    #[test]
    fn recompute_cannot_evade_unknown_control_domain_by_self_claimed_labels() {
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let peer_d = PeerId::random();
        let discovery_sources = vec![
            PeerDiscoverySource::StaticBootstrap,
            PeerDiscoverySource::Dht,
        ];
        let mut record_a = sample_record(peer_a, discovery_sources.clone());
        record_a.record.source_operator = Some("operator-a".to_string());
        record_a.record.source_asn = Some("AS64500".to_string());
        let mut record_b = sample_record(peer_b, discovery_sources.clone());
        record_b.record.source_operator = Some("operator-a".to_string());
        record_b.record.source_asn = Some("AS64500".to_string());
        let mut record_c = sample_record(peer_c, discovery_sources.clone());
        record_c.record.source_operator = Some("operator-b".to_string());
        record_c.record.source_asn = Some("AS64501".to_string());
        let mut record_d = sample_record(peer_d, discovery_sources);
        record_d.record.source_operator = Some("operator-c".to_string());
        record_d.record.source_asn = Some("AS64502".to_string());
        let discovered = HashMap::from([
            (peer_a, record_a),
            (peer_b, record_b),
            (peer_c, record_c),
            (peer_d, record_d),
        ]);
        let active = HashMap::from([
            (
                peer_a,
                transport_path(
                    peer_a,
                    "/ip4/10.0.0.1/udp/4101/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_b,
                transport_path(
                    peer_b,
                    "/ip4/10.1.0.1/udp/4102/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_c,
                transport_path(
                    peer_c,
                    "/ip4/10.2.0.1/udp/4103/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
            (
                peer_d,
                transport_path(
                    peer_d,
                    "/ip4/10.3.0.1/udp/4104/quic-v1",
                    TransportPathKind::Direct,
                ),
            ),
        ]);

        let healths = recompute_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &active,
            &PeerManagerPolicy::default(),
        );
        for peer in [peer_a, peer_b] {
            assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Blocked);
        }
        for peer in [peer_c, peer_d] {
            assert_eq!(healths[&peer].status, PeerManagerHealthStatus::Blocked);
        }
    }
    #[test]
    fn recompute_does_not_upgrade_unverified_discovery_with_zero_source_thresholds() {
        let peer = PeerId::random();
        let record = sample_record(
            peer,
            vec![
                PeerDiscoverySource::StaticBootstrap,
                PeerDiscoverySource::Dht,
            ],
        );
        let records = HashMap::from([(peer, record)]);
        let paths = HashMap::from([(
            peer,
            transport_path(
                peer,
                "/ip4/10.0.0.1/udp/4101/quic-v1",
                TransportPathKind::Direct,
            ),
        )]);
        let policy = PeerManagerPolicy {
            min_peer_discovery_sources: 0,
            min_active_discovery_sources: 0,
            ..PeerManagerPolicy::default()
        };
        let health = recompute_peer_manager_healths(
            &records,
            &DiscoveryObservations::default(),
            &paths,
            &policy,
        )
        .remove(&peer)
        .unwrap();
        assert_eq!(health.status, PeerManagerHealthStatus::Suspect);
        assert!(
            health
                .issues
                .contains(&PeerManagerHealthIssue::DiscoveryIndependenceUnverified)
        );
        assert!(health.discovery_sources.is_empty());
    }
}
