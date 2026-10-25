use super::*;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, kad, rendezvous, request_response};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn make_swarm(
    keypair: &libp2p::identity::Keypair,
) -> Swarm<super::super::swarm_behaviour::Behaviour> {
    super::super::swarm_behaviour::build_swarm(
        keypair,
        true,
        false,
        Duration::from_secs(10),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    )
}

fn signed_template(keypair: &libp2p::identity::Keypair) -> super::super::PeerRecord {
    super::super::tests::signed_discovery_peer_record(
        keypair,
        vec![crate::dht::PeerDiscoverySource::Rendezvous],
        super::super::now_ms(),
    )
    .record
}

fn operation_id(
    swarm: &Swarm<super::super::swarm_behaviour::Behaviour>,
    kind: RendezvousOperationKind,
    peer_id: PeerId,
) -> request_response::OutboundRequestId {
    *swarm
        .behaviour()
        .discovery_budget
        .rendezvous_by_request
        .iter()
        .find_map(|(id, operations)| {
            operations
                .iter()
                .any(|operation| operation.kind == kind && operation.peer_id == peer_id)
                .then_some(id)
        })
        .expect("tracked rendezvous operation")
}

fn schedule_register(
    swarm: &mut Swarm<super::super::swarm_behaviour::Behaviour>,
    pending: &mut HashSet<PeerId>,
    registered: &HashSet<PeerId>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &super::super::PeerRecord,
) -> request_response::OutboundRequestId {
    assert!(
        super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
            swarm,
            pending,
            registered,
            peer_id,
            local_peer_id,
            template,
        )
        .expect("rendezvous register helper")
    );
    operation_id(swarm, RendezvousOperationKind::Register, peer_id)
}

fn schedule_discover(
    swarm: &mut Swarm<super::super::swarm_behaviour::Behaviour>,
    pending: &mut HashSet<PeerId>,
    cookies: &HashMap<PeerId, rendezvous::Cookie>,
    peer_id: PeerId,
    local_peer_id: PeerId,
    template: &super::super::PeerRecord,
) -> request_response::OutboundRequestId {
    assert!(
        super::super::rendezvous_operations::maybe_discover_rendezvous_namespace(
            swarm,
            pending,
            cookies,
            peer_id,
            local_peer_id,
            template,
        )
        .expect("rendezvous discover helper")
    );
    operation_id(swarm, RendezvousOperationKind::Discover, peer_id)
}

fn handle_client_event(
    swarm: &mut Swarm<super::super::swarm_behaviour::Behaviour>,
    event: rendezvous::client::Event,
    pending_registers: &mut HashSet<PeerId>,
    registered_nodes: &mut HashSet<PeerId>,
    pending_discovers: &mut HashSet<PeerId>,
    local_peer_id: PeerId,
) {
    let mut cookies = HashMap::new();
    let mut pending_dht =
        HashMap::<kad::QueryId, super::super::kad_queries::PendingDhtQuery>::new();
    let mut pending_peer_record_requests = HashMap::<
        request_response::OutboundRequestId,
        super::super::discovery::PendingPeerRecordRequest,
    >::new();
    let mut pending_discovery_peer_records = HashSet::new();
    let mut pending_cached_peer_records = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let peers = [];
    let discovered_peer_records = super::super::peer_record_cache::PeerRecordCache::default();
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    super::super::rendezvous_operations::handle_client_event(
        event,
        super::super::rendezvous_operations::ClientEventContext {
            swarm,
            pending_registers,
            registered_nodes,
            pending_discovers,
            cookies: &mut cookies,
            pending_dht: &mut pending_dht,
            pending_peer_record_requests: &mut pending_peer_record_requests,
            pending_discovery_peer_records: &mut pending_discovery_peer_records,
            pending_cached_peer_records: &mut pending_cached_peer_records,
            cached_peer_record_cooldowns: &mut cached_peer_record_cooldowns,
            traffic_metrics: &traffic_metrics,
            peers: &peers,
            discovered_peer_records: &discovered_peer_records,
            local_peer_id,
            peer_record_template: None,
            max_error_messages: 16,
            event_errors: &event_errors,
        },
    );
}

