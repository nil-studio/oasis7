#![cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
use futures::future::poll_fn;
use libp2p::{
    PeerId, Transport,
    core::{
        Endpoint,
        transport::{DialOpts, ListenerId, PortUse, TransportEvent},
    },
    identity::Keypair,
    quic,
};
use std::{pin::Pin, time::Duration};

#[derive(Clone, Copy)]
enum Decision {
    Accept,
    Reject,
    Expire,
    Immediate,
}

fn exercise(decision: Decision) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let server_key = Keypair::generate_ed25519();
                let client_key = Keypair::generate_ed25519();
                let mut config = quic::Config::new(&server_key);
                if matches!(decision, Decision::Expire) {
                    config.handshake_timeout = Duration::from_millis(150);
                }
                let mut server = quic::tokio::Transport::new(config);
                let mut client = quic::tokio::Transport::new(quic::Config::new(&client_key));
                server
                    .listen_on(
                        ListenerId::next(),
                        "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
                    )
                    .unwrap();
                let addr = loop {
                    if let TransportEvent::NewAddress { listen_addr, .. } =
                        poll_fn(|cx| Pin::new(&mut server).poll(cx)).await
                    {
                        break listen_addr;
                    }
                };
                let dial = client
                    .dial(
                        addr,
                        DialOpts {
                            role: Endpoint::Dialer,
                            port_use: PortUse::Reuse,
                        },
                    )
                    .unwrap();
                let mut dial = tokio::spawn(dial);
                let upgrade = loop {
                    match poll_fn(|cx| Pin::new(&mut server).poll(cx)).await {
                        TransportEvent::Incoming { upgrade, .. } => break upgrade,
                        TransportEvent::ListenerError { error, .. } => {
                            panic!("listener failed: {error}")
                        }
                        _ => {}
                    }
                };
                // This holds the actual listener upgrade before a Swarm policy can admit it.
                if !matches!(decision, Decision::Immediate) {
                    assert!(
                        tokio::time::timeout(Duration::from_millis(500), &mut dial)
                            .await
                            .is_err(),
                        "QUIC completed its TLS handshake before inbound admission"
                    );
                }
                match decision {
                    Decision::Accept | Decision::Immediate => {
                        let (remote, _server_connection) = upgrade.await.expect("admitted upgrade");
                        assert_eq!(remote, PeerId::from(client_key.public()));
                        let (remote, _client_connection) =
                            dial.await.unwrap().expect("admitted dial");
                        assert_eq!(remote, PeerId::from(server_key.public()));
                    }
                    Decision::Reject => {
                        // Drop must refuse without starting the server handshake.
                        drop(upgrade);
                        assert!(dial.await.unwrap().is_err());
                    }
                    Decision::Expire => {
                        assert!(
                            matches!(upgrade.await, Err(quic::Error::HandshakeTimedOut)),
                            "admission waiting must not restart the handshake deadline"
                        );
                        assert!(dial.await.unwrap().is_err());
                    }
                }
                drop((client, server));
            })
            .await
            .expect("loopback admission scenario must terminate");
        });
}
#[test]
fn quic_inbound_gate_holds_handshake_then_allows() {
    exercise(Decision::Accept);
}
#[test]
fn quic_inbound_gate_rejects_unpolled_attempt() {
    exercise(Decision::Reject);
}
#[test]
fn quic_inbound_gate_preserves_original_deadline() {
    exercise(Decision::Expire);
}
#[test]
fn quic_inbound_default_accept_still_connects() {
    exercise(Decision::Immediate);
}

#[test]
fn quic_swarm_pending_inbound_policy_rejects_attempt() {
    use futures::StreamExt;
    use libp2p::{
        connection_limits,
        core::muxing::StreamMuxerBox,
        swarm::{Config, ListenError, Swarm, SwarmEvent},
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let server_key = Keypair::generate_ed25519();
                let transport = quic::tokio::Transport::new(quic::Config::new(&server_key))
                    .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
                    .boxed();
                let policy = connection_limits::Behaviour::new(
                    connection_limits::ConnectionLimits::default()
                        .with_max_pending_incoming(Some(0)),
                );
                let mut swarm = Swarm::new(
                    transport,
                    policy,
                    PeerId::from(server_key.public()),
                    Config::with_tokio_executor(),
                );
                swarm
                    .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
                    .unwrap();
                let addr = loop {
                    if let Some(SwarmEvent::NewListenAddr { address, .. }) = swarm.next().await {
                        break address;
                    }
                };
                let mut client =
                    quic::tokio::Transport::new(quic::Config::new(&Keypair::generate_ed25519()));
                let dial = client
                    .dial(
                        addr,
                        DialOpts {
                            role: Endpoint::Dialer,
                            port_use: PortUse::Reuse,
                        },
                    )
                    .unwrap();
                let dial = tokio::spawn(dial);
                loop {
                    match swarm.next().await {
                        Some(SwarmEvent::IncomingConnectionError {
                            error: ListenError::Denied { .. },
                            ..
                        }) => break,
                        Some(SwarmEvent::ConnectionEstablished { .. }) => {
                            panic!("policy-denied connection established")
                        }
                        None => panic!("swarm terminated before denial"),
                        _ => {}
                    }
                }
                assert!(dial.await.unwrap().is_err(), "denied QUIC dial must fail");
            })
            .await
            .expect("denied Swarm attempt must terminate");
        });
}
