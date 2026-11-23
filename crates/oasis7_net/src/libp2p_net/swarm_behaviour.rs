use super::rendezvous_client;
use libp2p::autonat;
use libp2p::core::muxing::StreamMuxerBox;
use libp2p::core::transport::OrTransport;
use libp2p::dcutr;
use libp2p::gossipsub::{self, MessageAuthenticity};
use libp2p::identity::Keypair;
use libp2p::kad::{self, store::MemoryStoreConfig};
use libp2p::multiaddr::Protocol;
use libp2p::noise;
use libp2p::relay;
use libp2p::rendezvous::server as rendezvous_server;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::{NetworkBehaviour, Swarm};
use libp2p::{Multiaddr, PeerId, StreamProtocol, Transport as _};

use oasis7_proto::distributed::RR_PROTOCOL_PREFIX;
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};

use super::connection_budget::ConnectionBudget;
use super::peer_record_admission::{PeerRecordAdmissionBudget, SharedPeerRecordAdmissionBudget};
use super::peer_record_dial_gate::DiscoveryDialPolicy;
use super::peer_record_protocol_dial::{ProtocolDialBehaviour, ProtocolDialOrigins};
use super::peer_record_store::PeerRecordStore;
use super::request_deadline::{LocalRequest, RequestCodec, deadline_in};
use super::transport_admission::TransportAdmissionGate;
use super::wire_bytes::{InstrumentedStreamMuxer, SharedLibp2pWireByteCounters};

pub(super) const KAD_MAX_PROVIDED_KEYS: usize = 65_536;

#[derive(NetworkBehaviour)]
#[behaviour(
    out_event = "BehaviourEvent",
    prelude = "libp2p::swarm::derive_prelude"
)]
pub(super) struct InnerBehaviour {
    pub(super) gossipsub: gossipsub::Behaviour,
    pub(super) request_response: request_response::Behaviour<RequestCodec>,
    pub(super) kademlia: kad::Behaviour<PeerRecordStore>,
    pub(super) autonat: Toggle<ProtocolDialBehaviour<autonat::Behaviour>>,
    pub(super) relay_client: ProtocolDialBehaviour<relay::client::Behaviour>,
    pub(super) dcutr: ProtocolDialBehaviour<dcutr::Behaviour>,
    pub(super) rendezvous_client: Toggle<rendezvous_client::Behaviour>,
    pub(super) rendezvous_server: Toggle<rendezvous_server::Behaviour>,
}

/// Filter at Swarm's final address resolution boundary: child protocols can
/// cache addresses internally, but those caches cannot extend signed leases.
pub(super) struct Behaviour {
    inner: InnerBehaviour,
    connection_budget: Option<ConnectionBudget>,
    admission_gate: TransportAdmissionGate,
    request_timeout: Option<std::time::Duration>,
    pub(super) discovery_budget: super::discovery_budget::DiscoveryWorkBudget,
    peer_record_admission_budget: SharedPeerRecordAdmissionBudget,
    dial_policy: DiscoveryDialPolicy,
    protocol_dials: ProtocolDialOrigins,
    automatic_dials: std::collections::HashSet<libp2p::swarm::ConnectionId>,
}

impl Behaviour {
    fn new(
        inner: InnerBehaviour,
        protocol_dials: ProtocolDialOrigins,
        peer_record_admission_budget: SharedPeerRecordAdmissionBudget,
        connection_budget: Option<ConnectionBudget>,
        admission_gate: TransportAdmissionGate,
        discovery_budget: super::discovery_budget::DiscoveryWorkBudget,
        request_timeout: Option<std::time::Duration>,
    ) -> Self {
        Self {
            inner,
            connection_budget,
            admission_gate,
            discovery_budget,
            request_timeout,
            peer_record_admission_budget,
            dial_policy: DiscoveryDialPolicy::default(),
            protocol_dials,
            automatic_dials: Default::default(),
        }
    }

    pub(super) fn send_rr_request(
        &mut self,
        peer: &PeerId,
        message: NetworkRequest,
        caller_deadline: Option<tokio::time::Instant>,
    ) -> request_response::OutboundRequestId {
        self.send_rr_request_with_exchange(peer, message, caller_deadline, None)
    }

