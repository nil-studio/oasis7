use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Transport,
    core::{
        muxing::StreamMuxerBox,
        transport::{self, ListenerId, TransportError, TransportEvent},
    },
    identity::Keypair,
    quic,
    swarm::{Config, ConnectionDenied, DialError, Swarm, SwarmEvent, dial_opts::DialOpts, dummy},
};
use std::{
    io,
    net::UdpSocket,
    num::NonZeroU8,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

struct Permit(Arc<AtomicUsize>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

type QuicTransport = transport::Boxed<(PeerId, StreamMuxerBox)>;

struct RecordingTransport {
    inner: QuicTransport,
    dialed: Arc<Mutex<Vec<Multiaddr>>>,
}

impl Transport for RecordingTransport {
    type Output = <QuicTransport as Transport>::Output;
    type Error = io::Error;
    type ListenerUpgrade = <QuicTransport as Transport>::ListenerUpgrade;
    type Dial = <QuicTransport as Transport>::Dial;

    fn listen_on(
        &mut self,
        id: ListenerId,
        address: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        self.inner.listen_on(id, address)
    }

    fn remove_listener(&mut self, id: ListenerId) -> bool {
        self.inner.remove_listener(id)
    }

    fn dial(
        &mut self,
        address: Multiaddr,
        opts: transport::DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        self.dialed.lock().unwrap().push(address.clone());
        self.inner.dial(address, opts)
    }

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        Pin::new(&mut self.inner).poll(cx)
    }
}

fn swarm(config: Config, dialed: Arc<Mutex<Vec<Multiaddr>>>) -> Swarm<dummy::Behaviour> {
    let key = Keypair::generate_ed25519();
    let transport = quic::tokio::Transport::new(quic::Config::new(&key))
        .map(|(peer, muxer), _| (peer, StreamMuxerBox::new(muxer)))
        .boxed();
    Swarm::new(
        RecordingTransport {
            inner: transport,
            dialed,
        }
        .boxed(),
        dummy::Behaviour,
        key.public().to_peer_id(),
        config.with_idle_connection_timeout(Duration::from_secs(30)),
    )
}

#[test]
fn a_multiaddress_dial_checks_each_attempt_and_holds_the_winner_until_close() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let mut listener = swarm(Config::with_tokio_executor(), Arc::default());
                let listener_peer = *listener.local_peer_id();
                let listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
                listener.listen_on(listen.clone()).unwrap();
                listener.listen_on(listen).unwrap();
                let mut addresses = Vec::new();
                while addresses.len() < 2 {
                    if let SwarmEvent::NewListenAddr { address, .. } = listener.select_next_some().await
                    {
                        addresses.push(address);
                    }
                }
                assert_ne!(addresses[0], addresses[1]);
                let denied = addresses[0].clone().with_p2p(listener_peer).unwrap();
                let supported = addresses[1].clone().with_p2p(listener_peer).unwrap();
                let unsupported: Multiaddr = "/ip4/127.0.0.1/tcp/9".parse().unwrap();
                let unsupported = unsupported.with_p2p(listener_peer).unwrap();
                let unused_port = UdpSocket::bind("127.0.0.1:0").unwrap();
                let cancelled: Multiaddr = format!(
                    "/ip4/127.0.0.1/udp/{}/quic-v1/p2p/{listener_peer}",
                    unused_port.local_addr().unwrap().port()
                )
                .parse()
                .unwrap();
                let incoming = Arc::new(Mutex::new(Vec::new()));
                let listener_incoming = Arc::clone(&incoming);
                let server = tokio::spawn(async move {
                    while let Some(event) = listener.next().await {
                        if let SwarmEvent::IncomingConnection { local_addr, .. } = event {
                            listener_incoming.lock().unwrap().push(local_addr);
                        }
                    }
                });

                let admitted = Arc::new(Mutex::new(Vec::new()));
                let attempts = Arc::clone(&admitted);
                let drops = Arc::new(AtomicUsize::new(0));
                let permit_drops = Arc::clone(&drops);
                let refused = denied.clone();
                let dialed = Arc::new(Mutex::new(Vec::new()));
                let observed_dials = Arc::clone(&dialed);
                let mut dialer = swarm(Config::with_tokio_executor().with_outbound_transport_admission(
                    move |connection, peer: Option<PeerId>, address| {
                        assert!(!observed_dials.lock().unwrap().contains(address));
                        attempts.lock().unwrap().push((connection, peer, address.clone()));
                        if address == &refused {
                            return Err(ConnectionDenied::new(io::Error::other("denied address")));
                        }
                        Ok(Box::new(Permit(Arc::clone(&permit_drops))))
                    },
                ), Arc::clone(&dialed));
                let opts = DialOpts::peer_id(listener_peer)
                    .addresses(vec![
                        denied.clone(),
                        unsupported.clone(),
                        supported.clone(),
                        cancelled.clone(),
                    ])
                    .override_dial_concurrency_factor(NonZeroU8::MIN)
                    .build();
                let connection = opts.connection_id();
                dialer.dial(opts).unwrap();
                assert_eq!(
                    &*admitted.lock().unwrap(),
                    &[
                        (connection, Some(listener_peer), denied),
                        (connection, Some(listener_peer), unsupported.clone()),
                        (connection, Some(listener_peer), supported.clone()),
                        (connection, Some(listener_peer), cancelled.clone()),
                    ],
                );
                assert_eq!(
                    &*dialed.lock().unwrap(),
                    &[unsupported.clone(), supported.clone(), cancelled],
                );
                // The unsupported transport fails synchronously, releasing its token.
                assert_eq!(drops.load(Ordering::SeqCst), 1);

                loop {
                    match dialer.select_next_some().await {
                        SwarmEvent::ConnectionEstablished {
                            connection_id,
                            endpoint,
                            concurrent_dial_errors,
                            ..
                        } => {
                            assert_eq!(connection_id, connection);
                            assert_eq!(endpoint.get_remote_address(), &supported);
                            let failures = concurrent_dial_errors.unwrap();
                            assert_eq!(failures.len(), 1);
                            assert_eq!(failures[0].0, unsupported);
                            break;
                        }
                        SwarmEvent::OutgoingConnectionError { error, .. } => {
                            panic!("admitted address did not connect: {error}");
                        }
                        _ => {}
                    }
                }
                // Closing the address race drops the unused attempt's permit.
                assert_eq!(drops.load(Ordering::SeqCst), 2);
                assert_eq!(
                    incoming.lock().unwrap().as_slice(),
                    std::slice::from_ref(&addresses[1]),
                );
                assert!(dialer.close_connection(connection));
                assert_eq!(drops.load(Ordering::SeqCst), 2);
                while !matches!(
                    dialer.select_next_some().await,
                    SwarmEvent::ConnectionClosed { connection_id, .. } if connection_id == connection
                ) {}
                assert_eq!(drops.load(Ordering::SeqCst), 3);

                let blocked_dials = Arc::new(Mutex::new(Vec::new()));
                let mut blocked = swarm(
                    Config::with_tokio_executor().with_outbound_transport_admission(|_, _, _| {
                        Err(ConnectionDenied::new(io::Error::other("no permits")))
                    }),
                    Arc::clone(&blocked_dials),
                );
                assert!(matches!(blocked.dial(supported), Err(DialError::Denied { .. })));
                assert!(blocked_dials.lock().unwrap().is_empty());
                server.abort();
            })
            .await
            .expect("outbound connection did not finish");
        });
}