fn register_event(
    request_id: request_response::OutboundRequestId,
    rendezvous_node: PeerId,
    succeeded: bool,
) -> rendezvous::client::Event {
    let namespace = rendezvous::Namespace::from_static("oasis7-budget-test");
    if succeeded {
        rendezvous::client::Event::Registered {
            request_id,
            rendezvous_node,
            ttl: 300,
            namespace,
        }
    } else {
        rendezvous::client::Event::RegisterFailed {
            request_id,
            rendezvous_node,
            namespace,
            error: rendezvous::ErrorCode::Unavailable,
        }
    }
}

fn discover_event(
    request_id: request_response::OutboundRequestId,
    rendezvous_node: PeerId,
    succeeded: bool,
) -> rendezvous::client::Event {
    if succeeded {
        rendezvous::client::Event::Discovered {
            request_id,
            rendezvous_node,
            registrations: Vec::new(),
            cookie: rendezvous::Cookie::for_all_namespaces(),
        }
    } else {
        rendezvous::client::Event::DiscoverFailed {
            request_id,
            rendezvous_node,
            namespace: None,
            error: rendezvous::ErrorCode::Unavailable,
        }
    }
}

#[test]
fn colliding_request_ids_in_app_and_rendezvous_behaviours_release_independently() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let remote_peer_id = PeerId::random();
        let template = signed_template(&keypair);
        let mut swarm = make_swarm(&keypair);
        let mut pending_rr = HashMap::new();
        let mut pending_connected = HashSet::new();
        let mut cooldowns = HashMap::new();
        let traffic = super::super::traffic_metrics::init_shared_traffic_metrics();
        assert!(
            super::super::discovery::maybe_request_connected_peer_record(
                &mut swarm,
                &mut pending_rr,
                &mut pending_connected,
                &mut cooldowns,
                &traffic,
                remote_peer_id,
                local_peer_id,
                false,
            )
        );
        let app_id = *pending_rr.keys().next().expect("app request ID");

        let mut pending_discovers = HashSet::new();
        let rendezvous_id = schedule_discover(
            &mut swarm,
            &mut pending_discovers,
            &HashMap::new(),
            remote_peer_id,
            local_peer_id,
            &template,
        );
        assert_eq!(
            app_id, rendezvous_id,
            "behaviours allocate IDs independently"
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 2);

        swarm
            .behaviour_mut()
            .discovery_budget
            .finish_request(app_id);
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 1);
        handle_client_event(
            &mut swarm,
            discover_event(rendezvous_id, remote_peer_id, false),
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);
        assert!(pending_discovers.is_empty());
    });
}

#[test]
fn all_rendezvous_terminal_events_match_exact_kind_and_request_id() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let template = signed_template(&keypair);
        let mut swarm = make_swarm(&keypair);
        swarm.add_external_address("/ip4/127.0.0.1/tcp/43101".parse().unwrap());
        let registered = HashSet::new();
        let mut pending_registers = HashSet::new();
        let mut pending_discovers = HashSet::new();
        let cookies = HashMap::new();
        let register_ok_peer = PeerId::random();
        let register_fail_peer = PeerId::random();
        let discover_ok_peer = PeerId::random();
        let discover_fail_peer = PeerId::random();
        let register_ok = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered,
            register_ok_peer,
            local_peer_id,
            &template,
        );
        let register_fail = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered,
            register_fail_peer,
            local_peer_id,
            &template,
        );
        let discover_ok = schedule_discover(
            &mut swarm,
            &mut pending_discovers,
            &cookies,
            discover_ok_peer,
            local_peer_id,
            &template,
        );
        let discover_fail = schedule_discover(
            &mut swarm,
            &mut pending_discovers,
            &cookies,
            discover_fail_peer,
            local_peer_id,
            &template,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 4);

        handle_client_event(
            &mut swarm,
            register_event(discover_ok, discover_ok_peer, false),
            &mut pending_registers,
            &mut HashSet::new(),
            &mut pending_discovers,
            local_peer_id,
        );
        handle_client_event(
            &mut swarm,
            discover_event(register_fail, register_fail_peer, false),
            &mut pending_registers,
            &mut HashSet::new(),
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 4);
        assert!(pending_registers.contains(&register_ok_peer));
        assert!(pending_registers.contains(&register_fail_peer));
        assert!(pending_discovers.contains(&discover_ok_peer));
        assert!(pending_discovers.contains(&discover_fail_peer));

        let mut registered_nodes = HashSet::new();
        handle_client_event(
            &mut swarm,
            register_event(register_ok, register_ok_peer, true),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 3);
        assert!(registered_nodes.contains(&register_ok_peer));
        assert!(!pending_registers.contains(&register_ok_peer));

        handle_client_event(
            &mut swarm,
            register_event(register_fail, register_fail_peer, false),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 2);
        handle_client_event(
            &mut swarm,
            discover_event(discover_ok, discover_ok_peer, true),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 1);
        assert!(!pending_discovers.contains(&discover_ok_peer));
        handle_client_event(
            &mut swarm,
            discover_event(discover_fail, discover_fail_peer, false),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);

        super::super::rendezvous_operations::forget_rendezvous_registration(
            &mut swarm,
            register_ok_peer,
        );
        registered_nodes.remove(&register_ok_peer);
        let newer_id = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered_nodes,
            register_ok_peer,
            local_peer_id,
            &template,
        );
        assert_ne!(newer_id, register_ok);
        handle_client_event(
            &mut swarm,
            register_event(register_ok, register_ok_peer, false),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 1);
        assert!(pending_registers.contains(&register_ok_peer));
        handle_client_event(
            &mut swarm,
            register_event(newer_id, register_ok_peer, true),
            &mut pending_registers,
            &mut registered_nodes,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);
        assert!(registered_nodes.contains(&register_ok_peer));
    });
}