    pub(super) fn send_rr_request_with_exchange(
        &mut self,
        peer: &PeerId,
        message: NetworkRequest,
        caller_deadline: Option<tokio::time::Instant>,
        exchange: Option<std::sync::Arc<super::encoded_exchange::Exchange>>,
    ) -> request_response::OutboundRequestId {
        // Legacy keeps its existing on-wire protocol timers. The command's
        // caller bound has already been applied to waiting and dispatch.
        let deadline = self.request_timeout.map(|timeout| {
            let configured = deadline_in(timeout);
            caller_deadline.map_or(configured, |caller| configured.min(caller))
        });
        let deadline = exchange.as_ref().map_or(deadline, |exchange| {
            let original = tokio::time::Instant::from_std(exchange.deadline());
            Some(deadline.map_or(original, |configured| configured.min(original)))
        });
        self.inner.request_response.send_request(
            peer,
            LocalRequest {
                message,
                deadline,
                exchange,
            },
        )
    }

    pub(super) fn note_handshake_deadline(&self) {
        if let Some(budget) = &self.connection_budget {
            budget.note_handshake_deadline();
        }
    }

    pub(super) fn discovery_dial_policy(&self) -> DiscoveryDialPolicy {
        self.dial_policy.clone()
    }

    pub(super) fn peer_record_admission_budget(&self) -> SharedPeerRecordAdmissionBudget {
        std::sync::Arc::clone(&self.peer_record_admission_budget)
    }
}

impl std::ops::Deref for Behaviour {
    type Target = InnerBehaviour;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl std::ops::DerefMut for Behaviour {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = <InnerBehaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = BehaviourEvent;

    fn handle_pending_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        let inner = &mut self.inner;
        self.admission_gate
            .handle_pending_inbound(id, local, remote, || {
                inner.handle_pending_inbound_connection(id, local, remote)
            })
    }
    fn handle_established_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        let admission = self
            .connection_budget
            .as_ref()
            .map(|budget| budget.authorize_inbound(id, peer))
            .transpose()?;
        let handler = self
            .inner
            .handle_established_inbound_connection(id, peer, local, remote)?;
        if let Some(admission) = admission {
            admission.establish()?;
        }
        Ok(handler)
    }
    fn handle_pending_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: Option<PeerId>,
        addresses: &[Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<Multiaddr>, libp2p::swarm::ConnectionDenied> {
        let automatic = self.automatic_dials.remove(&id);
        // Negotiated hole-punch/AutoNAT callbacks and relay transport continuations
        // authorize this operation's original endpoint once, never cached additions.
        let protocol_owned = self.protocol_dials.take(id);
        // Original automatic opts cannot be rewritten here; rejecting the
        // attempt ensures no unauthorized original address reaches transport.
        if automatic
            && !protocol_owned
            && addresses
                .iter()
                .any(|addr| !self.dial_policy.permits(peer, addr))
        {
            return Err(libp2p::swarm::ConnectionDenied::new(std::io::Error::other(
                "automatic dial lacks current signed or configured endpoint",
            )));
        }
        let augmented = self
            .inner
            .handle_pending_outbound_connection(id, peer, addresses, role)?;
        let filtered = self.dial_policy.filter(peer, augmented);
        self.admission_gate
            .handle_pending_outbound(id, peer, addresses, filtered)
    }
    fn handle_established_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: PeerId,
        address: &Multiaddr,
        role: libp2p::core::Endpoint,
        port: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        let admission = self
            .connection_budget
            .as_ref()
            .map(|budget| budget.authorize_outbound(id, address, peer))
            .transpose()?;
        let handler = self
            .inner
            .handle_established_outbound_connection(id, peer, address, role, port)?;
        if let Some(admission) = admission {
            admission.establish()?;
        }
        Ok(handler)
    }
    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm<'_>) {
        self.admission_gate.on_swarm_event(event);
        if let libp2p::swarm::FromSwarm::DialFailure(failure) = event {
            self.automatic_dials.remove(&failure.connection_id);
            self.protocol_dials.cleanup(failure.connection_id);
        }
        self.inner.on_swarm_event(event);
    }
    fn on_connection_handler_event(
        &mut self,
        peer: PeerId,
        id: libp2p::swarm::ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.inner.on_connection_handler_event(peer, id, event);
    }
    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.admission_gate.clear_outbound_session();
        let action = self.inner.poll(cx);
        if let std::task::Poll::Ready(libp2p::swarm::ToSwarm::Dial { opts }) = &action {
            self.automatic_dials.insert(opts.connection_id());
        }
        action
    }
}

#[derive(Debug)]
pub(super) enum BehaviourEvent {
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<LocalRequest, NetworkResponse>),
    Kademlia(kad::Event),
    Autonat(autonat::Event),
    RelayClient(relay::client::Event),
    Dcutr(dcutr::Event),
    RendezvousClient(rendezvous_client::Event),
    RendezvousServer(rendezvous_server::Event),
}

impl From<gossipsub::Event> for BehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        BehaviourEvent::Gossipsub(event)
    }
}

