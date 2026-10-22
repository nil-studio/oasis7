use super::super::discovery::{handle_request_response_request, maybe_queue_discovery_peer_record};
use super::super::peer_record_cache::PeerRecordCache;
use super::super::peer_record_time::DiscoveryClock;
use super::super::swarm_behaviour::build_swarm;
use super::super::transport_paths::dial_transport_path;
use super::*;

fn cached_response(cache: &PeerRecordCache, peer: PeerId) -> Result<Vec<u8>, WorldError> {
    handle_request_response_request(
        &NetworkRequest {
            protocol: "/aw/rr/1.0.0/get_cached_peer_record".into(),
            payload: peer.to_string().into_bytes(),
        },
        &HashMap::new(),
        None,
        &Keypair::generate_ed25519(),
        &Arc::default(),
        &Arc::default(),
        false,
        cache,
    )
}

#[test]
fn expiry_removes_cache_responses_dedup_and_queued_dials_but_preserves_session() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let signed = signed_discovery_peer_record(&key, vec![], now_ms());
    let clock = DiscoveryClock::default();
    let mut cache = PeerRecordCache::new(clock.clone());
    let lease = cache.prepare(peer, &signed).unwrap();
    cache.install(peer, signed.clone(), lease.clone());
    let original = cached_response(&cache, peer).unwrap();
    assert_eq!(
        serde_cbor::from_slice::<SignedPeerRecord>(&original).unwrap(),
        signed
    );
    let mut path = peer_record_transport_paths(&signed).unwrap().remove(0);
    path.record_lease = Some(lease.clone());
    let active = HashMap::from([(peer, path.clone())]);
    // Observe the signed expiry before a wall-clock rollback. Real-time reads
    // must now fail closed, not revive the old record at the earlier wall time.
    clock
        .observe(signed.record.expires_at_ms().unwrap(), Instant::now())
        .unwrap();
    assert!(cache.get(&peer).is_none());
    assert!(cached_response(&cache, peer).is_err());
    assert!(cache.prune(&active));
    assert!(cache.session_record(&peer).is_some());
    assert_eq!(
        filter_request_peers_by_lane(vec![peer], "/aw/rr/1.0.0/fetch_blob", &cache),
        vec![peer]
    );
    let (healths, quarantined, admitted) = refresh_peer_manager_healths(
        &cache,
        &super::super::peer_discovery_observations::DiscoveryObservations::default(),
        &active,
        &HashSet::from([peer]),
        &PeerManagerPolicy::default(),
        &Arc::default(),
        &Arc::default(),
        &Arc::default(),
        16,
    );
    assert!(
        healths[&peer]
            .issues
            .contains(&PeerManagerHealthIssue::MissingPeerRecord)
    );
    assert!(quarantined.is_empty());
    assert!(admitted.contains(&peer));
    let mut swarm = build_swarm(
        &Keypair::generate_ed25519(),
        false,
        false,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending = HashMap::new();
    let mut queued = HashSet::new();
    maybe_queue_discovery_peer_record(
        &mut swarm,
        &mut pending,
        &mut queued,
        &cache,
        peer,
        PeerId::random(),
        "world-a",
    );
    assert!(queued.contains(&peer));
    let mut last = HashMap::new();
    assert!(dial_transport_path(&mut swarm, &mut last, path).is_err());
    assert!(last.is_empty());
    cache.prune(&HashMap::new());
    assert!(cache.session_record(&peer).is_none());
}

#[test]
fn rejected_ingress_preserves_previous_record_and_observation() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let original = signed_discovery_peer_record(&key, vec![], now_ms());
    let mut cache = PeerRecordCache::from([(peer, original.clone())]);
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    observations
        .record(
            &original,
            super::super::peer_discovery_observations::DiscoveryIngress::Direct { peer },
            now_ms(),
        )
        .unwrap();
    let mut expired = original.record.clone();
    expired.published_at_ms = now_ms() - expired.ttl_ms;
    let expired = sign_peer_record(&expired, &key).unwrap();
    let mut swarm = build_swarm(
        &Keypair::generate_ed25519(),
        false,
        false,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let result = process_discovered_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut HashMap::new(),
        &mut HashMap::new(),
        &HashMap::new(),
        &mut HashSet::new(),
        None,
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        expired,
        super::super::peer_discovery_observations::DiscoveryIngress::Direct { peer },
    );
    assert!(result.is_err());
    assert_eq!(cache.get(&peer), Some(&original));
}

