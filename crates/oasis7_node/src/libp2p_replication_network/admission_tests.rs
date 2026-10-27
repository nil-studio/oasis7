use super::*;

fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for condition: {what}");
}

fn listening_addr_with_peer_id(network: &Libp2pReplicationNetwork) -> Multiaddr {
    network
        .listening_addrs()
        .into_iter()
        .find(|addr| addr.to_string().contains("127.0.0.1"))
        .expect("listener visible address")
        .with(libp2p::multiaddr::Protocol::P2p(network.peer_id()))
}

#[test]
fn admitted_handler_rejects_invalid_remote_request_before_expensive_work() {
    let listener = Libp2pReplicationNetwork::new(Libp2pReplicationNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listener addr")],
        ..Libp2pReplicationNetworkConfig::default()
    });
    wait_until(
        "listener bind",
        Instant::now() + Duration::from_secs(10),
        || !listener.listening_addrs().is_empty(),
    );
    let runs = Arc::new(AtomicUsize::new(0));
    let handler_runs = Arc::clone(&runs);
    listener
        .register_handler_with_admission(
            "/aw/node/replication/fetch-commit/1.0.0",
            Box::new(|payload| {
                if payload == b"valid" {
                    Ok(())
                } else {
                    Err(WorldError::NetworkRequestFailed {
                        code: DistributedErrorCode::ErrUnauthorized,
                        message: "invalid fetch request".to_string(),
                        retryable: false,
                    })
                }
            }),
            Box::new(move |_| {
                handler_runs.fetch_add(1, Ordering::SeqCst);
                Ok(b"commit".to_vec())
            }),
        )
        .expect("register admitted fetch handler");
    let dialer = Libp2pReplicationNetwork::new(Libp2pReplicationNetworkConfig {
        bootstrap_peers: vec![listening_addr_with_peer_id(&listener)],
        ..Libp2pReplicationNetworkConfig::default()
    });
    wait_until(
        "dialer connection",
        Instant::now() + Duration::from_secs(10),
        || !dialer.connected_peers().is_empty(),
    );

    let error = dialer
        .request("/aw/node/replication/fetch-commit/1.0.0", b"invalid")
        .expect_err("invalid request must be rejected");
    assert!(matches!(
        error.diagnostic_source(),
        WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrUnauthorized,
            ..
        }
    ));
    assert_eq!(runs.load(Ordering::SeqCst), 0);
    assert_eq!(
        dialer
            .request("/aw/node/replication/fetch-commit/1.0.0", b"valid")
            .expect("valid request"),
        b"commit"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1);
}
