use std::collections::{HashMap, HashSet};

use libp2p::PeerId;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed_dht::{PeerRecord, SignedPeerRecord};

use super::peer_manager::PeerManagerPolicy;
use crate::error::WorldError;

use super::discovery::process_discovered_peer_record;
use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_record_admission::{AdmittedPeerRecord, PeerRecordAdmissionWorkUnit};
use super::peer_record_cache::PeerRecordCache;
use super::swarm_behaviour::Behaviour;
use super::transport_paths::TransportPath;

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate installation must consume the authenticated ingress permit at the shared mutation boundary"
)]
pub(super) fn install_admitted_peer_record(
    swarm: &mut Swarm<Behaviour>,
    discovered_peer_records: &mut PeerRecordCache,
    discovery_observations: &mut DiscoveryObservations,
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    active_transport_paths: &HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
    template: Option<&PeerRecord>,
    peer_manager_policy: &PeerManagerPolicy,
    static_bootstrap_peer_ids: &HashSet<PeerId>,
    admitted: AdmittedPeerRecord,
    actual_source: Option<PeerId>,
    work_unit: PeerRecordAdmissionWorkUnit,
    ingress: super::peer_discovery_observations::DiscoveryIngress,
) -> Result<(), WorldError> {
    let record: SignedPeerRecord = admitted.consume(actual_source, work_unit)?;
    process_discovered_peer_record(
        swarm,
        discovered_peer_records,
        discovery_observations,
        known_transport_paths,
        last_dialed_transport_paths,
        active_transport_paths,
        failed_transport_path_labels,
        template,
        peer_manager_policy,
        static_bootstrap_peer_ids,
        record,
        ingress,
    )
}
