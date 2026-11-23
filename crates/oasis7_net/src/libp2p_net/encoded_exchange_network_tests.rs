use super::*;
use std::sync::{atomic::AtomicUsize, mpsc};

use crate::{Libp2pNetwork, Libp2pNetworkConfig, WorldError};
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};
use oasis7_proto::distributed_net::DistributedNetwork as _;

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(condition(), "real network condition did not complete");
}

fn pair(quic: bool) -> (Libp2pNetwork, Libp2pNetwork) {
    let address = if quic {
        "/ip4/127.0.0.1/udp/0/quic-v1"
    } else {
        "/ip4/127.0.0.1/tcp/0"
    };
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![address.parse().unwrap()],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(|| !server.listening_addrs().is_empty());
    let remote = server.listening_addrs()[0]
        .clone()
        .with(libp2p::multiaddr::Protocol::P2p(server.peer_id()));
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![address.parse().unwrap()],
        bootstrap_peers: vec![remote],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(|| client.connected_peers().contains(&server.peer_id()));
    (server, client)
}

#[test]
fn real_tcp_and_quic_consumer_keeps_two_exchanges_on_the_original_encoded_account() {
    for quic in [false, true] {
        let (server, client) = pair(quic);
        let protocol = "/aw/rr/1.0.0/encoded-operation";
        let called = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&called);
        server
            .register_handler(
                protocol,
                Box::new(move |bytes| {
                    observed.fetch_add(1, Ordering::AcqRel);
                    Ok(bytes.to_vec())
                }),
            )
            .unwrap();
        // The leading bytes are part of this application's payload framing and
        // are therefore inside the measured outer NetworkRequest/Response.
        let body = [0, 0, 0, 3, 24, 255, 0];
        let tx = request_bytes(protocol, &body).len();
        let rx = response_bytes(&body).len();
        // Fix the original allowance before either exchange. Two complete
        // exchanges fit; their remainder cannot admit a third full request.
        let state = ledger(2 * (tx + rx) + tx - 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2 {
            assert_eq!(
                client
                    .request_to_peer_with_encoded_budget(
                        protocol,
                        &body,
                        server.peer_id(),
                        Box::new(Budget::new(Arc::clone(&state), deadline))
                    )
                    .unwrap(),
                body
            );
        }
        {
            let actual = state.lock().unwrap();
            assert_eq!(actual.starts, [tx, tx]);
            assert_eq!(
                (actual.tx, actual.rx, actual.unknown, actual.finishes),
                (2 * tx, 2 * rx, 0, 2)
            );
            assert!(actual.pending.is_empty());
            assert!(actual.limit - actual.spent() < tx);
        }
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &body,
                    server.peer_id(),
                    Box::new(Budget::new(Arc::clone(&state), deadline))
                )
                .is_err()
        );
        assert_eq!(called.load(Ordering::Acquire), 2);
        let actual = state.lock().unwrap();
        assert_eq!((actual.tx, actual.rx, actual.unknown), (2 * tx, 2 * rx, 0));
    }
}

#[test]
fn real_response_exhaustion_stops_before_full_decode_and_keeps_confirmed_frame_bytes() {
    for quic in [false, true] {
        let (server, client) = pair(quic);
        let protocol = "/aw/rr/1.0.0/bounded-encoded-response";
        let called = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&called);
        let body = vec![255; 256];
        let reply = body.clone();
        server
            .register_handler(
                protocol,
                Box::new(move |_| {
                    observed.fetch_add(1, Ordering::AcqRel);
                    Ok(reply.clone())
                }),
            )
            .unwrap();
        let tx = request_bytes(protocol, &[24]).len();
        let complete_rx = response_bytes(&body).len();
        let available_rx = complete_rx / 2;
        let state = ledger(tx + available_rx);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &[24],
                    server.peer_id(),
                    Box::new(Budget::new(Arc::clone(&state), deadline)),
                )
                .is_err()
        );
        {
            let actual = state.lock().unwrap();
            assert_eq!((actual.tx, actual.rx), (tx, available_rx));
            assert!(actual.rx < complete_rx);
            assert_eq!((actual.unknown, actual.finishes), (0, 1));
            assert!(actual.pending.is_empty() && !actual.active);
            assert_eq!(actual.spent(), actual.limit);
        }
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &[24],
                    server.peer_id(),
                    Box::new(Budget::new(Arc::clone(&state), deadline)),
                )
                .is_err()
        );
        assert_eq!(called.load(Ordering::Acquire), 1);
        let actual = state.lock().unwrap();
        assert_eq!(
            (actual.tx, actual.rx, actual.unknown),
            (tx, available_rx, 0)
        );
    }
}

