use super::super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
use super::super::receiver_floor::PeerRecordReceiver;
use super::discovery::{PendingPeerRecordRequest, handle_peer_record_response};
use super::peer_discovery_observations::DiscoveryIngress;
use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_record_cache::PeerRecordCache;
use super::peer_record_time::DiscoveryClock;
use super::swarm_behaviour::build_swarm_with_receiver;
use super::*;
use oasis7_proto::distributed_dht::{
    DistributedDht as _, PeerRecordPublicationStateV2, PeerRecordPublicationV2,
};

#[cfg(unix)]
fn durable_relay_receiver_fixture() -> (
    tempfile::TempDir,
    Keypair,
    oasis7_proto::distributed_dht::PeerRecord,
    super::super::receiver_floor::SharedPeerRecordReceiver,
) {
    let root = tempfile::tempdir_in(
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root"),
    )
    .expect("receiver fixture root");
    let directory = root.path().join("receiver");
    std::fs::create_dir(&directory).expect("create receiver directory");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
        .expect("make receiver directory private");

    let keypair = Keypair::generate_ed25519();
    let template = super::signed_discovery_peer_record(&keypair, Vec::new(), now_ms()).record;
    let config = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::DurableCompatible,
        directory: Some(directory),
    };
    PeerRecordReceiver::initialize(&config, &keypair, &template)
        .expect("initialize durable receiver");
    let receiver = PeerRecordReceiver::open(&config, &keypair, Some(&template))
        .expect("open durable receiver");
    (root, keypair, template, receiver)
}

#[cfg(unix)]
fn signed_relay_record(
    with_owner_destination: bool,
) -> oasis7_proto::distributed_dht::SignedPeerRecord {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let relay_peer_id = PeerId::random();
    let mut record = super::signed_discovery_peer_record(
        &keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        now_ms(),
    )
    .record;
    record.direct_addrs.clear();
    let relay_address = format!("/dns4/relay.example/tcp/443/p2p/{relay_peer_id}/p2p-circuit");
    record.relay_addrs = vec![if with_owner_destination {
        format!("{relay_address}/p2p/{peer_id}")
    } else {
        relay_address
    }];
    super::super::peer_record::sign_peer_record_v2(
        &record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &keypair,
    )
    .expect("sign V2 relay record")
}

#[cfg(unix)]
#[test]
fn durable_candidate_accepts_relay_hop_with_owner_destination() {
    let (_root, local_keypair, template, receiver) = durable_relay_receiver_fixture();
    let signed = signed_relay_record(true);
    let peer_id = signed
        .record
        .peer_id
        .parse::<PeerId>()
        .expect("record peer id");
    let upstream = PeerId::random();
    let mut swarm = build_swarm_with_receiver(
        &local_keypair,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        receiver.clone(),
    );
    let mut cache = PeerRecordCache::with_dial_policy_and_receiver(
        DiscoveryClock::default(),
        swarm.behaviour().discovery_dial_policy(),
        receiver,
    );
    let mut observations = DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();
    let policy = PeerManagerPolicy {
        min_active_discovery_sources: 0,
        min_peer_discovery_sources: 0,
        ..PeerManagerPolicy::default()
    };

    super::super::peer_record_candidate::process_signed_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&template),
        &policy,
        &HashSet::new(),
        signed.clone(),
        DiscoveryIngress::PeerExchange { upstream },
    )
    .expect("a signed relay hop may precede its owner destination");

    assert_eq!(cache.get(&peer_id), Some(&signed));
    let path = known_paths
        .get(&peer_id)
        .and_then(|paths| paths.first())
        .expect("accepted relay candidate path");
    assert_eq!(
        path.kind,
        super::super::transport_paths::TransportPathKind::RelayReserved
    );
    assert_eq!(path.addr.to_string(), signed.record.relay_addrs[0]);
    assert!(
        path.record_lease
            .as_ref()
            .expect("durable lease")
            .validate()
            .is_ok()
    );

    let reservation = signed_relay_record(false);
    let reservation_peer_id = reservation
        .record
        .peer_id
        .parse::<PeerId>()
        .expect("reservation record peer id");
    super::super::peer_record_candidate::process_signed_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&template),
        &policy,
        &HashSet::new(),
        reservation.clone(),
        DiscoveryIngress::PeerExchange {
            upstream: PeerId::random(),
        },
    )
    .expect("a relay reservation may omit the owner destination component");
    assert_eq!(cache.get(&reservation_peer_id), Some(&reservation));
    let reservation_path = known_paths
        .get(&reservation_peer_id)
        .and_then(|paths| paths.first())
        .expect("accepted reservation path");
    assert_eq!(
        reservation_path.addr.to_string(),
        format!(
            "{}/p2p/{reservation_peer_id}",
            reservation.record.relay_addrs[0]
        )
    );
}

