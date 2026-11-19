use super::*;
use crate::libp2p_net::wire_byte_window::WireByteLimits;
use futures::{
    future::poll_fn,
    io::{AsyncReadExt, AsyncWriteExt},
};
use libp2p::{
    Transport,
    core::{
        Endpoint,
        transport::{DialOpts, ListenerId, PortUse, TransportEvent},
    },
    identity::Keypair,
    quic,
};
use std::time::Duration;

async fn quic_muxers() -> (quic::Connection, quic::Connection) {
    let server_key = Keypair::generate_ed25519();
    let client_key = Keypair::generate_ed25519();
    let mut server = quic::tokio::Transport::new(quic::Config::new(&server_key));
    let mut client = quic::tokio::Transport::new(quic::Config::new(&client_key));
    server
        .listen_on(
            ListenerId::next(),
            "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
        )
        .unwrap();
    let address = loop {
        if let TransportEvent::NewAddress { listen_addr, .. } =
            poll_fn(|cx| Pin::new(&mut server).poll(cx)).await
        {
            break listen_addr;
        }
    };
    let dial = client
        .dial(
            address,
            DialOpts {
                role: Endpoint::Dialer,
                port_use: PortUse::Reuse,
            },
        )
        .unwrap();
    let dial = tokio::spawn(dial);
    let incoming = loop {
        if let TransportEvent::Incoming { upgrade, .. } =
            poll_fn(|cx| Pin::new(&mut server).poll(cx)).await
        {
            break upgrade;
        }
    };
    let (_, server_muxer) = incoming.await.unwrap();
    let (_, client_muxer) = dial.await.unwrap().unwrap();
    (server_muxer, client_muxer)
}

#[test]
fn quic_muxer_shares_directional_and_total_byte_budget_across_streams() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let (mut server, client) = quic_muxers().await;
                let window = WireByteWindow::new(WireByteLimits {
                    inbound: 4,
                    outbound: 4,
                    total: 6,
                })
                .unwrap();
                let counters = Arc::new(Libp2pWireByteCounters {
                    window: Some(Arc::new(window)),
                    ..Libp2pWireByteCounters::default()
                });
                let mut client = InstrumentedStreamMuxer::new(client, Arc::clone(&counters));
                let mut stream = poll_fn(|cx| Pin::new(&mut client).poll_outbound(cx))
                    .await
                    .unwrap();

                let writes = [IoSlice::new(b"abcdef"), IoSlice::new(b"gh")];
                assert_eq!(
                    poll_fn(|cx| Pin::new(&mut stream).poll_write_vectored(cx, &writes))
                        .await
                        .unwrap(),
                    4,
                );
                let mut receiver = poll_fn(|cx| Pin::new(&mut server).poll_inbound(cx))
                    .await
                    .unwrap();
                let mut received = [0; 4];
                receiver.read_exact(&mut received).await.unwrap();
                assert_eq!(&received, b"abcd");

                let mut second = poll_fn(|cx| Pin::new(&mut client).poll_outbound(cx))
                    .await
                    .unwrap();
                assert_eq!(second.write(&[]).await.unwrap(), 0);
                assert!(
                    tokio::time::timeout(Duration::from_millis(20), second.write(b"x"))
                        .await
                        .is_err()
                );

                receiver.write_all(b"hello").await.unwrap();
                let mut first = [0; 5];
                let mut second = [0; 3];
                let mut reads = [IoSliceMut::new(&mut first), IoSliceMut::new(&mut second)];
                assert_eq!(
                    poll_fn(|cx| Pin::new(&mut stream).poll_read_vectored(cx, &mut reads))
                        .await
                        .unwrap(),
                    2,
                );
                assert_eq!(&first, b"he\0\0\0");
                assert_eq!(second, [0; 3]);
                assert!(
                    tokio::time::timeout(Duration::from_millis(20), stream.read(&mut first))
                        .await
                        .is_err()
                );
                assert_eq!(
                    snapshot_wire_byte_counters(&counters),
                    Libp2pWireByteSnapshot {
                        inbound_bytes: 2,
                        outbound_bytes: 4,
                    },
                );
            })
            .await
            .expect("QUIC byte window probe timed out")
        });
}

#[test]
fn real_tcp_yamux_write_zero_refunds_only_the_failed_poll() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let tcp = |key: &Keypair| {
                    Transport::boxed(
                        libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default())
                            .upgrade(libp2p::core::upgrade::Version::V1)
                            .authenticate(libp2p::noise::Config::new(key).unwrap())
                            .multiplex(libp2p::yamux::Config::default()),
                    )
                };
                let mut server = tcp(&Keypair::generate_ed25519());
                let mut client = tcp(&Keypair::generate_ed25519());
                server
                    .listen_on(ListenerId::next(), "/ip4/127.0.0.1/tcp/0".parse().unwrap())
                    .unwrap();
                let address = loop {
                    if let TransportEvent::NewAddress { listen_addr, .. } =
                        poll_fn(|cx| Pin::new(&mut server).poll(cx)).await
                    {
                        break listen_addr;
                    }
                };
                let dial = client
                    .dial(
                        address,
                        DialOpts {
                            role: Endpoint::Dialer,
                            port_use: PortUse::Reuse,
                        },
                    )
                    .unwrap();
                let dial = tokio::spawn(dial);
                let incoming = loop {
                    if let TransportEvent::Incoming { upgrade, .. } =
                        poll_fn(|cx| Pin::new(&mut server).poll(cx)).await
                    {
                        break upgrade;
                    }
                };
                let (_, mut server_muxer) = incoming.await.unwrap();
                let (_, client_muxer) = dial.await.unwrap().unwrap();

                let bytes = Arc::new(
                    WireByteWindow::new(WireByteLimits {
                        inbound: 8,
                        outbound: 8,
                        total: 16,
                    })
                    .unwrap(),
                );
                let mut client = InstrumentedStreamMuxer::new_yamux(
                    client_muxer,
                    Libp2pWireByteCounters::with_window(Arc::clone(&bytes)),
                );
                let mut stream = poll_fn(|cx| Pin::new(&mut client).poll_outbound(cx))
                    .await
                    .unwrap();
                let drive_client = tokio::spawn(async move {
                    poll_fn(|cx| {
                        let _ = Pin::new(&mut client).poll(cx);
                        Poll::<()>::Pending
                    })
                    .await;
                });
                stream.write_all(b"seen").await.unwrap();
                let mut remote = poll_fn(|cx| Pin::new(&mut server_muxer).poll_inbound(cx))
                    .await
                    .unwrap();
                let mut received = [0; 4];
                remote.read_exact(&mut received).await.unwrap();
                assert_eq!(&received, b"seen");

                stream.close().await.unwrap();
                assert_eq!(
                    stream.write(b"not sent").await.unwrap_err().kind(),
                    io::ErrorKind::WriteZero
                );
                assert_eq!(bytes.snapshot().unwrap().charged_bytes.outbound, 4);
                let failed = [IoSlice::new(b"vector"), IoSlice::new(b"data")];
                assert_eq!(
                    poll_fn(|cx| Pin::new(&mut stream).poll_write_vectored(cx, &failed))
                        .await
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::WriteZero
                );
                let snapshot = bytes.snapshot().unwrap();
                assert_eq!(snapshot.actual_bytes.outbound, 4);
                assert_eq!(snapshot.charged_bytes.outbound, 4);
                assert_eq!(snapshot.uncertain_charge_bytes.outbound, 0);
                assert_eq!(snapshot.reserved_bytes.outbound, 0);
                drive_client.abort();
            })
            .await
            .expect("real Yamux write-zero fixture timed out");
        });
}
