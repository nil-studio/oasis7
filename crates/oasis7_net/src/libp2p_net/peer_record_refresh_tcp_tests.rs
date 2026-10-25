use super::*;
use futures::{FutureExt, StreamExt};
use libp2p::identity::Keypair;
use libp2p::swarm::SwarmEvent;
use std::sync::{Arc, Mutex};

#[test]
fn tcp_due_direct_refresh_receives_new_immutable_owner_signature() {
    exercise_tcp_refresh(false);
}

#[test]
fn tcp_refresh_cooldown_denial_preserves_live_cache_and_authenticated_session() {
    exercise_tcp_refresh(true);
}

fn exercise_tcp_refresh(deny_cooldown: bool) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let a_key = Keypair::generate_ed25519();
        let b_key = Keypair::generate_ed25519();
        let a_peer = PeerId::from(a_key.public());
        let b_peer = PeerId::from(b_key.public());
        let old = super::super::tests::signed_discovery_peer_record(&b_key, vec![], super::super::now_ms() - 50_000);
        let template = old.record.clone();
        let mut cache = PeerRecordCache::default();
        cache.insert(b_peer, old.clone());
        let make_swarm = |key: &Keypair| super::super::swarm_behaviour::build_swarm(key, false, false,
            Duration::from_secs(2), super::super::wire_bytes::init_shared_wire_byte_counters());
        let mut a = make_swarm(&a_key);
        let mut b = make_swarm(&b_key);
        b.listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap()).unwrap();
        let listening = Arc::new(Mutex::new(Vec::new()));
        let reachability = Arc::new(Mutex::new(super::super::reachability::Libp2pReachabilitySnapshot::default()));
        let mut pending_dht = HashMap::new();
        let mut pending_records = HashSet::new();
        let mut pending_requests = HashMap::new();
        let mut pending_connected = HashSet::new();
        let mut connected_cooldowns = HashMap::new();
        let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
        let timeout = tokio::time::sleep(Duration::from_secs(5)).fuse();
        futures::pin_mut!(timeout);
        loop {
            futures::select! {
                event = b.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        listening.lock().unwrap().push(address.clone());
                        super::super::swarm_behaviour::dial_addr_with_optional_peer_id(&mut a,
                            address.with(libp2p::multiaddr::Protocol::P2p(b_peer))).unwrap();
                    }
                    SwarmEvent::Behaviour(super::super::swarm_behaviour::BehaviourEvent::RequestResponse(
                        request_response::Event::Message { message: request_response::Message::Request { request, channel, .. }, .. }
                    )) => {
                        let payload = super::super::discovery::handle_request_response_request(&request,
                            &HashMap::new(), Some(&template), &b_key, &listening, &reachability, true,
                            &PeerRecordCache::default()).unwrap();
                        b.behaviour_mut().request_response.send_response(channel,
                            oasis7_proto::distributed_net::NetworkResponse { payload }).unwrap();
                    }
                    _ => {}
                },
                event = a.select_next_some() => match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == b_peer => {
                        if deny_cooldown {
                            connected_cooldowns.insert(b_peer, super::super::now_ms() + 60_000);
                        }
                        refresh_due_records(&mut a, RefreshState { cache: &mut cache,
                            pending_dht: &mut pending_dht, pending_records: &mut pending_records,
                            pending_requests: &mut pending_requests, pending_connected: &mut pending_connected,
                            connected_cooldowns: &mut connected_cooldowns }, Some(&template), 1_000, a_peer, &metrics);
                        assert!(pending_dht.is_empty());
                        assert_eq!(cache.get(&b_peer), Some(&old));
                        if deny_cooldown {
                            assert!(pending_requests.is_empty());
                            assert!(pending_connected.is_empty());
                            assert_eq!(a.behaviour().discovery_budget.snapshot().admitted, 0);
                            assert!(a.is_connected(&b_peer));
                            break;
                        }
                        assert_eq!(pending_requests.len(), 1);
                    }
                    SwarmEvent::Behaviour(super::super::swarm_behaviour::BehaviourEvent::RequestResponse(
                        request_response::Event::Message { message: request_response::Message::Response { request_id, response }, .. }
                    )) => {
                        assert!(pending_requests.remove(&request_id).is_some());
                        a.behaviour_mut().discovery_budget.finish_request(request_id);
                        let renewed = super::super::discovery::decode_optional_peer_record_response(&response.payload).unwrap().unwrap();
                        super::super::peer_record::verify_signed_peer_record(&renewed).unwrap();
                        assert_eq!(renewed.record.peer_id, b_peer.to_string());
                        assert!(renewed.record.published_at_ms > old.record.published_at_ms);
                        assert_eq!(super::super::super::util::to_canonical_cbor(&renewed).unwrap(), response.payload);
                        cache.insert(b_peer, renewed.clone());
                        assert_eq!(cache.get(&b_peer), Some(&renewed));
                        assert!(a.is_connected(&b_peer));
                        break;
                    }
                    _ => {}
                },
                () = timeout => panic!("direct refresh did not return over TCP"),
            }
        }
    });
}
