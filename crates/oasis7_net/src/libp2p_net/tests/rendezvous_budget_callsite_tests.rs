use std::time::{Duration, Instant};

use super::super::{Libp2pNetwork, Libp2pNetworkConfig};
use libp2p::Multiaddr;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use oasis7_proto::distributed_dht::PeerRecord;

fn wait_until(what: &str, timeout: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(condition(), "timed out waiting for {what}");
}

fn rendezvous_event_count(network: &Libp2pNetwork) -> u64 {
    let metrics = network.traffic_metrics_snapshot();
    [
        "rendezvous_client.registered",
        "rendezvous_client.register_failed",
        "rendezvous_client.discovered",
        "rendezvous_client.discover_failed",
    ]
    .into_iter()
    .map(|kind| {
        metrics
            .control_plane
            .by_kind
            .get(kind)
            .copied()
            .unwrap_or(0)
    })
    .sum()
}

fn start_server() -> Libp2pNetwork {
    let server = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        keypair: Some(Keypair::generate_ed25519()),
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen address")],
        enable_rendezvous: true,
        enable_autonat: false,
        allow_loopback_external_addrs_for_testing: true,
        bootstrap_redial_interval_ms: 0,
        discovery_query_interval_ms: 0,
        republish_interval_ms: 0,
        ..Libp2pNetworkConfig::default()
    })
    .expect("start rendezvous server");
    wait_until("rendezvous server listener", Duration::from_secs(5), || {
        !server.listening_addrs().is_empty()
    });
    server
}

fn peer_address(network: &Libp2pNetwork) -> Multiaddr {
    network
        .listening_addrs()
        .into_iter()
        .find(|address| address.iter().any(|part| matches!(part, Protocol::Tcp(_))))
        .expect("TCP listener")
        .with_p2p(network.peer_id())
        .expect("append server peer id")
}

fn local_rendezvous_template(keypair: &Keypair) -> PeerRecord {
    super::super::tests::signed_discovery_peer_record(
        keypair,
        vec![crate::dht::PeerDiscoverySource::Rendezvous],
        super::super::now_ms(),
    )
    .record
}

#[test]
fn refresh_command_and_connection_event_obey_exhausted_budget_over_tcp() {
    let first_server = start_server();
    let client_key = Keypair::generate_ed25519();
    let client = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        keypair: Some(client_key.clone()),
        peer_record: Some(local_rendezvous_template(&client_key)),
        enable_rendezvous: true,
        enable_autonat: false,
        allow_loopback_external_addrs_for_testing: true,
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen address")],
        bootstrap_peers: vec![peer_address(&first_server)],
        bootstrap_redial_interval_ms: 0,
        discovery_query_interval_ms: 0,
        discovery_query_cooldown_ms: 0,
        republish_interval_ms: 0,
        ..Libp2pNetworkConfig::default()
    })
    .expect("start rendezvous client");

    wait_until("client listener", Duration::from_secs(5), || {
        !client.listening_addrs().is_empty()
    });
    wait_until("first TCP connection", Duration::from_secs(10), || {
        client.connected_peers().contains(&first_server.peer_id())
    });
    wait_until(
        "positive register and discover exchanges",
        Duration::from_secs(10),
        || {
            let metrics = client.traffic_metrics_snapshot();
            [
                "rendezvous_client.registered",
                "rendezvous_client.register_failed",
            ]
            .iter()
            .any(|kind| {
                metrics
                    .control_plane
                    .by_kind
                    .get(*kind)
                    .copied()
                    .unwrap_or(0)
                    > 0
            }) && [
                "rendezvous_client.discovered",
                "rendezvous_client.discover_failed",
            ]
            .iter()
            .any(|kind| {
                metrics
                    .control_plane
                    .by_kind
                    .get(*kind)
                    .copied()
                    .unwrap_or(0)
                    > 0
            })
        },
    );
    wait_until(
        "first connection work settles",
        Duration::from_secs(10),
        || client.traffic_metrics_snapshot().discovery_work.inflight == 0,
    );

    for _ in 0..256 {
        let before = client.traffic_metrics_snapshot().discovery_work;
        if before.window_starts >= before.max_window_starts {
            break;
        }
        client
            .enqueue_command(super::super::runtime_loop::Command::RefreshPeerDiscovery)
            .expect("enqueue production refresh command");
        wait_until(
            "refresh command budget decision",
            Duration::from_secs(5),
            || {
                let current = client.traffic_metrics_snapshot().discovery_work;
                current.admitted > before.admitted
                    || current.rejected_window > before.rejected_window
            },
        );
        wait_until(
            "refresh command work settles",
            Duration::from_secs(10),
            || client.traffic_metrics_snapshot().discovery_work.inflight == 0,
        );
    }
    let saturated = client.traffic_metrics_snapshot().discovery_work;
    assert_eq!(saturated.window_starts, saturated.max_window_starts);
    assert_eq!(saturated.inflight, 0);

    let events_before_periodic_retry = rendezvous_event_count(&client);
    let rejects_before_periodic_retry = saturated.rejected_window;
    client
        .enqueue_command(super::super::runtime_loop::Command::RefreshPeerDiscovery)
        .expect("enqueue exhausted periodic refresh");
    wait_until(
        "periodic refresh rejected by shared window",
        Duration::from_secs(5),
        || {
            client
                .traffic_metrics_snapshot()
                .discovery_work
                .rejected_window
                > rejects_before_periodic_retry
        },
    );
    assert_eq!(
        rendezvous_event_count(&client),
        events_before_periodic_retry,
        "RefreshPeerDiscovery must not schedule Rendezvous work after the shared budget is full"
    );

    let second_server = start_server();
    let connection_target = second_server.peer_id();
    let events_before_connection = rendezvous_event_count(&client);
    let rejects_before_connection = client
        .traffic_metrics_snapshot()
        .discovery_work
        .rejected_window;
    client
        .dial(peer_address(&second_server))
        .expect("dial second TCP rendezvous endpoint");
    wait_until(
        "second TCP connection and all event-branch denials",
        Duration::from_secs(10),
        || {
            let metrics = client.traffic_metrics_snapshot().discovery_work;
            client.connected_peers().contains(&connection_target)
                && metrics.rejected_window >= rejects_before_connection + 6
        },
    );
    assert_eq!(
        client
            .traffic_metrics_snapshot()
            .discovery_work
            .window_starts,
        saturated.max_window_starts
    );
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(
        rendezvous_event_count(&client),
        events_before_connection,
        "ConnectionEstablished must not send register/discover after budget exhaustion"
    );

    let events_before_connected_periodic = rendezvous_event_count(&client);
    let rejects_before_connected_periodic = client
        .traffic_metrics_snapshot()
        .discovery_work
        .rejected_window;
    client
        .enqueue_command(super::super::runtime_loop::Command::RefreshPeerDiscovery)
        .expect("enqueue connected periodic refresh");
    wait_until(
        "connected periodic refresh budget denials",
        Duration::from_secs(5),
        || {
            client
                .traffic_metrics_snapshot()
                .discovery_work
                .rejected_window
                >= rejects_before_connected_periodic + 2
        },
    );
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(
        rendezvous_event_count(&client),
        events_before_connected_periodic,
        "connected periodic refresh must also leave no new Rendezvous request when saturated"
    );
}