#[test]
fn synchronous_register_errors_release_inflight_but_keep_window_charge() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let template = signed_template(&keypair);
        let mut swarm = make_swarm(&keypair);
        let registered = HashSet::new();
        let mut pending = HashSet::new();
        for _ in 0..128 {
            let err = super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
                &mut swarm,
                &mut pending,
                &registered,
                PeerId::random(),
                local_peer_id,
                &template,
            )
            .expect_err("no external addresses cause synchronous register failure");
            assert!(matches!(
                err,
                crate::error::WorldError::NetworkProtocolUnavailable { protocol }
                    if protocol.contains("rendezvous register failed")
            ));
        }
        let before = swarm.behaviour().discovery_budget.snapshot();
        assert_eq!(before.inflight, 0);
        assert_eq!(before.admitted, 128);
        assert_eq!(before.window_starts, 128);

        swarm.add_external_address("/ip4/127.0.0.1/tcp/43102".parse().unwrap());
        assert!(
            !super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
                &mut swarm,
                &mut pending,
                &registered,
                PeerId::random(),
                local_peer_id,
                &template,
            )
            .expect("rate-limited registration is not a synchronous construction error")
        );
        let after = swarm.behaviour().discovery_budget.snapshot();
        assert_eq!(after.inflight, 0);
        assert_eq!(after.window_starts, 128);
        assert_eq!(after.rejected_window, 1);
    });
}

#[test]
fn disconnect_clears_local_pending_but_outstanding_operation_still_blocks_reissue() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let peer_id = PeerId::random();
        let template = signed_template(&keypair);
        let mut swarm = make_swarm(&keypair);
        swarm.add_external_address("/ip4/127.0.0.1/tcp/43103".parse().unwrap());
        let mut pending_registers = HashSet::new();
        let mut registered = HashSet::new();
        let request_id = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered,
            peer_id,
            local_peer_id,
            &template,
        );
        let mut peers = vec![peer_id];
        let mut admitted = HashSet::new();
        let mut quarantined = HashSet::new();
        let mut quarantine_disconnects = HashSet::new();
        let mut active_paths = HashMap::new();
        let mut last_dialed_paths = HashMap::new();
        let mut connected_cooldowns = HashMap::new();
        let mut cached_cooldowns = HashMap::new();
        let mut discovery_cooldowns = HashMap::new();
        let mut pending_discovers = HashSet::new();
        let mut cookies = HashMap::new();
        let connected = Arc::new(Mutex::new(HashSet::from([peer_id])));
        super::super::connection_lifecycle::clear_disconnected_peer_state(
            &mut peers,
            &mut admitted,
            &mut quarantined,
            &mut quarantine_disconnects,
            &mut active_paths,
            &mut last_dialed_paths,
            &mut connected_cooldowns,
            &mut cached_cooldowns,
            &mut discovery_cooldowns,
            &mut pending_registers,
            &mut pending_discovers,
            &mut registered,
            &mut cookies,
            &connected,
            peer_id,
        );
        assert!(pending_registers.is_empty());
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 1);
        assert!(
            !super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
                &mut swarm,
                &mut pending_registers,
                &registered,
                peer_id,
                local_peer_id,
                &template,
            )
            .expect("duplicate outstanding operation is refused")
        );
        assert_eq!(
            swarm.behaviour().discovery_budget.snapshot().window_starts,
            1
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 1);

        handle_client_event(
            &mut swarm,
            register_event(request_id, peer_id, false),
            &mut pending_registers,
            &mut registered,
            &mut pending_discovers,
            local_peer_id,
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);
        assert!(
            super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
                &mut swarm,
                &mut pending_registers,
                &registered,
                peer_id,
                local_peer_id,
                &template,
            )
            .expect("terminal failure releases the key")
        );
    });
}

