use super::*;
use libp2p::swarm::ConnectionId;

use crate::libp2p_net::transport_paths::{
    recompute_active_transport_path_for_peer, select_reconnect_transport_path_after_close,
    sync_static_bootstrap_transport_paths,
};

#[test]
fn libp2p_network_generates_peer_id() {
    let network = Libp2pNetwork::new(Libp2pNetworkConfig::default());
    assert!(!network.peer_id().to_string().is_empty());
}

#[test]
fn reconnect_after_final_close_retries_single_disconnected_static_path() {
    let peer_id = PeerId::random();
    let direct_addr = format!("/ip4/39.104.204.172/tcp/6831/p2p/{peer_id}")
        .parse()
        .expect("direct addr");
    let direct_path = active_transport_path_from_endpoint(&HashMap::new(), peer_id, &direct_addr);
    let mut known = HashMap::from([(peer_id, vec![direct_path.clone()])]);
    let mut active_transport_paths = HashMap::new();
    let mut failed = HashSet::new();

    let (previous_path, next_path) = select_reconnect_transport_path_after_close(
        &known,
        &mut active_transport_paths,
        &mut failed,
        peer_id,
        Some(direct_path.clone()),
    )
    .expect("single static path should be redialable");

    assert_eq!(previous_path, Some(direct_path.clone()));
    assert_eq!(next_path, direct_path);
    assert!(failed.contains(&direct_addr.to_string()));

    known.clear();
    failed.clear();
    assert!(
        select_reconnect_transport_path_after_close(
            &known,
            &mut active_transport_paths,
            &mut failed,
            peer_id,
            Some(direct_path.clone()),
        )
        .is_none()
    );
}

#[test]
fn static_bootstrap_paths_are_known_before_peer_record_exchange() {
    let peer_id = PeerId::random();
    let direct_addr: Multiaddr = format!("/ip4/39.104.204.172/tcp/6831/p2p/{peer_id}")
        .parse()
        .expect("direct addr");
    let transient_peer = PeerId::random();
    let transient_addr: Multiaddr = format!("/ip4/10.0.0.42/tcp/49200/p2p/{transient_peer}")
        .parse()
        .expect("transient addr");
    let mut known = HashMap::new();
    let mut failed = HashSet::from([direct_addr.to_string()]);

    sync_static_bootstrap_transport_paths(
        &mut known,
        &mut failed,
        &[direct_addr.clone(), transient_addr.clone()],
    );

    assert_eq!(
        known
            .get(&peer_id)
            .expect("static bootstrap peer path")
            .first()
            .expect("static bootstrap path")
            .addr,
        direct_addr
    );
    assert_eq!(
        known
            .get(&transient_peer)
            .expect("configured peer path")
            .first()
            .expect("configured path")
            .addr,
        transient_addr
    );
    assert!(!failed.contains(&direct_addr.to_string()));
}

#[test]
fn refresh_active_transport_path_after_partial_close_promotes_remaining_known_path() {
    let peer_id = PeerId::random();
    let direct_addr = format!("/ip4/39.104.205.67/tcp/5612/p2p/{peer_id}")
        .parse()
        .expect("direct addr");
    let relay_addr = format!(
        "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit/p2p/{peer_id}",
        PeerId::random()
    )
    .parse()
    .expect("relay addr");
    let mut known = HashMap::new();
    known.insert(
        peer_id,
        vec![
            active_transport_path_from_endpoint(&HashMap::new(), peer_id, &direct_addr),
            active_transport_path_from_endpoint(&HashMap::new(), peer_id, &relay_addr),
        ],
    );
    let direct_connection = ConnectionId::new_unchecked(1);
    let relay_connection = ConnectionId::new_unchecked(2);
    let mut active_transport_paths = HashMap::new();
    let mut established_transport_paths = HashMap::from([
        (
            direct_connection,
            active_transport_path_from_endpoint(&known, peer_id, &direct_addr),
        ),
        (
            relay_connection,
            active_transport_path_from_endpoint(&known, peer_id, &relay_addr),
        ),
    ]);
    let initial = recompute_active_transport_path_for_peer(
        &mut active_transport_paths,
        &established_transport_paths,
        &HashMap::from([(
            peer_id,
            HashSet::from([direct_connection, relay_connection]),
        )]),
        peer_id,
    )
    .expect("initial active path");
    assert_eq!(initial.addr, direct_addr);

    established_transport_paths.remove(&direct_connection);
    let refreshed = recompute_active_transport_path_for_peer(
        &mut active_transport_paths,
        &established_transport_paths,
        &HashMap::from([(peer_id, HashSet::from([relay_connection]))]),
        peer_id,
    )
    .expect("replacement path");

    assert_eq!(refreshed.addr, relay_addr);
    assert_eq!(
        active_transport_paths
            .get(&peer_id)
            .expect("active path after refresh")
            .addr,
        relay_addr
    );
}

#[test]
fn established_endpoint_cannot_borrow_a_signed_record_lease_from_another_port() {
    let signed = signed_discovery_peer_record(&Keypair::generate_ed25519(), vec![], 1_000);
    let peer_id: PeerId = signed.record.peer_id.parse().unwrap();
    let advertised: Multiaddr = format!("/ip4/127.0.0.1/udp/4103/quic-v1/p2p/{peer_id}")
        .parse()
        .unwrap();
    let established: Multiaddr = format!("/ip4/127.0.0.1/udp/4104/quic-v1/p2p/{peer_id}")
        .parse()
        .unwrap();
    let lease = super::super::peer_record_time::DiscoveryClock::default()
        .check_at(&signed.record, 1_000, Instant::now())
        .unwrap();

    let mut signed_path =
        active_transport_path_from_endpoint(&HashMap::new(), peer_id, &advertised);
    signed_path.record_lease = Some(lease);
    let known = HashMap::from([(peer_id, vec![signed_path])]);
    let mut active = HashMap::new();
    let mut dialed = HashMap::new();
    let mut failed = HashSet::from([advertised.to_string()]);

    // A successful, authenticated connection on the same IP and transport
    // family but a different UDP port cannot prove that the advertised signed
    // endpoint was established, clear its failure, or inherit its lease.
    let unrelated = super::super::transport_paths::note_established_transport_path(
        &known,
        &mut active,
        &mut dialed,
        &mut failed,
        peer_id,
        &established,
    );
    assert_eq!(unrelated.addr, established);
    assert!(unrelated.record_lease.is_none());
    assert!(failed.contains(&advertised.to_string()));
    assert_eq!(active.get(&peer_id).unwrap().addr, established);

    // An exact endpoint really can reuse the original signed lease; clearing
    // the failed label is reserved for this actual address only.
    let exact = super::super::transport_paths::note_established_transport_path(
        &known,
        &mut active,
        &mut dialed,
        &mut failed,
        peer_id,
        &advertised,
    );
    assert_eq!(exact.addr, advertised);
    assert!(exact.record_lease.is_some());
    assert!(!failed.contains(&advertised.to_string()));
}
