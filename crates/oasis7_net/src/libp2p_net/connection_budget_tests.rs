use super::*;
use crate::libp2p_net::{
    receiver_floor::PeerRecordReceiver,
    swarm_behaviour::{Behaviour, BehaviourEvent, build_swarm_with_receiver},
    wire_bytes::init_shared_wire_byte_counters,
};
use futures::{
    StreamExt,
    future::{Either, select},
};
use libp2p::{
    Transport,
    identity::Keypair,
    multiaddr::Protocol,
    relay,
    swarm::{
        DialError, ListenError, Swarm, SwarmEvent,
        dial_opts::{DialOpts, PeerCondition},
    },
};

fn limits() -> ConnectionLimits {
    ConnectionLimits {
        total: 3,
        pending: 2,
        inbound_unknown: 1,
        outbound: 2,
        per_peer: 1,
        handshake_timeout: Duration::from_secs(3),
    }
}

fn occupancy(budget: &ConnectionBudget, peer: PeerId) -> (usize, [u32; 2], u32) {
    let state = budget.0.state.lock().unwrap();
    (state.entries.len(), state.pending, state.for_peer(peer))
}

fn dial_for(peer: PeerId, address: Multiaddr) -> DialOpts {
    DialOpts::peer_id(peer)
        .condition(PeerCondition::Always)
        .addresses(vec![address])
        // Repeated TCP dials must be separate sockets so authentication is exercised.
        .allocate_new_port()
        .build()
}

#[test]
fn opposite_pending_lanes_share_capacity_and_authentication_preserves_occupancy() {
    let budget = ConnectionBudget::new(limits()).unwrap();
    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let incoming = dial_for(peer, address.clone()).connection_id();
    let outgoing = dial_for(peer, address.clone()).connection_id();
    let other = dial_for(peer, address.clone()).connection_id();
    let replacement = AttemptKey::Inbound(dial_for(peer, address.clone()).connection_id());
    let inbound = AttemptKey::Inbound(incoming);
    let outbound = AttemptKey::Outbound(outgoing, address.clone());
    let inbound_permit = budget.reserve(inbound.clone(), None).unwrap();
    assert!(budget.reserve(AttemptKey::Inbound(other), None).is_err());
    let outbound_permit = budget.reserve(outbound.clone(), Some(peer)).unwrap();
    assert!(
        budget
            .reserve(AttemptKey::Outbound(other, address), None)
            .is_err()
    );
    assert_eq!(occupancy(&budget, peer), (2, [1, 1], 1));

    assert!(budget.authorize_inbound(incoming, peer).is_err());
    assert_eq!(occupancy(&budget, peer), (2, [1, 1], 1));
    drop(outbound_permit);
    assert_eq!(occupancy(&budget, peer), (1, [1, 0], 0));
    let admission = budget.authorize_inbound(incoming, peer).unwrap();
    assert_eq!(occupancy(&budget, peer), (1, [1, 0], 1));
    admission.establish().unwrap();
    assert_eq!(occupancy(&budget, peer), (1, [0, 0], 1));
    assert!(budget.reserve(outbound.clone(), Some(peer)).is_err());

    let other_peer = Keypair::generate_ed25519().public().to_peer_id();
    let other_inbound = budget.reserve(AttemptKey::Inbound(other), None).unwrap();
    budget
        .authorize_inbound(other, other_peer)
        .unwrap()
        .establish()
        .unwrap();
    let last_outbound = budget.reserve(outbound, None).unwrap();
    assert_eq!(occupancy(&budget, peer), (3, [0, 1], 1));
    // The pending and inbound caps still have room, but the overall cap is full.
    assert!(budget.reserve(replacement.clone(), None).is_err());
    drop(inbound_permit);
    let replacement = budget.reserve(replacement, None).unwrap();
    assert_eq!(occupancy(&budget, peer), (3, [1, 1], 0));
    drop((replacement, last_outbound, other_inbound));
    assert_eq!(occupancy(&budget, peer), (0, [0, 0], 0));
}

#[test]
fn authenticated_inbound_releases_unknown_pool_but_retains_global_pending_permit() {
    let budget = ConnectionBudget::new(ConnectionLimits {
        total: 4,
        pending: 3,
        ..limits()
    })
    .unwrap();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let first_peer = Keypair::generate_ed25519().public().to_peer_id();
    let second_peer = Keypair::generate_ed25519().public().to_peer_id();
    let first = dial_for(first_peer, address.clone()).connection_id();
    let second = dial_for(second_peer, address.clone()).connection_id();
    let third = dial_for(first_peer, address).connection_id();

    let first_permit = budget.reserve(AttemptKey::Inbound(first), None).unwrap();
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 1);
    assert!(budget.reserve(AttemptKey::Inbound(second), None).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PendingInbound)
    );

    // Peer authentication transfers the unknown slot, but child behaviours
    // have not accepted the connection: it must still occupy total + pending.
    let first_admission = budget.authorize_inbound(first, first_peer).unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 1);
    assert_eq!(snapshot.pending, 1);
    assert_eq!(snapshot.pending_inbound_unknown, 0);
    assert_eq!(occupancy(&budget, first_peer), (1, [1, 0], 1));

    // A different unknown peer can now handshake without exceeding global P.
    let second_permit = budget.reserve(AttemptKey::Inbound(second), None).unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 2);
    assert_eq!(snapshot.pending, 2);
    assert_eq!(snapshot.pending_inbound_unknown, 1);
    assert!(budget.reserve(AttemptKey::Inbound(third), None).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PendingInbound)
    );

    let second_admission = budget.authorize_inbound(second, second_peer).unwrap();
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 0);
    first_admission.establish().unwrap();
    assert_eq!(budget.snapshot().unwrap().pending, 1);
    second_admission.establish().unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 2);
    assert_eq!(snapshot.established, 2);
    assert_eq!(snapshot.pending, 0);
    assert_eq!(snapshot.pending_inbound_unknown, 0);
    drop((first_permit, second_permit));
    assert_eq!(budget.snapshot().unwrap().total, 0);
}

