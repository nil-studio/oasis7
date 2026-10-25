use libp2p_core::Multiaddr;
use libp2p_identity::{Keypair, PeerId};
use libp2p_swarm::behaviour::ExternalAddrConfirmed;
use libp2p_swarm::{FromSwarm, NetworkBehaviour};

use crate::{
    client::{Behaviour, Event},
    codec::{ErrorCode, Message, Namespace},
};

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
            .get(&(rendezvous_node, namespace.clone())),
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
