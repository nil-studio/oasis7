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
    let _registration = s
        .register_authenticated_context_handler_with_admission(
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
    let _registration = s
        .register_authenticated_context_handler_with_admission(
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
    let _registration = s
        .register_authenticated_context_handler_with_admission(
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

#[test]
fn authenticated_owner_reclaims_only_closed_registration_and_fences_inflight() {
    let s = server();
    let c = client(&s);
    let (started, ready) = std::sync::mpsc::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    let wait_release = Mutex::new(wait_release);
    let old = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(move |_, _| {
                started.send(()).unwrap();
                wait_release.lock().unwrap().recv().unwrap();
                Ok(b"old-private-reply".to_vec())
            }),
        )
        .unwrap();
    assert!(
        s.register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(|_, _| Ok(Vec::new()))
        )
        .is_err()
    );
    let caller = c.clone();
    let peer = s.peer_id();
    let request = std::thread::spawn(move || {
        caller.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"old",
            peer,
            Duration::from_secs(2),
        )
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    old.close();
    let expected = c.peer_id();
    let current = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(move |ctx, _| {
                assert_eq!(ctx.peer_id(), expected);
                Ok(b"new".to_vec())
            }),
        )
        .unwrap();
    old.close();
    release.send(()).unwrap();
    assert!(request.join().unwrap().is_err());
    assert_eq!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"new",
            s.peer_id(),
            Duration::from_secs(2)
        )
        .unwrap(),
        b"new"
    );
    drop(current);
}

#[test]
fn authenticated_reply_restrictions_remain_live_after_handler_returns() {
    let s = server();
    let c = client(&s);
    let _owner = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(|ctx, payload| {
                match payload {
                    b"generation" => {
                        let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
                        ctx.restrict_reply_to_generation(generation.clone(), 1)?;
                        generation.store(2, Ordering::Release);
                    }
                    b"serving" => {
                        let serving = Arc::new(std::sync::atomic::AtomicBool::new(true));
                        ctx.restrict_reply_while(serving.clone())?;
                        serving.store(false, Ordering::Release);
                    }
                    b"expiry" => {
                        ctx.restrict_reply_until(Instant::now() + Duration::from_millis(1))?;
                        ctx.restrict_reply_until(Instant::now() + Duration::from_secs(30))?;
                        std::thread::sleep(Duration::from_millis(3));
                    }
                    b"overflow" => {
                        for _ in 0..4 {
                            ctx.restrict_reply_while(Arc::new(
                                std::sync::atomic::AtomicBool::new(true),
                            ))?;
                        }
                        let _ = ctx.restrict_reply_while(Arc::new(
                            std::sync::atomic::AtomicBool::new(true),
                        ));
                    }
                    _ => {}
                }
                Ok(b"private-result".to_vec())
            }),
        )
        .unwrap();
    for payload in [b"generation".as_slice(), b"serving", b"expiry", b"overflow"] {
        assert!(matches!(
            c.request_authenticated_to_peer_with_timeout(
                PROTOCOL,
                payload,
                s.peer_id(),
                Duration::from_secs(2)
            ),
            Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrNotAvailable,
                retryable: true,
                ..
            })
        ));
    }
    assert_eq!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"ok",
            s.peer_id(),
            Duration::from_secs(2)
        )
        .unwrap(),
        b"private-result"
    );
}

#[test]
fn authenticated_close_drops_idle_callback_resources_before_same_net_reopen() {
    struct Canary(Arc<AtomicUsize>);
    impl Drop for Canary {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let s = server();
    let c = client(&s);
    let dropped = Arc::new(AtomicUsize::new(0));
    let canary = Canary(dropped.clone());
    let old = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(move |_, _| {
                let _keep = &canary;
                Ok(b"old".to_vec())
            }),
        )
        .unwrap();
    assert_eq!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"old",
            s.peer_id(),
            Duration::from_secs(2)
        )
        .unwrap(),
        b"old"
    );
    old.close();
    // The inactive registry slot must not keep an idle driver's lock/callback
    // alive until its successor is constructed: that would deadlock reopen.
    wait(|| dropped.load(Ordering::SeqCst) == 1);
    let peer = c.peer_id();
    let _current = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(|_, _| Ok(())),
            Box::new(move |ctx, _| {
                assert_eq!(ctx.peer_id(), peer);
                Ok(b"reopened".to_vec())
            }),
        )
        .unwrap();
    old.close();
    assert_eq!(
        c.request_authenticated_to_peer_with_timeout(
            PROTOCOL,
            b"new",
            s.peer_id(),
            Duration::from_secs(2)
        )
        .unwrap(),
        b"reopened"
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn authenticated_noise_routing_rejection_preserves_configured_peer_service() {
    let s = server();
    let trusted = client(&s);
    let stranger = client(&s);
    let allowed = trusted.peer_id();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let _owner = s
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            16,
            Box::new(move |peer, _| {
                if *peer == allowed {
                    Ok(())
                } else {
                    Err(super::super::authenticated_requests::rejected(
                        "unconfigured Noise route",
                    ))
                }
            }),
            Box::new(move |ctx, bytes| {
                assert_eq!(ctx.peer_id(), allowed);
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(bytes.to_vec())
            }),
        )
        .unwrap();
    for _ in 0..17 {
        assert!(matches!(
            stranger.request_authenticated_to_peer_with_timeout(
                PROTOCOL,
                b"trusted-label",
                s.peer_id(),
                Duration::from_secs(2)
            ),
            Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
                ..
            })
        ));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        trusted
            .request_authenticated_to_peer_with_timeout(
                PROTOCOL,
                b"legitimate",
                s.peer_id(),
                Duration::from_secs(2)
            )
            .unwrap(),
        b"legitimate"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