#[test]
fn denied_inbound_authentication_keeps_unknown_slot_until_transport_ends() {
    let budget = ConnectionBudget::new(limits()).unwrap();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let outbound = dial_for(peer, address.clone()).connection_id();
    let outbound_permit = budget
        .reserve(AttemptKey::Outbound(outbound, address.clone()), Some(peer))
        .unwrap();
    budget
        .authorize_outbound(outbound, &address, peer)
        .unwrap()
        .establish()
        .unwrap();

    let incoming = dial_for(peer, address.clone()).connection_id();
    let refused = budget.reserve(AttemptKey::Inbound(incoming), None).unwrap();
    assert!(budget.authorize_inbound(incoming, peer).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PerPeer)
    );
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 1);
    let other = dial_for(peer, address).connection_id();
    assert!(budget.reserve(AttemptKey::Inbound(other), None).is_err());
    drop(refused);
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 0);
    let recovered = budget.reserve(AttemptKey::Inbound(other), None).unwrap();
    drop((recovered, outbound_permit));
    assert_eq!(budget.snapshot().unwrap().total, 0);
}

fn swarm(key: &Keypair, budget: Option<ConnectionBudget>) -> Swarm<Behaviour> {
    build_swarm_with_receiver(
        key,
        false,
        false,
        Duration::from_secs(3),
        init_shared_wire_byte_counters(),
        PeerRecordReceiver::legacy(),
        budget,
    )
}

async fn listen(
    swarm: &mut Swarm<impl libp2p::swarm::NetworkBehaviour>,
    address: &str,
) -> Multiaddr {
    swarm.listen_on(address.parse().unwrap()).unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

async fn budgeted_event(
    budgeted: &mut Swarm<Behaviour>,
    remote: &mut Swarm<Behaviour>,
) -> SwarmEvent<BehaviourEvent> {
    loop {
        if let Either::Left((event, _)) =
            select(budgeted.select_next_some(), remote.select_next_some()).await
        {
            return event;
        }
    }
}

#[test]
fn assembled_quic_swarm_shares_peer_occupancy_between_inbound_and_outbound() {
    exercise_direct_transport("/ip4/127.0.0.1/udp/0/quic-v1");
}

#[test]
fn assembled_tcp_swarm_shares_peer_occupancy_between_inbound_and_outbound() {
    exercise_direct_transport("/ip4/127.0.0.1/tcp/0");
}

fn exercise_direct_transport(bind: &str) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(12), async {
                let budget = ConnectionBudget::new(limits()).unwrap();
                let mut budgeted = swarm(&Keypair::generate_ed25519(), Some(budget.clone()));
                let mut remote = swarm(&Keypair::generate_ed25519(), None);
                let local_peer = *budgeted.local_peer_id();
                let remote_peer = *remote.local_peer_id();
                let local_address = listen(&mut budgeted, bind).await;
                let remote_address = listen(&mut remote, bind).await;
                remote.dial(dial_for(local_peer, local_address.clone())).unwrap();

                let first_incoming = loop {
                    if let SwarmEvent::IncomingConnection { connection_id, .. } =
                        budgeted_event(&mut budgeted, &mut remote).await
                    {
                        break connection_id;
                    }
                };
                assert_eq!(occupancy(&budget, remote_peer), (1, [1, 0], 0));
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::ConnectionEstablished { connection_id, peer_id, .. } => {
                            assert_eq!(connection_id, first_incoming);
                            assert_eq!(peer_id, remote_peer);
                            break;
                        }
                        SwarmEvent::IncomingConnectionError { error, .. } => {
                            panic!("first admissible peer failed: {error}");
                        }
                        _ => {}
                    }
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));

                remote.dial(dial_for(local_peer, local_address)).unwrap();
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::IncomingConnection { .. } => {
                            assert_eq!(occupancy(&budget, remote_peer), (2, [1, 0], 1));
                        }
                        SwarmEvent::IncomingConnectionError {
                            peer_id: Some(peer),
                            error: ListenError::Denied { .. },
                            ..
                        } => {
                            assert_eq!(peer, remote_peer);
                            break;
                        }
                        SwarmEvent::ConnectionEstablished { .. } => {
                            panic!("authenticated peer exceeded its shared bucket");
                        }
                        _ => {}
                    }
                }
                // Swarm has reported denial but the rejected muxer still needs teardown.
                assert_eq!(occupancy(&budget, remote_peer), (2, [1, 0], 1));
                while occupancy(&budget, remote_peer).0 == 2 {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(10),
                        budgeted_event(&mut budgeted, &mut remote),
                    )
                    .await;
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
                assert!(matches!(
                    budgeted.dial(dial_for(remote_peer, remote_address.clone())),
                    Err(DialError::Denied { .. })
                ));
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));

                // An unqualified address holds the outbound slot without a peer
                // charge until real authentication identifies the already occupied peer.
                let unknown = DialOpts::unknown_peer_id()
                    .address(remote_address.clone())
                    .allocate_new_port()
                    .build();
                let unknown_id = unknown.connection_id();
                budgeted.dial(unknown).unwrap();
                assert_eq!(occupancy(&budget, remote_peer), (2, [0, 1], 1));
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::OutgoingConnectionError {
                            connection_id,
                            peer_id,
                            error: DialError::Denied { .. },
                        } if connection_id == unknown_id => {
                            assert_eq!(peer_id, Some(remote_peer));
                            assert_eq!(budget.0.diagnostics.latest().0, Some(ResourceBudgetRejection::PerPeer));
                            break;
                        }
                        SwarmEvent::ConnectionEstablished { connection_id, .. }
                            if connection_id == unknown_id => panic!("unknown dial bypassed per-peer cap"),
                        _ => {}
                    }
                }
                while occupancy(&budget, remote_peer).0 == 2 {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(10),
                        budgeted_event(&mut budgeted, &mut remote),
                    )
                    .await;
                }

                assert!(budgeted.close_connection(first_incoming));
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
                while !matches!(
                    budgeted_event(&mut budgeted, &mut remote).await,
                    SwarmEvent::ConnectionClosed { connection_id, .. } if connection_id == first_incoming
                ) {}
                assert_eq!(occupancy(&budget, remote_peer), (0, [0, 0], 0));

                let outbound = dial_for(remote_peer, remote_address);
                let outgoing_id = outbound.connection_id();
                budgeted.dial(outbound).unwrap();
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 1], 1));
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::ConnectionEstablished { connection_id, peer_id, .. } => {
                            assert_eq!(connection_id, outgoing_id);
                            assert_eq!(peer_id, remote_peer);
                            break;
                        }
                        SwarmEvent::OutgoingConnectionError { error, .. } => {
                            panic!("outbound did not recover after close: {error}");
                        }
                        _ => {}
                    }
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
            }).await.expect("connection budget probe timed out");
        });
}