#[test]
fn real_concurrent_streams_do_not_exchange_their_opaque_accounts() {
    let (server, client) = pair(false);
    let protocol = "/aw/rr/1.0.0/concurrent-encoded-operation";
    let (entered, seen) = mpsc::channel();
    let (release, permits) = mpsc::channel();
    let permits = Mutex::new(permits);
    server
        .register_handler(
            protocol,
            Box::new(move |bytes| {
                entered.send(()).unwrap();
                permits
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok(bytes.to_vec())
            }),
        )
        .unwrap();
    let first = ledger(4096);
    let second = ledger(4096);
    let deadline = Instant::now() + Duration::from_secs(5);
    std::thread::scope(|threads| {
        let a = threads.spawn(|| {
            client.request_to_peer_with_encoded_budget(
                protocol,
                &[255; 7],
                server.peer_id(),
                Box::new(Budget::new(Arc::clone(&first), deadline)),
            )
        });
        let b = threads.spawn(|| {
            client.request_to_peer_with_encoded_budget(
                protocol,
                &[24; 19],
                server.peer_id(),
                Box::new(Budget::new(Arc::clone(&second), deadline)),
            )
        });
        seen.recv_timeout(Duration::from_secs(3)).unwrap();
        seen.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(first.lock().unwrap().active && second.lock().unwrap().active);
        release.send(()).unwrap();
        release.send(()).unwrap();
        assert_eq!(a.join().unwrap().unwrap(), [255; 7]);
        assert_eq!(b.join().unwrap().unwrap(), [24; 19]);
    });
    for (state, bytes) in [(&first, vec![255; 7]), (&second, vec![24; 19])] {
        let actual = state.lock().unwrap();
        assert_eq!(actual.tx, request_bytes(protocol, &bytes).len());
        assert_eq!(actual.rx, response_bytes(&bytes).len());
        assert_eq!((actual.unknown, actual.finishes), (0, 1));
    }
}

#[test]
fn real_authenticated_remote_error_charges_its_complete_encoded_response() {
    let (server, client) = pair(true);
    let protocol = "/oasis7/bft/private-candidate/2.0.0";
    let error = || WorldError::NetworkRequestFailed {
        code: DistributedErrorCode::ErrNotFound,
        message: "original material absent".into(),
        retryable: false,
    };
    let expected = crate::util::to_canonical_cbor(&ErrorResponse {
        code: DistributedErrorCode::ErrNotFound,
        message: "original material absent".into(),
        retryable: false,
    })
    .unwrap();
    let _registration = server
        .register_authenticated_context_handler_with_admission(
            protocol,
            64 * 1024,
            Box::new(|_, _| Ok(())),
            Box::new(move |_, _| Err(error())),
        )
        .unwrap();
    let state = ledger(4096);
    let result = client.request_authenticated_to_peer_with_encoded_budget(
        protocol,
        &[255, 24],
        server.peer_id(),
        Box::new(Budget::new(
            Arc::clone(&state),
            Instant::now() + Duration::from_secs(5),
        )),
    );
    assert!(matches!(
        result.unwrap_err().diagnostic_source(),
        WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotFound,
            retryable: false,
            ..
        }
    ));
    let actual = state.lock().unwrap();
    assert_eq!(actual.tx, request_bytes(protocol, &[255, 24]).len());
    assert_eq!(actual.rx, response_bytes(&expected).len());
    assert_eq!((actual.unknown, actual.finishes), (0, 1));
}

