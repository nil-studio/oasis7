//! Oasis7 thin Rendezvous client behaviour over `/rendezvous/1.0.0`.
//!
//! Preserves exact wire compatibility with upstream `libp2p::rendezvous` while
//! exposing outbound request IDs, optional automatic external-address refresh,
//! and fail-closed `InternalError` termination on unexpected response kinds.

use std::{
    collections::{HashMap, VecDeque},
    fmt, io, iter,
    task::{Context, Poll},
    time::Duration,
};

use futures::{
    AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
    future::{BoxFuture, FutureExt},
    stream::{FuturesUnordered, StreamExt},
};
use libp2p::{
    StreamProtocol,
    core::{Endpoint, Multiaddr, PeerRecord, SignedEnvelope, transport::PortUse},
    identity::{Keypair, PeerId, SigningError},
    rendezvous::{Cookie, ErrorCode, Namespace, PROTOCOL_NAME, Registration, Ttl},
    request_response::{self, OutboundRequestId, ProtocolSupport},
    swarm::{
        ConnectionDenied, ConnectionId, ExternalAddresses, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
};
use prost::Message as _;

const MAX_MESSAGE_LEN_BYTES: usize = 1024 * 1024;
const MAX_DISCOVERED_TTL_SECS: u64 = 31_556_952;

pub(super) struct Behaviour {
    events: VecDeque<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>,
    inner: request_response::Behaviour<Codec>,
    keypair: Keypair,
    automatic_registration_refresh: bool,
    waiting_for_register: HashMap<OutboundRequestId, (PeerId, Namespace)>,
    waiting_for_discovery: HashMap<OutboundRequestId, (PeerId, Option<Namespace>)>,
    discovered_peers: HashMap<PeerId, HashMap<Namespace, Vec<Multiaddr>>>,
    registered_namespaces: HashMap<(PeerId, Namespace), Ttl>,
    expiring_registrations: FuturesUnordered<BoxFuture<'static, (PeerId, Namespace)>>,
    external_addresses: ExternalAddresses,
}

impl Behaviour {
    pub(super) fn new(keypair: Keypair) -> Self {
        Self {
            events: VecDeque::new(),
            inner: request_response::Behaviour::with_codec(
                Codec,
                iter::once((PROTOCOL_NAME, ProtocolSupport::Outbound)),
                request_response::Config::default(),
            ),
            keypair,
            automatic_registration_refresh: true,
            waiting_for_register: HashMap::new(),
            waiting_for_discovery: HashMap::new(),
            discovered_peers: HashMap::new(),
            registered_namespaces: HashMap::new(),
            expiring_registrations: FuturesUnordered::from_iter(vec![
                futures::future::pending().boxed(),
            ]),
            external_addresses: ExternalAddresses::default(),
        }
    }

    /// Enable or disable registration refreshes triggered by external-address changes.
    ///
    /// Address tracking remains enabled, and address events continue to be forwarded to the
    /// inner request-response behaviour regardless of this setting.
    pub(super) fn with_automatic_registration_refresh(mut self, enabled: bool) -> Self {
        self.automatic_registration_refresh = enabled;
        self
    }

    pub(super) fn external_addresses(&self) -> impl Iterator<Item = &Multiaddr> {
        self.external_addresses.iter()
    }

    pub(super) fn register(
        &mut self,
        namespace: Namespace,
        rendezvous_node: PeerId,
        ttl: Option<Ttl>,
    ) -> Result<(), RegisterError> {
        self.register_with_request_id(namespace, rendezvous_node, ttl)
            .map(|_| ())
    }

    pub(super) fn register_with_request_id(
        &mut self,
        namespace: Namespace,
        rendezvous_node: PeerId,
        ttl: Option<Ttl>,
    ) -> Result<OutboundRequestId, RegisterError> {
        let external_addresses = self.external_addresses.iter().cloned().collect::<Vec<_>>();
        if external_addresses.is_empty() {
            return Err(RegisterError::NoExternalAddresses);
        }

        let peer_record = PeerRecord::new(&self.keypair, external_addresses)?;
        let req_id = self.inner.send_request(
            &rendezvous_node,
            Message::Register(NewRegistration {
                namespace: namespace.clone(),
                record: peer_record,
                ttl,
            }),
        );
        self.waiting_for_register
            .insert(req_id, (rendezvous_node, namespace));

        Ok(req_id)
    }

    #[allow(dead_code)]
    pub(super) fn unregister(&mut self, namespace: Namespace, rendezvous_node: PeerId) {
        self.registered_namespaces
            .retain(|(rz_node, ns), _| !(rz_node == &rendezvous_node && ns == &namespace));

        self.inner
            .send_request(&rendezvous_node, Message::Unregister(namespace));
    }

    #[allow(dead_code)]
    pub(super) fn discover(
        &mut self,
        namespace: Option<Namespace>,
        cookie: Option<Cookie>,
        limit: Option<u64>,
        rendezvous_node: PeerId,
    ) {
        self.discover_with_request_id(namespace, cookie, limit, rendezvous_node);
    }

    pub(super) fn discover_with_request_id(
        &mut self,
        namespace: Option<Namespace>,
        cookie: Option<Cookie>,
        limit: Option<u64>,
        rendezvous_node: PeerId,
    ) -> OutboundRequestId {
        let req_id = self.inner.send_request(
            &rendezvous_node,
            Message::Discover {
                namespace: namespace.clone(),
                cookie,
                limit,
            },
        );
        self.waiting_for_discovery
            .insert(req_id, (rendezvous_node, namespace));
        req_id
    }
}

#[derive(Debug)]
pub(super) enum RegisterError {
    NoExternalAddresses,
    FailedToMakeRecord(SigningError),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoExternalAddresses => {
                f.write_str("We don't know about any externally reachable addresses of ours")
            }
            Self::FailedToMakeRecord(_) => f.write_str("Failed to make a new PeerRecord"),
        }
    }
}

