#[cfg(unix)]
use super::*;
#[cfg(unix)]
use libp2p::identity::Keypair;
#[cfg(unix)]
use libp2p::{Multiaddr, PeerId};
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
