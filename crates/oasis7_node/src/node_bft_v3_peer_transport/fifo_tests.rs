//! Controlled public FIFO mechanism, independent of Runtime or World load.
use super::*;
use std::sync::atomic::AtomicBool;

fn connected_to(a: &Arc<Libp2pNetwork>) -> Arc<Libp2pNetwork> {
    let net = Arc::new(Libp2pNetwork::new(oasis7_net::Libp2pNetworkConfig {
        bootstrap_peers: vec![
            a.listening_addrs()[0]
                .clone()
                .with(libp2p::multiaddr::Protocol::P2p(a.peer_id())),
        ],
        ..Default::default()
    }));
    wait(|| net.connected_peers().contains(&a.peer_id()));
    net
}
fn case(expire: bool) {
    let a = Arc::new(Libp2pNetwork::new(oasis7_net::Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        ..Default::default()
    }));
    wait(|| !a.listening_addrs().is_empty());
    let b = connected_to(&a);
    let c = connected_to(&a);
    let context = context();
    let digest = context_digest(&context).unwrap();
    let first = proposal();
    let first_bytes = encode_bft_public_peer_message_v3(&context, &first).unwrap();
    let first_digest = message_digest(&digest, &first_bytes);
    let held = Arc::new(AtomicBool::new(true));
    let rejected_count = Arc::new(AtomicUsize::new(0));
    let inbox = Arc::new(Mutex::new(Inbox::new()));
    let gate = held.clone();
    let rejects = rejected_count.clone();
    let admit_context = digest.clone();
    let receive_context = digest.clone();
    let received = inbox.clone();
    let registration = b
        .register_authenticated_context_handler_with_admission(
            PROTOCOL,
            64 * 1024,
            Box::new(move |_, bytes| {
                let frame =
                    Frame::decode(bytes, &admit_context).map_err(|_| rejected("test frame"))?;
                if frame.digest == first_digest && gate.load(Ordering::Acquire) {
                    rejects.fetch_add(1, Ordering::AcqRel);
                    return Err(WorldError::NetworkRequestFailed {
                        code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                        message: "controlled FIFO test gate".into(),
                        retryable: true,
                    });
                }
                Ok(())
            }),
            Box::new(move |transport, bytes| {
                let frame =
                    Frame::decode(bytes, &receive_context).map_err(|_| rejected("test frame"))?;
                received
                    .lock()
                    .unwrap()
                    .receive(transport.peer_id(), frame, &context)
                    .map_err(|_| rejected("test assembly"))
            }),
        )
        .unwrap();
    let control = NodeBftLibp2pPeerTransportV3::new(
        c.clone(),
        super::context(),
        vec![a.peer_id().to_string()],
    )
    .unwrap();
    let sender = NodeBftLibp2pPeerTransportV3::new(
        a.clone(),
        super::context(),
        vec![b.peer_id().to_string(), c.peer_id().to_string()],
    )
    .unwrap();
    // Both parcels exist before sender notification. Only this test-owned first
    // deadline is short; production enqueue keeps its original 120-second TTL.
    let second = vote();
    let make = |message: &BftPublicPeerMessageV3, ttl: Duration| {
        let bytes = encode_bft_public_peer_message_v3(&super::context(), message).unwrap();
        sender
            .counters
            .bytes
            .fetch_add(bytes.len(), Ordering::AcqRel);
        sender.counters.count.fetch_add(1, Ordering::AcqRel);
        Arc::new(Parcel {
            digest: message_digest(&digest, &bytes),
            bytes,
            deadline: Instant::now() + ttl,
            counters: sender.counters.clone(),
            measurement: route_trace::parcel(message),
        })
    };
    let first_parcel = make(
        &first,
        if expire {
            Duration::from_millis(500)
        } else {
            Duration::from_secs(5)
        },
    );
    let second_parcel = make(&second, Duration::from_secs(5));
    let second_deadline = second_parcel.deadline;
    {
        let mut q = sender.outbound.0.lock().unwrap();
        q.queues
            .get_mut(&b.peer_id())
            .unwrap()
            .extend([first_parcel, second_parcel.clone()]);
    }
    sender.outbound.1.notify_all();
    sender
        .send_public(&c.peer_id().to_string(), &second)
        .unwrap();
    wait(|| rejected_count.load(Ordering::Acquire) >= 2);
    let mut other_route = Vec::new();
    wait(|| {
        other_route.extend(control.drain_public(8).unwrap());
        !other_route.is_empty()
    });
    assert_eq!(other_route[0].message, second);
    if !expire {
        assert!(
            inbox.lock().unwrap().drain(8).is_empty(),
            "B cannot overtake held A"
        );
    }
    if !expire {
        held.store(false, Ordering::Release);
    }
    let mut observed = Vec::new();
    wait(|| {
        observed.extend(inbox.lock().unwrap().drain(8));
        observed.len() == if expire { 1 } else { 2 }
    });
    assert_eq!(observed.last().unwrap().message, second);
    assert_eq!(
        second_parcel.deadline, second_deadline,
        "queued parcel deadline never renews"
    );
    if expire {
        assert!(
            sender
                .drain_errors()
                .iter()
                .any(|e| e == "public send cancelled/deadline")
        );
    } else {
        assert_eq!(observed[0].message, first);
    }
    drop(sender);
    drop(control);
    drop(registration);
}
#[test]
fn public_fifo_rate_limited_head_holds_same_route_until_release_but_not_other_route() {
    case(false);
}
#[test]
fn public_fifo_expired_test_head_releases_next_parcel_without_renewing_its_deadline() {
    case(true);
}