#[test]
fn real_tcp_and_quic_pending_response_observes_cancellation_before_more_io() {
    for quic in [false, true] {
        let (server, client) = pair(quic);
        let protocol = "/aw/rr/1.0.0/revoked-encoded-operation";
        let (entered, seen) = mpsc::channel();
        let (release, permits) = mpsc::channel();
        let permits = Mutex::new(permits);
        server
            .register_handler(
                protocol,
                Box::new(move |bytes| {
                    entered.send(()).unwrap();
                    permits
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(3))
                        .unwrap();
                    Ok(bytes.to_vec())
                }),
            )
            .unwrap();
        let state = ledger(4096);
        let deadline = Instant::now() + Duration::from_secs(5);
        let cancelled = Arc::new(AtomicBool::new(false));
        std::thread::scope(|threads| {
            let request = threads.spawn(|| {
                let mut budget = Budget::new(Arc::clone(&state), deadline);
                budget.cancelled = Arc::clone(&cancelled);
                client.request_to_peer_with_encoded_budget(
                    protocol,
                    &[255, 24],
                    server.peer_id(),
                    Box::new(budget),
                )
            });
            seen.recv_timeout(Duration::from_secs(2)).unwrap();
            wait_until(|| !state.lock().unwrap().pending.is_empty());
            {
                let actual = state.lock().unwrap();
                assert_eq!(actual.tx, request_bytes(protocol, &[255, 24]).len());
                assert_eq!((actual.rx, actual.unknown, actual.finishes), (0, 0, 0));
            }
            cancelled.store(true, Ordering::Release);
            // The real response wakes the pending socket. Its old reservation
            // must not authorize another stream poll after independent cancel.
            release.send(()).unwrap();
            assert!(request.join().unwrap().is_err());
        });
        wait_until(|| state.lock().unwrap().finishes == 1);
        assert!(
            Instant::now() < deadline,
            "independent cancel must precede expiry"
        );
        let before = {
            let actual = state.lock().unwrap();
            assert_eq!(actual.rx, 0);
            assert!(actual.unknown > 0 && actual.pending.is_empty() && !actual.active);
            (actual.tx, actual.rx, actual.unknown)
        };
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &[255, 24],
                    server.peer_id(),
                    Box::new(Budget::new(Arc::clone(&state), deadline)),
                )
                .is_err()
        );
        assert!(matches!(seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let actual = state.lock().unwrap();
        assert_eq!((actual.tx, actual.rx, actual.unknown), before);
    }
}

#[test]
fn real_deadline_stops_the_codec_and_late_response_cannot_refund_or_revive_it() {
    let (server, client) = pair(false);
    let protocol = "/aw/rr/1.0.0/cancel-encoded-operation";
    let (entered, seen) = mpsc::channel();
    let (release, permits) = mpsc::channel();
    let (finished, done) = mpsc::channel();
    let permits = Mutex::new(permits);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    server
        .register_handler(
            protocol,
            Box::new(move |bytes| {
                observed.fetch_add(1, Ordering::AcqRel);
                entered.send(()).unwrap();
                permits
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                finished.send(()).unwrap();
                Ok(bytes.to_vec())
            }),
        )
        .unwrap();
    let state = ledger(4096);
    std::thread::scope(|threads| {
        let request = threads.spawn(|| {
            client.request_to_peer_with_encoded_budget(
                protocol,
                &[255; 8],
                server.peer_id(),
                Box::new(Budget::new(
                    Arc::clone(&state),
                    Instant::now() + Duration::from_millis(750),
                )),
            )
        });
        seen.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.join().unwrap().is_err());
    });
    wait_until(|| state.lock().unwrap().finishes == 1);
    let before = {
        let actual = state.lock().unwrap();
        assert_eq!(actual.tx, request_bytes(protocol, &[255; 8]).len());
        assert_eq!(actual.rx, 0);
        assert!(actual.unknown > 0 && actual.pending.is_empty() && !actual.active);
        (actual.tx, actual.rx, actual.unknown)
    };
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        client
            .request_to_peer_with_encoded_budget(
                protocol,
                &[255; 8],
                server.peer_id(),
                Box::new(Budget::new(
                    Arc::clone(&state),
                    Instant::now() + Duration::from_secs(2)
                ))
            )
            .is_err()
    );
    assert_eq!(calls.load(Ordering::Acquire), 1);
    let actual = state.lock().unwrap();
    assert_eq!((actual.tx, actual.rx, actual.unknown), before);
}
