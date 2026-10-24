use super::*;
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};

fn request_response_message_counts(network: &Libp2pNetwork) -> (u64, u64, u64, u64) {
    let snapshot = network.traffic_metrics_snapshot();
    (
        snapshot.request.outbound.messages,
        snapshot.response.inbound.messages,
        snapshot.request.inbound.messages,
        snapshot.response.outbound.messages,
    )
}

fn inbound_peer_record_admissions(network: &Libp2pNetwork) -> u64 {
    let snapshot = network.traffic_metrics_snapshot();
    snapshot.request.inbound.messages + snapshot.response.inbound.messages
}

fn wait_for_request_response_quiescence(client: &Libp2pNetwork, server: &Libp2pNetwork) {
    let mut previous = None;
    let mut stable_observations = 0;
    wait_until(
        "both request-response peers settle their special traffic",
        Instant::now() + Duration::from_secs(10),
        || {
            let client_counts = request_response_message_counts(client);
            let server_counts = request_response_message_counts(server);
            let settled = client_counts.0 == client_counts.1
                && client_counts.2 == client_counts.3
                && server_counts.0 == server_counts.1
                && server_counts.2 == server_counts.3;
            let current = (client_counts, server_counts);
            if settled && previous == Some(current) {
                stable_observations += 1;
            } else {
                stable_observations = 0;
            }
            previous = Some(current);
            stable_observations >= 2
        },
    );
}

#[test]
fn tcp_special_peer_record_requests_share_per_source_admission_budget() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("server listen addr")],
        request_response_timeout: Duration::from_secs(3),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "peer-record service listener",
        Instant::now() + Duration::from_secs(10),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server.listening_addrs()[0]
        .clone()
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(3),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connected to peer-record service",
        Instant::now() + Duration::from_secs(10),
        || client.connected_peers().contains(&server.peer_id()),
    );

    wait_for_request_response_quiescence(&client, &server);
    let client_background_admissions = inbound_peer_record_admissions(&client);
    let server_background_admissions = inbound_peer_record_admissions(&server);
    assert!(client_background_admissions < 256);
    assert!(server_background_admissions < 256);

    let mut successful_responses = 0;
    let mut busy_responses = 0;
    for _ in 0..260 {
        let result = client.request_to_peer_with_timeout(
            super::super::discovery::RR_GET_CACHED_DISCOVERY_PEERS,
            &[],
            server.peer_id(),
            Duration::from_secs(3),
        );
        match result {
            Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrBusy,
                retryable: true,
                ..
            }) => busy_responses += 1,
            Ok(payload) => match serde_cbor::from_slice::<Vec<String>>(&payload) {
                Ok(peers) => {
                    assert!(peers.is_empty());
                    successful_responses += 1;
                }
                Err(_) => {
                    let error: ErrorResponse = serde_cbor::from_slice(&payload)
                        .expect("over-budget service request returns a bounded protocol error");
                    assert_eq!(error.code, DistributedErrorCode::ErrBusy);
                    assert!(error.retryable);
                    busy_responses += 1;
                }
            },
            Err(err) => panic!("over-budget request must be retryably busy: {err:?}"),
        }
    }
    wait_for_request_response_quiescence(&client, &server);
    let client_inbound_after_flood = inbound_peer_record_admissions(&client);
    let server_inbound_after_flood = inbound_peer_record_admissions(&server);
    assert_eq!(
        client_inbound_after_flood.checked_sub(260),
        Some(client_background_admissions),
        "the client's inbound request/response counters must contain only the 260 manual responses beyond the settled baseline"
    );
    assert_eq!(
        server_inbound_after_flood.checked_sub(260),
        Some(server_background_admissions),
        "the server's inbound request/response counters must contain only the 260 manual requests beyond the settled baseline"
    );
    let expected_successes = 256 - client_background_admissions.max(server_background_admissions);
    assert_eq!(
        successful_responses as u64, expected_successes,
        "each manual exchange consumes one admission at each side; the fuller source budget determines when normal responses stop"
    );
    assert!(
        busy_responses > 0,
        "the source budget must eventually reject"
    );
}

