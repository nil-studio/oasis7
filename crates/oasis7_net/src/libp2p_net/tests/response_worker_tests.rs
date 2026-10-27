use super::*;
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const FETCH_BLOB: &str = "/aw/node/replication/fetch-blob/1.0.0";
const GET_HEAD: &str = "/aw/rr/1.0.0/get_world_head";
const FETCH_COMMIT: &str = "/aw/node/replication/fetch-commit/1.0.0";
const FETCH_COMMIT_HEAD: &str = "/aw/node/replication/fetch-commit/head/1.0.0";

fn wait_until(what: &str, deadline: Instant, mut condition: impl FnMut() -> bool) {
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn slow_blob_handler_does_not_block_head_or_commit_responses() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let (started_tx, started_rx) = mpsc::channel();
    server
        .register_handler(
            FETCH_BLOB,
            Box::new(move |_| {
                started_tx.send(()).expect("report slow blob start");
                std::thread::sleep(Duration::from_millis(400));
                Ok(b"blob".to_vec())
            }),
        )
        .expect("register blob handler");
    server
        .register_handler(GET_HEAD, Box::new(|_| Ok(b"head".to_vec())))
        .expect("register head handler");
    server
        .register_handler(FETCH_COMMIT, Box::new(|_| Ok(b"commit".to_vec())))
        .expect("register commit handler");

    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );

    let blob_client = client.clone();
    let server_peer = server.peer_id();
    let blob =
        std::thread::spawn(move || blob_client.request_to_peer(FETCH_BLOB, b"blob", server_peer));
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("slow blob handler started");

    let deadline = Instant::now() + Duration::from_millis(300);
    assert_eq!(
        client
            .request_to_peer(GET_HEAD, b"head", server.peer_id())
            .expect("head response"),
        b"head"
    );
    assert_eq!(
        client
            .request_to_peer(FETCH_COMMIT, b"commit", server.peer_id())
            .expect("commit response"),
        b"commit"
    );
    assert!(
        Instant::now() < deadline,
        "control responses waited for the slow blob handler"
    );
    assert_eq!(
        blob.join()
            .expect("blob request thread")
            .expect("blob response"),
        b"blob"
    );
}

#[test]
fn slow_fetch_commit_handlers_do_not_block_fetch_commit_head() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let (started_tx, started_rx) = mpsc::channel();
    server
        .register_handler(
            FETCH_COMMIT,
            Box::new(move |_| {
                started_tx.send(()).expect("report slow commit start");
                std::thread::sleep(Duration::from_millis(400));
                Ok(b"commit".to_vec())
            }),
        )
        .expect("register fetch-commit handler");
    server
        .register_handler(FETCH_COMMIT_HEAD, Box::new(|_| Ok(b"head".to_vec())))
        .expect("register fetch-commit/head handler");

    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );

    let first_client = client.clone();
    let first_peer = server.peer_id();
    let first = std::thread::spawn(move || {
        first_client.request_to_peer(FETCH_COMMIT, b"first", first_peer)
    });
    let second_client = client.clone();
    let second_peer = server.peer_id();
    let second = std::thread::spawn(move || {
        second_client.request_to_peer(FETCH_COMMIT, b"second", second_peer)
    });
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first slow fetch-commit handler started");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second slow fetch-commit handler started");

    let deadline = Instant::now() + Duration::from_millis(300);
    assert_eq!(
        client
            .request_to_peer(FETCH_COMMIT_HEAD, b"head", server.peer_id())
            .expect("fetch-commit/head response"),
        b"head"
    );
    assert!(
        Instant::now() < deadline,
        "fetch-commit/head waited for the saturated fetch-commit workers"
    );
    assert_eq!(
        first
            .join()
            .expect("first fetch-commit thread")
            .expect("first fetch-commit response"),
        b"commit"
    );
    assert_eq!(
        second
            .join()
            .expect("second fetch-commit thread")
            .expect("second fetch-commit response"),
        b"commit"
    );
}