#[test]
fn real_multiaddress_quic_dial_shares_one_peer_admission_slot() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                let budget = ConnectionBudget::new(limits()).unwrap();
                let mut source = swarm(&Keypair::generate_ed25519(), Some(budget.clone()));
                let mut remote = swarm(&Keypair::generate_ed25519(), None);
                let peer = *remote.local_peer_id();
                let bind = "/ip4/127.0.0.1/udp/0/quic-v1";
                let first = listen(&mut remote, bind).await.with_p2p(peer).unwrap();
                let second = listen(&mut remote, bind).await.with_p2p(peer).unwrap();
                assert_ne!(first, second);
                assert_eq!(budget.0.diagnostics.latest().0, None);
                let opts = DialOpts::peer_id(peer)
                    .addresses(vec![first.clone(), second])
                    .build();
                let expected = opts.connection_id();
                source.dial(opts).unwrap();
                assert_eq!(occupancy(&budget, peer), (1, [0, 1], 1));
                assert_eq!(
                    budget.0.diagnostics.latest().0,
                    Some(ResourceBudgetRejection::PerPeer)
                );
                loop {
                    match budgeted_event(&mut source, &mut remote).await {
                        SwarmEvent::ConnectionEstablished {
                            connection_id,
                            endpoint,
                            ..
                        } if connection_id == expected => {
                            assert_eq!(endpoint.get_remote_address(), &first);
                            assert_eq!(occupancy(&budget, peer), (1, [0, 0], 1));
                            break;
                        }
                        SwarmEvent::OutgoingConnectionError {
                            connection_id,
                            error,
                            ..
                        } if connection_id == expected => {
                            panic!("first admissible QUIC address failed: {error}");
                        }
                        _ => {}
                    }
                }
            })
            .await
            .expect("real QUIC multi-address dial did not establish");
        });
}

