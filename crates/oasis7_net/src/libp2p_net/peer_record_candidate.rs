use std::collections::{HashMap, HashSet};

use libp2p::PeerId;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed_dht::{PeerRecord, SignedPeerRecord};

use crate::error::WorldError;

use super::peer_discovery_observations::{DiscoveryIngress, DiscoveryObservations};
use super::peer_manager::{
    PeerManagerHealthStatus, PeerManagerPolicy, recompute_peer_manager_healths,
};
use super::peer_record::validate_discovered_peer_record;
use super::peer_record_admission::{AdmittedPeerRecord, PeerRecordAdmissionWorkUnit};
use super::peer_record_cache::PeerRecordCache;
use super::swarm_behaviour::Behaviour;
use super::transport_paths::{
    TransportPath, dial_transport_path, peer_record_transport_paths,
    select_preferred_transport_path, sync_known_transport_paths,
};

#[derive(Debug)]
pub(super) enum PeerRecordInstallError {
    Rejected(WorldError),
    ReceiverConflict(WorldError),
}

impl PeerRecordInstallError {
    pub(super) fn invalidates_peer_projection(&self) -> bool {
        matches!(self, Self::ReceiverConflict(_))
    }

    pub(super) fn into_world_error(self) -> WorldError {
        match self {
            Self::Rejected(error) | Self::ReceiverConflict(error) => error,
        }
    }
}

impl From<WorldError> for PeerRecordInstallError {
    fn from(error: WorldError) -> Self {
        Self::Rejected(error)
    }
}

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
) -> Result<(), PeerRecordInstallError> {
    let record: SignedPeerRecord = admitted.consume(actual_source, work_unit)?;
    process_signed_peer_record(
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

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate admission preflights all local projections before the durable floor transaction"
)]
pub(super) fn process_signed_peer_record(
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
    record: SignedPeerRecord,
    ingress: DiscoveryIngress,
) -> Result<(), PeerRecordInstallError> {
    let receiver = discovered_peer_records.receiver();
    let _transaction = receiver.begin_ingress()?;
    validate_discovered_peer_record(&record, template)?;
    receiver.preflight_record(&record)?;
    let peer_id = record.record.peer_id.parse::<PeerId>().map_err(|_| {
        WorldError::NetworkProtocolUnavailable {
            protocol: "peer record peer_id must be valid".to_string(),
        }
    })?;
    let allow_single_source_bootstrap_dial = static_bootstrap_peer_ids.contains(&peer_id);
    let paths = peer_record_transport_paths(&record)?;
    let is_conflict = receiver.would_conflict(&record)?;
    let mut lease = if is_conflict {
        None
    } else {
        Some(discovered_peer_records.prepare(peer_id, &record)?)
    };
    let observed_at_ms = super::now_ms();
    if !is_conflict {
        discovery_observations.preflight_record(&record, &ingress, observed_at_ms)?;
    }

    let admission = receiver.admit_serialized(&record)?;
    if admission == super::receiver_floor::ReceiverAdmission::Conflict {
        let previous_paths = known_transport_paths.remove(&peer_id).unwrap_or_default();
        for path in previous_paths {
            failed_transport_path_labels.remove(&path.label());
        }
        discovered_peer_records.invalidate_candidate(peer_id);
        discovery_observations.remove(peer_id);
        last_dialed_transport_paths.remove(&peer_id);
        return Err(PeerRecordInstallError::ReceiverConflict(
            WorldError::NetworkProtocolUnavailable {
                protocol:
                    "peer record sequence conflicts with the durable receiver floor; peer isolated"
                        .into(),
            },
        ));
    }

    if admission == super::receiver_floor::ReceiverAdmission::Duplicate
        && discovered_peer_records
            .get(&peer_id)
            .is_some_and(|existing| {
                super::receiver_floor::same_publication_identity(existing, &record)
            })
    {
        // A semantically identical signed claim is idempotent: it cannot refresh
        // the old observation, candidate lease, or scheduled refresh.
        return Ok(());
    }

    let guard = receiver.lease_guard(&record)?;
    let lease = lease
        .take()
        .expect("non-conflicting receiver admission has a prepared candidate lease")
        .with_receiver_guard(guard);
    discovery_observations.record(&record, ingress, observed_at_ms)?;
    discovery_observations.set_static_bootstrap_peers(static_bootstrap_peer_ids.clone());
    discovered_peer_records.install(peer_id, record.clone(), lease.clone());
    let transport_paths: Vec<_> = paths
        .into_iter()
        .map(|mut path| {
            path.record_lease = Some(lease.clone());
            path
        })
        .collect();
    sync_known_transport_paths(
        known_transport_paths,
        failed_transport_path_labels,
        peer_id,
        transport_paths.clone(),
    );
    let peer_healths = recompute_peer_manager_healths(
        discovered_peer_records,
        discovery_observations,
        active_transport_paths,
        peer_manager_policy,
    );
    let peer_status = peer_healths
        .get(&peer_id)
        .map(|health| health.status)
        .unwrap_or(PeerManagerHealthStatus::Candidate);
    if let Some(preferred_path) =
        select_preferred_transport_path(transport_paths.as_slice(), failed_transport_path_labels)
    {
        let already_dialing_preferred = last_dialed_transport_paths
            .get(&peer_id)
            .map(|path| path.label() == preferred_path.label())
            .unwrap_or(false);
        let should_dial = (!matches!(peer_status, PeerManagerHealthStatus::Blocked)
            && !matches!(peer_status, PeerManagerHealthStatus::Suspect))
            || (matches!(peer_status, PeerManagerHealthStatus::Suspect)
                && allow_single_source_bootstrap_dial);
        let should_dial = should_dial
            && active_transport_paths
                .get(&peer_id)
                .map(|active| preferred_path.preference_rank() < active.preference_rank())
                .unwrap_or(true);
        if should_dial && !already_dialing_preferred {
            let _ = dial_transport_path(swarm, last_dialed_transport_paths, preferred_path);
        }
    }
    Ok(())
}
