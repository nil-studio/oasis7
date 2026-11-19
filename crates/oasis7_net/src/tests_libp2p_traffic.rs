use std::time::{Duration, Instant};

use libp2p::Multiaddr;
use oasis7_proto::distributed_dht::{
    PeerDeploymentMode, PeerDiscoverySource, PeerNodeRole, PeerReachabilityClass, PeerRecord,
};
use oasis7_proto::distributed_net::DistributedNetwork as _;

use super::{
    Libp2pNetwork, Libp2pNetworkConfig, PeerManagerPolicy, ResourceBudgetRejection, WorldError,
};

fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for condition: {what}");
}

#[test]
fn libp2p_traffic_metrics_track_requests_and_gossip_payloads() {
    let deadline = || Instant::now() + Duration::from_secs(10);
    let peer_manager_policy = PeerManagerPolicy {
        min_active_discovery_sources: 0,
        min_peer_discovery_sources: 0,
        min_active_peers_for_share_limits: 4,
        max_ipv4_subnet_active_peers: None,
        max_ipv4_subnet_share_per_mille: 1_000,
        block_ipv4_subnet_share_per_mille: 1_000,
        max_relay_domain_share_per_mille: 1_000,
        block_relay_domain_share_per_mille: 1_000,
        max_operator_share_per_mille: 1_000,
        block_operator_share_per_mille: 1_000,
        max_asn_share_per_mille: 1_000,
        block_asn_share_per_mille: 1_000,
        max_relayed_active_peer_share_per_mille: 1_000,
    };
    let peer_record = PeerRecord {
        peer_id: String::new(),
        node_id: "traffic-node".to_string(),
        world_id: "traffic-world".to_string(),
        network_id: "traffic-network".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Private,
        reachability_class: PeerReachabilityClass::Private,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![PeerDiscoverySource::Dht],
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 1,
        ttl_ms: 60_000,
    };

    let listen_addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().expect("multiaddr");
    let net1 = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![listen_addr],
        peer_record: Some(peer_record.clone()),
        peer_manager_policy: peer_manager_policy.clone(),
        ..Libp2pNetworkConfig::default()
    });
    wait_until("net1 listening", deadline(), || {
        !net1.listening_addrs().is_empty()
    });
    let dial_addr = net1
        .listening_addrs()
        .into_iter()
        .find(|addr| addr.to_string().contains("127.0.0.1"))
        .expect("net1 addr")
        .with(libp2p::multiaddr::Protocol::P2p(net1.peer_id()));

    net1.register_handler(
        "/aw/rr/1.0.0/ping",
        Box::new(|payload| {
            let mut out = payload.to_vec();
            out.extend_from_slice(b"-ok");
            Ok(out)
        }),
    )
    .expect("register");

    let net2 = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen")],
        bootstrap_peers: vec![dial_addr],
        peer_record: Some(peer_record),
        peer_manager_policy,
        ..Libp2pNetworkConfig::default()
    });

    wait_until("net2 connected", deadline(), || {
        !net2.connected_peers().is_empty()
    });
    wait_until("request converges", deadline(), || {
        match net2.request("/aw/rr/1.0.0/ping", b"ping") {
            Ok(reply) => reply == b"ping-ok".to_vec(),
            Err(WorldError::NetworkProtocolUnavailable { .. }) => false,
            Err(err) => panic!("unexpected request error: {err:?}"),
        }
    });

    let sub2 = net2.subscribe("aw.traffic").expect("sub2");
    let _sub1 = net1.subscribe("aw.traffic").expect("sub1");
    std::thread::sleep(Duration::from_millis(200));
    net1.publish("aw.traffic", b"hello").expect("publish");
    wait_until("gossipsub", deadline(), || {
        sub2.drain().iter().any(|payload| payload == b"hello")
    });

    let net1_traffic = net1.traffic_metrics_snapshot();
    assert_eq!(net1_traffic.gossip.outbound.payload_bytes, 5);
    assert_eq!(
        net1_traffic
            .by_protocol
            .get("/aw/rr/1.0.0/ping")
            .map(|lane| (lane.inbound.payload_bytes, lane.outbound.payload_bytes)),
        Some((4, 7))
    );
    assert_eq!(
        net1_traffic
            .by_protocol
            .get("/aw/rr/1.0.0/ping")
            .map(|lane| (lane.inbound.messages, lane.outbound.messages)),
        Some((1, 1))
    );

    let net2_traffic = net2.traffic_metrics_snapshot();
    assert_eq!(
        net2_traffic
            .by_protocol
            .get("/aw/rr/1.0.0/ping")
            .map(|lane| (lane.outbound.payload_bytes, lane.inbound.payload_bytes)),
        Some((4, 7))
    );
    assert_eq!(
        net2_traffic
            .by_topic
            .get("aw.traffic")
            .map(|lane| lane.inbound.payload_bytes),
        Some(5)
    );
}

