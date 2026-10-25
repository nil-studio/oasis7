#[cfg(unix)]
use super::*;
#[cfg(unix)]
use libp2p::identity::Keypair;
#[cfg(unix)]
use libp2p::{Multiaddr, PeerId};
#[cfg(unix)]
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};
#[cfg(unix)]
use oasis7_proto::distributed_dht::{
    DistributedDht as _, PeerRecord, PeerRecordPublicationStateV2, PeerRecordPublicationV2,
    SignedPeerRecord,
};
#[cfg(unix)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
fn config(
    keypair: &Keypair,
    peer_record: Option<PeerRecord>,
    bootstrap_peers: Vec<Multiaddr>,
    receiver_directory: Option<std::path::PathBuf>,
) -> Libp2pNetworkConfig {
    let peer_record_receiver = if let Some(directory) = receiver_directory {
        PeerRecordReceiverConfig {
            mode: PeerRecordReceiverMode::DurableCompatible,
            directory: Some(directory),
        }
    } else {
        PeerRecordReceiverConfig::default()
    };
    Libp2pNetworkConfig {
        keypair: Some(keypair.clone()),
        peer_record,
        peer_record_receiver,
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("TCP listen address")],
        bootstrap_peers,
        bootstrap_redial_interval_ms: 0,
        discovery_query_interval_ms: 0,
        republish_interval_ms: 0,
        allow_loopback_external_addrs_for_testing: true,
        request_response_timeout: Duration::from_secs(4),
        peer_manager_policy: PeerManagerPolicy {
            min_active_discovery_sources: 0,
            min_peer_discovery_sources: 0,
            ..PeerManagerPolicy::default()
        },
        ..Libp2pNetworkConfig::default()
    }
}

#[cfg(unix)]
fn tcp_peer_address(network: &Libp2pNetwork) -> Multiaddr {
    wait_until(
        "TCP receiver test listener",
        Instant::now() + Duration::from_secs(10),
        || !network.listening_addrs().is_empty(),
    );
    network
        .listening_addrs()
        .into_iter()
        .next()
        .expect("TCP listener is published")
        .with(libp2p::multiaddr::Protocol::P2p(network.peer_id()))
}

#[cfg(unix)]
fn connect(network: &Libp2pNetwork, peer: PeerId) {
    wait_until(
        "TCP peer connection",
        Instant::now() + Duration::from_secs(10),
        || network.connected_peers().contains(&peer),
    );
}

#[cfg(unix)]
fn open_receiver(config: &Libp2pNetworkConfig) -> Libp2pNetwork {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match Libp2pNetwork::try_new(config.clone()) {
            Ok(network) => return network,
            Err(error) if Instant::now() < deadline => {
                let message = format!("{error:?}");
                assert!(
                    message.contains("already in use"),
                    "unexpected receiver reopen failure: {message}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("receiver lock did not release after shutdown: {error:?}"),
        }
    }
}

#[cfg(unix)]
fn signed_v2(keypair: &Keypair, sequence: u64) -> SignedPeerRecord {
    let mut record = super::signed_discovery_peer_record(
        keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        super::now_ms(),
    )
    .record;
    record.direct_addrs.clear();
    record.ttl_ms = 300_000;
    super::super::peer_record::sign_peer_record_v2(
        &record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence,
            state: PeerRecordPublicationStateV2::Live,
        },
        keypair,
    )
    .expect("sign V2 test record")
}

#[cfg(unix)]
fn signed_v1(keypair: &Keypair) -> SignedPeerRecord {
    let mut record = super::signed_discovery_peer_record(
        keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        super::now_ms(),
    )
    .record;
    record.direct_addrs.clear();
    record.ttl_ms = 300_000;
    super::super::peer_record::sign_peer_record(&record, keypair).expect("sign V1 test record")
}

#[cfg(unix)]
fn signed_tombstone(keypair: &Keypair, sequence: u64) -> SignedPeerRecord {
    let mut record = signed_v2(keypair, sequence).record;
    record.node_id = format!("withdrawn-{}", record.peer_id);
    record.direct_addrs.clear();
    record.hole_punch_addrs.clear();
    record.relay_addrs.clear();
    super::super::peer_record::sign_peer_record_v2(
        &record,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        keypair,
    )
    .expect("sign V2 tombstone")
}

#[cfg(unix)]
fn store_and_verify_source_record(source: &Libp2pNetwork, record: &SignedPeerRecord) {
    if let Err(error) = source.put_peer_record("world-a", record) {
        assert!(
            format!("{error:?}")
                .contains("kad put_record failed: the quorum failed; needed 1 peers"),
            "unexpected source DHT write failure: {error:?}"
        );
    }
    let stored = source
        .get_peer_record("world-a", &record.record.peer_id)
        .expect("read source-local Kademlia record")
        .expect("source-local Kademlia record is present");
    assert_eq!(
        stored, *record,
        "source-local DHT store must retain the exact signed record for the TCP receiver query"
    );
}