impl std::error::Error for RegisterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoExternalAddresses => None,
            Self::FailedToMakeRecord(err) => Some(err),
        }
    }
}

impl From<SigningError> for RegisterError {
    fn from(err: SigningError) -> Self {
        Self::FailedToMakeRecord(err)
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(super) enum Event {
    Discovered {
        request_id: OutboundRequestId,
        rendezvous_node: PeerId,
        registrations: Vec<Registration>,
        cookie: Cookie,
    },
    DiscoverFailed {
        request_id: OutboundRequestId,
        rendezvous_node: PeerId,
        namespace: Option<Namespace>,
        error: ErrorCode,
    },
    Registered {
        request_id: OutboundRequestId,
        rendezvous_node: PeerId,
        ttl: Ttl,
        #[cfg_attr(
            not(test),
            expect(
                dead_code,
                reason = "Preserves upstream Event::Registered namespace field for protocol tests"
            )
        )]
        namespace: Namespace,
    },
    RegisterFailed {
        request_id: OutboundRequestId,
        rendezvous_node: PeerId,
        namespace: Namespace,
        error: ErrorCode,
    },
    Expired {
        peer: PeerId,
    },
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        <request_response::Behaviour<Codec> as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event);
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        let changed = self.external_addresses.on_swarm_event(&event);
        self.inner.on_swarm_event(event);

        if self.automatic_registration_refresh
            && changed
            && self.external_addresses.iter().count() > 0
        {
            let registered = self.registered_namespaces.clone();
            for ((rz_node, ns), ttl) in registered {
                if let Err(err) = self.register(ns, rz_node, Some(ttl)) {
                    tracing::warn!("refreshing registration failed: {err}");
                }
            }
        }
    }

    #[tracing::instrument(level = "trace", name = "NetworkBehaviour::poll", skip(self, cx))]
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Poll::Ready(event);
            }

            match self.inner.poll(cx) {
                Poll::Ready(ToSwarm::GenerateEvent(request_response::Event::Message {
                    message:
                        request_response::Message::Response {
                            request_id,
                            response,
                        },
                    ..
                })) => {
                    if let Some(event) = self.handle_response(&request_id, response) {
                        return Poll::Ready(ToSwarm::GenerateEvent(event));
                    }
                    continue;
                }
                Poll::Ready(ToSwarm::GenerateEvent(request_response::Event::OutboundFailure {
                    request_id,
                    ..
                })) => {
                    if let Some(event) = self.event_for_outbound_failure(&request_id) {
                        return Poll::Ready(ToSwarm::GenerateEvent(event));
                    }
                    continue;
                }
                Poll::Ready(ToSwarm::GenerateEvent(
                    request_response::Event::InboundFailure { .. }
                    | request_response::Event::ResponseSent { .. }
                    | request_response::Event::Message {
                        message: request_response::Message::Request { .. },
                        ..
                    },
                )) => {
                    tracing::debug!(
                        "ignoring unexpected inbound event on outbound-only rendezvous client"
                    );
                    continue;
                }
                Poll::Ready(other) => {
                    let new_to_swarm =
                        other.map_out(|_| unreachable!("GenerateEvent variants handled above"));
                    return Poll::Ready(new_to_swarm);
                }
                Poll::Pending => {}
            }

            if let Poll::Ready(Some((peer, expired_registration))) =
                self.expiring_registrations.poll_next_unpin(cx)
            {
                let Some(registrations) = self.discovered_peers.get_mut(&peer) else {
                    continue;
                };
                registrations.remove(&expired_registration);
                if registrations.is_empty() {
                    self.discovered_peers.remove(&peer);
                }
                return Poll::Ready(ToSwarm::GenerateEvent(Event::Expired { peer }));
            }

            return Poll::Pending;
        }
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let addrs = maybe_peer
            .map(|peer| self.discovered_peer_addrs(&peer).cloned().collect())
            .unwrap_or_default();
        Ok(addrs)
    }
}

