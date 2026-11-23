use super::*;
use crate::libp2p_net::{
    request_deadline::prepare_encoded_request,
    swarm_behaviour::{BehaviourEvent, build_swarm},
    wire_bytes::init_shared_wire_byte_counters,
};
use crate::{Libp2pNetwork, Libp2pNetworkConfig};
use futures::StreamExt;
use libp2p::{
    identity::Keypair,
    request_response::{Event, OutboundFailure},
    swarm::{DialError, SwarmEvent},
};

fn message() -> NetworkRequest {
    NetworkRequest {
        protocol: "/meter".into(),
        payload: vec![0, 24, 255],
    }
}

#[test]
fn prepared_bytes_are_single_use_and_unadmitted_codec_cannot_send() {
    runtime().block_on(async {
        for admitted in [false, true] {
            let state = ledger(1024);
            let exchange = Exchange::new(Box::new(Budget::new(
                Arc::clone(&state),
                Instant::now() + Duration::from_secs(5),
            )));
            prepare_encoded_request(message(), &exchange).unwrap();
            assert!(state.lock().unwrap().starts.is_empty());
            if admitted {
                exchange.admit().unwrap();
            }
            let mut io = PartialIo::new(Arc::clone(&state), vec![]);
            let protocol = StreamProtocol::new("/test/single-prepared-request");
            let mut codec = RequestCodec::default();
            let request = || LocalRequest {
                message: message(),
                deadline: None,
                exchange: Some(Arc::clone(&exchange)),
            };
            let first = codec.write_request(&protocol, &mut io, request()).await;
            assert_eq!(first.is_ok(), admitted);
            let sent = io.calls;
            let mut duplicate = RequestCodec::default();
            assert!(
                duplicate
                    .write_request(&protocol, &mut io, request())
                    .await
                    .is_err()
            );
            assert_eq!(
                io.calls, sent,
                "duplicate or unadmitted bytes must not reach the writer"
            );
            assert_eq!(
                io.written,
                if admitted {
                    request_bytes("/meter", &[0, 24, 255])
                } else {
                    vec![]
                }
            );
            drop(duplicate);
            drop(codec);
            drop(exchange);
            let actual = state.lock().unwrap();
            assert_eq!(actual.starts.len(), usize::from(admitted));
            assert_eq!(actual.finishes, usize::from(admitted));
            assert_eq!(actual.unknown, 0);
        }
    });
}

#[test]
fn known_closed_or_closing_local_queue_never_admits_an_exchange() {
    let network = Libp2pNetwork::new(Libp2pNetworkConfig::default());
    for initially_closed in [false, true] {
        let mut client = network.clone();
        let (sender, receiver) = futures::channel::mpsc::channel(1);
        client.command_tx = sender;
        let receiver = Arc::new(Mutex::new(Some(receiver)));
        if initially_closed {
            receiver.lock().unwrap().take();
        }
        let state = ledger(1024);
        let mut budget = Budget::new(Arc::clone(&state), Instant::now() + Duration::from_secs(5));
        let closing = Arc::clone(&receiver);
        budget.after_active = Some(Box::new(move || {
            closing.lock().unwrap().take();
        }));
        let error = client
            .request_to_peer_with_encoded_budget(
                "/aw/rr/1.0.0/closed-command-queue",
                &[255],
                libp2p::PeerId::random(),
                Box::new(budget),
            )
            .unwrap_err();
        assert!(format!("{error:?}").contains("queue disconnected"));
        assert!(receiver.lock().unwrap().is_none());
        let actual = state.lock().unwrap();
        assert!(actual.starts.is_empty());
        assert_eq!(
            (actual.tx, actual.rx, actual.unknown, actual.finishes),
            (0, 0, 0, 0)
        );
        assert!(actual.pending.is_empty());
    }
}