#[test]
fn old_register_ack_after_address_change_does_not_satisfy_new_registration() {
    super::super::runtime_support::run_on_libp2p_test_runtime(|| {
        let keypair = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        let peer_id = PeerId::random();
        let template = signed_template(&keypair);
        let mut swarm = make_swarm(&keypair);
        swarm.add_external_address("/ip4/127.0.0.1/tcp/43104".parse().unwrap());
        let mut pending_registers = HashSet::new();
        let mut registered = HashSet::new();
        let old_id = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered,
            peer_id,
            local_peer_id,
            &template,
        );
        swarm.add_external_address("/ip4/127.0.0.1/tcp/43105".parse().unwrap());
        handle_client_event(
            &mut swarm,
            register_event(old_id, peer_id, true),
            &mut pending_registers,
            &mut registered,
            &mut HashSet::new(),
            local_peer_id,
        );
        assert!(!registered.contains(&peer_id));
        assert!(
            !super::super::rendezvous_operations::rendezvous_registration_is_current(
                &swarm, peer_id
            )
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);

        let new_id = schedule_register(
            &mut swarm,
            &mut pending_registers,
            &registered,
            peer_id,
            local_peer_id,
            &template,
        );
        assert_ne!(old_id, new_id);
        handle_client_event(
            &mut swarm,
            register_event(new_id, peer_id, true),
            &mut pending_registers,
            &mut registered,
            &mut HashSet::new(),
            local_peer_id,
        );
        assert!(registered.contains(&peer_id));
        assert!(
            super::super::rendezvous_operations::rendezvous_registration_is_current(
                &swarm, peer_id
            )
        );
        assert_eq!(swarm.behaviour().discovery_budget.snapshot().inflight, 0);
    });
}

#[derive(Clone, Default)]
struct RawRendezvousCodec;

async fn read_frame<T: AsyncRead + Unpin + Send>(io: &mut T) -> io::Result<Vec<u8>> {
    let mut len = 0usize;
    for shift in (0..35).step_by(7) {
        let mut byte = [0];
        io.read_exact(&mut byte).await?;
        len |= usize::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            if len > 1024 * 1024 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut payload = vec![0; len];
            io.read_exact(&mut payload).await?;
            return Ok(payload);
        }
    }
    Err(io::ErrorKind::InvalidData.into())
}

async fn write_frame<T: AsyncWrite + Unpin + Send>(io: &mut T, payload: Vec<u8>) -> io::Result<()> {
    let mut frame = Vec::new();
    let mut len = payload.len() as u64;
    while len >= 128 {
        frame.push((len as u8 & 0x7f) | 0x80);
        len >>= 7;
    }
    frame.push(len as u8);
    frame.extend(payload);
    io.write_all(&frame).await?;
    io.close().await
}

impl request_response::Codec for RawRendezvousCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request> {
        read_frame(io).await
    }

    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response> {
        read_frame(io).await
    }

    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        request: Self::Request,
    ) -> io::Result<()> {
        write_frame(io, request).await
    }

    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        response: Self::Response,
    ) -> io::Result<()> {
        write_frame(io, response).await
    }
}

fn append_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 128 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn append_field(tag: u8, bytes: &[u8], output: &mut Vec<u8>) {
    output.push(tag);
    append_varint(bytes.len() as u64, output);
    output.extend_from_slice(bytes);
}

fn wrong_kind_discover_response() -> Vec<u8> {
    let mut response = Vec::new();
    append_field(
        18,
        &rendezvous::Cookie::for_all_namespaces().into_wire_encoding(),
        &mut response,
    );
    response.extend([24, 0]);
    let mut message = vec![8, 4];
    append_field(50, &response, &mut message);
    message
}

