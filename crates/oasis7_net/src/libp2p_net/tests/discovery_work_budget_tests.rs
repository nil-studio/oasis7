use super::*;
use crate::util::to_canonical_cbor;

#[test]
fn automatic_discovery_candidate_queries_obey_shared_inflight_limit() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let keypair = Keypair::generate_ed25519();
    let local = PeerId::from(keypair.public());
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        false,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending = HashMap::new();
    let mut peers = HashSet::new();
    let cache = PeerRecordCache::default();
    for _ in 0..40 {
        super::super::discovery::maybe_queue_discovery_peer_record(
            &mut swarm,
            &mut pending,
            &mut peers,
            &cache,
            PeerId::random(),
            local,
            "world-a",
        );
    }
    assert_eq!(pending.len(), 32);
    assert_eq!(peers.len(), 32);
}

#[test]
fn shared_discovery_budget_survives_dht_to_rr_switch_and_terminal_release() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let keypair = Keypair::generate_ed25519();
    let local = PeerId::from(keypair.public());
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        false,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut queries = HashMap::new();
    let mut peers = HashSet::new();
    let cache = PeerRecordCache::default();
    for _ in 0..31 {
        super::super::discovery::maybe_queue_discovery_peer_record(
            &mut swarm,
            &mut queries,
            &mut peers,
            &cache,
            PeerId::random(),
            local,
            "world-a",
        );
    }
    let mut requests = HashMap::new();
    let mut pending = HashSet::new();
    let mut cooldowns = HashMap::new();
    let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    assert!(
        super::super::discovery::maybe_request_connected_peer_record(
            &mut swarm,
            &mut requests,
            &mut pending,
            &mut cooldowns,
            &metrics,
            PeerId::random(),
            local,
            false
        )
    );
    let totals = metrics.lock().unwrap().request.outbound.messages;
    let refused_peer = PeerId::random();
    assert!(
        !super::super::discovery::maybe_request_connected_peer_record(
            &mut swarm,
            &mut requests,
            &mut pending,
            &mut cooldowns,
            &metrics,
            refused_peer,
            local,
            false
        )
    );
    assert!(!cooldowns.contains_key(&refused_peer));
    assert!(!pending.contains(&refused_peer));
    assert_eq!(metrics.lock().unwrap().request.outbound.messages, totals);
    let id = *queries.keys().next().unwrap();
    swarm.behaviour_mut().discovery_budget.finish_dht(id);
    swarm.behaviour_mut().discovery_budget.finish_dht(id); // Late duplicate is not a second release.
    assert!(
        super::super::discovery::maybe_request_connected_peer_record(
            &mut swarm,
            &mut requests,
            &mut pending,
            &mut cooldowns,
            &metrics,
            refused_peer,
            local,
            false
        )
    );
    let snapshot = swarm.behaviour().discovery_budget.snapshot();
    assert_eq!(snapshot.inflight, 32);
    assert_eq!(snapshot.window_starts, 33);
    assert_eq!(snapshot.rejected_inflight, 1);
}

#[test]
fn rendezvous_register_and_discover_share_the_dht_rr_window() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let mut swarm = super::super::swarm_behaviour::build_swarm(
            &keypair,
            true,
            false,
            std::time::Duration::from_secs(30),
            super::super::wire_bytes::init_shared_wire_byte_counters(),
        );
        swarm.add_external_address(
            "/ip4/127.0.0.1/tcp/43001"
                .parse()
                .expect("valid rendezvous registration address"),
        );
        let cache = PeerRecordCache::default();
        let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        let mut pending_dht = HashMap::new();
        let mut pending_discovery_peer_records = HashSet::new();
        let mut pending_peer_record_requests = HashMap::new();
        let mut pending_connected_peer_records = HashSet::new();
        let mut connected_peer_record_cooldowns = HashMap::new();

        // Exercise the production DHT and RR schedulers. Simulate terminal
        // completion between batches so the 32-operation concurrency limit
        // does not hide the 128-start rolling-window limit.
        for _ in 0..4 {
            for _ in 0..16 {
                super::super::discovery::maybe_queue_discovery_peer_record(
                    &mut swarm,
                    &mut pending_dht,
                    &mut pending_discovery_peer_records,
                    &cache,
                    PeerId::random(),
                    local_peer_id,
                    "world-a",
                );
                assert!(
                    super::super::discovery::maybe_request_connected_peer_record(
                        &mut swarm,
                        &mut pending_peer_record_requests,
                        &mut pending_connected_peer_records,
                        &mut connected_peer_record_cooldowns,
                        &traffic_metrics,
                        PeerId::random(),
                        local_peer_id,
                        false,
                    )
                );
            }

            assert_eq!(pending_dht.len(), 16);
            assert_eq!(pending_peer_record_requests.len(), 16);
            for query_id in pending_dht.keys().copied().collect::<Vec<_>>() {
                swarm.behaviour_mut().discovery_budget.finish_dht(query_id);
            }
            for request_id in pending_peer_record_requests
                .keys()
                .copied()
                .collect::<Vec<_>>()
            {
                swarm
                    .behaviour_mut()
                    .discovery_budget
                    .finish_request(request_id);
            }
            pending_dht.clear();
            pending_discovery_peer_records.clear();
            pending_peer_record_requests.clear();
            pending_connected_peer_records.clear();
        }

        let template = super::signed_discovery_peer_record(
            &keypair,
            vec![crate::dht::PeerDiscoverySource::Rendezvous],
            super::super::now_ms(),
        )
        .record;
        let mut pending_rendezvous_registers = HashSet::new();
        let registered_rendezvous_nodes = HashSet::new();
        let mut pending_rendezvous_discovers = HashSet::new();
        let rendezvous_cookies = HashMap::new();
        let register_result = super::super::discovery::maybe_register_rendezvous_namespace(
            &mut swarm,
            &mut pending_rendezvous_registers,
            &registered_rendezvous_nodes,
            PeerId::random(),
            local_peer_id,
            &template,
        )
        .expect("rendezvous register helper");
        let discover_result = super::super::discovery::maybe_discover_rendezvous_namespace(
            &mut swarm,
            &mut pending_rendezvous_discovers,
            &rendezvous_cookies,
            PeerId::random(),
            local_peer_id,
            &template,
        )
        .expect("rendezvous discover helper");

        assert_eq!(
            (register_result, discover_result),
            (false, false),
            "rendezvous operations must be refused after shared DHT/RR work exhausts the window"
        );
        assert!(pending_rendezvous_registers.is_empty());
        assert!(pending_rendezvous_discovers.is_empty());
        let snapshot = swarm.behaviour().discovery_budget.snapshot();
        assert_eq!(snapshot.admitted, 128);
        assert_eq!(snapshot.window_starts, 128);
        assert_eq!(snapshot.rejected_window, 2);
        assert_eq!(snapshot.inflight, 0);
    });
}

