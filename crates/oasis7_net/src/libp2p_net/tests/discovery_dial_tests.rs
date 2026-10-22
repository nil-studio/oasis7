use super::super::peer_discovery_observations::{DiscoveryIngress, DiscoveryObservations};
use super::signed_discovery_peer_record;
use crate::libp2p_net::PeerManagerPolicy;
use libp2p::PeerId;
use libp2p::identity::Keypair;
use std::collections::{HashMap, HashSet};

#[test]
fn process_discovered_peer_record_dials_only_locally_configured_seed() {
    crate::libp2p_net::runtime_support::run_on_libp2p_test_runtime(|| {
        for configured_seed in [false, true] {
            let mut swarm = crate::libp2p_net::swarm_behaviour::build_swarm(
                &Keypair::generate_ed25519(),
                false,
                true,
                std::time::Duration::from_secs(30),
                crate::libp2p_net::wire_bytes::init_shared_wire_byte_counters(),
            );
            let peer_key = Keypair::generate_ed25519();
            let record = signed_discovery_peer_record(
                &peer_key,
                vec![
                    crate::dht::PeerDiscoverySource::StaticBootstrap,
                    crate::dht::PeerDiscoverySource::Dht,
                    crate::dht::PeerDiscoverySource::Rendezvous,
                ],
                1,
            );
            let peer_id = PeerId::from(peer_key.public());
            let mut discovered_peer_records = HashMap::new();
            let mut known_transport_paths = HashMap::new();
            let mut last_dialed_transport_paths = HashMap::new();
            let active_transport_paths = HashMap::new();
            let mut failed_transport_path_labels = HashSet::new();

            crate::libp2p_net::discovery::process_discovered_peer_record(
                &mut swarm,
                &mut discovered_peer_records,
                &mut DiscoveryObservations::default(),
                &mut known_transport_paths,
                &mut last_dialed_transport_paths,
                &active_transport_paths,
                &mut failed_transport_path_labels,
                None,
                &PeerManagerPolicy::default(),
                &if configured_seed {
                    HashSet::from([peer_id])
                } else {
                    HashSet::new()
                },
                record.clone(),
                DiscoveryIngress::Dht {
                    upstream: Some(PeerId::random()),
                },
            )
            .expect("process candidate peer record");

            assert!(discovered_peer_records.contains_key(&peer_id));
            assert!(known_transport_paths.contains_key(&peer_id));
            assert_eq!(
                last_dialed_transport_paths.contains_key(&peer_id),
                configured_seed,
                "remote source claims must not grant bootstrap dial permission"
            );
        }
    });
}
