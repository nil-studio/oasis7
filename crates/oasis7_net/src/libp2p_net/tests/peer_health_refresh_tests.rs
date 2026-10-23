use super::super::peer_discovery_observations::DiscoveryObservations;
use super::*;

#[test]
fn refresh_peer_manager_healths_keeps_missing_record_active_peer_soft_blocked_for_request_fallback()
{
    let healthy_peer_key = Keypair::generate_ed25519();
    let healthy_peer = PeerId::from(healthy_peer_key.public());
    let unverified_peer = PeerId::random();
    let discovered_peer_records = PeerRecordCache::from([(
        healthy_peer,
        signed_discovery_peer_record(
            &healthy_peer_key,
            vec![
                crate::dht::PeerDiscoverySource::Dht,
                crate::dht::PeerDiscoverySource::Rendezvous,
            ],
            crate::libp2p_net::now_ms(),
        ),
    )]);
    let active_transport_paths = HashMap::from([
        (
            healthy_peer,
            active_transport_path_from_endpoint(
                &HashMap::new(),
                healthy_peer,
                &"/ip4/10.0.0.1/udp/4103/quic-v1"
                    .parse()
                    .expect("healthy endpoint"),
            ),
        ),
        (
            unverified_peer,
            active_transport_path_from_endpoint(
                &HashMap::new(),
                unverified_peer,
                &"/ip4/10.0.0.2/udp/4104/quic-v1"
                    .parse()
                    .expect("unverified endpoint"),
            ),
        ),
    ]);
    let event_peer_healths = Arc::new(Mutex::new(HashMap::new()));
    let event_block_artifacts = Arc::new(Mutex::new(HashMap::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));

    let (healths, quarantined, admitted) = refresh_peer_manager_healths(
        &discovered_peer_records,
        &DiscoveryObservations::default(),
        &active_transport_paths,
        &HashSet::from([healthy_peer]),
        &PeerManagerPolicy::default(),
        &event_peer_healths,
        &event_block_artifacts,
        &event_errors,
        32,
    );

    assert_eq!(
        healths[&healthy_peer].status,
        PeerManagerHealthStatus::Suspect
    );
    assert_eq!(
        healths[&unverified_peer].status,
        PeerManagerHealthStatus::Blocked
    );
    assert!(has_missing_peer_record_issue(&healths[&unverified_peer]));
    assert!(quarantined.is_empty());
    assert_eq!(admitted, HashSet::from([healthy_peer, unverified_peer]));
    let event_healths = event_peer_healths.lock().expect("lock peer healths");
    let unverified_health = event_healths
        .get(unverified_peer.to_string().as_str())
        .expect("unverified peer health");
    assert_eq!(unverified_health.status, PeerManagerHealthStatus::Blocked);
    assert!(!peer_requires_active_quarantine(unverified_peer, &healths));
}

#[test]
fn refresh_peer_manager_healths_admits_record_exchange_pending_active_peer() {
    let bootstrap_peer = PeerId::random();
    let active_transport_paths = HashMap::from([(
        bootstrap_peer,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            bootstrap_peer,
            &"/ip4/10.0.0.2/tcp/6832"
                .parse()
                .expect("bootstrap endpoint"),
        ),
    )]);
    let event_peer_healths = Arc::new(Mutex::new(HashMap::new()));
    let event_block_artifacts = Arc::new(Mutex::new(HashMap::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));

    let (healths, quarantined, admitted) = refresh_peer_manager_healths(
        &PeerRecordCache::default(),
        &DiscoveryObservations::default(),
        &active_transport_paths,
        &HashSet::new(),
        &PeerManagerPolicy::default(),
        &event_peer_healths,
        &event_block_artifacts,
        &event_errors,
        32,
    );

    assert!(has_missing_peer_record_issue(&healths[&bootstrap_peer]));
    assert!(quarantined.is_empty());
    assert_eq!(admitted, HashSet::from([bootstrap_peer]));
    assert!(!peer_requires_active_quarantine(bootstrap_peer, &healths));
}

#[test]
fn refresh_peer_manager_healths_does_not_quarantine_static_single_source_peer() {
    let peer_key = Keypair::generate_ed25519();
    let peer_id = PeerId::from(peer_key.public());
    let discovered_peer_records = PeerRecordCache::from([(
        peer_id,
        signed_discovery_peer_record(
            &peer_key,
            vec![crate::dht::PeerDiscoverySource::StaticBootstrap],
            crate::libp2p_net::now_ms(),
        ),
    )]);
    let active_transport_paths = HashMap::from([(
        peer_id,
        active_transport_path_from_endpoint(
            &HashMap::new(),
            peer_id,
            &"/ip4/10.0.0.2/tcp/6832"
                .parse()
                .expect("bootstrap endpoint"),
        ),
    )]);
    let event_peer_healths = Arc::new(Mutex::new(HashMap::new()));
    let event_block_artifacts = Arc::new(Mutex::new(HashMap::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));

    let (_healths, quarantined, admitted) = refresh_peer_manager_healths(
        &discovered_peer_records,
        &DiscoveryObservations::default(),
        &active_transport_paths,
        &HashSet::new(),
        &PeerManagerPolicy::default(),
        &event_peer_healths,
        &event_block_artifacts,
        &event_errors,
        32,
    );

    assert!(quarantined.is_empty());
    assert_eq!(admitted, HashSet::from([peer_id]));
}

