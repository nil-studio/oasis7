//! Actual Noise TCP request-response identity, admission and cancellation.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
const PROTOCOL: &str = "/oasis7/bft/header-peer/3.0.0";
fn wait(mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn server() -> Libp2pNetwork {
    let n = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        request_response_timeout: Duration::from_millis(200),
        ..Default::default()
    });
    wait(|| !n.listening_addrs().is_empty());
    n
}
fn client(s: &Libp2pNetwork) -> Libp2pNetwork {
    let n = Libp2pNetwork::new(Libp2pNetworkConfig {
        bootstrap_peers: vec![
            s.listening_addrs()[0]
                .clone()
                .with(libp2p::multiaddr::Protocol::P2p(s.peer_id())),
        ],
        request_response_timeout: Duration::from_secs(2),
        ..Default::default()
    });
    wait(|| n.connected_peers().contains(&s.peer_id()));
    n
}
#[test]
fn authenticated_noise_context_is_remote_peer_and_original_cancellation() {
    let s = server();
    let c = client(&s);
    let expected = c.peer_id();
    assert!(
        s.register_handler(PROTOCOL, Box::new(|_| Ok(Vec::new())))
            .is_err()
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let cancelled = Arc::new(AtomicUsize::new(0));
    let seen_cancel = cancelled.clone();
    s.register_authenticated_context_handler_with_admission(
        PROTOCOL,
        16,
        Box::new(move |peer, _| {
            assert_eq!(*peer, expected);
            Ok(())
        }),
        Box::new(move |ctx, payload| {
            assert_eq!(ctx.peer_id(), expected);
            observed.fetch_add(1, Ordering::SeqCst);
            if payload == b"block" {
                while !ctx.request_context().is_cancelled() {
                    std::thread::yield_now();
                }
                seen_cancel.fetch_add(1, Ordering::SeqCst);
                return Err(super::super::authenticated_requests::rejected("cancelled"));
            }
            Ok(payload.to_vec())
        }),
    )
    .unwrap();
    assert!(
        s.register_handler(PROTOCOL, Box::new(|_| Ok(b"replacement".to_vec())))
            .is_err()
    );
    assert!(
        s.register_context_handler_with_admission(
            PROTOCOL,
            Box::new(|_| Ok(())),
            Box::new(|_, _| Ok(Vec::new()))
        )
        .is_err()
    );
    assert!(
        s.register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(|_, _| Ok(Vec::new()))
        )
        .is_err()
    );
    assert_eq!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"private-input",
            s.peer_id(),
            Duration::from_secs(2)
        )
        .unwrap(),
        b"private-input"
    );
    assert!(matches!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            &[7; 17],
            s.peer_id(),
            Duration::from_secs(2)
        ),
        Err(WorldError::NetworkRequestFailed {
            code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let _ = c.request_authenticated_to_peer_with_timeout(
        PROTOCOL,
        b"block",
        s.peer_id(),
        Duration::from_secs(2),
    );
    wait(|| cancelled.load(Ordering::SeqCst) == 1);
    assert!(s.published().is_empty());
}
#[test]
fn authenticated_admission_rejects_untrusted_peer_before_handler() {
    let s = server();
    let c = client(&s);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    s.register_authenticated_context_handler_with_admission(
        PROTOCOL,
        16,
        Box::new(|_, _| {
            Err(super::super::authenticated_requests::rejected(
                "peer not authorized",
            ))
        }),
        Box::new(move |_, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(Vec::new())
        }),
    )
    .unwrap();
    assert!(matches!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"claimed-trusted",
            s.peer_id(),
            Duration::from_secs(2)
        ),
        Err(WorldError::NetworkRequestFailed {
            code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn authenticated_budget_uses_peer_not_payload_and_is_shared() {
    let mut budget = super::super::authenticated_requests::Budget::default();
    let peer = libp2p::PeerId::random();
    for _ in 0..16 {
        budget.admit(peer).unwrap();
    }
    assert!(budget.admit(peer).is_err());
    for _ in 0..7 {
        let peer = libp2p::PeerId::random();
        for _ in 0..16 {
            budget.admit(peer).unwrap();
        }
    }
    assert!(budget.admit(libp2p::PeerId::random()).is_err());
}

#[test]
fn authenticated_noise_overload_is_rejected_without_unbounded_workers() {
    let s = server();
    let c = client(&s);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    s.register_authenticated_context_handler_with_admission(
        PROTOCOL,
        16,
        Box::new(|_, _| Ok(())),
        Box::new(move |ctx, _| {
            observed.fetch_add(1, Ordering::SeqCst);
            while !ctx.request_context().is_cancelled() {
                std::thread::yield_now();
            }
            Err(super::super::authenticated_requests::rejected(
                "cancelled overload probe",
            ))
        }),
    )
    .unwrap();
    let requests = (0..32)
        .map(|_| {
            let c = c.clone();
            let peer = s.peer_id();
            std::thread::spawn(move || {
                c.request_authenticated_to_peer_with_timeout(
                    PROTOCOL,
                    b"private",
                    peer,
                    Duration::from_secs(2),
                )
            })
        })
        .collect::<Vec<_>>();
    let errors = requests
        .into_iter()
        .filter_map(|r| r.join().unwrap().err())
        .collect::<Vec<_>>();
    assert!(errors.iter().any(|e| matches!(
        e,
        WorldError::NetworkRequestFailed {
            code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited
                | oasis7_proto::distributed::DistributedErrorCode::ErrBusy
                | oasis7_proto::distributed::DistributedErrorCode::ErrOverloaded,
            ..
        }
    )));
    assert!(calls.load(Ordering::SeqCst) < 32);
}