#[test]
fn cached_discovery_list_is_bounded_and_deduplicated() {
    use super::super::discovery::decode_cached_discovery_peers_response as decode;
    let peer = PeerId::random();
    assert_eq!(
        decode(&to_canonical_cbor(&vec![peer.to_string(); 128]).unwrap()).unwrap(),
        vec![peer]
    );
    assert!(decode(&to_canonical_cbor(&vec![peer.to_string(); 129]).unwrap()).is_err());
    assert!(decode(&vec![0; 16 * 1024 + 1]).is_err());
    // Declared enormous CBOR array must be refused before allocating for its hint.
    assert!(decode(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).is_err());
}

#[test]
fn cached_discovery_server_returns_stable_first_128_live_ids() {
    let keypair = Keypair::generate_ed25519();
    let mut cache = PeerRecordCache::default();
    let mut reversed_cache = PeerRecordCache::default();
    let records: Vec<_> = (0..140)
        .map(|_| {
            let key = Keypair::generate_ed25519();
            let record = signed_discovery_peer_record(
                &key,
                vec![crate::dht::PeerDiscoverySource::Dht],
                super::super::now_ms(),
            );
            (PeerId::from(key.public()), record)
        })
        .collect();
    let mut expected: Vec<_> = records.iter().map(|(peer, _)| peer.to_string()).collect();
    expected.sort();
    expected.truncate(128);
    for (peer, record) in &records {
        cache.insert(*peer, record.clone());
    }
    for (peer, record) in records.into_iter().rev() {
        reversed_cache.insert(peer, record);
    }
    let request = NetworkRequest {
        protocol: "/aw/rr/1.0.0/get_cached_discovery_peers".into(),
        payload: Vec::new(),
    };
    let handlers = HashMap::new();
    let listening = Arc::new(Mutex::new(Vec::new()));
    let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let serve = |cache| {
        super::super::discovery::handle_request_response_request(
            &request,
            &handlers,
            None,
            &keypair,
            &listening,
            &reachability,
            false,
            cache,
        )
        .unwrap()
    };
    let payload = serve(&cache);
    assert_eq!(payload, serve(&reversed_cache));
    assert!(payload.len() <= 16 * 1024);
    let ids: Vec<String> = serde_cbor::from_slice(&payload).unwrap();
    assert_eq!(ids, expected);
    assert_eq!(
        super::super::discovery::decode_cached_discovery_peers_response(&payload)
            .unwrap()
            .len(),
        128
    );
}

#[test]
fn automatic_discovery_provider_progress_bounds_cumulative_results() {
    let mut pending = PendingDhtQuery::DiscoverPeers {
        peers: HashSet::new(),
        error: None,
    };
    let clock = super::super::peer_record_time::DiscoveryClock::default();
    let first: HashSet<_> = (0..100).map(|_| PeerId::random()).collect();
    let second: HashSet<_> = (0..100).map(|_| PeerId::random()).collect();
    let all: HashSet<_> = first.union(&second).copied().collect();
    for (providers, is_last) in [(first, false), (second, true)] {
        let action = handle_dht_progress(
            &mut pending,
            kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                key: RecordKey::new(&"world-a-discovery"),
                providers,
            })),
            is_last,
            &clock,
        );
        if is_last {
            let DhtProgressAction::DiscoverPeers(found) = action else {
                panic!("terminal query must return its bounded candidate set");
            };
            assert_eq!(found.len(), 128);
            assert!(found.iter().all(|peer| all.contains(peer)));
        } else {
            assert!(matches!(action, DhtProgressAction::None));
        }
    }
    let PendingDhtQuery::DiscoverPeers { peers, .. } = pending else {
        unreachable!();
    };
    assert_eq!(peers.len(), 128);
}

#[test]
fn traffic_snapshot_without_discovery_field_uses_default_budget_policy() {
    let snapshot = Libp2pTrafficMetricsSnapshot::default();
    let mut legacy = serde_json::to_value(&snapshot).unwrap();
    legacy.as_object_mut().unwrap().remove("discovery_work");
    let restored: Libp2pTrafficMetricsSnapshot = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        restored.discovery_work,
        DiscoveryWorkMetricsSnapshot::default()
    );
    assert_eq!(restored, snapshot);
}