impl From<request_response::Event<LocalRequest, NetworkResponse>> for BehaviourEvent {
    fn from(event: request_response::Event<LocalRequest, NetworkResponse>) -> Self {
        BehaviourEvent::RequestResponse(event)
    }
}

impl From<kad::Event> for BehaviourEvent {
    fn from(event: kad::Event) -> Self {
        BehaviourEvent::Kademlia(event)
    }
}

impl From<autonat::Event> for BehaviourEvent {
    fn from(event: autonat::Event) -> Self {
        BehaviourEvent::Autonat(event)
    }
}

impl From<relay::client::Event> for BehaviourEvent {
    fn from(event: relay::client::Event) -> Self {
        BehaviourEvent::RelayClient(event)
    }
}

impl From<dcutr::Event> for BehaviourEvent {
    fn from(event: dcutr::Event) -> Self {
        BehaviourEvent::Dcutr(event)
    }
}

impl From<rendezvous_client::Event> for BehaviourEvent {
    fn from(event: rendezvous_client::Event) -> Self {
        BehaviourEvent::RendezvousClient(event)
    }
}

impl From<rendezvous_server::Event> for BehaviourEvent {
    fn from(event: rendezvous_server::Event) -> Self {
        BehaviourEvent::RendezvousServer(event)
    }
}

#[cfg(test)]
pub(super) fn build_swarm(
    keypair: &Keypair,
    enable_rendezvous: bool,
    enable_autonat: bool,
    request_response_timeout: std::time::Duration,
    wire_byte_counters: SharedLibp2pWireByteCounters,
) -> Swarm<Behaviour> {
    build_swarm_with_receiver(
        keypair,
        enable_rendezvous,
        enable_autonat,
        request_response_timeout,
        wire_byte_counters,
        super::receiver_floor::PeerRecordReceiver::legacy(),
        None,
    )
}

pub(super) fn build_swarm_with_receiver(
    keypair: &Keypair,
    enable_rendezvous: bool,
    enable_autonat: bool,
    request_response_timeout: std::time::Duration,
    wire_byte_counters: SharedLibp2pWireByteCounters,
    peer_record_receiver: super::receiver_floor::SharedPeerRecordReceiver,
    connection_budget: Option<ConnectionBudget>,
) -> Swarm<Behaviour> {
    let swarm_config = libp2p::swarm::Config::with_tokio_executor()
        .with_idle_connection_timeout(std::time::Duration::from_secs(30));
    let admission_gate = connection_budget
        .as_ref()
        .map(ConnectionBudget::admission_gate)
        .unwrap_or_default();

    let peer_id = PeerId::from(keypair.public());
    let gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(keypair.clone()),
        gossipsub::Config::default(),
    )
    .expect("gossipsub config");

    let protocols = vec![(
        StreamProtocol::new(RR_PROTOCOL_PREFIX),
        ProtocolSupport::Full,
    )];
    let request_response = request_response::Behaviour::with_codec(
        RequestCodec::default(),
        protocols,
        request_response::Config::default().with_request_timeout(request_response_timeout),
    );

    let store_config = MemoryStoreConfig {
        max_provided_keys: KAD_MAX_PROVIDED_KEYS,
        ..MemoryStoreConfig::default()
    };
    let peer_record_admission_budget = PeerRecordAdmissionBudget::shared();
    let store = PeerRecordStore::with_admission_budget_and_receiver(
        peer_id,
        store_config,
        std::sync::Arc::clone(&peer_record_admission_budget),
        std::sync::Arc::clone(&peer_record_receiver),
    );
    let kademlia = kad::Behaviour::new(peer_id, store);
    let (relay_transport, relay_client) = relay::client::new(peer_id);

    let protocol_dials = ProtocolDialOrigins::default();
    let inner = InnerBehaviour {
        gossipsub,
        request_response,
        kademlia,
        autonat: Toggle::from(enable_autonat.then(|| {
            ProtocolDialBehaviour::new(
                autonat::Behaviour::new(peer_id, Default::default()),
                protocol_dials.clone(),
            )
        })),
        relay_client: ProtocolDialBehaviour::new(relay_client, protocol_dials.clone()),
        dcutr: ProtocolDialBehaviour::new(dcutr::Behaviour::new(peer_id), protocol_dials.clone()),
        rendezvous_client: Toggle::from(enable_rendezvous.then(|| {
            rendezvous_client::Behaviour::new(keypair.clone())
                .with_automatic_registration_refresh(false)
        })),
        rendezvous_server: Toggle::from(
            enable_rendezvous.then(|| rendezvous_server::Behaviour::new(Default::default())),
        ),
    };

    let wire_window = wire_byte_counters.window();
    let request_timeout = wire_window.as_ref().map(|_| request_response_timeout);
    let behaviour = Behaviour::new(
        inner,
        protocol_dials,
        peer_record_admission_budget,
        connection_budget,
        admission_gate.clone(),
        super::discovery_budget::DiscoveryWorkBudget::with_wire_window(wire_window),
        request_timeout,
    );

    let quic_counters = wire_byte_counters.clone();
    let quic_transport = libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(keypair))
        .map(move |(peer, muxer), _| {
            (
                peer,
                StreamMuxerBox::new(InstrumentedStreamMuxer::new(muxer, quic_counters.clone())),
            )
        });
    let tcp_counters = wire_byte_counters.clone();
    let tcp_transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default())
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(noise::Config::new(keypair).expect("noise config"))
        .multiplex(libp2p::yamux::Config::default())
        .map(move |(peer, muxer), _| {
            (
                peer,
                StreamMuxerBox::new(InstrumentedStreamMuxer::new_yamux(
                    muxer,
                    tcp_counters.clone(),
                )),
            )
        });
    let direct_transport =
        OrTransport::new(quic_transport, tcp_transport).map(|output, _| output.into_inner());
    let relay_transport = relay_transport
        .upgrade(libp2p::core::upgrade::Version::V1Lazy)
        .authenticate(noise::Config::new(keypair).expect("relay noise config"))
        .multiplex(libp2p::yamux::Config::default())
        .map(move |(peer, muxer), _| {
            (
                peer,
                StreamMuxerBox::new(InstrumentedStreamMuxer::new_yamux(
                    muxer,
                    wire_byte_counters.clone(),
                )),
            )
        });
    let base_transport = OrTransport::new(direct_transport, relay_transport)
        .map(|output, _| output.into_inner())
        .boxed();
    let transport = admission_gate.wrap_transport(base_transport).boxed();

    Swarm::new(transport, behaviour, peer_id, swarm_config)
}