#[test]
fn pending_tcp_handshake_expires_and_releases_its_socket_and_budget() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address: Multiaddr = format!(
                    "/ip4/127.0.0.1/tcp/{}",
                    listener.local_addr().unwrap().port()
                )
                .parse()
                .unwrap();
                let target = Keypair::generate_ed25519().public().to_peer_id();
                let budget = ConnectionBudget::new(ConnectionLimits {
                    handshake_timeout: Duration::from_secs(1),
                    outbound: 1,
                    ..limits()
                })
                .unwrap();
                let mut budgeted = swarm(&Keypair::generate_ed25519(), Some(budget.clone()));
                let opts = dial_for(target, address.clone());
                let connection = opts.connection_id();
                budgeted.dial(opts).unwrap();
                assert_eq!(occupancy(&budget, target), (1, [0, 1], 1));

                let other_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let other_peer = Keypair::generate_ed25519().public().to_peer_id();
                let other_address: Multiaddr = format!(
                    "/ip4/127.0.0.1/tcp/{}",
                    other_listener.local_addr().unwrap().port()
                )
                .parse()
                .unwrap();
                assert!(matches!(
                    budgeted.dial(dial_for(other_peer, other_address)),
                    Err(DialError::Denied { .. })
                ));
                assert_eq!(
                    budget.0.diagnostics.latest().0,
                    Some(ResourceBudgetRejection::PendingOutbound)
                );
                assert_eq!(occupancy(&budget, other_peer), (1, [0, 1], 0));

                let mut completion = Box::pin(async {
                    loop {
                        if let SwarmEvent::OutgoingConnectionError {
                            connection_id,
                            error,
                            ..
                        } = budgeted.select_next_some().await
                        {
                            assert_eq!(connection_id, connection);
                            return error;
                        }
                    }
                });
                let mut accept = Box::pin(listener.accept());
                let (socket, _) = match select(&mut accept, completion.as_mut()).await {
                    Either::Left((accepted, _)) => accepted.unwrap(),
                    Either::Right((error, _)) => {
                        panic!("dial failed before the TCP handshake: {error}")
                    }
                };
                assert_eq!(occupancy(&budget, target), (1, [0, 1], 1));

                let DialError::Transport(errors) = completion.await else {
                    panic!("expected the pending transport to time out");
                };
                let [(failed_address, libp2p::core::transport::TransportError::Other(error))] =
                    errors.as_slice()
                else {
                    panic!("unexpected transport errors: {errors:?}");
                };
                assert_eq!(failed_address, &address.with_p2p(target).unwrap());
                assert_eq!(error.kind(), io::ErrorKind::TimedOut);
                assert_eq!(error.to_string(), "transport handshake deadline expired");
                assert_eq!(occupancy(&budget, target), (0, [0, 0], 0));
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), other_listener.accept())
                        .await
                        .is_err()
                );

                // Drain any handshake bytes and verify that timing out shut down the real socket.
                let mut bytes = [0; 512];
                loop {
                    socket.readable().await.unwrap();
                    match socket.try_read(&mut bytes) {
                        Ok(0) => break,
                        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => break,
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                        Err(error) => panic!("unexpected socket error: {error}"),
                    }
                }
            })
            .await
            .expect("pending TCP handshake probe timed out");
        });
}

#[test]
fn expired_pending_permit_rejects_authentication_before_peer_accounting() {
    let budget = ConnectionBudget::new(limits()).unwrap();
    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let connection = dial_for(peer, "/ip4/127.0.0.1/tcp/1".parse().unwrap()).connection_id();
    let key = AttemptKey::Inbound(connection);
    let permit = budget.reserve(key.clone(), None).unwrap();
    budget
        .0
        .state
        .lock()
        .unwrap()
        .entries
        .get_mut(&key)
        .unwrap()
        .deadline = Instant::now();
    assert!(budget.authorize_inbound(connection, peer).is_err());
    assert_eq!(occupancy(&budget, peer), (1, [1, 0], 0));
    drop(permit);
    assert_eq!(occupancy(&budget, peer), (0, [0, 0], 0));
}

#[path = "connection_budget_relay_io_tests.rs"]
mod relay_io;

#[test]
fn assembled_relay_circuit_uses_shared_connection_budget_and_recovers_after_close() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(12), exercise_relay_circuit())
                .await
                .expect("relay circuit budget probe timed out");
        });
}

async fn exercise_relay_circuit() {
    let relay_key = Keypair::generate_ed25519();
    let relay_peer = relay_key.public().to_peer_id();
    let transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default())
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(libp2p::noise::Config::new(&relay_key).unwrap())
        .multiplex(libp2p::yamux::Config::default())
        .boxed();
    let mut server = Swarm::new(
        transport,
        relay::Behaviour::new(relay_peer, relay::Config::default()),
        relay_peer,
        Config::with_tokio_executor().with_idle_connection_timeout(Duration::from_secs(12)),
    );
    let relay_address = listen(&mut server, "/ip4/127.0.0.1/tcp/0").await;
    server.add_external_address(relay_address.clone());
    let relay_task = tokio::spawn(async move { while server.next().await.is_some() {} });

    let source_budget = ConnectionBudget::new(limits()).unwrap();
    let destination_budget = ConnectionBudget::new(limits()).unwrap();
    let (mut source, source_bytes) =
        relay_io::swarm_with_byte_window(&Keypair::generate_ed25519(), source_budget.clone());
    let (mut destination, destination_bytes) =
        relay_io::swarm_with_byte_window(&Keypair::generate_ed25519(), destination_budget.clone());
    relay_io::seed_direct_candidate(&mut source).await;
    relay_io::seed_direct_candidate(&mut destination).await;
    let source_peer = *source.local_peer_id();
    let destination_peer = *destination.local_peer_id();
    let listener = destination
        .listen_on(
            relay_address
                .clone()
                .with(Protocol::P2p(relay_peer))
                .with(Protocol::P2pCircuit),
        )
        .unwrap();
    let circuit_address = loop {
        match destination.select_next_some().await {
            SwarmEvent::NewListenAddr {
                address,
                listener_id,
            } if listener_id == listener => {
                assert!(
                    address
                        .iter()
                        .any(|component| component == Protocol::P2pCircuit)
                );
                break address;
            }
            SwarmEvent::ListenerClosed {
                listener_id,
                reason,
                ..
            } if listener_id == listener => {
                panic!("relay reservation terminated: {reason:?}");
            }
            _ => {}
        }
    };
    assert_eq!(occupancy(&destination_budget, relay_peer), (1, [0, 0], 1));
    source.dial(dial_for(relay_peer, relay_address)).unwrap();
    loop {
        match budgeted_event(&mut source, &mut destination).await {
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                assert_eq!(peer_id, relay_peer);
                assert!(!endpoint.is_relayed());
                break;
            }
            SwarmEvent::OutgoingConnectionError { error, .. } => {
                panic!("relay carrier failed: {error}")
            }
            _ => {}
        }
    }

    let first = dial_relay_circuit(
        &mut source,
        &mut destination,
        &source_budget,
        &destination_budget,
        circuit_address.clone(),
    )
    .await;
    relay_io::observe_dcutr_budget_refusal(
        &mut source,
        &mut destination,
        &source_budget,
        &destination_budget,
    )
    .await;
    relay_io::verify_nested_request(
        &mut source,
        &mut destination,
        &source_bytes,
        &destination_bytes,
    )
    .await;
    assert!(matches!(
        source.dial(dial_for(destination_peer, circuit_address.clone())),
        Err(DialError::Denied { .. })
    ));
    assert_eq!(occupancy(&source_budget, destination_peer), (2, [0, 0], 1));

    assert!(source.close_connection(first));
    assert_eq!(occupancy(&source_budget, destination_peer), (2, [0, 0], 1));
    let (mut source_closed, mut destination_closed) = (false, false);
    while !(source_closed && destination_closed) {
        match select(source.select_next_some(), destination.select_next_some()).await {
            Either::Left((SwarmEvent::ConnectionClosed { connection_id, .. }, _))
                if connection_id == first =>
            {
                source_closed = true
            }
            Either::Right((
                SwarmEvent::ConnectionClosed {
                    peer_id, endpoint, ..
                },
                _,
            )) if peer_id == source_peer && endpoint.is_relayed() => destination_closed = true,
            _ => {}
        }
    }
    assert_eq!(occupancy(&source_budget, destination_peer), (1, [0, 0], 0));
    assert_eq!(occupancy(&destination_budget, source_peer), (1, [0, 0], 0));
    assert_eq!(occupancy(&source_budget, relay_peer), (1, [0, 0], 1));
    assert_eq!(occupancy(&destination_budget, relay_peer), (1, [0, 0], 1));

    dial_relay_circuit(
        &mut source,
        &mut destination,
        &source_budget,
        &destination_budget,
        circuit_address,
    )
    .await;
    relay_task.abort();
}