fn timed_network(bootstrap: Vec<Multiaddr>, ttl: i64) -> Libp2pNetwork {
    let key = Keypair::generate_ed25519();
    let mut record = signed_discovery_peer_record(&key, vec![], now_ms()).record;
    record.direct_addrs.clear();
    record.ttl_ms = ttl;
    Libp2pNetwork::new(Libp2pNetworkConfig {
        keypair: Some(key),
        peer_record: Some(record),
        bootstrap_peers: bootstrap,
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        allow_loopback_external_addrs_for_testing: true,
        discovery_query_interval_ms: 0,
        republish_interval_ms: 0,
        bootstrap_redial_interval_ms: 0,
        request_response_timeout: Duration::from_secs(2),
        ..Default::default()
    })
}
fn timed_listen(network: &Libp2pNetwork) -> Multiaddr {
    wait_until("listener", Instant::now() + Duration::from_secs(10), || {
        !network.listening_addrs().is_empty()
    });
    network.listening_addrs()[0]
        .clone()
        .with(libp2p::multiaddr::Protocol::P2p(network.peer_id()))
}
#[test]
fn tcp_cached_record_expires_without_disconnecting_authenticated_sessions() {
    let b = timed_network(vec![], 2_000);
    let a = timed_network(vec![timed_listen(&b)], 60_000);
    wait_until(
        "A caches B",
        Instant::now() + Duration::from_secs(10),
        || {
            a.debug_peer_healths().iter().any(|health| {
                health.peer_id == b.peer_id().to_string() && !has_missing_peer_record_issue(health)
            })
        },
    );
    let c = timed_network(vec![timed_listen(&a)], 60_000);
    wait_until(
        "C connects A",
        Instant::now() + Duration::from_secs(10),
        || c.connected_peers().contains(&a.peer_id()),
    );
    let request = || {
        c.request_with_providers(
            super::super::discovery::RR_GET_CACHED_PEER_RECORD,
            b.peer_id().to_string().as_bytes(),
            &[a.peer_id().to_string()],
        )
        .unwrap()
    };
    let signed = super::super::discovery::decode_optional_peer_record_response(&request())
        .unwrap()
        .expect("fresh cached B");
    super::super::peer_record::verify_signed_peer_record(&signed).unwrap();
    wait_until(
        "signed expiry",
        Instant::now() + Duration::from_secs(5),
        || now_ms() >= signed.record.expires_at_ms().unwrap(),
    );
    wait_until(
        "idle A loses expired B discovery evidence",
        Instant::now() + Duration::from_secs(2),
        || {
            a.debug_peer_healths().iter().any(|health| {
                health.peer_id == b.peer_id().to_string() && has_missing_peer_record_issue(health)
            })
        },
    );
    assert!(
        super::super::discovery::decode_optional_peer_record_response(&request())
            .unwrap()
            .is_none()
    );
    assert!(c.connected_peers().contains(&a.peer_id()));
    assert!(a.connected_peers().contains(&b.peer_id()));
}

#[test]
fn dht_result_expiring_between_selection_and_completion_is_not_delivered() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let signed = signed_discovery_peer_record(&key, vec![], now_ms());
    let clock = DiscoveryClock::default();
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::get_peer_record(peer, sender);
    handle_dht_progress(
        &mut pending,
        libp2p::kad::QueryResult::GetRecord(Ok(libp2p::kad::GetRecordOk::FoundRecord(
            libp2p::kad::PeerRecord {
                peer: Some(PeerId::random()),
                record: libp2p::kad::Record {
                    key: RecordKey::new(&"key"),
                    value: to_canonical_cbor(&signed).unwrap(),
                    publisher: None,
                    expires: None,
                },
            },
        ))),
        false,
        &clock,
    );
    assert!(matches!(
        &pending,
        PendingDhtQuery::GetPeerRecord {
            record: Some(_),
            ..
        }
    ));
    clock
        .observe(signed.record.expires_at_ms().unwrap(), Instant::now())
        .unwrap();
    handle_dht_progress(
        &mut pending,
        libp2p::kad::QueryResult::GetRecord(Ok(
            libp2p::kad::GetRecordOk::FinishedWithNoAdditionalRecord {
                cache_candidates: Default::default(),
            },
        )),
        true,
        &clock,
    );
    assert!(receiver.recv().unwrap().is_err());
}

