use super::*;
use crate::libp2p_net::{
    wire_byte_window::{WireByteLimits, WireByteWindow},
    wire_bytes::Libp2pWireByteCounters,
};
use libp2p::request_response::{Event, Message};
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};

pub(super) fn swarm_with_byte_window(
    key: &Keypair,
    connections: ConnectionBudget,
) -> (Swarm<Behaviour>, Arc<WireByteWindow>) {
    let window = Arc::new(
        WireByteWindow::new(WireByteLimits {
            inbound: 64 * 1024,
            outbound: 64 * 1024,
            total: 64 * 1024,
        })
        .unwrap(),
    );
    let counters = Libp2pWireByteCounters::with_window(Arc::clone(&window));
    let swarm = build_swarm_with_receiver(
        key,
        false,
        false,
        Duration::from_secs(3),
        counters,
        PeerRecordReceiver::legacy(),
        Some(connections),
    );
    (swarm, window)
}

pub(super) async fn verify_nested_request(
    source: &mut Swarm<Behaviour>,
    destination: &mut Swarm<Behaviour>,
    source_bytes: &WireByteWindow,
    destination_bytes: &WireByteWindow,
) {
    let before_source = source_bytes.snapshot().unwrap().actual_bytes;
    let before_destination = destination_bytes.snapshot().unwrap().actual_bytes;
    let source_peer = *source.local_peer_id();
    let destination_peer = *destination.local_peer_id();
    let payload = vec![3; 2_048];
    let outgoing = source.behaviour_mut().send_rr_request(
        &destination_peer,
        NetworkRequest {
            protocol: "/test/shared-relay-window".into(),
            payload: payload.clone(),
        },
        None,
    );

    loop {
        match select(source.select_next_some(), destination.select_next_some()).await {
            Either::Right((
                SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(Event::Message {
                    peer,
                    message:
                        Message::Request {
                            request, channel, ..
                        },
                    ..
                })),
                _,
            )) => {
                assert_eq!(peer, source_peer);
                assert_eq!(request.message.payload, payload);
                destination
                    .behaviour_mut()
                    .request_response
                    .send_response(
                        channel,
                        NetworkResponse {
                            payload: request.message.payload,
                        },
                    )
                    .unwrap();
            }
            Either::Left((
                SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(Event::Message {
                    peer,
                    message:
                        Message::Response {
                            request_id,
                            response,
                        },
                    ..
                })),
                _,
            )) => {
                assert_eq!((peer, request_id), (destination_peer, outgoing));
                assert_eq!(response.payload, payload);
                break;
            }
            Either::Left((
                SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(Event::OutboundFailure {
                    request_id,
                    error,
                    ..
                })),
                _,
            )) if request_id == outgoing => panic!("relayed business request failed: {error}"),
            _ => {}
        }
    }

    let nested_payload_bytes = u64::try_from(payload.len()).unwrap() * 2;
    for (before, after) in [
        (before_source, source_bytes.snapshot().unwrap().actual_bytes),
        (
            before_destination,
            destination_bytes.snapshot().unwrap().actual_bytes,
        ),
    ] {
        assert!(after.inbound - before.inbound >= nested_payload_bytes);
        assert!(after.outbound - before.outbound >= nested_payload_bytes);
    }
}

pub(super) async fn seed_direct_candidate(swarm: &mut Swarm<Behaviour>) {
    use libp2p::swarm::{FromSwarm, NetworkBehaviour, NewExternalAddrCandidate};

    let address = listen(swarm, "/ip4/127.0.0.1/tcp/0").await;
    // Fixture substitutes a deterministic observation for public NAT discovery.
    // The DCUtR protocol negotiation and resulting dial run on the real Swarm.
    swarm
        .behaviour_mut()
        .on_swarm_event(FromSwarm::NewExternalAddrCandidate(
            NewExternalAddrCandidate { addr: &address },
        ));
}

pub(super) async fn observe_dcutr_budget_refusal(
    source: &mut Swarm<Behaviour>,
    destination: &mut Swarm<Behaviour>,
    source_budget: &ConnectionBudget,
    destination_budget: &ConnectionBudget,
) {
    let source_peer = *source.local_peer_id();
    let destination_peer = *destination.local_peer_id();
    loop {
        let (event, expected, budget) =
            match select(source.select_next_some(), destination.select_next_some()).await {
                Either::Left((event, _)) => (event, destination_peer, source_budget),
                Either::Right((event, _)) => (event, source_peer, destination_budget),
            };
        if let SwarmEvent::Behaviour(BehaviourEvent::Dcutr(event)) = event
            && event.remote_peer_id == expected
        {
            assert!(event.result.is_err());
            assert_eq!(
                budget.0.diagnostics.latest().0,
                Some(ResourceBudgetRejection::PerPeer)
            );
            assert_eq!(occupancy(budget, expected), (2, [0, 0], 1));
            return;
        }
    }
}