async fn dial_relay_circuit(
    source: &mut Swarm<Behaviour>,
    destination: &mut Swarm<Behaviour>,
    source_budget: &ConnectionBudget,
    destination_budget: &ConnectionBudget,
    address: Multiaddr,
) -> ConnectionId {
    let source_peer = *source.local_peer_id();
    let destination_peer = *destination.local_peer_id();
    let address = address.with_p2p(destination_peer).unwrap();
    let opts = dial_for(destination_peer, address.clone());
    let connection = opts.connection_id();
    source.dial(opts).unwrap();
    assert_eq!(occupancy(source_budget, destination_peer), (2, [0, 1], 1));
    let mut incoming = None;
    let (mut outbound, mut inbound) = (false, false);
    while !(outbound && inbound) {
        match select(source.select_next_some(), destination.select_next_some()).await {
            Either::Left((
                SwarmEvent::ConnectionEstablished {
                    connection_id,
                    peer_id,
                    endpoint,
                    ..
                },
                _,
            )) if connection_id == connection => {
                assert_eq!(peer_id, destination_peer);
                assert!(endpoint.is_relayed());
                assert_eq!(endpoint.get_remote_address(), &address);
                outbound = true;
            }
            Either::Left((
                SwarmEvent::OutgoingConnectionError {
                    connection_id,
                    error,
                    ..
                },
                _,
            )) if connection_id == connection => panic!("outbound relay circuit failed: {error}"),
            Either::Right((
                SwarmEvent::IncomingConnection {
                    connection_id,
                    local_addr,
                    ..
                },
                _,
            )) if local_addr
                .iter()
                .any(|component| component == Protocol::P2pCircuit) =>
            {
                assert!(incoming.replace(connection_id).is_none());
                assert_eq!(occupancy(destination_budget, source_peer), (2, [1, 0], 0));
            }
            Either::Right((
                SwarmEvent::ConnectionEstablished {
                    connection_id,
                    peer_id,
                    endpoint,
                    ..
                },
                _,
            )) if peer_id == source_peer => {
                assert!(endpoint.is_relayed());
                assert_eq!(Some(connection_id), incoming);
                inbound = true;
            }
            Either::Right((SwarmEvent::IncomingConnectionError { error, .. }, _)) => {
                panic!("inbound relay circuit failed: {error}")
            }
            _ => {}
        }
    }
    assert_eq!(occupancy(source_budget, destination_peer), (2, [0, 0], 1));
    assert_eq!(occupancy(destination_budget, source_peer), (2, [0, 0], 1));
    connection
}

