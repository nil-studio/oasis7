//! A local protocol fixture sends authenticated peer records with untrusted TTLs.
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
use libp2p::{
    Swarm, Transport,
    core::{PeerRecord, upgrade},
    identity::Keypair,
    noise, rendezvous,
    request_response::{self, ProtocolSupport},
    swarm::{NetworkBehaviour, SwarmEvent},
    yamux,
};
use std::{io, time::Duration};

#[derive(Clone, Default)]
struct WireCodec;

async fn read_frame<T: AsyncRead + Unpin + Send>(io: &mut T) -> io::Result<Vec<u8>> {
    let mut len = 0usize;
    for shift in (0..35).step_by(7) {
        let mut b = [0];
        io.read_exact(&mut b).await?;
        len |= usize::from(b[0] & 127) << shift;
        if b[0] & 128 == 0 {
            if len > 1024 * 1024 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut bytes = vec![0; len];
            io.read_exact(&mut bytes).await?;
            return Ok(bytes);
        }
    }
    Err(io::ErrorKind::InvalidData.into())
}

impl request_response::Codec for WireCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;
    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Vec<u8>> {
        read_frame(io).await
    }
    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Vec<u8>> {
        read_frame(io).await
    }
    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        bytes: Vec<u8>,
    ) -> io::Result<()> {
        write_frame(io, bytes).await
    }
    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        bytes: Vec<u8>,
    ) -> io::Result<()> {
        write_frame(io, bytes).await
    }
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
async fn write_frame<T: AsyncWrite + Unpin + Send>(io: &mut T, bytes: Vec<u8>) -> io::Result<()> {
    let mut frame = Vec::new();
    varint(bytes.len() as u64, &mut frame);
    frame.extend(bytes);
    io.write_all(&frame).await?;
    io.close().await
}
fn field(tag: u8, bytes: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}
fn discovery_response(record: &PeerRecord, ttl: u64) -> Vec<u8> {
    let mut registration = Vec::new();
    field(10, b"ttl-regression", &mut registration);
    field(
        18,
        &record
            .clone()
            .into_signed_envelope()
            .into_protobuf_encoding(),
        &mut registration,
    );
    registration.push(24);
    varint(ttl, &mut registration);
    let mut response = Vec::new();
    field(10, &registration, &mut response);
    field(
        18,
        &rendezvous::Cookie::for_all_namespaces().into_wire_encoding(),
        &mut response,
    );
    response.extend([24, 0]);
    let mut message = vec![8, 4];
    field(50, &response, &mut message);
    message
}
fn swarm<B: NetworkBehaviour>(key: &Keypair, behaviour: B) -> Swarm<B> {
    let transport = libp2p::core::transport::MemoryTransport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(key).unwrap())
        .multiplex(yamux::Config::default())
        .boxed();
    Swarm::new(
        transport,
        behaviour,
        key.public().to_peer_id(),
        libp2p::swarm::Config::with_tokio_executor(),
    )
}

#[test]
fn discovery_ttl_boundaries_survive_timer_poll_and_subsequent_request() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), async {
            let server_key = Keypair::generate_ed25519();
            let client_key = Keypair::generate_ed25519();
            let peer_key = Keypair::generate_ed25519();
            let record = PeerRecord::new(&peer_key, vec!["/memory/77".parse().unwrap()]).unwrap();
            let mut server = swarm(
                &server_key,
                request_response::Behaviour::<WireCodec>::new(
                    [(rendezvous::PROTOCOL_NAME, ProtocolSupport::Full)],
                    request_response::Config::default(),
                ),
            );
            let mut client = swarm(
                &client_key,
                super::super::rendezvous_client::Behaviour::new(client_key.clone()),
            );
            server.listen_on("/memory/0".parse().unwrap()).unwrap();
            let address = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                    break address;
                }
            };
            client.dial(address).unwrap();
            loop {
                if let futures::future::Either::Left((
                    SwarmEvent::ConnectionEstablished { .. },
                    _,
                )) =
                    futures::future::select(client.select_next_some(), server.select_next_some())
                        .await
                {
                    break;
                }
            }
            let ttls = [1, 31_556_951, 31_556_952, 31_556_953, u64::MAX, 2];
            let mut sent = 0;
            let mut received = 0;
            client
                .behaviour_mut()
                .discover(None, None, None, server_key.public().to_peer_id());
            while received < ttls.len() {
                // Re-entering this loop polls the expiry futures before delivering the next
                // response; checking only Discovered would miss the original panic.
                match futures::future::select(client.select_next_some(), server.select_next_some())
                    .await
                {
                    futures::future::Either::Left((event, _)) => match event {
                        SwarmEvent::Behaviour(
                            super::super::rendezvous_client::Event::Discovered {
                                registrations,
                                ..
                            },
                        ) => {
                            assert_eq!(registrations[0].ttl, ttls[received]);
                            assert_eq!(
                                registrations[0].record.peer_id(),
                                peer_key.public().to_peer_id()
                            );
                            received += 1;
                            if received < ttls.len() {
                                client.behaviour_mut().discover(
                                    None,
                                    None,
                                    None,
                                    server_key.public().to_peer_id(),
                                );
                            }
                        }
                        SwarmEvent::Behaviour(
                            super::super::rendezvous_client::Event::DiscoverFailed {
                                error, ..
                            },
                        ) => panic!("fixture rejected: {error:?}"),
                        _ => {}
                    },
                    futures::future::Either::Right((
                        SwarmEvent::Behaviour(request_response::Event::Message {
                            message: request_response::Message::Request { channel, .. },
                            ..
                        }),
                        _,
                    )) => {
                        server
                            .behaviour_mut()
                            .send_response(channel, discovery_response(&record, ttls[sent]))
                            .unwrap();
                        sent += 1;
                    }
                    _ => {}
                }
            }
            assert_eq!(sent, ttls.len());
        })
        .await
        .expect("local rendezvous worker keeps responding");
    });
}