#[cfg(unix)]
#[test]
fn durable_dht_store_accepts_relay_hop_with_owner_destination() {
    use super::super::peer_record_admission::PeerRecordAdmissionBudget;
    use super::super::peer_record_store::PeerRecordStore;
    use libp2p::kad::Record;
    use libp2p::kad::RecordKey;
    use libp2p::kad::store::{MemoryStoreConfig, RecordStore};

    let (_root, _local_keypair, _template, receiver) = durable_relay_receiver_fixture();
    let signed = signed_relay_record(true);
    let key = RecordKey::new(&oasis7_proto::distributed::dht_peer_record_key(
        &signed.record.world_id,
        &signed.record.peer_id,
    ));
    let bytes = crate::util::to_canonical_cbor(&signed).expect("encode signed relay record");
    let mut store = PeerRecordStore::with_admission_budget_and_receiver(
        PeerId::random(),
        MemoryStoreConfig::default(),
        PeerRecordAdmissionBudget::shared(),
        receiver.clone(),
    );

    store
        .put(Record {
            key: key.clone(),
            value: bytes,
            publisher: None,
            expires: None,
        })
        .expect("durable DHT admission accepts the signed relay path");

    assert!(
        store.get(&key).is_some(),
        "accepted relay record is readable"
    );
    assert!(
        receiver.lease_guard(&signed).is_ok(),
        "the DHT store commits the V2 floor before exposing the record"
    );
}

#[cfg(unix)]
#[test]
fn lower_v2_sequence_cannot_replace_a_higher_sequence_candidate() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let upstream = PeerId::random();
    let local_peer_id = PeerId::random();
    let sign_at_sequence = |sequence| {
        let mut record = super::signed_discovery_peer_record(
            &keypair,
            vec![crate::dht::PeerDiscoverySource::Dht],
            now_ms(),
        )
        .record;
        record.direct_addrs.clear();
        super::super::peer_record::sign_peer_record_v2(
            &record,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence,
                state: PeerRecordPublicationStateV2::Live,
            },
            &keypair,
        )
        .expect("sign V2 peer record")
    };
    let higher = sign_at_sequence(2);
    let lower = sign_at_sequence(1);
    let local_keypair = Keypair::generate_ed25519();
    let local_template = super::signed_discovery_peer_record(
        &local_keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        now_ms(),
    )
    .record;
    let directory = tempfile::tempdir_in(
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root"),
    )
    .expect("receiver temp directory");
    let receiver_directory = directory.path().join("receiver");
    std::fs::create_dir(&receiver_directory).expect("create receiver directory");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&receiver_directory, std::fs::Permissions::from_mode(0o700))
        .expect("make receiver directory private");
    let receiver_config = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::DurableCompatible,
        directory: Some(receiver_directory),
    };
    PeerRecordReceiver::initialize(&receiver_config, &local_keypair, &local_template)
        .expect("initialize receiver floor");
    let receiver =
        PeerRecordReceiver::open(&receiver_config, &local_keypair, Some(&local_template))
            .expect("open receiver floor");
    let mut swarm = build_swarm_with_receiver(
        &local_keypair,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        receiver.clone(),
    );
    let mut cache = PeerRecordCache::with_dial_policy_and_receiver(
        DiscoveryClock::default(),
        Default::default(),
        receiver,
    );
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

    for signed in [&higher, &lower] {
        let payload = crate::util::to_canonical_cbor(signed).expect("encode signed V2 record");
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
            local_peer_id,
            &mut pending_connected_peer_records,
            &mut pending_cached_peer_records,
            &mut pending_cached_discovery_peers,
            16,
            &event_errors,
            &policy,
            &static_bootstrap_peers,
        );
    }

    assert_eq!(
        cache.get(&peer_id),
        Some(&higher),
        "a valid but lower V2 sequence must not replace the cached candidate"
    );
    assert!(
        event_errors
            .lock()
            .unwrap()
            .iter()
            .any(|message| message.contains("V2 sequence regressed"))
    );
}