impl Behaviour {
    fn event_for_outbound_failure(&mut self, req_id: &OutboundRequestId) -> Option<Event> {
        if let Some((rendezvous_node, namespace)) = self.waiting_for_register.remove(req_id) {
            return Some(Event::RegisterFailed {
                request_id: *req_id,
                rendezvous_node,
                namespace,
                error: ErrorCode::Unavailable,
            });
        }

        if let Some((rendezvous_node, namespace)) = self.waiting_for_discovery.remove(req_id) {
            return Some(Event::DiscoverFailed {
                request_id: *req_id,
                rendezvous_node,
                namespace,
                error: ErrorCode::Unavailable,
            });
        }

        None
    }

    fn handle_response(
        &mut self,
        request_id: &OutboundRequestId,
        response: Message,
    ) -> Option<Event> {
        match response {
            Message::RegisterResponse(Ok(ttl)) => {
                let Some((rendezvous_node, namespace)) =
                    self.waiting_for_register.remove(request_id)
                else {
                    return self.event_for_unexpected_response(request_id);
                };
                if self.automatic_registration_refresh {
                    self.registered_namespaces
                        .insert((rendezvous_node, namespace.clone()), ttl);
                }
                Some(Event::Registered {
                    request_id: *request_id,
                    rendezvous_node,
                    ttl,
                    namespace,
                })
            }
            Message::RegisterResponse(Err(error_code)) => {
                let Some((rendezvous_node, namespace)) =
                    self.waiting_for_register.remove(request_id)
                else {
                    return self.event_for_unexpected_response(request_id);
                };
                Some(Event::RegisterFailed {
                    request_id: *request_id,
                    rendezvous_node,
                    namespace,
                    error: error_code,
                })
            }
            Message::DiscoverResponse(Ok((registrations, cookie))) => {
                let Some((rendezvous_node, _ns)) = self.waiting_for_discovery.remove(request_id)
                else {
                    return self.event_for_unexpected_response(request_id);
                };
                for registration in &registrations {
                    let peer_id = registration.record.peer_id();
                    let addresses = registration.record.addresses();
                    let namespace = registration.namespace.clone();
                    let ttl = registration.ttl.min(MAX_DISCOVERED_TTL_SECS);

                    let new_addr_events = addresses
                        .iter()
                        .filter_map(|address| {
                            if self.discovered_peer_addrs(&peer_id).any(|a| a == address) {
                                return None;
                            }
                            Some(ToSwarm::NewExternalAddrOfPeer {
                                peer_id,
                                address: address.clone(),
                            })
                        })
                        .collect::<Vec<_>>();
                    self.events.extend(new_addr_events);

                    self.discovered_peers
                        .entry(peer_id)
                        .or_default()
                        .insert(namespace.clone(), addresses.to_owned());

                    self.expiring_registrations.push(
                        async move {
                            futures_timer::Delay::new(Duration::from_secs(ttl)).await;
                            (peer_id, namespace)
                        }
                        .boxed(),
                    );
                }

                Some(Event::Discovered {
                    request_id: *request_id,
                    rendezvous_node,
                    registrations,
                    cookie,
                })
            }
            Message::DiscoverResponse(Err(error_code)) => {
                let Some((rendezvous_node, ns)) = self.waiting_for_discovery.remove(request_id)
                else {
                    return self.event_for_unexpected_response(request_id);
                };
                Some(Event::DiscoverFailed {
                    request_id: *request_id,
                    rendezvous_node,
                    namespace: ns,
                    error: error_code,
                })
            }
            Message::Register(_) | Message::Unregister(_) | Message::Discover { .. } => {
                self.event_for_unexpected_response(request_id)
            }
        }
    }

