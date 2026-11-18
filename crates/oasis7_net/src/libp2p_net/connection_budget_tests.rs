use super::*;
use crate::libp2p_net::{
    receiver_floor::PeerRecordReceiver,
    swarm_behaviour::{Behaviour, BehaviourEvent, build_swarm_with_receiver},
    wire_bytes::init_shared_wire_byte_counters,
};
use futures::{
    StreamExt,
    future::{Either, select},
};
use libp2p::{
    identity::Keypair,
    swarm::{
        DialError, ListenError, Swarm, SwarmEvent,
        dial_opts::{DialOpts, PeerCondition},
    },
};
use std::time::Duration;

fn limits() -> ConnectionLimits {
    ConnectionLimits {
        total: 3,
        pending: 2,
        inbound_unknown: 1,
        outbound: 1,
        per_peer: 1,
    }
}

fn occupancy(budget: &ConnectionBudget, peer: PeerId) -> (usize, [u32; 2], u32) {
    let state = budget.0.state.lock().unwrap();
    (state.entries.len(), state.pending, state.for_peer(peer))
}

fn dial_for(peer: PeerId, address: Multiaddr) -> DialOpts {
    DialOpts::peer_id(peer)
        .condition(PeerCondition::Always)
        .addresses(vec![address])
        .build()
}

#[test]
fn opposite_pending_lanes_share_capacity_and_authentication_preserves_occupancy() {
    let budget = ConnectionBudget::new(limits()).unwrap();
    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let incoming = dial_for(peer, address.clone()).connection_id();
    let outgoing = dial_for(peer, address.clone()).connection_id();
    let other = dial_for(peer, address.clone()).connection_id();
    let inbound = AttemptKey::Inbound(incoming);
    let outbound = AttemptKey::Outbound(outgoing, address.clone());
    let inbound_permit = budget.reserve(inbound.clone(), None).unwrap();
    assert!(budget.reserve(AttemptKey::Inbound(other), None).is_err());
    let outbound_permit = budget.reserve(outbound.clone(), Some(peer)).unwrap();
    assert!(
        budget
            .reserve(AttemptKey::Outbound(other, address), None)
            .is_err()
    );
    assert_eq!(occupancy(&budget, peer), (2, [1, 1], 1));

    assert!(budget.establish_inbound(incoming, peer).is_err());
    assert_eq!(occupancy(&budget, peer), (2, [1, 1], 1));
    drop(outbound_permit);
    assert_eq!(occupancy(&budget, peer), (1, [1, 0], 0));
    budget.establish_inbound(incoming, peer).unwrap();
    assert_eq!(occupancy(&budget, peer), (1, [0, 0], 1));
    assert!(budget.reserve(outbound, Some(peer)).is_err());
    drop(inbound_permit);
    assert_eq!(occupancy(&budget, peer), (0, [0, 0], 0));
}

fn swarm(key: &Keypair, budget: Option<ConnectionBudget>) -> Swarm<Behaviour> {
    build_swarm_with_receiver(
        key,
        false,
        false,
        Duration::from_secs(3),
        init_shared_wire_byte_counters(),
        PeerRecordReceiver::legacy(),
        budget,
    )
}

async fn listen(swarm: &mut Swarm<Behaviour>) -> Multiaddr {
    swarm
        .listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap())
        .unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

async fn budgeted_event(
    budgeted: &mut Swarm<Behaviour>,
    remote: &mut Swarm<Behaviour>,
) -> SwarmEvent<BehaviourEvent> {
    loop {
        if let Either::Left((event, _)) =
            select(budgeted.select_next_some(), remote.select_next_some()).await
        {
            return event;
        }
    }
}

#[test]
fn assembled_quic_swarm_shares_peer_occupancy_between_inbound_and_outbound() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(12), async {
                let budget = ConnectionBudget::new(limits()).unwrap();
                let mut budgeted = swarm(&Keypair::generate_ed25519(), Some(budget.clone()));
                let mut remote = swarm(&Keypair::generate_ed25519(), None);
                let local_peer = *budgeted.local_peer_id();
                let remote_peer = *remote.local_peer_id();
                let local_address = listen(&mut budgeted).await;
                let remote_address = listen(&mut remote).await;
                remote.dial(dial_for(local_peer, local_address.clone())).unwrap();

                let first_incoming = loop {
                    if let SwarmEvent::IncomingConnection { connection_id, .. } =
                        budgeted_event(&mut budgeted, &mut remote).await
                    {
                        break connection_id;
                    }
                };
                assert_eq!(occupancy(&budget, remote_peer), (1, [1, 0], 0));
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::ConnectionEstablished { connection_id, peer_id, .. } => {
                            assert_eq!(connection_id, first_incoming);
                            assert_eq!(peer_id, remote_peer);
                            break;
                        }
                        SwarmEvent::IncomingConnectionError { error, .. } => {
                            panic!("first admissible peer failed: {error}");
                        }
                        _ => {}
                    }
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));

                remote.dial(dial_for(local_peer, local_address)).unwrap();
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::IncomingConnection { .. } => {
                            assert_eq!(occupancy(&budget, remote_peer), (2, [1, 0], 1));
                        }
                        SwarmEvent::IncomingConnectionError {
                            peer_id: Some(peer),
                            error: ListenError::Denied { .. },
                            ..
                        } => {
                            assert_eq!(peer, remote_peer);
                            break;
                        }
                        SwarmEvent::ConnectionEstablished { .. } => {
                            panic!("authenticated peer exceeded its shared bucket");
                        }
                        _ => {}
                    }
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
                assert!(matches!(
                    budgeted.dial(dial_for(remote_peer, remote_address.clone())),
                    Err(DialError::Denied { .. })
                ));
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));

                assert!(budgeted.close_connection(first_incoming));
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
                while !matches!(
                    budgeted_event(&mut budgeted, &mut remote).await,
                    SwarmEvent::ConnectionClosed { connection_id, .. } if connection_id == first_incoming
                ) {}
                assert_eq!(occupancy(&budget, remote_peer), (0, [0, 0], 0));

                let outbound = dial_for(remote_peer, remote_address);
                let outgoing_id = outbound.connection_id();
                budgeted.dial(outbound).unwrap();
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 1], 1));
                loop {
                    match budgeted_event(&mut budgeted, &mut remote).await {
                        SwarmEvent::ConnectionEstablished { connection_id, peer_id, .. } => {
                            assert_eq!(connection_id, outgoing_id);
                            assert_eq!(peer_id, remote_peer);
                            break;
                        }
                        SwarmEvent::OutgoingConnectionError { error, .. } => {
                            panic!("outbound did not recover after close: {error}");
                        }
                        _ => {}
                    }
                }
                assert_eq!(occupancy(&budget, remote_peer), (1, [0, 0], 1));
            }).await.expect("connection budget probe timed out");
        });
}
