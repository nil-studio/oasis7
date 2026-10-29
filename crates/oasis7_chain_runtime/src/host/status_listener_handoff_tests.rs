use super::*;
use std::io::{Read, Write};

#[test]
fn startup_status_listener_handoff_preserves_socket_and_releases_replication_guards() {
    let status_reservation = TcpListener::bind("127.0.0.1:0").expect("reserve status port");
    let replication_reservation =
        TcpListener::bind("127.0.0.1:0").expect("reserve replication port");
    let gossip_reservation = UdpSocket::bind("127.0.0.1:0").expect("reserve gossip port");
    let status_addr = status_reservation.local_addr().expect("status address");
    let replication_addr = replication_reservation
        .local_addr()
        .expect("replication address");
    let gossip_addr = gossip_reservation.local_addr().expect("gossip address");
    drop((
        status_reservation,
        replication_reservation,
        gossip_reservation,
    ));

    let status_bind = status_addr.to_string();
    let mut options =
        parse_options(["--status-bind", status_bind.as_str()].into_iter()).expect("parse options");
    options.node_gossip_bind = Some(gossip_addr);
    options.replication_network_listen_addrs =
        vec![format!("/ip4/127.0.0.1/tcp/{}", replication_addr.port())];

    let bind_guard = reserve_startup_reconcile_bind_guard(&options)
        .expect("startup preflight should own configured listeners");
    assert_tcp_addr_in_use(status_addr);
    assert_tcp_addr_in_use(replication_addr);
    assert_udp_addr_in_use(gossip_addr);

    let status_listener = bind_guard.into_status_listener();
    let replication_probe =
        TcpListener::bind(replication_addr).expect("replication guard released at handoff");
    let gossip_probe = UdpSocket::bind(gossip_addr).expect("gossip guard released at handoff");
    drop((replication_probe, gossip_probe));

    // A competitor cannot claim the address in the former drop/rebind gap.
    assert_tcp_addr_in_use(status_addr);

    let node_config =
        NodeConfig::new("listener-handoff-test", "w1", NodeRole::Sequencer).expect("node config");
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(node_config)));
    let private_key_hex = hex::encode([9u8; 32]);
    let signer = feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: oasis7::world_service::sign_read_request(
            "service",
            (),
            private_key_hex.as_str(),
        )
        .expect("test signer")
        .subject_public_key,
        private_key_hex,
    };
    let root = std::env::temp_dir();
    let mut server = status_admission::start_test_server_with_listener(
        root.as_path(),
        status_listener,
        runtime,
        signer,
    )
    .expect("status server should adopt the prebound listener");

    assert_tcp_addr_in_use(status_addr);
    let mut stream = TcpStream::connect(status_addr).expect("connect to adopted listener");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set response timeout");
    stream
        .write_all(
            format!(
                "GET /v1/chain/status HTTP/1.1\r\nHost: {status_addr}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .expect("write status request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("read status response");
    assert!(response.starts_with(b"HTTP/1.1 200 OK\r\n"));

    status_server_support::stop_chain_status_server(&mut server);
    let released = TcpListener::bind(status_addr).expect("status listener released at shutdown");
    drop(released);
}

fn assert_tcp_addr_in_use(address: std::net::SocketAddr) {
    let error = TcpListener::bind(address).expect_err("TCP listener must retain its address");
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
}

fn assert_udp_addr_in_use(address: std::net::SocketAddr) {
    let error = UdpSocket::bind(address).expect_err("gossip guard must retain its address");
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
}