#[test]
fn refresh_peer_manager_healths_uses_peer_id_order_for_constrained_pending_peers() {
    let first_key = Keypair::generate_ed25519();
    let second_key = Keypair::generate_ed25519();
    let first_peer = PeerId::from(first_key.public());
    let second_peer = PeerId::from(second_key.public());
    let admitted_peer = first_peer.min(second_peer);
    let blocked_peer = first_peer.max(second_peer);
    let discovery_sources = vec![
        crate::dht::PeerDiscoverySource::Dht,
        crate::dht::PeerDiscoverySource::Rendezvous,
    ];
    let discovered_peer_records = PeerRecordCache::from([
        (
            first_peer,
            signed_discovery_peer_record(
                &first_key,
                discovery_sources.clone(),
                crate::libp2p_net::now_ms(),
            ),
        ),
        (
            second_peer,
            signed_discovery_peer_record(
                &second_key,
                discovery_sources,
                crate::libp2p_net::now_ms(),
            ),
        ),
    ]);
    let active_transport_paths = HashMap::from([
        (
            first_peer,
            active_transport_path_from_endpoint(
                &HashMap::new(),
                first_peer,
                &"/ip4/10.0.0.1/udp/4103/quic-v1"
                    .parse()
                    .expect("first endpoint"),
            ),
        ),
        (
            second_peer,
            active_transport_path_from_endpoint(
                &HashMap::new(),
                second_peer,
                &"/ip4/10.0.0.2/udp/4104/quic-v1"
                    .parse()
                    .expect("second endpoint"),
            ),
        ),
    ]);
    let event_peer_healths = Arc::new(Mutex::new(HashMap::new()));
    let event_block_artifacts = Arc::new(Mutex::new(HashMap::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));

    let (healths, quarantined, admitted) = refresh_peer_manager_healths(
        &discovered_peer_records,
        &DiscoveryObservations::default(),
        &active_transport_paths,
        &HashSet::new(),
        &PeerManagerPolicy {
            min_active_discovery_sources: 0,
            max_ipv4_subnet_active_peers: Some(1),
            ..PeerManagerPolicy::default()
        },
        &event_peer_healths,
        &event_block_artifacts,
        &event_errors,
        32,
    );

    assert_eq!(admitted, HashSet::from([admitted_peer]));
    assert_eq!(quarantined, HashSet::from([blocked_peer]));
    assert_eq!(
        healths[&admitted_peer].status,
        PeerManagerHealthStatus::Suspect
    );
    assert_eq!(
        healths[&blocked_peer].status,
        PeerManagerHealthStatus::Blocked
    );
}

#[test]
fn recordless_sessions_cannot_bypass_or_dilute_unknown_control_budgets() {
    for signed_count in [0, 2] {
        let keys: Vec<_> = (0..4).map(|_| Keypair::generate_ed25519()).collect();
        let peers: Vec<_> = keys.iter().map(|key| PeerId::from(key.public())).collect();
        let discovered = keys
            .iter()
            .take(signed_count)
            .map(|key| {
                (
                    PeerId::from(key.public()),
                    signed_discovery_peer_record(
                        key,
                        vec![crate::dht::PeerDiscoverySource::StaticBootstrap],
                        crate::libp2p_net::now_ms(),
                    ),
                )
            })
            .fold(PeerRecordCache::default(), |mut cache, (peer, record)| {
                cache.insert(peer, record);
                cache
            });
        let paths: HashMap<_, _> = peers
            .iter()
            .enumerate()
            .map(|(index, peer)| {
                (
                    *peer,
                    active_transport_path_from_endpoint(
                        &HashMap::new(),
                        *peer,
                        &format!("/ip4/198.51.{index}.1/tcp/4103").parse().unwrap(),
                    ),
                )
            })
            .collect();
        let (healths, quarantined, admitted) = refresh_peer_manager_healths(
            &discovered,
            &DiscoveryObservations::default(),
            &paths,
            &HashSet::new(),
            &PeerManagerPolicy::default(),
            &Arc::new(Mutex::new(HashMap::new())),
            &Arc::new(Mutex::new(HashMap::new())),
            &Arc::new(Mutex::new(Vec::new())),
            16,
        );
        let mut ordered = peers.clone();
        ordered.sort_unstable();
        assert_eq!(
            admitted,
            ordered[..3].iter().copied().collect(),
            "signed_count={signed_count}"
        );
        assert_eq!(quarantined, HashSet::from([ordered[3]]));
        assert!(
            healths
                .values()
                .all(|health| health.status != PeerManagerHealthStatus::Active)
        );
        assert!(healths[&ordered[3]].issues.iter().any(|issue| matches!(issue,
            PeerManagerHealthIssue::OperatorConcentration { source_operator, .. } if source_operator == "unknown")));
    }
}
