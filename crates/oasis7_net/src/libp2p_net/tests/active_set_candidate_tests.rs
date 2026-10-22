use crate::libp2p_net::peer_discovery_observations::DiscoveryObservations;
use std::collections::BTreeSet;

use super::super::peer_manager::{PeerManagerHealthStatus, PeerManagerPolicy};
use super::super::peer_manager_active_set::{
    ActivePeerCandidate, ActivePeerSetStats, candidate_status_with_active_set,
    candidate_transport_admission_status, candidate_would_degrade_admitted_peers,
};
use super::*;

#[test]
fn active_set_candidate_status_flags_bucket_overflow_without_full_recompute() {
    let admitted_key = Keypair::generate_ed25519();
    let admitted_peer = PeerId::from(admitted_key.public());
    let candidate_key = Keypair::generate_ed25519();
    let candidate_peer = PeerId::from(candidate_key.public());
    let admitted_record = signed_discovery_peer_record(
        &admitted_key,
        vec![
            crate::dht::PeerDiscoverySource::Dht,
            crate::dht::PeerDiscoverySource::Rendezvous,
        ],
        1,
    );
    let candidate_record = signed_discovery_peer_record(
        &candidate_key,
        vec![
            crate::dht::PeerDiscoverySource::Dht,
            crate::dht::PeerDiscoverySource::Rendezvous,
        ],
        1,
    );
    let admitted_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        admitted_peer,
        &"/ip4/10.0.0.1/udp/4103/quic-v1"
            .parse()
            .expect("admitted endpoint"),
    );
    let candidate_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        candidate_peer,
        &"/ip4/10.0.0.2/udp/4104/quic-v1"
            .parse()
            .expect("candidate endpoint"),
    );
    let stats = ActivePeerSetStats::new(
        &HashMap::from([(admitted_peer, admitted_record)]),
        &DiscoveryObservations::default(),
        &HashMap::from([(admitted_peer, admitted_path)]),
    );
    let candidate = ActivePeerCandidate::from_record_and_path(
        &candidate_record,
        &DiscoveryObservations::default(),
        &candidate_path,
    );

    let policy = PeerManagerPolicy {
        min_active_peers_for_share_limits: 0,
        ..PeerManagerPolicy::default()
    };
    assert_eq!(
        candidate_status_with_active_set(&candidate, &stats, &policy),
        PeerManagerHealthStatus::Blocked
    );
    assert!(candidate_would_degrade_admitted_peers(
        &candidate, &stats, &policy,
    ));
}

#[test]
fn claimed_observer_candidate_counts_toward_provider_share_limits() {
    let first_key = Keypair::generate_ed25519();
    let second_key = Keypair::generate_ed25519();
    let third_key = Keypair::generate_ed25519();
    let observer_key = Keypair::generate_ed25519();
    let first_peer = PeerId::from(first_key.public());
    let second_peer = PeerId::from(second_key.public());
    let third_peer = PeerId::from(third_key.public());
    let observer_peer = PeerId::from(observer_key.public());
    let discovery_sources = vec![
        crate::dht::PeerDiscoverySource::Dht,
        crate::dht::PeerDiscoverySource::Rendezvous,
    ];
    let mut observer_record =
        signed_discovery_peer_record(&observer_key, discovery_sources.clone(), 4);
    observer_record.record.node_role = PeerNodeRole::ObserverLight.as_str().to_string();
    observer_record.record.capability_lanes =
        PeerNodeRole::ObserverLight.default_capability_lanes();

    let stats = ActivePeerSetStats::new(
        &HashMap::from([
            (
                first_peer,
                signed_discovery_peer_record(&first_key, discovery_sources.clone(), 1),
            ),
            (
                second_peer,
                signed_discovery_peer_record(&second_key, discovery_sources.clone(), 2),
            ),
            (
                third_peer,
                signed_discovery_peer_record(&third_key, discovery_sources, 3),
            ),
        ]),
        &DiscoveryObservations::default(),
        &HashMap::from([
            (
                first_peer,
                active_transport_path_from_endpoint(
                    &HashMap::new(),
                    first_peer,
                    &"/ip4/10.0.0.1/udp/4101/quic-v1"
                        .parse()
                        .expect("first endpoint"),
                ),
            ),
            (
                second_peer,
                active_transport_path_from_endpoint(
                    &HashMap::new(),
                    second_peer,
                    &"/ip4/10.0.0.2/udp/4102/quic-v1"
                        .parse()
                        .expect("second endpoint"),
                ),
            ),
            (
                third_peer,
                active_transport_path_from_endpoint(
                    &HashMap::new(),
                    third_peer,
                    &"/ip4/10.20.30.40/udp/4103/quic-v1"
                        .parse()
                        .expect("third endpoint"),
                ),
            ),
        ]),
    );
    let observer_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        observer_peer,
        &"/ip4/10.0.0.89/udp/4104/quic-v1"
            .parse()
            .expect("observer endpoint"),
    );
    let candidate = ActivePeerCandidate::from_record_and_path(
        &observer_record,
        &DiscoveryObservations::default(),
        &observer_path,
    );

    assert_eq!(
        candidate_status_with_active_set(&candidate, &stats, &PeerManagerPolicy::default()),
        PeerManagerHealthStatus::Blocked
    );
    assert!(candidate_would_degrade_admitted_peers(
        &candidate,
        &stats,
        &PeerManagerPolicy::default(),
    ));
}

