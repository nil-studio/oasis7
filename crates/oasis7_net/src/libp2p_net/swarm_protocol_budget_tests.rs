use super::*;
use crate::libp2p_net::{
    connection_budget::ConnectionLimits,
    receiver_floor::PeerRecordReceiver,
    resource_budget::{BudgetDiagnostics, ResourceBudgetRejection},
    wire_bytes::init_shared_wire_byte_counters,
};
use futures::{
    StreamExt,
    future::{Either, select},
};
use libp2p::swarm::SwarmEvent;
use std::{sync::Arc, time::Duration};

fn swarm(
    key: &Keypair,
    budget: Option<ConnectionBudget>,
    config: autonat::Config,
) -> Swarm<Behaviour> {
    let mut swarm = build_swarm_with_receiver(
        key,
        false,
        true,
        Duration::from_secs(3),
        init_shared_wire_byte_counters(),
        PeerRecordReceiver::legacy(),
        budget,
    );
    let peer = *swarm.local_peer_id();
    let behaviour = swarm.behaviour_mut();
    behaviour.inner.autonat = Toggle::from(Some(ProtocolDialBehaviour::new(
        autonat::Behaviour::new(peer, config),
        behaviour.protocol_dials.clone(),
    )));
    swarm
}

async fn listen(swarm: &mut Swarm<Behaviour>) -> Multiaddr {
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    loop {
        if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
            return address;
        }
    }
}

#[test]
fn actual_autonat_dial_back_obeys_established_peer_capacity() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(8), async {
                let diagnostics = Arc::new(BudgetDiagnostics::default());
                let budget = ConnectionBudget::with_diagnostics(
                    ConnectionLimits {
                        total: 4,
                        pending: 3,
                        inbound_unknown: 2,
                        outbound: 2,
                        per_peer: 1,
                        handshake_timeout: Duration::from_secs(3),
                    },
                    Arc::clone(&diagnostics),
                )
                .unwrap();
                let mut server = swarm(
                    &Keypair::generate_ed25519(),
                    Some(budget.clone()),
                    autonat::Config {
                        boot_delay: Duration::from_secs(60),
                        only_global_ips: false,
                        ..Default::default()
                    },
                );
                let mut client = swarm(
                    &Keypair::generate_ed25519(),
                    None,
                    autonat::Config {
                        boot_delay: Duration::from_millis(400),
                        retry_interval: Duration::from_secs(1),
                        throttle_server_period: Duration::ZERO,
                        only_global_ips: false,
                        ..Default::default()
                    },
                );
                let server_peer = *server.local_peer_id();
                let client_peer = *client.local_peer_id();
                let server_addr = listen(&mut server).await;
                let _client_addr = listen(&mut client).await;
                client
                    .behaviour_mut()
                    .autonat
                    .as_mut()
                    .unwrap()
                    .add_server(server_peer, Some(server_addr.clone()));
                client
                    .dial(server_addr.with_p2p(server_peer).unwrap())
                    .unwrap();

                let mut incoming_probe = None;
                loop {
                    if let Either::Left((event, _)) =
                        select(server.select_next_some(), client.select_next_some()).await
                    {
                        match event {
                            SwarmEvent::Behaviour(BehaviourEvent::Autonat(
                                autonat::Event::InboundProbe(autonat::InboundProbeEvent::Request {
                                    probe_id,
                                    peer,
                                    ..
                                }),
                            )) if peer == client_peer => incoming_probe = Some(probe_id),
                            SwarmEvent::Behaviour(BehaviourEvent::Autonat(
                                autonat::Event::InboundProbe(autonat::InboundProbeEvent::Error {
                                    probe_id,
                                    ..
                                }),
                            )) if incoming_probe == Some(probe_id) => {
                                let (reason, _) = diagnostics.latest();
                                assert_eq!(reason, Some(ResourceBudgetRejection::PerPeer));
                                let snapshot = budget.snapshot().unwrap();
                                assert_eq!(
                                    (snapshot.total, snapshot.pending, snapshot.established),
                                    (1, 0, 1)
                                );
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            })
            .await
            .expect("actual AutoNAT probe did not hit per-peer budget");
        });
}