#[test]
fn timed_out_slow_response_surfaces_as_retryable_transport_failure() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    server
        .register_handler(
            FETCH_BLOB,
            Box::new(|_| {
                std::thread::sleep(Duration::from_millis(200));
                Ok(b"late blob".to_vec())
            }),
        )
        .expect("register blob handler");

    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_millis(50),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );

    let error = client
        .request_to_peer(FETCH_BLOB, b"blob", server.peer_id())
        .expect_err("late response must time out");
    assert!(super::super::error_mapping::world_error_is_retryable_connection_gap(&error));
    assert!(matches!(
        error.diagnostic_source(),
        WorldError::NetworkRequestFailed {
            retryable: true,
            ..
        }
    ));
}

#[test]
fn expired_queued_requests_skip_handlers_and_release_worker_capacity() {
    let started_slow_handlers = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        request_response_timeout: Duration::from_millis(50),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let (started_tx, started_rx) = mpsc::channel();
    let handler_count = Arc::clone(&started_slow_handlers);
    server
        .register_handler(
            FETCH_COMMIT,
            Box::new(move |payload| {
                if payload == b"slow" {
                    handler_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    started_tx.send(()).expect("report slow handler start");
                    std::thread::sleep(Duration::from_millis(200));
                }
                Ok(payload.to_vec())
            }),
        )
        .expect("register fetch-commit handler");

    let short_client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr.clone()],
        request_response_timeout: Duration::from_millis(50),
        ..Libp2pNetworkConfig::default()
    });
    let long_client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "short client connect",
        Instant::now() + Duration::from_secs(5),
        || short_client.connected_peers().contains(&server.peer_id()),
    );
    wait_until(
        "long client connect",
        Instant::now() + Duration::from_secs(5),
        || long_client.connected_peers().contains(&server.peer_id()),
    );

    let mut slow_requests = Vec::new();
    for _ in 0..2 {
        let client = short_client.clone();
        let peer = server.peer_id();
        slow_requests.push(std::thread::spawn(move || {
            client.request_to_peer(FETCH_COMMIT, b"slow", peer)
        }));
    }
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first slow handler started");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second slow handler started");
    let peer = server.peer_id();
    let queued_client = short_client.clone();
    let queued =
        std::thread::spawn(move || queued_client.request_to_peer(FETCH_COMMIT, b"slow", peer));

    for request in slow_requests {
        request
            .join()
            .expect("slow request thread")
            .expect_err("slow request must time out");
    }
    queued
        .join()
        .expect("queued request thread")
        .expect_err("queued request must time out");
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        started_slow_handlers.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "expired queued request must not execute the expensive handler"
    );
    assert_eq!(
        long_client
            .request_to_peer(FETCH_COMMIT, b"fast", server.peer_id())
            .expect("released worker capacity serves the next request"),
        b"fast"
    );
}

#[test]
fn timed_out_handlers_keep_the_fixed_execution_pool_bounded_until_they_exit() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        request_response_timeout: Duration::from_millis(50),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let (started_tx, started_rx) = mpsc::channel();
    server
        .register_handler(
            FETCH_COMMIT,
            Box::new(move |payload| {
                if payload == b"slow" {
                    started_tx.send(()).expect("report slow handler start");
                    std::thread::sleep(Duration::from_millis(250));
                }
                Ok(payload.to_vec())
            }),
        )
        .expect("register fetch-commit handler");
    let short_client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr.clone()],
        request_response_timeout: Duration::from_millis(50),
        ..Libp2pNetworkConfig::default()
    });
    let long_client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "clients connect",
        Instant::now() + Duration::from_secs(5),
        || {
            long_client.connected_peers().contains(&server.peer_id())
                && short_client.connected_peers().contains(&server.peer_id())
        },
    );
    let mut slow = Vec::new();
    for _ in 0..2 {
        let client = short_client.clone();
        let peer = server.peer_id();
        slow.push(std::thread::spawn(move || {
            client.request_to_peer(FETCH_COMMIT, b"slow", peer)
        }));
    }
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first start");
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second start");
    for request in slow {
        request
            .join()
            .expect("request thread")
            .expect_err("must time out");
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        long_client
            .request_to_peer(FETCH_COMMIT, b"fast", server.peer_id())
            .expect("fast response after occupied workers exit"),
        b"fast"
    );
}

