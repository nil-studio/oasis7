use std::{io, time::Duration};

use futures::{StreamExt, io::Cursor};
use libp2p::{
    Multiaddr, Swarm, Transport,
    core::upgrade,
    identity::{Keypair, PeerId},
    noise,
    rendezvous::{self, ErrorCode, Namespace},
    swarm::{FromSwarm, NetworkBehaviour, SwarmEvent, behaviour::ExternalAddrConfirmed},
    yamux,
};

use super::{Behaviour, Event, MAX_MESSAGE_LEN_BYTES, Message, encode_varint_usize, read_frame};

fn peer_id() -> PeerId {
    Keypair::generate_ed25519().public().to_peer_id()
}

fn confirm_address(behaviour: &mut Behaviour, address: &Multiaddr) {
    behaviour.on_swarm_event(FromSwarm::ExternalAddrConfirmed(ExternalAddrConfirmed {
        addr: address,
    }));
}

#[test]
fn automatic_registration_refresh_is_enabled_by_default_and_carries_request_id() {
    let mut behaviour = Behaviour::new(Keypair::generate_ed25519());
    let rendezvous_node = peer_id();
    let namespace = Namespace::from_static("test");
    let first_address: Multiaddr = "/ip4/127.0.0.1/tcp/41001".parse().unwrap();
    let second_address: Multiaddr = "/ip4/127.0.0.1/tcp/41002".parse().unwrap();

    confirm_address(&mut behaviour, &first_address);
    assert_eq!(
        behaviour.external_addresses().cloned().collect::<Vec<_>>(),
        vec![first_address.clone()]
    );

    let request_id = behaviour
        .register_with_request_id(namespace.clone(), rendezvous_node, Some(300))
        .unwrap();
    let event = behaviour
        .handle_response(&request_id, Message::RegisterResponse(Ok(300)))
        .unwrap();
    assert!(matches!(
        event,
        Event::Registered {
            request_id: event_request_id,
            rendezvous_node: event_node,
            namespace: event_namespace,
            ..
        } if event_request_id == request_id
            && event_node == rendezvous_node
            && event_namespace == namespace
    ));
    assert_eq!(
        behaviour
            .registered_namespaces
            .get(&(rendezvous_node, namespace)),
        Some(&300)
    );

    confirm_address(&mut behaviour, &second_address);
    assert_eq!(behaviour.waiting_for_register.len(), 1);
    assert_eq!(
        behaviour.external_addresses().cloned().collect::<Vec<_>>(),
        [second_address, first_address]
    );
}

#[test]
fn disabled_refresh_still_tracks_addresses_and_does_not_retain_registration_state() {
    let mut behaviour =
        Behaviour::new(Keypair::generate_ed25519()).with_automatic_registration_refresh(false);
    let rendezvous_node = peer_id();
    let namespace = Namespace::from_static("test");
    let first_address: Multiaddr = "/ip4/127.0.0.1/tcp/42001".parse().unwrap();
    let second_address: Multiaddr = "/ip4/127.0.0.1/tcp/42002".parse().unwrap();

    confirm_address(&mut behaviour, &first_address);
    assert_eq!(
        behaviour.external_addresses().cloned().collect::<Vec<_>>(),
        vec![first_address.clone()]
    );
    assert!(behaviour.waiting_for_register.is_empty());

    let request_id = behaviour
        .register_with_request_id(namespace.clone(), rendezvous_node, Some(300))
        .unwrap();
    let event = behaviour
        .handle_response(&request_id, Message::RegisterResponse(Ok(300)))
        .unwrap();
    assert!(matches!(
        event,
        Event::Registered {
            request_id: event_request_id,
            ..
        } if event_request_id == request_id
    ));
    assert!(behaviour.registered_namespaces.is_empty());

    // A stale entry must also not trigger a hidden refresh when this mode is disabled.
    behaviour
        .registered_namespaces
        .insert((rendezvous_node, namespace), 300);
    confirm_address(&mut behaviour, &second_address);

    assert_eq!(behaviour.waiting_for_register.len(), 0);
    assert_eq!(
        behaviour.external_addresses().cloned().collect::<Vec<_>>(),
        [second_address, first_address]
    );
}