#[cfg(unix)]
fn assert_remote_record_rejected(receiver: &Libp2pNetwork, peer_id: PeerId, expected_reason: &str) {
    let error = receiver
        .get_peer_record("world-a", &peer_id.to_string())
        .expect_err("durable receiver must refuse this remote record");
    assert!(
        format!("{error:?}").contains(expected_reason),
        "unexpected rejection: {error:?}"
    );
}

#[cfg(unix)]
#[test]
fn tcp_dht_receiver_persists_v2_floor_across_restart_and_allows_higher_sequence() {
    use std::os::unix::fs::PermissionsExt;

    let physical_temp_dir =
        std::fs::canonicalize(std::env::temp_dir()).expect("resolve temporary directory");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let scratch = physical_temp_dir.join(format!(
        "oasis7-peer-record-receiver-transport-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&scratch).expect("create isolated TCP fixture directory");
    std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o700))
        .expect("restrict TCP fixture directory permissions");
    let receiver_directory = scratch.join("receiver");

    let source_keypair = Keypair::generate_ed25519();
    let source_peer = PeerId::from(source_keypair.public());
    let source = Libp2pNetwork::try_new(config(&source_keypair, None, Vec::new(), None))
        .expect("start TCP DHT source without a local peer-record template");
    let source_address = tcp_peer_address(&source);

    let receiver_keypair = Keypair::generate_ed25519();
    let receiver_template =
        super::signed_discovery_peer_record(&receiver_keypair, Vec::new(), super::now_ms()).record;
    let receiver_config = config(
        &receiver_keypair,
        Some(receiver_template),
        vec![source_address],
        Some(receiver_directory.clone()),
    );
    Libp2pNetwork::initialize_peer_record_receiver(&receiver_config)
        .expect("explicitly initialize durable receiver floor");

    let receiver = open_receiver(&receiver_config);
    connect(&receiver, source_peer);

    let high = signed_v2(&source_keypair, 2);
    assert_eq!(high.record.peer_id, source_peer.to_string());
    store_and_verify_source_record(&source, &high);
    let initial = receiver
        .get_peer_record("world-a", &source_peer.to_string())
        .expect("fetch V2 record over TCP Kademlia")
        .expect("first V2 record is accepted");
    assert_eq!(
        initial, high,
        "initial high sequence is admitted and returned"
    );

    drop(receiver);
    let lower = signed_v2(&source_keypair, 1);
    store_and_verify_source_record(&source, &lower);
    let receiver = open_receiver(&receiver_config);
    connect(&receiver, source_peer);
    assert_remote_record_rejected(&receiver, source_peer, "peer-record V2 sequence regressed");

    drop(receiver);
    let legacy = signed_v1(&source_keypair);
    store_and_verify_source_record(&source, &legacy);
    let receiver = open_receiver(&receiver_config);
    connect(&receiver, source_peer);
    assert_remote_record_rejected(
        &receiver,
        source_peer,
        "legacy peer record rejected after V2 floor migration",
    );

    drop(receiver);
    let higher = signed_v2(&source_keypair, 3);
    store_and_verify_source_record(&source, &higher);
    let receiver = open_receiver(&receiver_config);
    connect(&receiver, source_peer);
    let recovered = receiver
        .get_peer_record("world-a", &source_peer.to_string())
        .expect("fetch higher V2 record after reopen")
        .expect("higher V2 sequence remains admissible");
    assert_eq!(
        recovered, higher,
        "a valid higher sequence recovers after stale and legacy records were refused"
    );

    drop(receiver);
    drop(source);
    std::thread::sleep(Duration::from_millis(100));
    std::fs::remove_dir_all(&scratch).expect("remove isolated TCP receiver fixture");
}