#[test]
fn saturated_blob_queue_returns_a_legacy_compatible_retryable_busy_response() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    server
        .register_handler(
            FETCH_BLOB,
            Box::new(|_| {
                std::thread::sleep(Duration::from_millis(400));
                Ok(b"blob".to_vec())
            }),
        )
        .expect("register blob handler");
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(3),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );

    let mut requests = Vec::new();
    for _ in 0..11 {
        let client = client.clone();
        let peer = server.peer_id();
        requests.push(std::thread::spawn(move || {
            client.request_to_peer(FETCH_BLOB, b"blob", peer)
        }));
    }
    let responses: Vec<_> = requests
        .into_iter()
        .map(|request| request.join().expect("request thread"))
        .collect();
    let overloads: Vec<_> = responses
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|response| serde_cbor::from_slice::<ErrorResponse>(&response).ok())
        .filter(|error| {
            error.code == DistributedErrorCode::ErrBusy
                && error.retryable
                && error.message.contains("response worker overloaded")
        })
        .collect();
    assert!(
        !overloads.is_empty(),
        "a saturated requester must receive a legacy-compatible retryable busy response with overload context"
    );
}

#[test]
fn cooperative_blocked_blob_work_observes_deadline_and_lane_recovers() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        request_response_timeout: Duration::from_millis(120),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server.listening_addrs()[0]
        .clone()
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let cancelled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = Arc::clone(&cancelled);
    server
        .register_context_handler_with_admission(
            FETCH_BLOB,
            Box::new(|_| Ok(())),
            Box::new(move |context, payload| {
                if payload == b"block" {
                    while !context.is_cancelled() {
                        std::thread::yield_now();
                    }
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(WorldError::NetworkRequestFailed {
                        code: DistributedErrorCode::ErrTimeout,
                        message: "cooperative cancellation".into(),
                        retryable: true,
                    });
                }
                Ok(b"recovered".to_vec())
            }),
        )
        .unwrap();
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(1),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );
    let mut blocked = Vec::new();
    for _ in 0..2 {
        let client = client.clone();
        let peer = server.peer_id();
        blocked.push(std::thread::spawn(move || {
            client.request_to_peer(FETCH_BLOB, b"block", peer)
        }));
    }
    wait_until(
        "cooperative cancellation",
        Instant::now() + Duration::from_secs(2),
        || cancelled.load(std::sync::atomic::Ordering::SeqCst) == 2,
    );
    for request in blocked {
        let _ = request.join();
    }
    assert_eq!(
        client
            .request_to_peer(FETCH_BLOB, b"ok", server.peer_id())
            .unwrap(),
        b"recovered"
    );
}

#[test]
fn pre_admission_rejects_blob_requests_before_the_expensive_handler() {
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().expect("listen addr")],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "server listen",
        Instant::now() + Duration::from_secs(5),
        || !server.listening_addrs().is_empty(),
    );
    let server_addr = server
        .listening_addrs()
        .into_iter()
        .next()
        .expect("server address")
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let runs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handler_runs = Arc::clone(&runs);
    server
        .register_handler_with_admission(
            FETCH_BLOB,
            Box::new(|payload| {
                if payload == b"authorized" {
                    Ok(())
                } else {
                    Err(WorldError::NetworkRequestFailed {
                        code: DistributedErrorCode::ErrBadRequest,
                        message: "unauthenticated".to_string(),
                        retryable: false,
                    })
                }
            }),
            Box::new(move |_| {
                handler_runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(b"blob".to_vec())
            }),
        )
        .expect("register admitted handler");
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![server_addr],
        request_response_timeout: Duration::from_secs(2),
        ..Libp2pNetworkConfig::default()
    });
    wait_until(
        "client connect",
        Instant::now() + Duration::from_secs(5),
        || client.connected_peers().contains(&server.peer_id()),
    );
    for _ in 0..12 {
        let response = client
            .request_to_peer(FETCH_BLOB, b"unauthenticated", server.peer_id())
            .expect("rejection response");
        let error: ErrorResponse = serde_cbor::from_slice(&response).expect("error response");
        assert_eq!(error.code, DistributedErrorCode::ErrBadRequest);
    }
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(
        client
            .request_to_peer(FETCH_BLOB, b"authorized", server.peer_id())
            .expect("authorized response"),
        b"blob"
    );
    assert_eq!(runs.load(std::sync::atomic::Ordering::SeqCst), 1);
}