#[test]
fn discovery_request_receiving_register_response_fails_once_for_discovery() {
    let mut behaviour = Behaviour::new(Keypair::generate_ed25519());
    let rendezvous_node = peer_id();
    let namespace = Namespace::from_static("test");
    let request_id =
        behaviour.discover_with_request_id(Some(namespace.clone()), None, None, rendezvous_node);

    let event = behaviour
        .handle_response(&request_id, Message::RegisterResponse(Ok(300)))
        .expect("wrong response kind should terminate the pending discovery");
    match event {
        Event::DiscoverFailed {
            request_id: event_request_id,
            rendezvous_node: event_node,
            namespace: event_namespace,
            error,
        } => {
            assert_eq!(event_request_id, request_id);
            assert_eq!(event_node, rendezvous_node);
            assert_eq!(event_namespace, Some(namespace));
            assert_eq!(error, ErrorCode::InternalError);
        }
        _ => panic!("wrong response kind must fail the original discovery"),
    }

    assert!(
        behaviour
            .handle_response(&request_id, Message::RegisterResponse(Ok(300)))
            .is_none()
    );
    assert!(behaviour.waiting_for_discovery.is_empty());
}

#[test]
fn registration_request_receiving_discovery_response_fails_once_for_registration() {
    let mut behaviour = Behaviour::new(Keypair::generate_ed25519());
    let rendezvous_node = peer_id();
    let namespace = Namespace::from_static("test");
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/43001".parse().unwrap();
    confirm_address(&mut behaviour, &address);

    let request_id = behaviour
        .register_with_request_id(namespace.clone(), rendezvous_node, Some(300))
        .unwrap();
    let event = behaviour
        .handle_response(
            &request_id,
            Message::DiscoverResponse(Err(ErrorCode::Unavailable)),
        )
        .expect("wrong response kind should terminate the pending registration");
    match event {
        Event::RegisterFailed {
            request_id: event_request_id,
            rendezvous_node: event_node,
            namespace: event_namespace,
            error,
        } => {
            assert_eq!(event_request_id, request_id);
            assert_eq!(event_node, rendezvous_node);
            assert_eq!(event_namespace, namespace);
            assert_eq!(error, ErrorCode::InternalError);
        }
        _ => panic!("wrong response kind must fail the original registration"),
    }

    assert!(
        behaviour
            .handle_response(
                &request_id,
                Message::DiscoverResponse(Err(ErrorCode::Unavailable)),
            )
            .is_none()
    );
    assert!(behaviour.waiting_for_register.is_empty());
}

#[test]
fn request_message_received_as_response_fails_closed_without_panicking() {
    let mut behaviour = Behaviour::new(Keypair::generate_ed25519());
    let rendezvous_node = peer_id();
    let namespace = Namespace::from_static("test");
    let request_id =
        behaviour.discover_with_request_id(Some(namespace.clone()), None, None, rendezvous_node);

    let event = behaviour
        .handle_response(&request_id, Message::Unregister(namespace.clone()))
        .expect("request message in response position should fail pending work");
    assert!(matches!(
        event,
        Event::DiscoverFailed {
            request_id: event_request_id,
            error: ErrorCode::InternalError,
            namespace: Some(event_namespace),
            ..
        } if event_request_id == request_id && event_namespace == namespace
    ));
    assert!(
        behaviour
            .handle_response(&request_id, Message::Unregister(namespace))
            .is_none()
    );
}

#[test]
fn unregister_forgets_only_exact_node_and_namespace_pair() {
    let mut behaviour = Behaviour::new(Keypair::generate_ed25519());
    let node_a = peer_id();
    let node_b = peer_id();
    let ns_alpha = Namespace::from_static("alpha");
    let ns_beta = Namespace::from_static("beta");

    behaviour
        .registered_namespaces
        .insert((node_a, ns_alpha.clone()), 300);
    behaviour
        .registered_namespaces
        .insert((node_a, ns_beta.clone()), 300);
    behaviour
        .registered_namespaces
        .insert((node_b, ns_alpha.clone()), 300);

    behaviour.unregister(ns_alpha.clone(), node_a);

    assert!(
        !behaviour
            .registered_namespaces
            .contains_key(&(node_a, ns_alpha.clone()))
    );
    assert_eq!(
        behaviour.registered_namespaces.get(&(node_a, ns_beta)),
        Some(&300)
    );
    assert_eq!(
        behaviour.registered_namespaces.get(&(node_b, ns_alpha)),
        Some(&300)
    );
}

