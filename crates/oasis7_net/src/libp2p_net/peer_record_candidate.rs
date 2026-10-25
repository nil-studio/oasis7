use std::collections::{HashMap, HashSet};

use libp2p::PeerId;
use libp2p::swarm::Swarm;
use oasis7_proto::distributed_dht::{PeerRecord, PeerRecordPublicationStateV2, SignedPeerRecord};

use crate::error::WorldError;

use super::peer_discovery_observations::{DiscoveryIngress, DiscoveryObservations};
use super::peer_manager::{
    PeerManagerHealthStatus, PeerManagerPolicy, recompute_peer_manager_healths,
};
use super::peer_record::validate_discovered_peer_record;
use super::peer_record_admission::{AdmittedPeerRecord, PeerRecordAdmissionWorkUnit};
use super::peer_record_cache::PeerRecordCache;
use super::rendezvous_trigger::RendezvousTriggerContext;
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
#[cfg(test)]
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
    install_admitted_peer_record_with_rendezvous_triggers(
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
        admitted,
        actual_source,
        work_unit,
        ingress,
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate installation preserves the shared admission mutation boundary"
)]
pub(super) fn install_admitted_peer_record_with_rendezvous_triggers(
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
    ingress: DiscoveryIngress,
    rendezvous_triggers: Option<&RendezvousTriggerContext>,
) -> Result<(), PeerRecordInstallError> {
    let evidence = rendezvous_triggers
        .filter(|triggers| {
            actual_source.is_some()
                && triggers.target().to_string() == admitted.record().record.peer_id
        })
        .map(|triggers| triggers.active_evidence(std::time::Instant::now()))
        .unwrap_or_default();
    let result = admitted
        .consume(actual_source, work_unit)
        .map_err(PeerRecordInstallError::from)
        .and_then(|record| {
            process_signed_peer_record_with_rendezvous_triggers(
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
                &evidence,
            )
        });
    if let Some(triggers) = rendezvous_triggers {
        // Completing either a live admission or a tombstone consumes the one-shot
        // association for every concurrent DHT/RR path sharing this context.
        triggers.clear();
    }
    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "A DHT candidate is committed with its exact ingress and local rendezvous association"
)]
pub(super) fn install_dht_discovered_peer_record(
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
    upstream: Option<PeerId>,
    rendezvous_triggers: Option<&RendezvousTriggerContext>,
    event_errors: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    max_error_messages: usize,
) -> Result<(), PeerRecordInstallError> {
    let observed_record = admitted.record().clone();
    let attempt_evidence = super::rendezvous_trigger::active_attempt_evidence(
        &observed_record,
        upstream,
        rendezvous_triggers,
    );
    install_admitted_peer_record_with_rendezvous_triggers(
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
        admitted,
        upstream,
        PeerRecordAdmissionWorkUnit::KadDiscoveryPeerRecordResult,
        DiscoveryIngress::Dht { upstream },
        rendezvous_triggers,
    )?;
    if !attempt_evidence.is_empty() {
        super::rendezvous_trigger::record_linked_observation_diagnostic(
            &observed_record,
            &format!("dht-upstream={upstream:?}"),
            discovery_observations,
            &attempt_evidence,
            event_errors,
            max_error_messages,
        );
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate admission preflights all local projections before the durable floor transaction"
)]
#[cfg(test)]
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
    process_signed_peer_record_with_rendezvous_triggers(
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
        &[],
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Candidate admission preflights all local projections before the durable floor transaction"
)]
fn process_signed_peer_record_with_rendezvous_triggers(
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
    rendezvous_triggers: &[super::rendezvous_trigger::RendezvousTriggerEvidence],
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
    let is_tombstone = record
        .publication
        .as_ref()
        .is_some_and(|publication| publication.state == PeerRecordPublicationStateV2::Tombstone);
    let allow_single_source_bootstrap_dial = static_bootstrap_peer_ids.contains(&peer_id);
    let paths = if is_tombstone {
        Vec::new()
    } else {
        peer_record_transport_paths(&record)?
    };
    let is_conflict = receiver.would_conflict(&record)?;
    let mut lease = if is_conflict || is_tombstone {
        None
    } else {
        Some(discovered_peer_records.prepare(peer_id, &record)?)
    };
    let observed_at_ms = super::now_ms();
    if is_tombstone {
        // A tombstone is not a candidate lease, but its signed TTL and local
        // clock policy still apply before the durable floor is advanced.
        discovered_peer_records.clock().check(&record.record)?;
    } else if !is_conflict {
        discovery_observations.preflight_record(&record, &ingress, observed_at_ms)?;
    }

    let admission = receiver.admit_serialized(&record)?;
    if admission == super::receiver_floor::ReceiverAdmission::Conflict {
        revoke_candidate_projection(
            peer_id,
            discovered_peer_records,
            discovery_observations,
            known_transport_paths,
            last_dialed_transport_paths,
            failed_transport_path_labels,
        );
        return Err(PeerRecordInstallError::ReceiverConflict(
            WorldError::NetworkProtocolUnavailable {
                protocol:
                    "peer record sequence conflicts with the durable receiver floor; peer isolated"
                        .into(),
            },
        ));
    }

    if is_tombstone {
        revoke_candidate_projection(
            peer_id,
            discovered_peer_records,
            discovery_observations,
            known_transport_paths,
            last_dialed_transport_paths,
            failed_transport_path_labels,
        );
        return Ok(());
    }

    if admission == super::receiver_floor::ReceiverAdmission::Duplicate
        && discovered_peer_records
            .get(&peer_id)
            .is_some_and(|existing| {
                super::receiver_floor::same_publication_identity(existing, &record)
            })
    {
        // A semantically identical signed claim is idempotent: it cannot refresh
        // the old observation, candidate lease, or scheduled refresh. Only the
        // exact already-observed signed payload may merge local trigger evidence.
        discovery_observations
            .merge_rendezvous_triggers_for_exact_record(&record, rendezvous_triggers);
        return Ok(());
    }

    let guard = receiver.lease_guard(&record)?;
    let lease = lease
        .take()
        .expect("non-conflicting receiver admission has a prepared candidate lease")
        .with_receiver_guard(guard);
    discovery_observations.record_with_rendezvous_triggers(
        &record,
        ingress,
        observed_at_ms,
        rendezvous_triggers,
    )?;
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

fn revoke_candidate_projection(
    peer_id: PeerId,
    discovered_peer_records: &mut PeerRecordCache,
    discovery_observations: &mut DiscoveryObservations,
    known_transport_paths: &mut HashMap<PeerId, Vec<TransportPath>>,
    last_dialed_transport_paths: &mut HashMap<PeerId, TransportPath>,
    failed_transport_path_labels: &mut HashSet<String>,
) {
    discovered_peer_records.invalidate_candidate(peer_id);
    discovery_observations.remove(peer_id);
    last_dialed_transport_paths.remove(&peer_id);
    let remove_paths = if let Some(paths) = known_transport_paths.get_mut(&peer_id) {
        paths.retain(|path| {
            if path.record_lease.is_some() {
                failed_transport_path_labels.remove(&path.label());
                false
            } else {
                true
            }
        });
        paths.is_empty()
    } else {
        false
    };
    if remove_paths {
        known_transport_paths.remove(&peer_id);
    }
}