#[test]
fn active_set_stats_count_claimed_observers_without_trusting_discovery_claims() {
    let provider_key = Keypair::generate_ed25519();
    let observer_key = Keypair::generate_ed25519();
    let provider_peer = PeerId::from(provider_key.public());
    let observer_peer = PeerId::from(observer_key.public());
    let provider_record =
        signed_discovery_peer_record(&provider_key, vec![crate::dht::PeerDiscoverySource::Dht], 1);
    let mut observer_record = signed_discovery_peer_record(
        &observer_key,
        vec![crate::dht::PeerDiscoverySource::Rendezvous],
        2,
    );
    observer_record.record.node_role = PeerNodeRole::ObserverLight.as_str().to_string();
    observer_record.record.capability_lanes =
        PeerNodeRole::ObserverLight.default_capability_lanes();

    let stats = ActivePeerSetStats::new(
        &HashMap::from([
            (provider_peer, provider_record),
            (observer_peer, observer_record),
        ]),
        &DiscoveryObservations::default(),
        &HashMap::from([
            (
                provider_peer,
                active_transport_path_from_endpoint(
                    &HashMap::new(),
                    provider_peer,
                    &"/ip4/10.0.0.1/udp/4101/quic-v1"
                        .parse()
                        .expect("provider endpoint"),
                ),
            ),
            (
                observer_peer,
                active_transport_path_from_endpoint(
                    &HashMap::new(),
                    observer_peer,
                    &"/ip4/10.0.0.89/udp/4104/quic-v1"
                        .parse()
                        .expect("observer endpoint"),
                ),
            ),
        ]),
    );

    assert_eq!(stats.active_peer_count, 2);
    assert_eq!(stats.active_discovery_sources, BTreeSet::new());
    assert_eq!(
        stats.ipv4_subnet_counts,
        HashMap::from([("10.0.0".to_string(), 2)])
    );
}

#[test]
fn active_set_candidate_count_limit_allows_third_and_blocks_fourth_subnet_peer() {
    let candidate = ActivePeerCandidate {
        discovery_independence_verified: true,
        discovery_source_labels: BTreeSet::from(["dht", "rendezvous"]),
        ipv4_subnet_bucket: Some("10.0.0".to_string()),
        relay_domain: None,
        source_operator: None,
        source_asn: None,
        relay_reserved: false,
        counts_toward_share_limits: true,
    };
    let policy = PeerManagerPolicy {
        max_ipv4_subnet_active_peers: Some(3),
        ..PeerManagerPolicy::default()
    };
    let two_peer_stats = ActivePeerSetStats {
        active_peer_count: 2,
        active_discovery_sources: BTreeSet::from(["dht", "rendezvous"]),
        ipv4_subnet_counts: HashMap::from([("10.0.0".to_string(), 2)]),
        ..ActivePeerSetStats::default()
    };
    let three_peer_stats = ActivePeerSetStats {
        active_peer_count: 3,
        active_discovery_sources: BTreeSet::from(["dht", "rendezvous"]),
        ipv4_subnet_counts: HashMap::from([("10.0.0".to_string(), 3)]),
        ..ActivePeerSetStats::default()
    };

    assert_eq!(
        candidate_status_with_active_set(&candidate, &two_peer_stats, &policy),
        PeerManagerHealthStatus::Active
    );
    assert!(!candidate_would_degrade_admitted_peers(
        &candidate,
        &two_peer_stats,
        &policy,
    ));
    assert_eq!(
        candidate_status_with_active_set(&candidate, &three_peer_stats, &policy),
        PeerManagerHealthStatus::Blocked
    );
    assert!(candidate_would_degrade_admitted_peers(
        &candidate,
        &three_peer_stats,
        &policy,
    ));
}

