//! Actual pending transport admission, authentication deadlines and release.
use super::*;

#[test]
fn pending_tcp_handshake_expires_and_releases_its_socket_and_budget() {
    for (total, pending, reason) in [
        (1, 1, ResourceBudgetRejection::ConnectionTotal),
        (3, 1, ResourceBudgetRejection::PendingTotal),
        (3, 2, ResourceBudgetRejection::PendingOutbound),
    ] {
        exercise_pending_tcp_handshake_limit(total, pending, reason);
    }
}

fn exercise_pending_tcp_handshake_limit(total: u32, pending: u32, reason: ResourceBudgetRejection) {
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
                    total,
                    pending,
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
                assert_eq!(budget.0.diagnostics.latest().0, Some(reason));
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

                let dial_error = completion.await;
                assert!(
                    crate::libp2p_net::resource_budget::dial_hit_handshake_deadline(&dial_error)
                );
                let DialError::Transport(errors) = dial_error else {
                    panic!("expected the pending transport to time out");
                };
                let [(failed_address, libp2p::core::transport::TransportError::Other(error))] =
                    errors.as_slice()
                else {
                    panic!("unexpected transport errors: {errors:?}");
                };
                assert_eq!(failed_address, &address.with_p2p(target).unwrap());
                let inner_error = error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<io::Error>())
                    .unwrap_or(error);
                assert_eq!(inner_error.kind(), io::ErrorKind::TimedOut);
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
fn unknown_inbound_pool_refuses_a_real_socket_then_recovers_after_actual_close() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                let budget = ConnectionBudget::new(limits()).unwrap();
                let mut local = swarm(&Keypair::generate_ed25519(), Some(budget.clone()));
                let address = listen(&mut local, "/ip4/127.0.0.1/tcp/0").await;
                let port = address
                    .iter()
                    .find_map(|protocol| match protocol {
                        Protocol::Tcp(port) => Some(port),
                        _ => None,
                    })
                    .unwrap();
                // A raw socket cannot assert a PeerId and stays in the shared
                // unknown pool until the actual Noise/TCP upgrade terminates.
                let original =
                    tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                        .await
                        .unwrap();
                let original_id = loop {
                    if let SwarmEvent::IncomingConnection { connection_id, .. } =
                        local.select_next_some().await
                    {
                        break connection_id;
                    }
                };
                let before = budget.snapshot().unwrap();
                assert_eq!(
                    (before.total, before.pending, before.pending_inbound_unknown),
                    (1, 1, 1)
                );

                let rejected =
                    tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                        .await
                        .unwrap();
                loop {
                    match local.select_next_some().await {
                        SwarmEvent::IncomingConnectionError {
                            connection_id,
                            error: ListenError::Denied { .. },
                            ..
                        } if connection_id != original_id => break,
                        SwarmEvent::IncomingConnection { .. } => {
                            panic!("unknown cap permitted another handshake")
                        }
                        SwarmEvent::ConnectionEstablished { .. } => {
                            panic!("raw unauthenticated TCP became a peer")
                        }
                        _ => {}
                    }
                }
                assert_eq!(
                    budget.0.diagnostics.latest().0,
                    Some(ResourceBudgetRejection::PendingInbound)
                );
                assert_eq!(budget.snapshot().unwrap(), before);
                let mut byte = [0];
                loop {
                    rejected.readable().await.unwrap();
                    match rejected.try_read(&mut byte) {
                        Ok(0) => break,
                        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => break,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                        other => {
                            panic!("refused socket did not close before handshake I/O: {other:?}")
                        }
                    }
                }
                drop((rejected, original));
                while budget.snapshot().unwrap().total != 0 {
                    let _ = local.select_next_some().await;
                }

                let mut fresh = swarm(&Keypair::generate_ed25519(), None);
                fresh
                    .dial(dial_for(*local.local_peer_id(), address))
                    .unwrap();
                loop {
                    if let SwarmEvent::ConnectionEstablished { peer_id, .. } =
                        budgeted_event(&mut local, &mut fresh).await
                    {
                        assert_eq!(peer_id, *fresh.local_peer_id());
                        break;
                    }
                }
                let recovered = budget.snapshot().unwrap();
                assert_eq!(
                    (
                        recovered.total,
                        recovered.established,
                        recovered.pending_inbound_unknown
                    ),
                    (1, 1, 0)
                );
            })
            .await
            .expect("real inbound admission/release probe timed out");
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
