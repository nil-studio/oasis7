//! Loopback TCP evidence for publisher B -> trusted cache A -> independent receiver C.
use super::*;

fn network(node: &str, bootstrap_peers: Vec<Multiaddr>) -> Libp2pNetwork {
    let key = Keypair::generate_ed25519();
    let mut template =
        signed_discovery_peer_record(&key, vec![crate::dht::PeerDiscoverySource::Dht], 0).record;
    template.node_id = node.into();
    template.direct_addrs.clear();
    Libp2pNetwork::new(Libp2pNetworkConfig {
        keypair: Some(key),
        peer_record: Some(template),
        bootstrap_peers,
        allow_loopback_external_addrs_for_testing: true,
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        discovery_query_interval_ms: 100,
        request_response_timeout: Duration::from_secs(2),
        ..Default::default()
    })
}

fn listen(network: &Libp2pNetwork) -> Multiaddr {
    wait_until(
        "TCP listener",
        Instant::now() + Duration::from_secs(10),
        || !network.listening_addrs().is_empty(),
    );
    network
        .listening_addrs()
        .into_iter()
        .next()
        .unwrap()
        .with(libp2p::multiaddr::Protocol::P2p(network.peer_id()))
}

#[test]
fn discovery_observation_three_node_tcp_cache_keeps_signature_and_local_seed_boundary() {
    let publisher_b = network("publisher-b", vec![]);
    let cache_a = network("cache-a", vec![listen(&publisher_b)]);
    wait_until(
        "A observes configured seed B",
        Instant::now() + Duration::from_secs(10),
        || {
            cache_a.debug_peer_healths().iter().any(|health| {
                health.peer_id == publisher_b.peer_id().to_string()
                    && health
                        .discovery_sources
                        .contains(&crate::dht::PeerDiscoverySource::StaticBootstrap)
            })
        },
    );
    let receiver_c = network("receiver-c", vec![listen(&cache_a)]);
    wait_until(
        "C connects to cache A",
        Instant::now() + Duration::from_secs(10),
        || receiver_c.connected_peers().contains(&cache_a.peer_id()),
    );
    let response = receiver_c
        .request_with_providers(
            super::super::discovery::RR_GET_CACHED_PEER_RECORD,
            publisher_b.peer_id().to_string().as_bytes(),
            &[cache_a.peer_id().to_string()],
        )
        .unwrap();
    let forwarded: Option<SignedPeerRecord> = serde_cbor::from_slice(&response).unwrap();
    let forwarded = forwarded.expect("A has B's signed record");
    super::super::peer_record::verify_signed_peer_record(&forwarded).unwrap();
    assert_eq!(forwarded.record.peer_id, publisher_b.peer_id().to_string());
    assert_eq!(
        forwarded.record.discovery_sources,
        vec![crate::dht::PeerDiscoverySource::Dht]
    );
    wait_until(
        "C independently records B without inheriting A's seed trust",
        Instant::now() + Duration::from_secs(10),
        || {
            receiver_c.debug_peer_healths().iter().any(|health| {
                health.peer_id == publisher_b.peer_id().to_string()
                    && !health
                        .discovery_sources
                        .contains(&crate::dht::PeerDiscoverySource::StaticBootstrap)
                    && health
                        .issues
                        .contains(&PeerManagerHealthIssue::DiscoveryIndependenceUnverified)
                    && health.status != PeerManagerHealthStatus::Active
            })
        },
    );
}

#[test]
fn discovery_observation_dht_retains_only_the_selected_records_actual_upstream() {
    let publisher = Keypair::generate_ed25519();
    let peer_id = PeerId::from(publisher.public());
    let signed =
        signed_discovery_peer_record(&publisher, vec![crate::dht::PeerDiscoverySource::Dht], 1);
    let upstream = PeerId::random();
    let mut pending = PendingDhtQuery::DiscoverPeerRecord {
        peer_id,
        upstream: None,
        record: None,
        error: None,
    };
    let found = |signed: &SignedPeerRecord, via| {
        kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
            peer: via,
            record: kad::Record {
                key: RecordKey::new(&"peer"),
                value: to_canonical_cbor(signed).unwrap(),
                publisher: None,
                expires: None,
            },
        })))
    };
    assert!(matches!(
        handle_dht_progress(&mut pending, found(&signed, Some(upstream)), false),
        DhtProgressAction::None
    ));
    let unrelated = signed_discovery_peer_record(
        &Keypair::generate_ed25519(),
        vec![crate::dht::PeerDiscoverySource::Dht],
        1,
    );
    // A later well-signed response for the wrong target cannot replace either record or upstream.
    let result = handle_dht_progress(
        &mut pending,
        found(&unrelated, Some(PeerId::random())),
        true,
    );
    let DhtProgressAction::DiscoverPeerRecord {
        upstream: observed,
        result,
        ..
    } = result
    else {
        panic!("discovery result");
    };
    assert_eq!(observed, Some(upstream));
    assert_eq!(result.unwrap().unwrap(), signed);
}