#[test]
fn active_set_candidate_status_admits_distinct_peer_without_degrading_existing_active_set() {
    let admitted_key = Keypair::generate_ed25519();
    let admitted_peer = PeerId::from(admitted_key.public());
    let candidate_key = Keypair::generate_ed25519();
    let candidate_peer = PeerId::from(candidate_key.public());
    let admitted_record = signed_discovery_peer_record(
        &admitted_key,
        vec![
            crate::dht::PeerDiscoverySource::Dht,
            crate::dht::PeerDiscoverySource::Rendezvous,
        ],
        1,
    );
    let candidate_record = signed_discovery_peer_record(
        &candidate_key,
        vec![
            crate::dht::PeerDiscoverySource::Dht,
            crate::dht::PeerDiscoverySource::Rendezvous,
        ],
        1,
    );
    let admitted_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        admitted_peer,
        &"/ip4/10.0.0.1/udp/4103/quic-v1"
            .parse()
            .expect("admitted endpoint"),
    );
    let candidate_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        candidate_peer,
        &"/ip4/10.0.1.2/udp/4104/quic-v1"
            .parse()
            .expect("candidate endpoint"),
    );
    let stats = ActivePeerSetStats::new(
        &HashMap::from([(admitted_peer, admitted_record)]),
        &DiscoveryObservations::default(),
        &HashMap::from([(admitted_peer, admitted_path)]),
    );
    let candidate = ActivePeerCandidate::from_record_and_path(
        &candidate_record,
        &DiscoveryObservations::default(),
        &candidate_path,
    );

    assert_eq!(
        candidate_status_with_active_set(&candidate, &stats, &PeerManagerPolicy::default()),
        PeerManagerHealthStatus::Suspect
    );
    assert!(!candidate_would_degrade_admitted_peers(
        &candidate,
        &stats,
        &PeerManagerPolicy::default(),
    ));
}

#[test]
fn active_set_candidate_status_counts_unique_candidate_discovery_sources() {
    let candidate = ActivePeerCandidate {
        discovery_independence_verified: true,
        discovery_source_labels: BTreeSet::from(["dht"]),
        ipv4_subnet_bucket: None,
        relay_domain: None,
        source_operator: None,
        source_asn: None,
        relay_reserved: false,
        counts_toward_share_limits: true,
    };

    assert_eq!(
        candidate_status_with_active_set(
            &candidate,
            &ActivePeerSetStats::default(),
            &PeerManagerPolicy::default(),
        ),
        PeerManagerHealthStatus::Suspect
    );
}

#[test]
fn active_set_candidate_status_projects_unique_active_discovery_source_union() {
    let candidate = ActivePeerCandidate {
        discovery_independence_verified: true,
        discovery_source_labels: BTreeSet::from(["dht"]),
        ipv4_subnet_bucket: None,
        relay_domain: None,
        source_operator: None,
        source_asn: None,
        relay_reserved: false,
        counts_toward_share_limits: true,
    };
    let active_set_stats = ActivePeerSetStats {
        active_peer_count: 1,
        active_discovery_sources: BTreeSet::from(["rendezvous"]),
        ..ActivePeerSetStats::default()
    };
    let policy = PeerManagerPolicy {
        min_active_discovery_sources: 3,
        min_peer_discovery_sources: 1,
        ..PeerManagerPolicy::default()
    };

    assert_eq!(
        candidate_status_with_active_set(&candidate, &active_set_stats, &policy),
        PeerManagerHealthStatus::Suspect
    );
}

#[test]
fn active_set_candidate_cannot_evade_unknown_operator_or_asn_bucket() {
    let admitted_key = Keypair::generate_ed25519();
    let candidate_key = Keypair::generate_ed25519();
    let admitted_peer = PeerId::from(admitted_key.public());
    let candidate_peer = PeerId::from(candidate_key.public());
    let mut admitted =
        signed_discovery_peer_record(&admitted_key, vec![crate::dht::PeerDiscoverySource::Dht], 1);
    admitted.record.source_operator = Some("claimed-one".to_string());
    admitted.record.source_asn = Some("AS11111".to_string());
    let mut incoming = signed_discovery_peer_record(
        &candidate_key,
        vec![crate::dht::PeerDiscoverySource::Rendezvous],
        2,
    );
    incoming.record.source_operator = Some("claimed-two".to_string());
    incoming.record.source_asn = Some("AS22222".to_string());
    let admitted_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        admitted_peer,
        &"/ip4/10.0.0.1/udp/4101/quic-v1".parse().unwrap(),
    );
    let candidate_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        candidate_peer,
        &"/ip4/10.0.1.1/udp/4102/quic-v1".parse().unwrap(),
    );
    let observations = DiscoveryObservations::default();
    let stats = ActivePeerSetStats::new(
        &HashMap::from([(admitted_peer, admitted)]),
        &observations,
        &HashMap::from([(admitted_peer, admitted_path)]),
    );
    let candidate =
        ActivePeerCandidate::from_record_and_path(&incoming, &observations, &candidate_path);
    let policy = PeerManagerPolicy {
        min_active_peers_for_share_limits: 0,
        ..PeerManagerPolicy::default()
    };
    assert_eq!(candidate.source_operator.as_deref(), Some("unknown"));
    assert_eq!(candidate.source_asn.as_deref(), Some("unknown"));
    assert_eq!(
        candidate_status_with_active_set(&candidate, &stats, &policy),
        PeerManagerHealthStatus::Blocked
    );
    assert!(candidate_would_degrade_admitted_peers(
        &candidate, &stats, &policy
    ));
}