/// Targeted discovery reuses an established connection under the shared
/// profile. A stale peer-list entry must not turn work admission into a dial.
pub(super) fn may_reuse_discovery_connection(swarm: &Swarm<Behaviour>, peer: PeerId) -> bool {
    swarm.behaviour().connection_budget.is_none() || swarm.is_connected(&peer)
}

pub(super) fn dial_addr_with_optional_peer_id(
    swarm: &mut Swarm<Behaviour>,
    addr: Multiaddr,
) -> Result<libp2p::swarm::ConnectionId, Box<dyn std::error::Error + Send + Sync>> {
    let (peer_id, dial_addr) = split_peer_id(addr);
    let opts = if let Some(peer_id) = peer_id {
        swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, dial_addr.clone());
        libp2p::swarm::dial_opts::DialOpts::peer_id(peer_id)
            .addresses(vec![dial_addr])
            .build()
    } else {
        libp2p::swarm::dial_opts::DialOpts::unknown_peer_id()
            .address(dial_addr)
            .build()
    };
    let connection_id = opts.connection_id();
    swarm.dial(opts)?;
    Ok(connection_id)
}

/// Dial an already accepted record endpoint without persisting it in routing caches.
pub(super) fn dial_record_addr(
    swarm: &mut Swarm<Behaviour>,
    addr: Multiaddr,
) -> Result<libp2p::swarm::ConnectionId, Box<dyn std::error::Error + Send + Sync>> {
    let (peer, addr) = split_peer_id(addr);
    let opts = if let Some(peer) = peer {
        libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
            .addresses(vec![addr])
            .build()
    } else {
        libp2p::swarm::dial_opts::DialOpts::unknown_peer_id()
            .address(addr)
            .build()
    };
    // Record-driven application dials receive the same final freshness check as
    // child protocol dials; only explicit local caller actions bypass it.
    let connection_id = opts.connection_id();
    swarm.behaviour_mut().automatic_dials.insert(connection_id);
    swarm.dial(opts)?;
    Ok(connection_id)
}

pub(super) fn split_peer_id(mut addr: Multiaddr) -> (Option<PeerId>, Multiaddr) {
    let peer_id = match addr.pop() {
        Some(Protocol::P2p(peer)) => Some(peer),
        Some(protocol) => {
            addr.push(protocol);
            None
        }
        None => None,
    };
    (peer_id, addr)
}

pub(super) fn ensure_peer_id(mut addr: Multiaddr, peer_id: PeerId) -> Multiaddr {
    let needs_peer_id = !matches!(addr.iter().last(), Some(Protocol::P2p(_)));
    if needs_peer_id {
        addr.push(Protocol::P2p(peer_id));
    }
    addr
}