#[test]
fn v2_only_rejects_a_legacy_local_publisher_configuration_before_startup() {
    let keypair = Keypair::generate_ed25519();
    let template = super::signed_discovery_peer_record(&keypair, vec![], now_ms()).record;
    let result = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        keypair: Some(keypair),
        peer_record: Some(template),
        peer_record_receiver: PeerRecordReceiverConfig {
            mode: PeerRecordReceiverMode::V2Only,
            directory: Some(std::path::PathBuf::from("unused-receiver-state")),
        },
        ..Default::default()
    });
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("strict V2 receive mode cannot publish a local V1 record"),
    };
    assert!(format!("{error:?}").contains("durable V2 publication"));
}

#[cfg(unix)]
#[test]
fn same_sequence_conflict_revokes_queued_dial_and_keeps_inflight_connection_unverified() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build libp2p test runtime");
    let _runtime_guard = runtime.enter();

    let local_keypair = Keypair::generate_ed25519();
    let local_template =
        super::signed_discovery_peer_record(&local_keypair, Vec::new(), now_ms()).record;
    let directory = tempfile::tempdir_in(
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root"),
    )
    .expect("receiver temp directory");
    let receiver_directory = directory.path().join("receiver");
    std::fs::create_dir(&receiver_directory).expect("create receiver directory");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&receiver_directory, std::fs::Permissions::from_mode(0o700))
        .expect("make receiver directory private");
    let receiver_config = PeerRecordReceiverConfig {
        mode: PeerRecordReceiverMode::DurableCompatible,
        directory: Some(receiver_directory),
    };
    PeerRecordReceiver::initialize(&receiver_config, &local_keypair, &local_template)
        .expect("initialize receiver floor");
    let receiver =
        PeerRecordReceiver::open(&receiver_config, &local_keypair, Some(&local_template))
            .expect("open receiver floor");

    let remote_keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(remote_keypair.public());
    let upstream = PeerId::random();
    let mut record = super::signed_discovery_peer_record(
        &remote_keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        now_ms(),
    )
    .record;
    record.capability_lanes = vec![NetworkLane::Sync, NetworkLane::Control];
    record.direct_addrs = vec!["/ip4/127.0.0.1/tcp/4103".to_string()];
    let sign = |record: &oasis7_proto::distributed_dht::PeerRecord| {
        super::super::peer_record::sign_peer_record_v2(
            record,
            PeerRecordPublicationV2 {
                schema_version: 2,
                sequence: 2,
                state: PeerRecordPublicationStateV2::Live,
            },
            &remote_keypair,
        )
        .expect("sign V2 peer record")
    };
    let original = sign(&record);
    let mut forked_record = record.clone();
    forked_record.node_id.push_str("-fork");
    let conflict = sign(&forked_record);
    let mut swarm = build_swarm_with_receiver(
        &local_keypair,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        receiver.clone(),
    );
    let mut cache = PeerRecordCache::with_dial_policy_and_receiver(
        DiscoveryClock::default(),
        swarm.behaviour().discovery_dial_policy(),
        receiver,
    );
    let mut observations = DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let mut active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();
    let static_peers = HashSet::new();
    let policy = PeerManagerPolicy {
        min_active_discovery_sources: 0,
        min_peer_discovery_sources: 0,
        ..PeerManagerPolicy::default()
    };
    let ingress = DiscoveryIngress::PeerExchange { upstream };
    super::super::peer_record_candidate::process_signed_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&local_template),
        &policy,
        &static_peers,
        original.clone(),
        ingress.clone(),
    )
    .expect("install original V2 candidate");
    let active_path = known_paths
        .get(&peer_id)
        .and_then(|paths| paths.first())
        .expect("original record path")
        .clone();
    assert!(
        active_path
            .record_lease
            .as_ref()
            .expect("discovered path carries its record lease")
            .validate()
            .is_ok()
    );
    let queued_old_path = active_path.clone();
    active_paths.insert(peer_id, active_path);
    assert_eq!(
        super::super::runtime_loop::filter_request_peers_by_lane(
            vec![peer_id],
            "/aw/node/replication/fetch-blob/1.0.0",
            &cache,
        ),
        Vec::<PeerId>::new(),
        "the active session is restricted by its signed capability metadata"
    );

    let event_healths = Arc::new(Mutex::new(HashMap::new()));
    let event_block_artifacts = Arc::new(Mutex::new(HashMap::new()));
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let event_reachability = Arc::new(Mutex::new(Default::default()));
    let mut pending_quarantine_disconnects = HashSet::new();
    let (mut healths, mut quarantined, mut admitted) =
        super::super::connection_lifecycle::refresh_peer_manager_views(
            &mut swarm,
            &cache,
            &observations,
            &active_paths,
            &HashSet::new(),
            &policy,
            &event_healths,
            &event_block_artifacts,
            &mut pending_quarantine_disconnects,
            &event_errors,
            16,
            &event_reachability,
        );
    assert!(healths.contains_key(&peer_id));
    assert!(quarantined.is_empty());
    assert!(
        event_healths
            .lock()
            .expect("health snapshot")
            .get(&peer_id.to_string())
            .expect("original health entry")
            .discovery_sources
            .contains(&crate::dht::PeerDiscoverySource::PeerExchange)
    );

    // Keep one queued path for the final gate, and separately start a dial
    // before conflict isolation to model an in-flight transport attempt.
    let mut queued_swarm = build_swarm_with_receiver(
        &local_keypair,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        cache.receiver(),
    );
    let mut queued_dials = HashMap::new();
    let mut started_dials = HashMap::new();
    super::super::transport_paths::dial_transport_path(
        &mut swarm,
        &mut started_dials,
        queued_old_path.clone(),
    )
    .expect("the live record lease initially authorizes a started dial");
    assert!(started_dials.contains_key(&peer_id));

    let error = super::super::peer_record_candidate::process_signed_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&local_template),
        &policy,
        &static_peers,
        conflict,
        ingress,
    )
    .expect_err("same-sequence signed fork must isolate the peer");
    assert!(error.invalidates_peer_projection());
    assert!(cache.get(&peer_id).is_none());
    assert!(
        cache.session_record(&peer_id).is_some(),
        "conflict must retain metadata while the session is active"
    );
    assert!(cache.prune(&active_paths));
    assert!(cache.session_record(&peer_id).is_some());
    assert!(!known_paths.contains_key(&peer_id));
    assert!(!last_dialed_paths.contains_key(&peer_id));
    let queued_error = super::super::transport_paths::dial_transport_path(
        &mut queued_swarm,
        &mut queued_dials,
        queued_old_path.clone(),
    )
    .expect_err("the final gate must reject a queued dial after durable conflict isolation");
    assert!(format!("{queued_error:?}").contains("stale or isolated"));
    assert!(
        queued_dials.is_empty(),
        "a rejected queue item starts no dial"
    );
    assert_eq!(
        super::super::runtime_loop::filter_request_peers_by_lane(
            vec![peer_id],
            "/aw/node/replication/fetch-blob/1.0.0",
            &cache,
        ),
        Vec::<PeerId>::new(),
        "an invalidated record must not make a restricted active session appear unknown"
    );

    (healths, quarantined, admitted) =
        super::super::connection_lifecycle::refresh_peer_manager_views_after_record_error(
            &error,
            &mut swarm,
            &cache,
            &observations,
            &active_paths,
            &admitted,
            &policy,
            &event_healths,
            &event_block_artifacts,
            &mut pending_quarantine_disconnects,
            &event_errors,
            16,
            &event_reachability,
        )
        .expect("same-sequence conflicts refresh the production health projection");
    assert_eq!(
        event_healths
            .lock()
            .expect("refreshed health snapshot")
            .get(&peer_id.to_string())
            .expect("active session health remains published")
            .discovery_sources,
        Vec::<crate::dht::PeerDiscoverySource>::new(),
        "the invalidated record must not preserve its old source attribution"
    );
    let active_health = healths
        .get(&peer_id)
        .expect("active session remains visible to health projection");
    assert!(active_health.discovery_sources.is_empty());
    assert!(
        active_health
            .issues
            .contains(&PeerManagerHealthIssue::MissingPeerRecord)
    );
    assert!(quarantined.is_empty());
    assert!(admitted.contains(&peer_id));
    assert!(observations.observed_sources(&original).is_empty());

    let connection_id = libp2p::swarm::ConnectionId::new_unchecked(9_001);
    let endpoint = libp2p::core::connection::ConnectedPoint::Dialer {
        address: queued_old_path.addr.clone(),
        role_override: libp2p::core::connection::Endpoint::Dialer,
        port_use: libp2p::core::transport::PortUse::New,
    };
    let mut established_paths = HashMap::new();
    let mut established_by_peer = HashMap::new();
    // The already-started attempt can still produce ConnectionEstablished,
    // but the production lifecycle helper cannot restore the invalidated lease.
    let (established_path, dialed_addr) =
        super::super::connection_lifecycle::record_established_connection(
            &known_paths,
            &mut active_paths,
            &mut last_dialed_paths,
            &mut failed_path_labels,
            &mut established_paths,
            &mut established_by_peer,
            peer_id,
            connection_id,
            &endpoint,
        );
    let established_path = established_path.expect("established connection path");
    assert_eq!(dialed_addr, Some(queued_old_path.addr.clone()));
    assert!(
        established_path.record_lease.is_none(),
        "a connection event arriving after invalidation is only an unverified path"
    );
    assert!(
        active_paths[&peer_id].record_lease.is_none(),
        "ConnectionEstablished must not restore the stale candidate lease"
    );
    assert!(cache.get(&peer_id).is_none(), "no candidate is reinstalled");
    assert!(observations.observed_sources(&original).is_empty());
    assert_eq!(
        super::super::runtime_loop::filter_request_peers_by_lane(
            vec![peer_id],
            "/aw/node/replication/fetch-blob/1.0.0",
            &cache,
        ),
        Vec::<PeerId>::new(),
        "an unverified established path does not erase the session's old restrictions"
    );

    (healths, quarantined, admitted) =
        super::super::connection_lifecycle::refresh_peer_manager_views(
            &mut swarm,
            &cache,
            &observations,
            &active_paths,
            &admitted,
            &policy,
            &event_healths,
            &event_block_artifacts,
            &mut pending_quarantine_disconnects,
            &event_errors,
            16,
            &event_reachability,
        );
    let established_health = healths
        .get(&peer_id)
        .expect("the established transport remains visible");
    assert!(established_health.discovery_sources.is_empty());
    assert!(
        established_health
            .issues
            .contains(&PeerManagerHealthIssue::MissingPeerRecord)
    );
    assert!(quarantined.is_empty());
    assert!(admitted.contains(&peer_id));
    assert!(
        event_healths
            .lock()
            .expect("post-establishment health snapshot")
            .get(&peer_id.to_string())
            .expect("established session health remains projected")
            .discovery_sources
            .is_empty()
    );

    let ordinary_error = super::super::peer_record_candidate::PeerRecordInstallError::Rejected(
        crate::error::WorldError::NetworkProtocolUnavailable {
            protocol: "ordinary invalid record".into(),
        },
    );
    assert!(
        super::super::connection_lifecycle::refresh_peer_manager_views_after_record_error(
            &ordinary_error,
            &mut swarm,
            &cache,
            &observations,
            &active_paths,
            &admitted,
            &policy,
            &event_healths,
            &event_block_artifacts,
            &mut pending_quarantine_disconnects,
            &event_errors,
            16,
            &event_reachability,
        )
        .is_none()
    );
}