#[test]
fn transport_admission_preserves_degraded_session_without_waiving_quotas() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let record = signed_discovery_peer_record(&key, vec![crate::dht::PeerDiscoverySource::Dht], 1);
    let path = active_transport_path_from_endpoint(
        &HashMap::new(),
        peer,
        &"/ip4/10.0.0.1/udp/4101/quic-v1".parse().unwrap(),
    );
    let candidate = ActivePeerCandidate::from_record_and_path(
        &record,
        &DiscoveryObservations::default(),
        &path,
    );
    let stats = ActivePeerSetStats::default();
    let policy = PeerManagerPolicy::default();
    assert_eq!(
        candidate_status_with_active_set(&candidate, &stats, &policy),
        PeerManagerHealthStatus::Suspect
    );
    assert_eq!(
        candidate_transport_admission_status(&candidate, &stats, &policy),
        PeerManagerHealthStatus::Active
    );
    let constrained = ActivePeerSetStats {
        active_peer_count: 3,
        operator_counts: HashMap::from([("unknown".to_string(), 3)]),
        asn_counts: HashMap::from([("unknown".to_string(), 3)]),
        ..ActivePeerSetStats::default()
    };
    assert_eq!(
        candidate_transport_admission_status(&candidate, &constrained, &policy),
        PeerManagerHealthStatus::Blocked
    );
}

#[test]
fn claimed_observer_role_cannot_bypass_transport_admission_budgets() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let mut record =
        signed_discovery_peer_record(&key, vec![crate::dht::PeerDiscoverySource::Dht], 1);
    record.record.node_role = PeerNodeRole::ObserverLight.as_str().to_string();
    record.record.capability_lanes = PeerNodeRole::ObserverLight.default_capability_lanes();
    let path = active_transport_path_from_endpoint(
        &HashMap::new(),
        peer,
        &"/ip4/10.0.0.1/udp/4101/quic-v1".parse().unwrap(),
    );
    let candidate = ActivePeerCandidate::from_record_and_path(
        &record,
        &DiscoveryObservations::default(),
        &path,
    );
    let policy = PeerManagerPolicy::default();
    assert_eq!(
        candidate_transport_admission_status(&candidate, &ActivePeerSetStats::default(), &policy),
        PeerManagerHealthStatus::Active
    );
    let constrained = ActivePeerSetStats {
        active_peer_count: 3,
        operator_counts: HashMap::from([("unknown".to_string(), 3)]),
        asn_counts: HashMap::from([("unknown".to_string(), 3)]),
        ..ActivePeerSetStats::default()
    };
    assert_eq!(
        candidate_transport_admission_status(&candidate, &constrained, &policy),
        PeerManagerHealthStatus::Blocked
    );
    assert!(candidate_would_degrade_admitted_peers(
        &candidate,
        &constrained,
        &policy
    ));
}

#[test]
fn recordless_active_paths_use_unknown_buckets_in_full_and_incremental_stats() {
    let first = PeerId::random();
    let second = PeerId::random();
    let first_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        first,
        &"/ip4/10.0.0.1/udp/4101/quic-v1".parse().unwrap(),
    );
    let second_path = active_transport_path_from_endpoint(
        &HashMap::new(),
        second,
        &"/ip4/10.0.1.1/udp/4102/quic-v1".parse().unwrap(),
    );
    let full = ActivePeerSetStats::new(
        &HashMap::new(),
        &DiscoveryObservations::default(),
        &HashMap::from([(first, first_path.clone()), (second, second_path.clone())]),
    );
    let mut incremental = ActivePeerSetStats::default();
    incremental.add_unverified_peer(&first_path);
    incremental.add_unverified_peer(&second_path);
    assert_eq!(full, incremental);
    assert_eq!(full.active_peer_count, 2);
    assert_eq!(full.operator_counts.get("unknown"), Some(&2));
    assert_eq!(full.asn_counts.get("unknown"), Some(&2));
    assert_eq!(full.ipv4_subnet_counts.len(), 2);
    let candidate = ActivePeerCandidate::from_unverified_path(&second_path);
    let policy = PeerManagerPolicy {
        min_active_peers_for_share_limits: 0,
        ..PeerManagerPolicy::default()
    };
    assert_eq!(
        candidate_transport_admission_status(&candidate, &full, &policy),
        PeerManagerHealthStatus::Blocked
    );
}