#[test]
fn actual_no_address_and_refused_tcp_dials_spend_original_exchange_count() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let mut swarm = build_swarm(
                &Keypair::generate_ed25519(),
                false,
                false,
                Duration::from_secs(2),
                init_shared_wire_byte_counters(),
            );
            let peer = Keypair::generate_ed25519().public().to_peer_id();
            let state = ledger(4096);
            state.lock().unwrap().max_exchanges = Some(2);
            let deadline = Instant::now() + Duration::from_secs(10);
            for with_address in [false, true] {
                if with_address {
                    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    let port = listener.local_addr().unwrap().port();
                    drop(listener);
                    swarm.add_peer_address(
                        peer,
                        format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap(),
                    );
                } else {
                    // A synchronous NoAddresses failure is delivered to the
                    // behaviour, without an OutgoingConnectionError event.
                    // Verify the actual dial result before the budgeted RR
                    // dispatch exercises the same empty address set.
                    assert!(matches!(swarm.dial(peer), Err(DialError::NoAddresses)));
                }
                let exchange = Exchange::new(Box::new(Budget::new(Arc::clone(&state), deadline)));
                prepare_encoded_request(message(), &exchange).unwrap();
                exchange.admit().unwrap();
                let request = swarm.behaviour_mut().send_rr_request_with_exchange(
                    &peer,
                    message(),
                    None,
                    Some(exchange),
                );
                tokio::time::timeout(Duration::from_secs(3), async {
                    let mut saw_transport_failure = false;
                    let mut saw_request_failure = false;
                    while !saw_request_failure || (with_address && !saw_transport_failure) {
                        match swarm.select_next_some().await {
                            SwarmEvent::OutgoingConnectionError {
                                peer_id: Some(failed),
                                error,
                                ..
                            } if with_address && failed == peer => {
                                assert!(matches!(error, DialError::Transport(_)), "{error:?}");
                                saw_transport_failure = true;
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                                Event::OutboundFailure {
                                    request_id, error, ..
                                },
                            )) if request_id == request => {
                                assert!(matches!(error, OutboundFailure::DialFailure), "{error:?}");
                                saw_request_failure = true;
                            }
                            _ => {}
                        }
                    }
                })
                .await
                .unwrap();
            }
            drop(swarm);
            let third = Exchange::new(Box::new(Budget::new(Arc::clone(&state), deadline)));
            prepare_encoded_request(message(), &third).unwrap();
            assert!(third.admit().is_err());
            drop(third);
            let actual = state.lock().unwrap();
            assert_eq!(
                actual.starts,
                [request_bytes("/meter", &[0, 24, 255]).len(); 2]
            );
            assert_eq!(
                (actual.tx, actual.rx, actual.unknown, actual.finishes),
                (0, 0, 0, 2)
            );
            assert!(actual.pending.is_empty());
        });
}

#[test]
fn repeated_real_protocol_failure_exhausts_the_same_original_exchange_limit() {
    use oasis7_proto::network_failure::{FailureCause, FailureOrigin, NetworkFailure};

    let (server, client) = super::network_tests::UnsupportedPeer::connect();
    let state = ledger(1024);
    state.lock().unwrap().max_exchanges = Some(1);
    let deadline = Instant::now() + Duration::from_secs(5);
    let protocol = "/aw/rr/1.0.0/exhausted-negotiation";
    for first in [true, false] {
        let error = client
            .request_to_peer_with_encoded_budget(
                protocol,
                &[255],
                server.peer,
                Box::new(Budget::new(Arc::clone(&state), deadline)),
            )
            .unwrap_err();
        if first {
            assert_eq!(
                error.network_failure(Some(protocol)).cause,
                FailureCause::MissingHandler
            );
        } else {
            assert_eq!(
                error.network_failure(Some(protocol)),
                NetworkFailure::observed(
                    FailureOrigin::LocalTransport,
                    FailureCause::Unknown,
                    Some(protocol),
                )
            );
            assert!(matches!(
                error.diagnostic_source(),
                crate::WorldError::NetworkProtocolUnavailable { protocol: phase }
                    if phase == "encoded exchange admission failed"
            ));
        }
        // Observe the first exchange's completed owner before retrying, so
        // only max_exchanges, not a still-active owner, can reject the retry.
        let finished_by = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&state) != 1 && Instant::now() < finished_by {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(Arc::strong_count(&state), 1);
        assert!(Instant::now() < deadline);
        let actual = state.lock().unwrap();
        assert!(!actual.active);
        assert_eq!(actual.starts, [request_bytes(protocol, &[255]).len()]);
        assert_eq!(actual.max_exchanges, Some(1));
        assert!(actual.limit > actual.starts[0]);
        assert_eq!(
            (actual.tx, actual.rx, actual.unknown, actual.finishes),
            (0, 0, 0, 1)
        );
        assert!(actual.pending.is_empty());
        // The refused retry never reaches the actual runtime RR dispatch.
        assert_eq!(
            client.traffic_metrics_snapshot().by_protocol[protocol]
                .outbound
                .messages,
            1
        );
    }
}