#[test]
fn typed_net_budget_caps_real_business_substream_before_remote_decode() {
    for address in ["/ip4/127.0.0.1/tcp/0", "/ip4/127.0.0.1/udp/0/quic-v1"] {
        exercise_active_business_substream(address, ResourceBudgetRejection::BytesTotal);
    }
}

#[test]
fn active_bootstrap_respects_the_real_outbound_byte_limit() {
    exercise_active_business_substream("/ip4/127.0.0.1/tcp/0", ResourceBudgetRejection::BytesOut);
}

fn exercise_active_business_substream(address: &str, reason: ResourceBudgetRejection) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let outbound_limit: u64 = if reason == ResourceBudgetRejection::BytesOut {
        8192
    } else {
        16384
    };
    let profile = serde_json::from_value(serde_json::json!({
        "profile": "v1-discovery-resource-budget-1",
        "connections_total": 3, "pending_connections_total": 2, "pending_inbound_unknown": 1,
        "pending_outbound": 2, "connections_per_peer": 1, "handshake_timeout_ms": 3000,
        "bytes_in_per_window": 16384, "bytes_out_per_window": outbound_limit,
        "bytes_total_per_window": 16384
    }))
    .unwrap();
    let server = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        listen_addrs: vec![address.parse().unwrap()],
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let observer = Arc::clone(&seen);
    let protocol = "/test/shared-business-budget";
    server
        .register_handler(
            protocol,
            Box::new(move |payload| {
                observer.fetch_add(1, Ordering::Relaxed);
                Ok(payload.to_vec())
            }),
        )
        .unwrap();
    wait_until(
        "transport listener",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );

    let peer = server.peer_id();
    let bootstrap = server.listening_addrs()[0].clone().with_p2p(peer).unwrap();
    let client = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        p2p_resource_budget: profile,
        bootstrap_peers: vec![bootstrap],
        enable_autonat: false,
        request_response_timeout: Duration::from_millis(800),
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    wait_until(
        "authenticated bootstrap peer",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&peer),
    );
    assert_eq!(
        client
            .request_to_peer_with_timeout(protocol, b"probe", peer, Duration::from_secs(3))
            .unwrap_or_else(|error| panic!(
                "small request failed: {error:?}; client {:?}; server {:?}; budget {:?}",
                client.debug_errors(),
                server.debug_errors(),
                client.traffic_metrics_snapshot().resource_budget,
            )),
        b"probe",
    );
    assert_eq!(seen.load(Ordering::Relaxed), 1);
    assert!(
        client
            .request_to_peer_with_timeout(protocol, &[7; 32768], peer, Duration::from_secs(3))
            .is_err()
    );
    assert_eq!(
        seen.load(Ordering::Relaxed),
        1,
        "receiver decoded an over-budget request"
    );

    let status = client.traffic_metrics_snapshot().resource_budget;
    let connections = status.connections.unwrap();
    assert_eq!(
        (
            connections.total,
            connections.pending,
            connections.established
        ),
        (1, 0, 1)
    );
    let bytes = status.bytes.unwrap();
    if reason == ResourceBudgetRejection::BytesOut {
        assert_eq!(bytes.actual_bytes.outbound, outbound_limit);
        assert_eq!(bytes.charged_bytes.outbound, outbound_limit);
        assert!(bytes.charged_bytes.total < 16384);
    } else {
        assert_eq!(bytes.charged_bytes.total, 16384);
    }
    assert!(bytes.actual_bytes.inbound > 0 && bytes.actual_bytes.outbound > 0);
    assert_eq!(bytes.reserved_bytes.total, 0);
    assert_eq!(status.last_rejection, Some(reason));
    let retry_ms = status.next_attempt_delay_ms.unwrap();
    assert!(retry_ms > 0 && retry_ms <= 61_000);
}