#[test]
fn static_configuration_cannot_launder_another_port_record_lease() {
    let key = Keypair::generate_ed25519();
    let peer = PeerId::from(key.public());
    let signed = signed_discovery_peer_record(&key, vec![], now_ms());
    let mut path = peer_record_transport_paths(&signed).unwrap().remove(0);
    path.record_lease = Some(DiscoveryClock::default().check(&signed.record).unwrap());
    let record_label = path.label();
    let configured: Multiaddr = format!("/ip4/127.0.0.1/udp/4999/quic-v1/p2p/{peer}")
        .parse()
        .unwrap();
    let mut known = HashMap::from([(peer, vec![path])]);
    super::super::transport_paths::sync_static_bootstrap_transport_paths(
        &mut known,
        &mut HashSet::new(),
        std::slice::from_ref(&configured),
    );
    assert!(
        known[&peer]
            .iter()
            .find(|path| path.label() == record_label)
            .unwrap()
            .record_lease
            .is_some()
    );
    assert!(
        known[&peer]
            .iter()
            .any(|path| path.addr == configured && path.record_lease.is_none())
    );
}

#[test]
fn record_dial_does_not_install_permanent_kademlia_route() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _runtime_guard = runtime.enter();
    let key = Keypair::generate_ed25519();
    let signed = signed_discovery_peer_record(&key, vec![], now_ms());
    let peer = PeerId::from(key.public());
    let clock = DiscoveryClock::default();
    let lease = clock.check(&signed.record).unwrap();
    let mut swarm = build_swarm(
        &Keypair::generate_ed25519(),
        false,
        false,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let path = peer_record_transport_paths(&signed).unwrap().remove(0);
    assert!(swarm.behaviour().discovery_dial_policy().install(
        peer,
        vec![path.addr.clone()],
        lease
    ));
    super::super::swarm_behaviour::dial_record_addr(&mut swarm, path.addr).unwrap();
    assert!(
        !swarm
            .behaviour_mut()
            .kademlia
            .kbuckets()
            .any(|bucket| bucket
                .iter()
                .any(|entry| *entry.node.key.preimage() == peer)),
        "record dial must not install a permanent route"
    );
}

#[test]
fn final_resolution_rejects_expired_record_even_when_kad_retains_the_address() {
    use libp2p::swarm::{ConnectionId, NetworkBehaviour};
    let key = Keypair::generate_ed25519();
    let signed = signed_discovery_peer_record(&key, vec![], now_ms());
    let peer = PeerId::from(key.public());
    let clock = DiscoveryClock::default();
    let lease = clock.check(&signed.record).unwrap();
    let mut swarm = build_swarm(
        &Keypair::generate_ed25519(),
        false,
        false,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let path = peer_record_transport_paths(&signed).unwrap().remove(0);
    assert!(swarm.behaviour().discovery_dial_policy().install(
        peer,
        vec![path.addr.clone()],
        lease
    ));
    swarm
        .behaviour_mut()
        .kademlia
        .add_address(&peer, path.addr.clone());
    assert!(
        !swarm
            .behaviour_mut()
            .handle_pending_outbound_connection(
                ConnectionId::new_unchecked(44),
                Some(peer),
                &[],
                libp2p::core::Endpoint::Dialer
            )
            .unwrap()
            .is_empty()
    );
    clock
        .observe(signed.record.expires_at_ms().unwrap(), Instant::now())
        .unwrap();
    assert!(
        swarm
            .behaviour_mut()
            .handle_pending_outbound_connection(
                ConnectionId::new_unchecked(45),
                Some(peer),
                &[],
                libp2p::core::Endpoint::Dialer
            )
            .unwrap()
            .is_empty()
    );
    assert!(super::super::swarm_behaviour::dial_record_addr(&mut swarm, path.addr).is_err());
}
