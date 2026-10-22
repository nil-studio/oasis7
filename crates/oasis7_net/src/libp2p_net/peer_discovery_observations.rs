use std::collections::{BTreeSet, HashMap, HashSet};

use crate::error::WorldError;
use libp2p::PeerId;
use oasis7_proto::distributed_dht::{PeerDiscoverySource, SignedPeerRecord};

const MAX_RECORD_OBSERVATIONS: usize = 1024;
const MAX_SIGNED_PAYLOAD_BYTES: usize = 16 * 1024;
const MAX_INGRESSES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DiscoveryIngress {
    Dht { upstream: Option<PeerId> },
    PeerExchange { upstream: PeerId },
    Direct { peer: PeerId },
}

#[derive(Debug)]
struct RecordObservation {
    signed_payload: Vec<u8>,
    ingresses: Vec<DiscoveryIngress>,
    observed_at_ms: i64,
}

/// Local evidence is deliberately separate from the immutable signed claim.
#[derive(Debug, Default)]
pub(super) struct DiscoveryObservations {
    static_bootstrap_peers: HashSet<PeerId>,
    records: HashMap<PeerId, RecordObservation>,
}

impl DiscoveryObservations {
    pub(super) fn set_static_bootstrap_peers(&mut self, peers: HashSet<PeerId>) {
        self.static_bootstrap_peers = peers;
    }

    pub(super) fn record(
        &mut self,
        record: &SignedPeerRecord,
        ingress: DiscoveryIngress,
        now_ms: i64,
    ) -> Result<(), WorldError> {
        let peer = record
            .record
            .peer_id
            .parse::<PeerId>()
            .map_err(|_| invalid_observation("invalid observation peer identity"))?;
        if matches!(ingress, DiscoveryIngress::Direct { peer: direct } if direct != peer) {
            return Err(invalid_observation("direct observation identity mismatch"));
        }
        let payload = serde_cbor::to_vec(record)?;
        if payload.len() > MAX_SIGNED_PAYLOAD_BYTES {
            return Err(invalid_observation(
                "observation payload exceeds local capacity",
            ));
        }
        if let Some(previous) = self.records.get_mut(&peer) {
            // Regressing wall time must not refresh evidence or replace newer observations.
            if now_ms < previous.observed_at_ms {
                return Err(invalid_observation("observation clock regressed"));
            }
            if previous.signed_payload == payload {
                if !previous.ingresses.contains(&ingress)
                    && previous.ingresses.len() < MAX_INGRESSES
                {
                    previous.ingresses.push(ingress);
                }
                previous.observed_at_ms = now_ms;
                return Ok(());
            }
        }
        if !self.records.contains_key(&peer)
            && self.records.len() >= MAX_RECORD_OBSERVATIONS
            && let Some(oldest) = self
                .records
                .iter()
                .min_by_key(|(peer, observation)| (observation.observed_at_ms, peer.to_string()))
                .map(|(peer, _)| *peer)
        {
            self.records.remove(&oldest);
        }
        self.records.insert(
            peer,
            RecordObservation {
                signed_payload: payload,
                ingresses: vec![ingress],
                observed_at_ms: now_ms,
            },
        );
        Ok(())
    }

    fn matching(&self, record: &SignedPeerRecord) -> Option<&RecordObservation> {
        let peer = record.record.peer_id.parse::<PeerId>().ok()?;
        let observation = self.records.get(&peer)?;
        let payload = serde_cbor::to_vec(record).ok()?;
        (observation.signed_payload == payload).then_some(observation)
    }

    pub(super) fn is_static_bootstrap(&self, record: &SignedPeerRecord) -> bool {
        record
            .record
            .peer_id
            .parse::<PeerId>()
            .ok()
            .is_some_and(|peer| self.static_bootstrap_peers.contains(&peer))
    }

    pub(super) fn observed_sources(&self, record: &SignedPeerRecord) -> Vec<PeerDiscoverySource> {
        let mut sources = Vec::new();
        if self.is_static_bootstrap(record) {
            sources.push(PeerDiscoverySource::StaticBootstrap);
        }
        if let Some(observation) = self.matching(record) {
            for ingress in &observation.ingresses {
                let source = match ingress {
                    DiscoveryIngress::Dht { .. } => PeerDiscoverySource::Dht,
                    DiscoveryIngress::PeerExchange { .. } => PeerDiscoverySource::PeerExchange,
                    DiscoveryIngress::Direct { .. } => continue,
                };
                if !sources.contains(&source) {
                    sources.push(source);
                }
            }
        }
        sources
    }

    pub(super) fn trusted_source_labels(
        &self,
        record: &SignedPeerRecord,
    ) -> BTreeSet<&'static str> {
        // A channel or a different upstream identity is not an independent control domain.
        // Current configured seeds remain usable for bootstrap material acquisition.
        if self.is_static_bootstrap(record) {
            BTreeSet::from(["static_bootstrap"])
        } else {
            BTreeSet::new()
        }
    }
}