#[test]
fn admitted_list_and_128_peer_record_candidates_fit_one_actual_response_path() {
    use super::super::discovery::{PendingPeerRecordRequest, handle_peer_record_response};
    use super::super::peer_discovery_observations::DiscoveryObservations;
    use super::super::peer_record_cache::PeerRecordCache;
    use super::super::swarm_behaviour::build_swarm;

    let upstream = PeerId::random();
    let local_peer = PeerId::random();
    let keys: Vec<_> = (0..128).map(|_| Keypair::generate_ed25519()).collect();
    let peers: Vec<_> = keys.iter().map(|key| PeerId::from(key.public())).collect();
    let list_payload =
        crate::util::to_canonical_cbor(&peers.iter().map(ToString::to_string).collect::<Vec<_>>())
            .expect("encode bounded discovery list");

    let mut swarm = build_swarm(
        &Keypair::generate_ed25519(),
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut cache = PeerRecordCache::default();
    let mut observations = DiscoveryObservations::default();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut known_transport_paths = HashMap::new();
    let mut last_dialed_transport_paths = HashMap::new();
    let active_transport_paths = HashMap::new();
    let mut failed_transport_path_labels = HashSet::new();
    let mut pending_discovery_peer_records = HashSet::new();
    let mut pending_connected_peer_records = HashSet::new();
    let mut pending_cached_peer_records = HashSet::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let policy = super::super::peer_manager::PeerManagerPolicy::default();
    let static_bootstrap_peers = HashSet::new();

    handle_peer_record_response(
        &mut swarm,
        PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id: upstream },
        upstream,
        list_payload.as_slice(),
        &mut pending_peer_record_requests,
        &mut pending_dht,
        &mut cache,
        &mut observations,
        &mut known_transport_paths,
        &mut last_dialed_transport_paths,
        &active_transport_paths,
        &[],
        &traffic_metrics,
        &mut failed_transport_path_labels,
        &mut pending_discovery_peer_records,
        &mut cached_peer_record_cooldowns,
        None,
        local_peer,
        &mut pending_connected_peer_records,
        &mut pending_cached_peer_records,
        &mut pending_cached_discovery_peers,
        16,
        &event_errors,
        &policy,
        &static_bootstrap_peers,
    );

    for (peer_id, key) in peers.iter().copied().zip(keys.iter()) {
        let mut record = super::signed_discovery_peer_record(
            key,
            vec![crate::dht::PeerDiscoverySource::Dht],
            now_ms(),
        )
        .record;
        record.direct_addrs.clear();
        let signed = crate::libp2p_net::peer_record::sign_peer_record(&record, key)
            .expect("sign peer-record response");
        let payload = crate::util::to_canonical_cbor(&signed).expect("encode signed response");
        handle_peer_record_response(
            &mut swarm,
            PendingPeerRecordRequest::CachedPeerRecord {
                ask_peer: upstream,
                peer_id,
                tried_proxies: vec![upstream],
            },
            upstream,
            payload.as_slice(),
            &mut pending_peer_record_requests,
            &mut pending_dht,
            &mut cache,
            &mut observations,
            &mut known_transport_paths,
            &mut last_dialed_transport_paths,
            &active_transport_paths,
            &[],
            &traffic_metrics,
            &mut failed_transport_path_labels,
            &mut pending_discovery_peer_records,
            &mut cached_peer_record_cooldowns,
            None,
            local_peer,
            &mut pending_connected_peer_records,
            &mut pending_cached_peer_records,
            &mut pending_cached_discovery_peers,
            16,
            &event_errors,
            &policy,
            &static_bootstrap_peers,
        );
    }

    assert_eq!(cache.values().count(), 128);
    assert!(event_errors.lock().unwrap().is_empty());
}

#[test]
fn dht_store_puts_and_public_result_share_the_same_global_budget() {
    use super::super::kad_queries::{PendingDhtQuery, handle_dht_progress_with_admission};
    use super::super::peer_record_admission::PeerRecordAdmissionBudget;
    use super::super::peer_record_store::PeerRecordStore;

    let shared = PeerRecordAdmissionBudget::shared();
    let key = Keypair::generate_ed25519();
    let signed = super::signed_discovery_peer_record(
        &key,
        vec![crate::dht::PeerDiscoverySource::Dht],
        now_ms(),
    );
    let peer_id = PeerId::from(key.public());
    let value = crate::util::to_canonical_cbor(&signed).expect("encode signed record");
    let key = oasis7_proto::distributed::dht_peer_record_key(
        &signed.record.world_id,
        &signed.record.peer_id,
    );
    let record = libp2p::kad::Record {
        key: libp2p::kad::RecordKey::new(&key),
        value: value.clone(),
        publisher: None,
        expires: None,
    };
    let mut store = PeerRecordStore::with_admission_budget(
        PeerId::random(),
        libp2p::kad::store::MemoryStoreConfig::default(),
        Arc::clone(&shared),
    );
    for _ in 0..512 {
        libp2p::kad::store::RecordStore::put(&mut store, record.clone())
            .expect("bounded valid DHT record put");
    }
    assert_eq!(libp2p::kad::store::RecordStore::records(&store).count(), 1);

    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetPeerRecord {
        peer_id,
        response: Some(sender),
        record: None,
        error: None,
    };
    let result = libp2p::kad::QueryResult::GetRecord(Ok(libp2p::kad::GetRecordOk::FoundRecord(
        libp2p::kad::PeerRecord { peer: None, record },
    )));
    handle_dht_progress_with_admission(
        &mut pending,
        result,
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
        &shared,
    );
    let outcome = receiver
        .recv()
        .expect("public get-peer-record response is delivered");
    assert!(
        matches!(
            outcome,
            Err(WorldError::NetworkRequestFailed {
                code: DistributedErrorCode::ErrBusy,
                retryable: true,
                ..
            })
        ),
        "the public DHT result must share the exhausted global store budget; got {outcome:?}"
    );
    assert_eq!(libp2p::kad::store::RecordStore::records(&store).count(), 1);
}