#[test]
fn kademlia_and_rendezvous_use_one_window_on_the_same_authenticated_connection() {
    use crate::libp2p_net::{
        wire_byte_window::{WireByteLimits, WireByteWindow},
        wire_bytes::Libp2pWireByteCounters,
    };
    use libp2p::{kad, rendezvous};

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                let bytes = Arc::new(WireByteWindow::new(WireByteLimits {
                    inbound: 16_384, outbound: 16_384, total: 16_384,
                }).unwrap());
                let counters = Libp2pWireByteCounters::with_window(Arc::clone(&bytes));
                let budget = ConnectionBudget::new(limits()).unwrap();
                let mut source = build_swarm_with_receiver(
                    &Keypair::generate_ed25519(),
                    true,
                    false,
                    Duration::from_secs(3),
                    counters,
                    PeerRecordReceiver::legacy(),
                    Some(budget.clone()),
                );
                let mut server = build_swarm_with_receiver(
                    &Keypair::generate_ed25519(),
                    true,
                    false,
                    Duration::from_secs(3),
                    init_shared_wire_byte_counters(),
                    PeerRecordReceiver::legacy(),
                    None,
                );
                server.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));
                let peer = *server.local_peer_id();
                let address = listen(&mut server, "/ip4/127.0.0.1/tcp/0").await;
                source.behaviour_mut().kademlia.add_address(&peer, address.clone());
                let dial = dial_for(peer, address);
                let connection = dial.connection_id();
                source.dial(dial).unwrap();
                while !matches!(
                    budgeted_event(&mut source, &mut server).await,
                    SwarmEvent::ConnectionEstablished { connection_id, .. } if connection_id == connection
                ) {}
                let before = bytes.snapshot().unwrap().actual_bytes;

                let query = source.behaviour_mut().kademlia.get_closest_peers(peer.to_bytes());
                loop {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                        id, result: kad::QueryResult::GetClosestPeers(result), step, ..
                    })) = budgeted_event(&mut source, &mut server).await
                        && id == query && step.last
                    {
                        let result = result.expect("real Kademlia query must complete");
                        assert!(result.peers.iter().any(|entry| entry.peer_id == peer));
                        break;
                    }
                }
                let after_kad = bytes.snapshot().unwrap().actual_bytes;
                assert!(after_kad.inbound > before.inbound && after_kad.outbound > before.outbound);

                let request = source.behaviour_mut().rendezvous_client.as_mut().unwrap()
                    .discover_with_request_id(None, None, None, peer);
                loop {
                    match budgeted_event(&mut source, &mut server).await {
                        SwarmEvent::Behaviour(BehaviourEvent::RendezvousClient(
                            rendezvous::client::Event::Discovered { request_id, rendezvous_node, .. },
                        )) if request_id == request => {
                            assert_eq!(rendezvous_node, peer);
                            break;
                        }
                        SwarmEvent::Behaviour(BehaviourEvent::RendezvousClient(
                            rendezvous::client::Event::DiscoverFailed { request_id, error, .. },
                        )) if request_id == request => panic!("real rendezvous query failed: {error:?}"),
                        _ => {}
                    }
                }
                let after_rendezvous = bytes.snapshot().unwrap();
                assert!(
                    after_rendezvous.actual_bytes.inbound > after_kad.inbound
                        && after_rendezvous.actual_bytes.outbound > after_kad.outbound
                );
                assert_eq!(after_rendezvous.actual_bytes, after_rendezvous.charged_bytes);
                assert!(after_rendezvous.charged_bytes.total <= 16_384);
                assert_eq!(occupancy(&budget, peer), (1, [0, 0], 1));
            }).await.expect("shared Kademlia/rendezvous fixture timed out");
        });
}

#[test]
fn real_request_deadline_survives_natural_byte_window_recovery() {
    use crate::libp2p_net::{
        wire_byte_window::{WireByteLimits, WireByteWindow},
        wire_bytes::Libp2pWireByteCounters,
    };
    use libp2p::request_response::{Event, Message, OutboundFailure};
    use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(16), async {
                // Seed an aged full bucket. The original 10-second swarm
                // negotiation deadline expires before its real bucket expiry.
                // The configured RR request timeout applies after negotiation.
                let (window, expiry) = WireByteWindow::with_aged_first_read(
                    WireByteLimits { inbound: 16_384, outbound: 16_384, total: 16_384 },
                    Duration::from_secs(49),
                );
                let window = Arc::new(window);
                let mut client = build_swarm_with_receiver(
                    &Keypair::generate_ed25519(),
                    false,
                    false,
                    Duration::from_millis(350),
                    Libp2pWireByteCounters::with_window(Arc::clone(&window)),
                    PeerRecordReceiver::legacy(),
                    Some(ConnectionBudget::new(limits()).unwrap()),
                );
                let mut server = swarm(&Keypair::generate_ed25519(), None);
                let peer = *server.local_peer_id();
                let address = listen(&mut server, "/ip4/127.0.0.1/tcp/0").await;
                let dial = dial_for(peer, address);
                let connection = dial.connection_id();
                client.dial(dial).unwrap();
                while !matches!(
                    budgeted_event(&mut client, &mut server).await,
                    SwarmEvent::ConnectionEstablished { connection_id, .. } if connection_id == connection
                ) {}
                let request = |payload: &[u8]| NetworkRequest {
                    protocol: "/test/window-deadline".into(),
                    payload: payload.to_vec(),
                };
                let expired = client.behaviour_mut().request_response.send_request(&peer, super::super::request_deadline::LocalRequest { message: request(b"expired"), deadline: None });
                loop {
                    match select(client.select_next_some(), server.select_next_some()).await {
                        Either::Left((SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                            Event::OutboundFailure { request_id, error, .. },
                        )), _)) if request_id == expired => {
                            assert!(matches!(error, OutboundFailure::Timeout), "{error}");
                            assert!(tokio::time::Instant::now() < expiry);
                            break;
                        }
                        Either::Right((SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                            Event::Message { message: Message::Request { .. }, .. },
                        )), _)) => panic!("expired request reached the server before window recovery"),
                        _ => {}
                    }
                }
                assert_eq!(window.snapshot().unwrap().charged_bytes.total, 16_384);
                tokio::time::sleep_until(expiry + Duration::from_millis(1)).await;
                // Other substreams can use recovered capacity immediately.
                // The seeded full charge must be gone even if control I/O ran.
                assert!(window.snapshot().unwrap().charged_bytes.total < 16_384);

                let fresh = client.behaviour_mut().request_response.send_request(&peer, super::super::request_deadline::LocalRequest { message: request(b"fresh"), deadline: None });
                loop {
                    match select(client.select_next_some(), server.select_next_some()).await {
                        Either::Right((SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                            Event::Message { message: Message::Request { request, channel, .. }, .. },
                        )), _)) => {
                            assert_eq!(request.message.payload, b"fresh", "expired request was revived");
                            server.behaviour_mut().request_response
                                .send_response(channel, NetworkResponse { payload: b"ack".to_vec() })
                                .unwrap();
                        }
                        Either::Left((SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                            Event::Message { message: Message::Response { request_id, response }, .. },
                        )), _)) => {
                            assert_eq!(request_id, fresh);
                            assert_eq!(response.payload, b"ack");
                            break;
                        }
                        Either::Left((SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                            Event::OutboundFailure { request_id, error, .. },
                        )), _)) => panic!("unexpected second terminal response {request_id}: {error}"),
                        _ => {}
                    }
                }
                assert!(window.snapshot().unwrap().actual_bytes.total > 0);
            }).await.expect("natural byte window RR fixture timed out");
        });
}