fn invalid_observation(reason: &str) -> WorldError {
    WorldError::NetworkProtocolUnavailable {
        protocol: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_proto::distributed_dht::{PeerDeploymentMode, PeerNodeRole, PeerReachabilityClass};
    fn sample_record(
        peer_id: PeerId,
        discovery_sources: Vec<PeerDiscoverySource>,
    ) -> SignedPeerRecord {
        SignedPeerRecord {
            record: oasis7_proto::distributed_dht::PeerRecord {
                peer_id: peer_id.to_string(),
                node_id: format!("node-{peer_id}"),
                world_id: "world-a".to_string(),
                network_id: "world-a".to_string(),
                node_role: PeerNodeRole::FullStorage.as_str().to_string(),
                deployment_mode: PeerDeploymentMode::Hybrid,
                reachability_class: PeerReachabilityClass::Hybrid,
                direct_addrs: Vec::new(),
                hole_punch_addrs: Vec::new(),
                relay_addrs: Vec::new(),
                discovery_sources,
                capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
                source_operator: None,
                source_asn: None,
                published_at_ms: 0,
                ttl_ms: 60_000,
            },
            identity_public_key_protobuf_hex: "abcd".to_string(),
            signature_hex: "beef".to_string(),
        }
    }

    #[test]
    fn observations_bind_exact_signed_payload_without_mutation() {
        let peer = PeerId::random();
        let record = sample_record(
            peer,
            vec![
                PeerDiscoverySource::StaticBootstrap,
                PeerDiscoverySource::Manual,
            ],
        );
        let original = serde_cbor::to_vec(&record).unwrap();
        let mut observations = DiscoveryObservations::default();
        observations
            .record(&record, DiscoveryIngress::Dht { upstream: None }, 10)
            .unwrap();
        observations
            .record(
                &record,
                DiscoveryIngress::PeerExchange {
                    upstream: PeerId::random(),
                },
                11,
            )
            .unwrap();
        assert_eq!(serde_cbor::to_vec(&record).unwrap(), original);
        assert_eq!(
            observations.observed_sources(&record),
            vec![PeerDiscoverySource::Dht, PeerDiscoverySource::PeerExchange]
        );
        assert!(observations.trusted_source_labels(&record).is_empty());
        let mut replacement = record.clone();
        replacement.record.published_at_ms += 1;
        assert!(observations.observed_sources(&replacement).is_empty());
        observations
            .record(&replacement, DiscoveryIngress::Direct { peer }, 12)
            .unwrap();
        assert!(observations.observed_sources(&record).is_empty());
    }

    #[test]
    fn local_static_configuration_is_revocable_and_not_a_signed_claim() {
        let peer = PeerId::random();
        let record = sample_record(peer, vec![PeerDiscoverySource::StaticBootstrap]);
        let mut observations = DiscoveryObservations::default();
        assert!(!observations.is_static_bootstrap(&record));
        observations.set_static_bootstrap_peers(HashSet::from([peer]));
        assert!(observations.is_static_bootstrap(&record));
        assert_eq!(
            observations.trusted_source_labels(&record),
            BTreeSet::from(["static_bootstrap"])
        );
        observations.set_static_bootstrap_peers(HashSet::new());
        assert!(!observations.is_static_bootstrap(&record));
        assert!(observations.observed_sources(&record).is_empty());
    }

    #[test]
    fn clock_rollback_and_incorrect_direct_sender_cannot_refresh_evidence() {
        let peer = PeerId::random();
        let record = sample_record(peer, vec![PeerDiscoverySource::Dht]);
        let mut observations = DiscoveryObservations::default();
        observations
            .record(&record, DiscoveryIngress::Dht { upstream: None }, 10)
            .unwrap();
        assert!(
            observations
                .record(&record, DiscoveryIngress::Direct { peer }, 9)
                .is_err()
        );
        assert!(
            observations
                .record(
                    &record,
                    DiscoveryIngress::Direct {
                        peer: PeerId::random()
                    },
                    11
                )
                .is_err()
        );
        let observation = observations.matching(&record).unwrap();
        assert_eq!(observation.observed_at_ms, 10);
        assert_eq!(observation.ingresses.len(), 1);
    }

    #[test]
    fn observation_storage_is_bounded_and_eviction_loses_trust() {
        let mut observations = DiscoveryObservations::default();
        let first = sample_record(PeerId::random(), vec![PeerDiscoverySource::Dht]);
        observations
            .record(&first, DiscoveryIngress::Dht { upstream: None }, 0)
            .unwrap();
        for timestamp in 1..=MAX_RECORD_OBSERVATIONS {
            let record = sample_record(PeerId::random(), vec![PeerDiscoverySource::Dht]);
            for _ in 0..MAX_INGRESSES + 2 {
                observations
                    .record(
                        &record,
                        DiscoveryIngress::PeerExchange {
                            upstream: PeerId::random(),
                        },
                        timestamp as i64,
                    )
                    .unwrap();
            }
        }
        assert_eq!(observations.records.len(), MAX_RECORD_OBSERVATIONS);
        assert!(observations.matching(&first).is_none());
        assert!(
            observations
                .records
                .values()
                .all(|record| record.ingresses.len() <= MAX_INGRESSES)
        );
    }
}