#[cfg(unix)]
#[test]
fn tcp_dht_tombstone_is_forwarded_verbatim_until_higher_live_replaces_it() {
    use std::os::unix::fs::PermissionsExt;

    let physical_temp_dir =
        std::fs::canonicalize(std::env::temp_dir()).expect("resolve temporary directory");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let scratch = physical_temp_dir.join(format!(
        "oasis7-peer-record-tombstone-transport-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&scratch).expect("create isolated TCP fixture directory");
    std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o700))
        .expect("restrict TCP fixture directory permissions");

    let private_directory = |name: &str| {
        let directory = scratch.join(name);
        std::fs::create_dir(&directory).expect("create durable state directory");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .expect("restrict durable state directory permissions");
        directory
    };

    let source_keypair = Keypair::generate_ed25519();
    let source_peer = PeerId::from(source_keypair.public());
    let source_template =
        super::signed_discovery_peer_record(&source_keypair, Vec::new(), super::now_ms()).record;
    let mut source_config = config(
        &source_keypair,
        Some(source_template),
        Vec::new(),
        Some(private_directory("source-receiver")),
    );
    source_config.peer_record_publication_directory = Some(private_directory("source-publisher"));
    Libp2pNetwork::initialize_peer_record_receiver(&source_config)
        .expect("initialize source durable receiver");
    Libp2pNetwork::initialize_peer_record_publication(&source_config)
        .expect("initialize source durable publisher");
    let source = Libp2pNetwork::try_new(source_config).expect("start durable TCP DHT source");
    let source_address = tcp_peer_address(&source);

    let receiver_keypair = Keypair::generate_ed25519();
    let receiver_template =
        super::signed_discovery_peer_record(&receiver_keypair, Vec::new(), super::now_ms()).record;
    let mut receiver_config = config(
        &receiver_keypair,
        Some(receiver_template),
        vec![source_address],
        Some(private_directory("receiver-floor")),
    );
    receiver_config.peer_record_publication_directory =
        Some(private_directory("receiver-publisher"));
    Libp2pNetwork::initialize_peer_record_receiver(&receiver_config)
        .expect("initialize receiver durable floor");
    Libp2pNetwork::initialize_peer_record_publication(&receiver_config)
        .expect("initialize receiver durable publisher");
    let receiver = Libp2pNetwork::try_new(receiver_config).expect("start durable TCP DHT receiver");
    connect(&receiver, source_peer);
    let receiver_address = tcp_peer_address(&receiver);

    let observer_keypair = Keypair::generate_ed25519();
    let observer = Libp2pNetwork::try_new(config(
        &observer_keypair,
        None,
        vec![receiver_address],
        None,
    ))
    .expect("start TCP observer");
    connect(&observer, receiver.peer_id());

    let high = signed_v2(&source_keypair, 2);
    store_and_verify_source_record(&source, &high);
    let initial = receiver
        .get_peer_record("world-a", &source_peer.to_string())
        .expect("fetch first signed V2 record over TCP")
        .expect("sequence 2 live record is accepted");
    assert_eq!(initial, high);

    let tombstone = signed_tombstone(&source_keypair, 3);
    store_and_verify_source_record(&source, &tombstone);
    let received_tombstone = receiver
        .get_peer_record("world-a", &source_peer.to_string())
        .expect("fetch tombstone over TCP")
        .expect("signed tombstone advances the receiver floor");
    assert_eq!(received_tombstone, tombstone);
    let forwarded_bytes = observer
        .request_to_peer(
            super::super::discovery::RR_GET_CACHED_PEER_RECORD,
            source_peer.to_string().as_bytes(),
            receiver.peer_id(),
        )
        .expect("receiver forwards its current durable tombstone over TCP");
    let forwarded: Option<SignedPeerRecord> =
        serde_cbor::from_slice(&forwarded_bytes).expect("decode forwarded tombstone");
    assert_eq!(
        forwarded,
        Some(tombstone.clone()),
        "forwarding preserves the received signature and exact sequence"
    );

    let higher_live = signed_v2(&source_keypair, 4);
    store_and_verify_source_record(&source, &higher_live);
    let recovered = receiver
        .get_peer_record("world-a", &source_peer.to_string())
        .expect("fetch higher live record over TCP")
        .expect("sequence 4 restores the live record");
    assert_eq!(recovered, higher_live);
    let no_stale_tombstone = observer.request_to_peer(
        super::super::discovery::RR_GET_CACHED_PEER_RECORD,
        source_peer.to_string().as_bytes(),
        receiver.peer_id(),
    );
    match no_stale_tombstone {
        Ok(bytes) => match super::super::discovery::decode_optional_peer_record_response(&bytes) {
            Ok(Some(forwarded)) => assert_eq!(
                forwarded, higher_live,
                "the only forwarded record after recovery must be the exact higher Live"
            ),
            Ok(None) => {
                let error: ErrorResponse =
                    serde_cbor::from_slice(&bytes).expect("decode NotFound response");
                assert_eq!(error.code, DistributedErrorCode::ErrNotFound);
                assert_eq!(
                    error.message,
                    format!("cached peer record not found for peer={source_peer}")
                );
                assert!(error.retryable);
            }
            Err(error) => panic!("unexpected post-recovery RR response: {error:?}"),
        },
        Err(error) => panic!("unexpected post-recovery transport failure: {error:?}"),
    }

    drop(observer);
    drop(receiver);
    drop(source);
    std::thread::sleep(Duration::from_millis(100));
    std::fs::remove_dir_all(&scratch).expect("remove isolated TCP tombstone fixture");
}