fn raw_rendezvous_server(
    keypair: &libp2p::identity::Keypair,
) -> Swarm<request_response::Behaviour<RawRendezvousCodec>> {
    use libp2p::Transport as _;
    let transport = libp2p::tcp::tokio::Transport::new(libp2p::tcp::Config::default())
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(libp2p::noise::Config::new(keypair).expect("noise config"))
        .multiplex(libp2p::yamux::Config::default())
        .boxed();
    let behaviour = request_response::Behaviour::<RawRendezvousCodec>::new(
        [(
            rendezvous::PROTOCOL_NAME,
            request_response::ProtocolSupport::Full,
        )],
        request_response::Config::default(),
    );
    Swarm::new(
        transport,
        behaviour,
        PeerId::from(keypair.public()),
        libp2p::swarm::Config::with_tokio_executor(),
    )
}

#[test]
fn real_wrong_kind_response_emits_failure_id_and_releases_production_budget() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), async {
            let client_key = libp2p::identity::Keypair::generate_ed25519();
            let client_peer = PeerId::from(client_key.public());
            let server_key = libp2p::identity::Keypair::generate_ed25519();
            let server_peer = PeerId::from(server_key.public());
            let template = signed_template(&client_key);
            let mut client = make_swarm(&client_key);
            client.add_external_address("/ip4/127.0.0.1/tcp/43106".parse().unwrap());
            let mut server = raw_rendezvous_server(&server_key);
            server
                .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
                .expect("server listen");
            let listen_addr = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                    break address;
                }
            };
            client
                .dial(
                    listen_addr
                        .with_p2p(server_peer)
                        .expect("server peer address"),
                )
                .expect("client dial");
            loop {
                if let futures::future::Either::Left((
                    SwarmEvent::ConnectionEstablished { peer_id, .. },
                    _,
                )) =
                    futures::future::select(client.select_next_some(), server.select_next_some())
                        .await
                    && peer_id == server_peer
                {
                    break;
                }
            }

            let mut pending_registers = HashSet::new();
            let registered_nodes = HashSet::new();
            assert!(
                super::super::rendezvous_operations::maybe_register_rendezvous_namespace(
                    &mut client,
                    &mut pending_registers,
                    &registered_nodes,
                    server_peer,
                    client_peer,
                    &template,
                )
                .expect("schedule registration")
            );
            let request_id = operation_id(&client, RendezvousOperationKind::Register, server_peer);
            let mut sent_wrong_response = false;
            let mut pending_discovers = HashSet::new();
            let mut registered_nodes = registered_nodes;
            let terminal = loop {
                match futures::future::select(client.select_next_some(), server.select_next_some())
                    .await
                {
                    futures::future::Either::Left((
                        SwarmEvent::Behaviour(
                            super::super::swarm_behaviour::BehaviourEvent::RendezvousClient(event),
                        ),
                        _,
                    )) if matches!(
                        &event,
                        rendezvous::client::Event::RegisterFailed { request_id: id, .. }
                            if *id == request_id
                    ) =>
                    {
                        let rendezvous::client::Event::RegisterFailed { error, .. } = &event else {
                            unreachable!();
                        };
                        assert_eq!(*error, rendezvous::ErrorCode::InternalError);
                        break event;
                    }
                    futures::future::Either::Right((
                        SwarmEvent::Behaviour(request_response::Event::Message {
                            message: request_response::Message::Request { channel, .. },
                            ..
                        }),
                        _,
                    )) => {
                        server
                            .behaviour_mut()
                            .send_response(channel, wrong_kind_discover_response())
                            .expect("send a valid but wrong-kind rendezvous response");
                        sent_wrong_response = true;
                    }
                    _ => {}
                }
            };
            assert!(sent_wrong_response);
            handle_client_event(
                &mut client,
                terminal,
                &mut pending_registers,
                &mut registered_nodes,
                &mut pending_discovers,
                client_peer,
            );
            assert!(pending_registers.is_empty());
            let metrics = client.behaviour().discovery_budget.snapshot();
            assert_eq!(metrics.window_starts, 1);
            assert_eq!(metrics.inflight, 0);
        })
        .await
        .expect("wrong-kind rendezvous exchange terminates");
    });
}