    fn event_for_unexpected_response(&mut self, request_id: &OutboundRequestId) -> Option<Event> {
        if let Some((rendezvous_node, namespace)) = self.waiting_for_register.remove(request_id) {
            return Some(Event::RegisterFailed {
                request_id: *request_id,
                rendezvous_node,
                namespace,
                error: ErrorCode::InternalError,
            });
        }

        self.waiting_for_discovery
            .remove(request_id)
            .map(|(rendezvous_node, namespace)| Event::DiscoverFailed {
                request_id: *request_id,
                rendezvous_node,
                namespace,
                error: ErrorCode::InternalError,
            })
    }

    fn discovered_peer_addrs(&self, peer: &PeerId) -> impl Iterator<Item = &Multiaddr> {
        self.discovered_peers
            .get(peer)
            .map(|addrs| addrs.values().flatten())
            .unwrap_or_default()
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Message {
    Register(NewRegistration),
    RegisterResponse(Result<Ttl, ErrorCode>),
    Unregister(Namespace),
    Discover {
        namespace: Option<Namespace>,
        cookie: Option<Cookie>,
        limit: Option<u64>,
    },
    DiscoverResponse(Result<(Vec<Registration>, Cookie), ErrorCode>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NewRegistration {
    pub namespace: Namespace,
    pub record: PeerRecord,
    pub ttl: Option<u64>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Codec;

impl request_response::Codec for Codec {
    type Protocol = StreamProtocol;
    type Request = Message;
    type Response = Message;

    async fn read_request<T>(&mut self, _: &Self::Protocol, io: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_frame(io).await
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_frame(io).await
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_frame(io, req).await
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_frame(io, res).await
    }
}

fn encode_varint_usize(mut value: usize, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push(((value as u8) & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

async fn write_frame<T: AsyncWrite + Unpin + Send>(io: &mut T, message: Message) -> io::Result<()> {
    let proto_msg = wire::Message::from(message);
    let encoded_len = proto_msg.encoded_len();
    if encoded_len > MAX_MESSAGE_LEN_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "rendezvous frame exceeds maximum message length",
        ));
    }
    let mut frame = Vec::with_capacity(5 + encoded_len);
    encode_varint_usize(encoded_len, &mut frame);
    proto_msg
        .encode(&mut frame)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    io.write_all(&frame).await?;
    io.flush().await
}

async fn read_frame<T: AsyncRead + Unpin + Send>(io: &mut T) -> io::Result<Message> {
    let mut len = 0usize;
    for shift in (0..28).step_by(7) {
        let mut b = [0u8; 1];
        io.read_exact(&mut b).await?;
        let byte = b[0];
        if byte == 0 && shift > 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "non-minimal rendezvous frame length varint",
            ));
        }
        len |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            if len > MAX_MESSAGE_LEN_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "rendezvous frame exceeds maximum message length",
                ));
            }
            let mut buf = vec![0u8; len];
            io.read_exact(&mut buf).await?;
            let proto_msg = wire::Message::decode(buf.as_slice())
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            return Message::try_from(proto_msg)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "rendezvous frame length varint overflow",
    ))
}