#[test]
fn real_transport_deadline_reports_a_fixed_budget_status() {
    let profile = serde_json::from_str(
        r#"{"profile":"v1-discovery-resource-budget-1",
        "connections_total":3,"pending_connections_total":2,"pending_inbound_unknown":1,
        "pending_outbound":2,"connections_per_peer":1,"handshake_timeout_ms":1000,
        "bytes_in_per_window":16384,"bytes_out_per_window":16384,"bytes_total_per_window":16384}"#,
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        p2p_resource_budget: profile,
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let address = format!(
        "/ip4/127.0.0.1/tcp/{}",
        listener.local_addr().unwrap().port()
    );
    client.dial(address.parse().unwrap()).unwrap();
    let mut accepted = None;
    wait_until(
        "pending actual TCP socket",
        Instant::now() + Duration::from_secs(5),
        || match listener.accept() {
            Ok((socket, _)) => {
                accepted = Some(socket);
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => panic!("TCP accept failed: {error}"),
        },
    );
    assert!(accepted.is_some());
    wait_until(
        "fixed handshake deadline status",
        Instant::now() + Duration::from_secs(5),
        || {
            let budget = client.traffic_metrics_snapshot().resource_budget;
            budget.last_rejection == Some(crate::ResourceBudgetRejection::HandshakeDeadline)
                && budget.connections.unwrap().total == 0
        },
    );
    assert!(client.connected_peers().is_empty());
}

#[test]
fn typed_net_budget_caps_real_gossip_before_delivery() {
    let profile = serde_json::from_str(
        r#"{"profile":"v1-discovery-resource-budget-1",
        "connections_total":3,"pending_connections_total":2,"pending_inbound_unknown":1,
        "pending_outbound":2,"connections_per_peer":1,"handshake_timeout_ms":3000,
        "bytes_in_per_window":8192,"bytes_out_per_window":8192,"bytes_total_per_window":16384}"#,
    )
    .unwrap();
    let receiver = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        p2p_resource_budget: profile,
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let sender = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let topic = "aw.traffic.shared-gossip-budget";
    let subscription = receiver.subscribe(topic).unwrap();
    let _publisher_subscription = sender.subscribe(topic).unwrap();
    let deadline = || Instant::now() + Duration::from_secs(5);
    wait_until("gossip listener", deadline(), || {
        !receiver.listening_addrs().is_empty()
    });
    sender
        .dial(
            receiver.listening_addrs()[0]
                .clone()
                .with_p2p(receiver.peer_id())
                .unwrap(),
        )
        .unwrap();

    wait_until("gossip subscription propagated", deadline(), || {
        sender.publish(topic, b"budgeted gossip").is_ok()
    });
    wait_until("small gossip received", deadline(), || {
        subscription
            .drain()
            .iter()
            .any(|payload| payload == b"budgeted gossip")
    });

    sender.publish(topic, &[7; 16384]).unwrap();
    wait_until(
        "receiver reached its shared inbound cap",
        deadline(),
        || {
            receiver
                .traffic_metrics_snapshot()
                .resource_budget
                .last_rejection
                == Some(crate::ResourceBudgetRejection::BytesIn)
        },
    );
    let status = receiver.traffic_metrics_snapshot();
    let budget = status.resource_budget;
    let bytes = budget.bytes.unwrap();
    assert_eq!(bytes.actual_bytes.inbound, 8192);
    assert_eq!(bytes.charged_bytes.inbound, 8192);
    assert!(bytes.charged_bytes.total <= 16384);
    assert_eq!(bytes.reserved_bytes.total, 0);
    assert_eq!(budget.connections.unwrap().established, 1);
    assert_eq!(status.by_topic[topic].inbound.messages, 1);
    assert!(subscription.drain().is_empty());
}
