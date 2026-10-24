use super::super::peer_discovery_observations::DiscoveryObservations;
use super::*;
use crate::util::to_canonical_cbor;

fn assert_mismatched_peer_record_response_is_rejected(
    kind: super::super::discovery::PendingPeerRecordRequest,
    target_peer_id: PeerId,
) {
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &Keypair::generate_ed25519(),
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let wrong_peer_key = Keypair::generate_ed25519();
    let wrong_peer_id = PeerId::from(wrong_peer_key.public());
    assert_ne!(wrong_peer_id, target_peer_id);
    let payload = to_canonical_cbor(&super::signed_discovery_peer_record(
        &wrong_peer_key,
        vec![crate::dht::PeerDiscoverySource::Dht],
        crate::libp2p_net::now_ms(),
    ))
    .expect("encode mismatched signed peer record");
    let local_peer_id = PeerId::random();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut discovered_peer_records = PeerRecordCache::default();
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

    match &kind {
        super::super::discovery::PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id, ..
        } => {
            assert_eq!(*peer_id, target_peer_id);
            pending_connected_peer_records.insert(*peer_id);
        }
        super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord { peer_id, .. } => {
            assert_eq!(*peer_id, target_peer_id);
            pending_cached_peer_records.insert(*peer_id);
        }
        super::super::discovery::PendingPeerRecordRequest::CachedDiscoveryPeers { .. } => {
            panic!("target-binding helper only covers peer-record responses")
        }
    }

    let response_source = match &kind {
        super::super::discovery::PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id, ..
        } => *peer_id,
        super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
            ask_peer, ..
        } => *ask_peer,
        super::super::discovery::PendingPeerRecordRequest::CachedDiscoveryPeers { peer_id } => {
            *peer_id
        }
    };
    super::super::discovery::handle_peer_record_response(
        &mut swarm,
        kind,
        response_source,
        payload.as_slice(),
        &mut pending_peer_record_requests,
        &mut pending_dht,
        &mut discovered_peer_records,
        &mut DiscoveryObservations::default(),
        &mut known_transport_paths,
        &mut last_dialed_transport_paths,
        &active_transport_paths,
        &[],
        &traffic_metrics,
        &mut failed_transport_path_labels,
        &mut pending_discovery_peer_records,
        &mut cached_peer_record_cooldowns,
        None,
        local_peer_id,
        &mut pending_connected_peer_records,
        &mut pending_cached_peer_records,
        &mut pending_cached_discovery_peers,
        16,
        &event_errors,
        &PeerManagerPolicy::default(),
        &HashSet::new(),
    );

    assert!(
        discovered_peer_records.is_empty(),
        "record for wrong peer must not enter discovered cache"
    );
    assert!(pending_dht.is_empty());
}

#[test]
fn connected_peer_record_response_rejects_signed_record_for_wrong_target() {
    let target_peer_id = PeerId::random();
    assert_mismatched_peer_record_response_is_rejected(
        super::super::discovery::PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id: target_peer_id,
            target_owned_route_expected: false,
        },
        target_peer_id,
    );
}

#[test]
fn cached_peer_record_response_rejects_signed_record_for_wrong_target() {
    let target_peer_id = PeerId::random();
    let ask_peer = PeerId::random();
    assert_mismatched_peer_record_response_is_rejected(
        super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
            ask_peer,
            peer_id: target_peer_id,
            tried_proxies: vec![ask_peer],
        },
        target_peer_id,
    );
}

#[test]
fn connected_peer_record_outbound_failure_falls_back_via_another_connected_peer() {
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &Keypair::generate_ed25519(),
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let target_peer_id = PeerId::random();
    let fallback_proxy = PeerId::random();
    let local_peer_id = PeerId::random();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_connected_peer_records = HashSet::from([target_peer_id]);
    let mut connected_peer_record_cooldowns = HashMap::from([(target_peer_id, i64::MAX)]);
    let mut pending_cached_peer_records = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();

    super::super::discovery::handle_peer_record_outbound_failure(
        &mut swarm,
        super::super::discovery::PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id: target_peer_id,
            target_owned_route_expected: false,
        },
        &mut pending_peer_record_requests,
        &mut pending_connected_peer_records,
        &mut connected_peer_record_cooldowns,
        &mut pending_cached_peer_records,
        &mut cached_peer_record_cooldowns,
        &mut pending_cached_discovery_peers,
        &mut cached_discovery_peer_cooldowns,
        &[target_peer_id, fallback_proxy],
        &traffic_metrics,
        local_peer_id,
    );

    assert!(!pending_connected_peer_records.contains(&target_peer_id));
    assert!(connected_peer_record_cooldowns.contains_key(&target_peer_id));
    assert!(pending_cached_peer_records.contains(&target_peer_id));
    assert!(cached_peer_record_cooldowns.contains_key(&target_peer_id));
    let retried = pending_peer_record_requests
        .values()
        .next()
        .expect("connected peer record failure should retry through a cache proxy");
    assert!(matches!(
        retried,
        super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
            ask_peer,
            peer_id,
            tried_proxies
        } if *ask_peer == fallback_proxy
            && *peer_id == target_peer_id
            && *tried_proxies == vec![fallback_proxy]
    ));
}

#[test]
fn connected_peer_record_failure_prefers_target_owned_dht_route_over_cache_proxy() {
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &Keypair::generate_ed25519(),
        false,
        true,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let target_peer_id = PeerId::random();
    let fallback_proxy = PeerId::random();
    let local_peer_id = PeerId::random();
    let mut pending_dht = HashMap::new();
    let mut pending_discovery_peer_records = HashSet::new();
    let discovered_peer_records = PeerRecordCache::default();
    super::super::discovery::maybe_queue_discovery_peer_record(
        &mut swarm,
        &mut pending_dht,
        &mut pending_discovery_peer_records,
        &discovered_peer_records,
        target_peer_id,
        local_peer_id,
        "world-a",
    );
    assert!(pending_discovery_peer_records.contains(&target_peer_id));
    assert!(pending_dht.values().any(|query| matches!(
        query,
        super::super::kad_queries::PendingDhtQuery::DiscoverPeerRecord {
            peer_id,
            upstream: None,
            ..
        } if *peer_id == target_peer_id
    )));

    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_connected_peer_records = HashSet::from([target_peer_id]);
    let mut connected_peer_record_cooldowns = HashMap::from([(target_peer_id, i64::MAX)]);
    let mut pending_cached_peer_records = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_discovery_peer_cooldowns = HashMap::new();
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();

    super::super::discovery::handle_peer_record_outbound_failure(
        &mut swarm,
        super::super::discovery::PendingPeerRecordRequest::ConnectedPeerRecord {
            peer_id: target_peer_id,
            target_owned_route_expected: true,
        },
        &mut pending_peer_record_requests,
        &mut pending_connected_peer_records,
        &mut connected_peer_record_cooldowns,
        &mut pending_cached_peer_records,
        &mut cached_peer_record_cooldowns,
        &mut pending_cached_discovery_peers,
        &mut cached_discovery_peer_cooldowns,
        &[target_peer_id, fallback_proxy],
        &traffic_metrics,
        local_peer_id,
    );

    assert!(
        pending_cached_peer_records.is_empty(),
        "a pending target-owned DHT route must not be replaced by cache-only fallback"
    );
    assert!(pending_peer_record_requests.is_empty());
}