impl From<Message> for wire::Message {
    fn from(message: Message) -> Self {
        use wire::message::{
            Discover, DiscoverResponse, MessageType, Register, RegisterResponse, ResponseStatus,
            Unregister,
        };
        match message {
            Message::Register(NewRegistration {
                namespace,
                record,
                ttl,
            }) => Self {
                r#type: Some(MessageType::Register as i32),
                register: Some(Register {
                    ns: Some(namespace.into()),
                    signed_peer_record: Some(
                        record.into_signed_envelope().into_protobuf_encoding(),
                    ),
                    ttl,
                }),
                register_response: None,
                unregister: None,
                discover: None,
                discover_response: None,
            },
            Message::RegisterResponse(Ok(ttl)) => Self {
                r#type: Some(MessageType::RegisterResponse as i32),
                register_response: Some(RegisterResponse {
                    status: Some(ResponseStatus::Ok as i32),
                    status_text: None,
                    ttl: Some(ttl),
                }),
                register: None,
                unregister: None,
                discover: None,
                discover_response: None,
            },
            Message::RegisterResponse(Err(err)) => Self {
                r#type: Some(MessageType::RegisterResponse as i32),
                register_response: Some(RegisterResponse {
                    status: Some(ResponseStatus::from(err) as i32),
                    status_text: None,
                    ttl: None,
                }),
                register: None,
                unregister: None,
                discover: None,
                discover_response: None,
            },
            Message::Unregister(namespace) => Self {
                r#type: Some(MessageType::Unregister as i32),
                unregister: Some(Unregister {
                    ns: Some(namespace.into()),
                    id: None,
                }),
                register: None,
                register_response: None,
                discover: None,
                discover_response: None,
            },
            Message::Discover {
                namespace,
                cookie,
                limit,
            } => Self {
                r#type: Some(MessageType::Discover as i32),
                discover: Some(Discover {
                    ns: namespace.map(Into::into),
                    limit,
                    cookie: cookie.map(Cookie::into_wire_encoding),
                }),
                register: None,
                register_response: None,
                unregister: None,
                discover_response: None,
            },
            Message::DiscoverResponse(Ok((registrations, cookie))) => Self {
                r#type: Some(MessageType::DiscoverResponse as i32),
                discover_response: Some(DiscoverResponse {
                    registrations: registrations
                        .into_iter()
                        .map(|reg| Register {
                            ns: Some(reg.namespace.into()),
                            signed_peer_record: Some(
                                reg.record.into_signed_envelope().into_protobuf_encoding(),
                            ),
                            ttl: Some(reg.ttl),
                        })
                        .collect(),
                    cookie: Some(cookie.into_wire_encoding()),
                    status: Some(ResponseStatus::Ok as i32),
                    status_text: None,
                }),
                register: None,
                register_response: None,
                unregister: None,
                discover: None,
            },
            Message::DiscoverResponse(Err(err)) => Self {
                r#type: Some(MessageType::DiscoverResponse as i32),
                discover_response: Some(DiscoverResponse {
                    registrations: Vec::new(),
                    cookie: None,
                    status: Some(ResponseStatus::from(err) as i32),
                    status_text: None,
                }),
                register: None,
                register_response: None,
                unregister: None,
                discover: None,
            },
        }
    }
}

impl TryFrom<wire::Message> for Message {
    type Error = &'static str;