#[test]
fn read_frame_rejects_non_minimal_and_oversized_length_prefixes() {
    futures::executor::block_on(async {
        let mut non_minimal = Cursor::new(vec![0x80, 0x00]);
        let err = read_frame(&mut non_minimal).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let mut oversized_prefix = Vec::new();
        encode_varint_usize(MAX_MESSAGE_LEN_BYTES + 1, &mut oversized_prefix);
        let mut oversized = Cursor::new(oversized_prefix);
        let err = read_frame(&mut oversized).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    });
}

fn memory_swarm<B: NetworkBehaviour>(key: &Keypair, behaviour: B) -> Swarm<B> {
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
fn live_client_interoperates_with_upstream_rendezvous_server() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(10), async {
            let server_key = Keypair::generate_ed25519();
            let server_peer = server_key.public().to_peer_id();
            let client_key = Keypair::generate_ed25519();
            let client_peer = client_key.public().to_peer_id();
            let namespace = Namespace::from_static("oasis7-live-interop");

            let mut server = memory_swarm(
                &server_key,
                rendezvous::server::Behaviour::new(rendezvous::server::Config::default()),
            );
            let mut client = memory_swarm(
                &client_key,
                Behaviour::new(client_key.clone()).with_automatic_registration_refresh(false),
            );

            server.listen_on("/memory/0".parse().unwrap()).unwrap();
            let server_addr = loop {
                if let SwarmEvent::NewListenAddr { address, .. } = server.select_next_some().await {
                    break address;
                }
            };
            client.add_external_address("/memory/9001".parse().unwrap());
            client.dial(server_addr).unwrap();

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

            let reg_req = client
                .behaviour_mut()
                .register_with_request_id(namespace.clone(), server_peer, None)
                .expect("register over established connection");
            loop {
                if let futures::future::Either::Left((
                    SwarmEvent::Behaviour(Event::Registered {
                        request_id,
                        rendezvous_node,
                        namespace: event_ns,
                        ttl,
                    }),
                    _,
                )) =
                    futures::future::select(client.select_next_some(), server.select_next_some())
                        .await
                {
                    assert_eq!(request_id, reg_req);
                    assert_eq!(rendezvous_node, server_peer);
                    assert_eq!(event_ns, namespace);
                    assert_eq!(ttl, rendezvous::DEFAULT_TTL);
                    break;
                }
            }

            let disc_req = client.behaviour_mut().discover_with_request_id(
                Some(namespace.clone()),
                None,
                None,
                server_peer,
            );
            let cookie = loop {
                if let futures::future::Either::Left((
                    SwarmEvent::Behaviour(Event::Discovered {
                        request_id,
                        rendezvous_node,
                        registrations,
                        cookie,
                    }),
                    _,
                )) =
                    futures::future::select(client.select_next_some(), server.select_next_some())
                        .await
                {
                    assert_eq!(request_id, disc_req);
                    assert_eq!(rendezvous_node, server_peer);
                    assert_eq!(registrations.len(), 1);
                    assert_eq!(registrations[0].record.peer_id(), client_peer);
                    assert_eq!(registrations[0].namespace, namespace);
                    break cookie;
                }
            };

            // Delta discovery with cookie returns zero new registrations.
            let delta_req = client.behaviour_mut().discover_with_request_id(
                Some(namespace.clone()),
                Some(cookie),
                None,
                server_peer,
            );
            loop {
                if let futures::future::Either::Left((
                    SwarmEvent::Behaviour(Event::Discovered {
                        request_id,
                        registrations,
                        ..
                    }),
                    _,
                )) =
                    futures::future::select(client.select_next_some(), server.select_next_some())
                        .await
                {
                    assert_eq!(request_id, delta_req);
                    assert!(registrations.is_empty());
                    break;
                }
            }
        })
        .await
        .expect("live rendezvous server interop timed out");
    });
}