#[test]
fn real_tracked_handshake_deadline_does_not_dial_the_alternate_path() {
    use crate::libp2p_net::transport_paths::{
        bootstrap_transport_path_state, dial_transport_path, retry_transport_path_after_error,
        take_retryable_transport_path,
    };

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(6), async {
                let stalled = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let alternate = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let peer = Keypair::generate_ed25519().public().to_peer_id();
                let address = |listener: &tokio::net::TcpListener| {
                    format!(
                        "/ip4/127.0.0.1/tcp/{}/p2p/{peer}",
                        listener.local_addr().unwrap().port()
                    )
                    .parse::<Multiaddr>()
                    .unwrap()
                };
                let first_address = address(&stalled);
                let (known, mut failed, _) =
                    bootstrap_transport_path_state(&[first_address.clone(), address(&alternate)]);
                let first = known[&peer]
                    .iter()
                    .find(|path| path.addr == first_address)
                    .unwrap()
                    .clone();
                let mut pending = std::collections::HashMap::new();
                let budget = ConnectionBudget::new(ConnectionLimits {
                    handshake_timeout: Duration::from_secs(1),
                    ..limits()
                })
                .unwrap();
                let mut client = swarm(&Keypair::generate_ed25519(), Some(budget));
                dial_transport_path(&mut client, &mut pending, first).unwrap();
                let connection = pending[&peer].connection_id;

                let mut terminal = Box::pin(async {
                    loop {
                        if let SwarmEvent::OutgoingConnectionError {
                            connection_id,
                            peer_id,
                            error,
                        } = client.select_next_some().await
                        {
                            assert_eq!(connection_id, connection);
                            assert_eq!(peer_id, Some(peer));
                            return error;
                        }
                    }
                });
                let mut accept = Box::pin(stalled.accept());
                let (_socket, _) = match select(&mut accept, terminal.as_mut()).await {
                    Either::Left((accepted, _)) => accepted.unwrap(),
                    Either::Right((error, _)) => {
                        panic!("tracked TCP dial failed before accept: {error}")
                    }
                };
                let error = terminal.await;
                assert!(crate::libp2p_net::resource_budget::dial_hit_handshake_deadline(&error));
                if let Some(path) =
                    take_retryable_transport_path(&mut pending, peer, connection, &error)
                {
                    retry_transport_path_after_error(
                        &mut client,
                        &known,
                        &mut pending,
                        &mut failed,
                        path,
                    )
                    .unwrap();
                }
                assert!(
                    !pending.contains_key(&peer),
                    "timed-out dial started alternate tracking"
                );
                assert!(
                    failed.is_empty(),
                    "local deadline mislabeled the remote path"
                );
                let pump = async {
                    loop {
                        client.select_next_some().await;
                    }
                };
                let attempt = select(Box::pin(alternate.accept()), Box::pin(pump));
                assert!(
                    tokio::time::timeout(Duration::from_millis(150), attempt)
                        .await
                        .is_err()
                );
            })
            .await
            .expect("tracked deadline fixture timed out");
        });
}