    fn try_from(message: wire::Message) -> Result<Self, Self::Error> {
        use wire::message::{MessageType, ResponseStatus};
        let msg_type = message
            .r#type
            .and_then(|t| MessageType::try_from(t).ok())
            .ok_or("inconsistent wire message type")?;

        match msg_type {
            MessageType::Register => {
                let reg = message.register.ok_or("missing register payload")?;
                let signed_peer_record =
                    reg.signed_peer_record.ok_or("missing signed peer record")?;
                let namespace = reg
                    .ns
                    .map(Namespace::new)
                    .transpose()
                    .map_err(|_| "invalid namespace")?
                    .ok_or("missing namespace")?;
                let envelope = SignedEnvelope::from_protobuf_encoding(&signed_peer_record)
                    .map_err(|_| "bad signed envelope")?;
                let record = PeerRecord::from_signed_envelope(envelope)
                    .map_err(|_| "bad signed peer record")?;
                Ok(Self::Register(NewRegistration {
                    namespace,
                    record,
                    ttl: reg.ttl,
                }))
            }
            MessageType::RegisterResponse => {
                let resp = message
                    .register_response
                    .ok_or("missing register_response payload")?;
                let status = resp
                    .status
                    .and_then(|s| ResponseStatus::try_from(s).ok())
                    .ok_or("bad status code")?;
                if status == ResponseStatus::Ok {
                    Ok(Self::RegisterResponse(Ok(resp
                        .ttl
                        .ok_or("missing ttl in register_response")?)))
                } else {
                    Ok(Self::RegisterResponse(Err(status.try_into()?)))
                }
            }
            MessageType::Unregister => {
                let unreg = message.unregister.ok_or("missing unregister payload")?;
                let namespace = unreg
                    .ns
                    .map(Namespace::new)
                    .transpose()
                    .map_err(|_| "invalid namespace")?
                    .ok_or("missing namespace")?;
                Ok(Self::Unregister(namespace))
            }
            MessageType::Discover => {
                let disc = message.discover.ok_or("missing discover payload")?;
                let namespace = disc
                    .ns
                    .map(Namespace::new)
                    .transpose()
                    .map_err(|_| "invalid namespace")?;
                let cookie = disc
                    .cookie
                    .map(Cookie::from_wire_encoding)
                    .transpose()
                    .map_err(|_| "invalid cookie")?;
                Ok(Self::Discover {
                    namespace,
                    cookie,
                    limit: disc.limit,
                })
            }
            MessageType::DiscoverResponse => {
                let resp = message
                    .discover_response
                    .ok_or("missing discover_response payload")?;
                let status = resp
                    .status
                    .and_then(|s| ResponseStatus::try_from(s).ok())
                    .ok_or("bad status code")?;
                if status == ResponseStatus::Ok {
                    let cookie_bytes = resp.cookie.ok_or("missing cookie in discover_response")?;
                    let registrations = resp
                        .registrations
                        .into_iter()
                        .map(|reg| {
                            let namespace = reg
                                .ns
                                .map(Namespace::new)
                                .transpose()
                                .map_err(|_| "invalid namespace")?
                                .ok_or("missing namespace")?;
                            let signed_peer_record =
                                reg.signed_peer_record.ok_or("missing signed peer record")?;
                            let envelope =
                                SignedEnvelope::from_protobuf_encoding(&signed_peer_record)
                                    .map_err(|_| "bad signed envelope")?;
                            let record = PeerRecord::from_signed_envelope(envelope)
                                .map_err(|_| "bad signed peer record")?;
                            let ttl = reg.ttl.ok_or("missing ttl")?;
                            Ok(Registration {
                                namespace,
                                record,
                                ttl,
                            })
                        })
                        .collect::<Result<Vec<_>, &'static str>>()?;
                    let cookie =
                        Cookie::from_wire_encoding(cookie_bytes).map_err(|_| "invalid cookie")?;
                    Ok(Self::DiscoverResponse(Ok((registrations, cookie))))
                } else {
                    Ok(Self::DiscoverResponse(Err(status.try_into()?)))
                }
            }
        }
    }
}

impl TryFrom<wire::message::ResponseStatus> for ErrorCode {
    type Error = &'static str;

    fn try_from(value: wire::message::ResponseStatus) -> Result<Self, Self::Error> {
        use wire::message::ResponseStatus;
        match value {
            ResponseStatus::Ok => Err("ok status is not an error code"),
            ResponseStatus::EInvalidNamespace => Ok(Self::InvalidNamespace),
            ResponseStatus::EInvalidSignedPeerRecord => Ok(Self::InvalidSignedPeerRecord),
            ResponseStatus::EInvalidTtl => Ok(Self::InvalidTtl),
            ResponseStatus::EInvalidCookie => Ok(Self::InvalidCookie),
            ResponseStatus::ENotAuthorized => Ok(Self::NotAuthorized),
            ResponseStatus::EInternalError => Ok(Self::InternalError),
            ResponseStatus::EUnavailable => Ok(Self::Unavailable),
        }
    }
}