#[cfg(unix)]
#[test]
fn v2_only_receiver_starts_with_durable_publisher_and_rejects_v1_records() {
    use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};

    let temp_root = tempfile::tempdir_in(
        std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp root"),
    )
    .expect("strict receiver fixture root");
    let local_keypair = Keypair::generate_ed25519();
    let mut local_template =
        super::signed_discovery_peer_record(&local_keypair, Vec::new(), now_ms()).record;
    local_template.direct_addrs.clear();

    let config = Libp2pNetworkConfig {
        keypair: Some(local_keypair.clone()),
        peer_record: Some(local_template),
        peer_record_publication_directory: Some(temp_root.path().join("publisher")),
        peer_record_receiver: PeerRecordReceiverConfig {
            mode: PeerRecordReceiverMode::V2Only,
            directory: Some(temp_root.path().join("receiver")),
        },
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("TCP listener")],
        republish_interval_ms: 0,
        discovery_query_interval_ms: 0,
        bootstrap_redial_interval_ms: 0,
        allow_loopback_external_addrs_for_testing: true,
        request_response_timeout: Duration::from_secs(3),
        ..Libp2pNetworkConfig::default()
    };
    Libp2pNetwork::initialize_peer_record_publication(&config)
        .expect("initialize durable local V2 publisher");
    Libp2pNetwork::initialize_peer_record_receiver(&config)
        .expect("initialize V2-only inbound floor");
    let network = Libp2pNetwork::try_new(config)
        .expect("V2-only receiver starts with its durable local publisher");

    let remote_keypair = Keypair::generate_ed25519();
    let mut remote_record =
        super::signed_discovery_peer_record(&remote_keypair, Vec::new(), now_ms()).record;
    remote_record.direct_addrs.clear();
    let v2 = super::super::peer_record::sign_peer_record_v2(
        &remote_record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &remote_keypair,
    )
    .expect("sign V2 remote record");
    let put_result = network.put_peer_record("world-a", &v2);
    let stored_v2 = network
        .get_peer_record("world-a", &v2.record.peer_id)
        .expect("read locally stored V2 record")
        .expect("V2 record passed the strict receiver and reached the DHT store");
    assert_eq!(stored_v2, v2);
    if let Err(error) = put_result {
        assert!(
            format!("{error:?}")
                .contains("kad put_record failed: the quorum failed; needed 1 peers"),
            "V2 passed local strict admission; only an unavailable remote quorum may fail: {error:?}"
        );
    }

    let v1 = super::super::peer_record::sign_peer_record(&remote_record, &remote_keypair)
        .expect("sign legacy remote record");
    let error = network
        .put_peer_record("world-a", &v1)
        .expect_err("V2-only receiver rejects a V1 record at the DHT store");
    assert!(
        format!("{error:?}").contains("the value is too large to be stored"),
        "unexpected V1 rejection: {error:?}"
    );
    assert_eq!(
        network
            .get_peer_record("world-a", &v2.record.peer_id)
            .expect("read after V1 rejection")
            .expect("the accepted V2 record remains stored"),
        v2,
        "rejected V1 input cannot replace the accepted V2 record"
    );
}