#[cfg(test)]
mod dial_gate_tests {
    use super::*;
    use libp2p::core::Endpoint;
    use libp2p::swarm::ConnectionId;

    #[test]
    fn validated_child_original_exception_is_one_shot_and_never_trusts_cached_augmentation() {
        for role in [Endpoint::Dialer, Endpoint::Listener] {
            let keypair = Keypair::generate_ed25519();
            let mut swarm = build_swarm(
                &keypair,
                false,
                false,
                std::time::Duration::from_secs(30),
                super::super::wire_bytes::init_shared_wire_byte_counters(),
            );
            let peer = PeerId::random();
            let original: Multiaddr = "/ip4/127.0.0.1/tcp/4100".parse().unwrap();
            let stale: Multiaddr = "/ip4/127.0.0.1/tcp/4101".parse().unwrap();
            let builder = libp2p::swarm::dial_opts::DialOpts::peer_id(peer)
                .addresses(vec![original.clone()])
                .extend_addresses_through_behaviour();
            let opts = if role == Endpoint::Listener {
                builder.override_role().build()
            } else {
                builder.build()
            };
            let id = opts.connection_id();
            let behaviour = swarm.behaviour_mut();
            behaviour.kademlia.add_address(&peer, stale.clone());
            super::super::peer_record_protocol_dial::emit_test_dial(
                behaviour.protocol_dials.clone(),
                opts,
            );
            behaviour.automatic_dials.insert(id);
            assert!(
                behaviour
                    .handle_pending_outbound_connection(
                        id,
                        Some(peer),
                        std::slice::from_ref(&original),
                        role
                    )
                    .unwrap()
                    .is_empty()
            );
            // Reusing either this ID or the peer does not reuse protocol authority.
            behaviour.automatic_dials.insert(id);
            assert!(
                behaviour
                    .handle_pending_outbound_connection(
                        id,
                        Some(peer),
                        std::slice::from_ref(&original),
                        role
                    )
                    .is_err()
            );
            let untagged = ConnectionId::new_unchecked(999);
            behaviour.automatic_dials.insert(untagged);
            assert!(
                behaviour
                    .handle_pending_outbound_connection(untagged, Some(peer), &[stale], role)
                    .is_err()
            );
        }
    }

    #[test]
    fn final_resolution_filters_cached_kad_addresses_and_denies_automatic_originals() {
        let keypair = Keypair::generate_ed25519();
        let mut swarm = build_swarm(
            &keypair,
            false,
            false,
            std::time::Duration::from_secs(30),
            super::super::wire_bytes::init_shared_wire_byte_counters(),
        );
        let peer = PeerId::random();
        let cached: Multiaddr = "/ip4/127.0.0.1/tcp/4100".parse().unwrap();
        let behaviour = swarm.behaviour_mut();
        behaviour.kademlia.add_address(&peer, cached.clone());
        // Kademlia still owns its cached route; final resolution excludes it.
        assert!(
            behaviour
                .handle_pending_outbound_connection(
                    ConnectionId::new_unchecked(1),
                    Some(peer),
                    &[],
                    Endpoint::Dialer,
                )
                .unwrap()
                .is_empty()
        );
        let automatic = ConnectionId::new_unchecked(2);
        behaviour.automatic_dials.insert(automatic);
        assert!(
            behaviour
                .handle_pending_outbound_connection(
                    automatic,
                    Some(peer),
                    std::slice::from_ref(&cached),
                    Endpoint::Dialer,
                )
                .is_err()
        );
        assert!(!behaviour.automatic_dials.contains(&automatic));
        // Explicit caller addresses remain the caller's action; only automatic
        // augmentation is filtered, and no new trust is installed by this call.
        assert!(
            behaviour
                .handle_pending_outbound_connection(
                    ConnectionId::new_unchecked(3),
                    Some(peer),
                    std::slice::from_ref(&cached),
                    Endpoint::Dialer,
                )
                .is_ok()
        );
        let mut configured = cached.clone();
        configured.push(Protocol::P2p(peer));
        behaviour
            .discovery_dial_policy()
            .set_configured(&[configured]);
        let resolved = behaviour
            .handle_pending_outbound_connection(
                ConnectionId::new_unchecked(4),
                Some(peer),
                &[],
                Endpoint::Dialer,
            )
            .unwrap();
        assert!(
            resolved
                .iter()
                .any(|addr| split_peer_id(addr.clone()).1 == cached)
        );
    }
}

#[cfg(test)]
#[path = "swarm_protocol_budget_tests.rs"]
mod protocol_budget_tests;