impl From<ErrorCode> for wire::message::ResponseStatus {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::InvalidNamespace => Self::EInvalidNamespace,
            ErrorCode::InvalidSignedPeerRecord => Self::EInvalidSignedPeerRecord,
            ErrorCode::InvalidTtl => Self::EInvalidTtl,
            ErrorCode::InvalidCookie => Self::EInvalidCookie,
            ErrorCode::NotAuthorized => Self::ENotAuthorized,
            ErrorCode::InternalError => Self::EInternalError,
            ErrorCode::Unavailable => Self::EUnavailable,
        }
    }
}

mod wire {
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub(super) struct Message {
        #[prost(enumeration = "message::MessageType", optional, tag = "1")]
        pub r#type: Option<i32>,
        #[prost(message, optional, tag = "2")]
        pub register: Option<message::Register>,
        #[prost(message, optional, tag = "3")]
        pub register_response: Option<message::RegisterResponse>,
        #[prost(message, optional, tag = "4")]
        pub unregister: Option<message::Unregister>,
        #[prost(message, optional, tag = "5")]
        pub discover: Option<message::Discover>,
        #[prost(message, optional, tag = "6")]
        pub discover_response: Option<message::DiscoverResponse>,
    }

    pub(super) mod message {
        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub(in super::super) struct Register {
            #[prost(string, optional, tag = "1")]
            pub ns: Option<String>,
            #[prost(bytes = "vec", optional, tag = "2")]
            pub signed_peer_record: Option<Vec<u8>>,
            #[prost(uint64, optional, tag = "3")]
            pub ttl: Option<u64>,
        }

        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub(in super::super) struct RegisterResponse {
            #[prost(enumeration = "ResponseStatus", optional, tag = "1")]
            pub status: Option<i32>,
            #[prost(string, optional, tag = "2")]
            pub status_text: Option<String>,
            #[prost(uint64, optional, tag = "3")]
            pub ttl: Option<u64>,
        }

        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub(in super::super) struct Unregister {
            #[prost(string, optional, tag = "1")]
            pub ns: Option<String>,
            #[prost(bytes = "vec", optional, tag = "2")]
            pub id: Option<Vec<u8>>,
        }

        #[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
        pub(in super::super) struct Discover {
            #[prost(string, optional, tag = "1")]
            pub ns: Option<String>,
            #[prost(uint64, optional, tag = "2")]
            pub limit: Option<u64>,
            #[prost(bytes = "vec", optional, tag = "3")]
            pub cookie: Option<Vec<u8>>,
        }

        #[derive(Clone, PartialEq, ::prost::Message)]
        pub(in super::super) struct DiscoverResponse {
            #[prost(message, repeated, tag = "1")]
            pub registrations: Vec<Register>,
            #[prost(bytes = "vec", optional, tag = "2")]
            pub cookie: Option<Vec<u8>>,
            #[prost(enumeration = "ResponseStatus", optional, tag = "3")]
            pub status: Option<i32>,
            #[prost(string, optional, tag = "4")]
            pub status_text: Option<String>,
        }

        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub(in super::super) enum MessageType {
            Register = 0,
            RegisterResponse = 1,
            Unregister = 2,
            Discover = 3,
            DiscoverResponse = 4,
        }

        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub(in super::super) enum ResponseStatus {
            Ok = 0,
            EInvalidNamespace = 100,
            EInvalidSignedPeerRecord = 101,
            EInvalidTtl = 102,
            EInvalidCookie = 103,
            ENotAuthorized = 200,
            EInternalError = 300,
            EUnavailable = 400,
        }
    }
}

#[cfg(test)]
#[path = "rendezvous_client_tests.rs"]
mod tests;
