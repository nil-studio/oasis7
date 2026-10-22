use futures::StreamExt;
use libp2p::{
    gossipsub, identity, noise, request_response,
    swarm::{NetworkBehaviour, SwarmEvent},
    yamux,
};
use std::time::Duration;

#[derive(NetworkBehaviour)]
struct Behaviour {
    rpc: request_response::cbor::Behaviour<Vec<u8>, Vec<u8>>,
    gossip: gossipsub::Behaviour,
}

use oasis7_libp2p_compat_protocol::{Adapter, Event, IDENTITY_MESSAGE, RPC_PROTOCOL, TOPIC};

pub(crate) struct Network {
    swarm: libp2p::Swarm<Behaviour>,
    topic: gossipsub::IdentTopic,
}

impl Adapter for Network {
    type PeerId = libp2p::PeerId;
    type ResponseChannel = request_response::ResponseChannel<Vec<u8>>;
    type PublishError = gossipsub::PublishError;

    fn identity() {
        let key = identity::Keypair::ed25519_from_bytes([42; 32]).unwrap();
        println!("{}", key.public().to_peer_id());
        let signature = key.sign(IDENTITY_MESSAGE).unwrap();
        assert!(key.public().verify(IDENTITY_MESSAGE, &signature));
    }

    fn addresses(addresses: &[String]) {
        for address in addresses {
            println!("{}", address.parse::<libp2p::Multiaddr>().unwrap());
        }
    }

    fn new(server: bool, address: &str) -> Self {
        let key = identity::Keypair::generate_ed25519();
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .unwrap()
            .with_quic()
            .with_behaviour(|key| Behaviour {
                rpc: request_response::cbor::Behaviour::new(
                    [(
                        libp2p::StreamProtocol::new(RPC_PROTOCOL),
                        request_response::ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
                gossip: gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub::ConfigBuilder::default()
                        .heartbeat_interval(Duration::from_millis(100))
                        .flood_publish(true)
                        .build()
                        .unwrap(),
                )
                .unwrap(),
            })
            .unwrap()
            .build();
        let topic = gossipsub::IdentTopic::new(TOPIC);
        swarm.behaviour_mut().gossip.subscribe(&topic).unwrap();
        if server {
            let address = if address == "quic" {
                "/ip4/127.0.0.1/udp/0/quic-v1"
            } else {
                "/ip4/127.0.0.1/tcp/0"
            };
            swarm.listen_on(address.parse().unwrap()).unwrap();
        } else {
            swarm
                .dial(address.parse::<libp2p::Multiaddr>().unwrap())
                .unwrap();
        }
        Self { swarm, topic }
    }

    async fn next(&mut self) -> Event<Self::PeerId, Self::ResponseChannel> {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    return Event::Listening(format!(
                        "{address}/p2p/{}",
                        self.swarm.local_peer_id()
                    ));
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    return Event::Connected(peer_id);
                }
                SwarmEvent::Behaviour(BehaviourEvent::Rpc(request_response::Event::Message {
                    message,
                    ..
                })) => {
                    return match message {
                        request_response::Message::Request {
                            request, channel, ..
                        } => Event::Request(request, channel),
                        request_response::Message::Response { response, .. } => {
                            Event::Response(response)
                        }
                    };
                }
                SwarmEvent::Behaviour(BehaviourEvent::Gossip(gossipsub::Event::Subscribed {
                    peer_id,
                    ..
                })) => return Event::Subscribed(peer_id),
                SwarmEvent::Behaviour(BehaviourEvent::Gossip(gossipsub::Event::Message {
                    message,
                    ..
                })) => return Event::Gossip(message.data),
                SwarmEvent::Behaviour(BehaviourEvent::Rpc(
                    request_response::Event::OutboundFailure { error, .. },
                )) => return Event::OutboundFailure(error.to_string()),
                _ => {}
            }
        }
    }

    fn request(&mut self, peer: &Self::PeerId, bytes: Vec<u8>) {
        self.swarm.behaviour_mut().rpc.send_request(peer, bytes);
    }
    fn respond(&mut self, channel: Self::ResponseChannel, bytes: Vec<u8>) {
        self.swarm
            .behaviour_mut()
            .rpc
            .send_response(channel, bytes)
            .unwrap();
    }
    fn publish(&mut self, bytes: Vec<u8>) -> Result<(), Self::PublishError> {
        self.swarm
            .behaviour_mut()
            .gossip
            .publish(self.topic.clone(), bytes)
            .map(|_| ())
    }
}