#[test]
fn stale_and_mixed_dial_errors_are_scoped_to_the_exact_retry_path() {
    use crate::libp2p_net::transport_paths::{
        DialedTransportPath, bootstrap_transport_path_state, take_retryable_transport_path,
    };
    use libp2p::{core::transport::TransportError, swarm::TransportHandshakeDeadlineElapsed};
    use std::collections::HashMap;

    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let address = |port| {
        format!("/ip4/127.0.0.1/tcp/{port}/p2p/{peer}")
            .parse::<Multiaddr>()
            .unwrap()
    };
    let first_address = address(1);
    let other_address = address(2);
    let (known, _, _) = bootstrap_transport_path_state(std::slice::from_ref(&first_address));
    let path = known[&peer][0].clone();
    let old = dial_for(peer, first_address.clone()).connection_id();
    let current = dial_for(peer, first_address.clone()).connection_id();
    let mut pending = HashMap::from([(
        peer,
        DialedTransportPath {
            connection_id: current,
            path: path.clone(),
        },
    )]);
    let marked = || {
        TransportError::Other(io::Error::new(
            io::ErrorKind::TimedOut,
            TransportHandshakeDeadlineElapsed,
        ))
    };

    let stale = DialError::Transport(vec![(first_address.clone(), marked())]);
    assert!(take_retryable_transport_path(&mut pending, peer, old, &stale).is_none());
    assert_eq!(pending[&peer].connection_id, current);

    let mixed = DialError::Transport(vec![
        (other_address, marked()),
        (
            first_address,
            TransportError::Other(io::Error::from(io::ErrorKind::ConnectionRefused)),
        ),
    ]);
    assert_eq!(
        take_retryable_transport_path(&mut pending, peer, current, &mixed),
        Some(path)
    );
    assert!(pending.is_empty());
}

#[test]
fn elapsed_caller_deadline_blocks_late_payload_after_real_refill() {
    use crate::libp2p_net::{Libp2pNetwork, Libp2pNetworkConfig};
    use oasis7_proto::{distributed_net::DistributedNetwork as _, network_failure::FailureCause};
    use std::sync::mpsc;

    fn wait_until(message: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < deadline, "{message}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let profile = serde_json::from_str(
        r#"{"profile":"v1-discovery-resource-budget-1",
        "connections_total":3,"pending_connections_total":2,"pending_inbound_unknown":1,
        "pending_outbound":2,"connections_per_peer":1,"handshake_timeout_ms":3000,
        "bytes_in_per_window":16384,"bytes_out_per_window":16384,"bytes_total_per_window":16384}"#,
    )
    .unwrap();
    let server = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let protocol = "/test/caller-window-deadline";
    let (received_tx, received) = mpsc::channel();
    server
        .register_handler(
            protocol,
            Box::new(move |payload| {
                received_tx.send(payload.to_vec()).unwrap();
                Ok(b"ack".to_vec())
            }),
        )
        .unwrap();
    wait_until("server listener unavailable", || {
        !server.listening_addrs().is_empty()
    });

    let client = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        p2p_resource_budget: profile,
        enable_autonat: false,
        request_response_timeout: Duration::from_secs(8),
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    let window = client.wire_byte_counters.window().unwrap();
    let expiry = window
        .seed_aged_first_read(Duration::from_secs(58))
        .into_std();
    let peer = server.peer_id();
    client
        .dial(server.listening_addrs()[0].clone().with_p2p(peer).unwrap())
        .unwrap();
    wait_until("authenticated TCP connection unavailable", || {
        client.connected_peers().contains(&peer)
    });

    let original_retry_deadline = Instant::now() + Duration::from_millis(350);
    let error = client
        .request_to_peer_with_timeout(protocol, b"expired", peer, Duration::from_millis(350))
        .unwrap_err();
    assert_eq!(
        error.network_failure(Some(protocol)).cause,
        FailureCause::Timeout
    );
    assert!(
        Instant::now() < expiry,
        "caller deadline did not precede byte recovery"
    );
    assert!(
        received.try_recv().is_err(),
        "request arrived while caller was expired"
    );
    std::thread::sleep(
        expiry.saturating_duration_since(Instant::now()) + Duration::from_millis(10),
    );

    wait_until("expired request did not reach actual terminal", || {
        client
            .debug_errors()
            .iter()
            .any(|error| error.contains("RequestDeadlineElapsed"))
    });
    let retry = client
        .request_to_peer_before(protocol, b"late retry", peer, original_retry_deadline)
        .unwrap_err();
    assert_eq!(
        retry.network_failure(Some(protocol)).cause,
        FailureCause::Timeout
    );
    assert_eq!(
        client
            .request_to_peer_with_timeout(protocol, b"fresh", peer, Duration::from_secs(3))
            .unwrap(),
        b"ack",
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).unwrap(),
        b"fresh"
    );
    assert!(received.try_recv().is_err());
    assert_eq!(
        client
            .traffic_metrics_snapshot()
            .resource_budget
            .connections
            .unwrap()
            .established,
        1
    );

    let legacy = Libp2pNetwork::try_new(Libp2pNetworkConfig {
        enable_autonat: false,
        ..Libp2pNetworkConfig::default()
    })
    .unwrap();
    assert_eq!(
        legacy.traffic_metrics_snapshot().resource_budget.profile,
        "legacy"
    );
    legacy
        .dial(server.listening_addrs()[0].clone().with_p2p(peer).unwrap())
        .unwrap();
    wait_until("legacy authenticated TCP connection unavailable", || {
        legacy.connected_peers().contains(&peer)
    });
    let expired = legacy
        .request_to_peer_before(protocol, b"legacy stale", peer, original_retry_deadline)
        .unwrap_err();
    assert_eq!(
        expired.network_failure(Some(protocol)).cause,
        FailureCause::Timeout
    );
    assert_eq!(
        legacy
            .request_to_peer_before(
                protocol,
                b"legacy live",
                peer,
                Instant::now() + Duration::from_secs(3)
            )
            .unwrap(),
        b"ack",
    );
    assert_eq!(
        received.recv_timeout(Duration::from_secs(1)).unwrap(),
        b"legacy live"
    );
    assert!(received.try_recv().is_err());
}
