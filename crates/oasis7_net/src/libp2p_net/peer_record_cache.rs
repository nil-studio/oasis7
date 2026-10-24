use super::peer_record_refresh::{REFRESH_BATCH, REFRESH_RETRY, RecordRefreshSchedule};
use super::peer_record_time::{DiscoveryClock, RecordLease};
use super::transport_paths::TransportPath;
use crate::error::WorldError;
use libp2p::PeerId;
use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, SignedPeerRecord};
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

const MAX_RECORDS: usize = 1024;
const MAX_PAYLOAD_BYTES: usize = crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES;

#[derive(Debug)]
struct CachedRecord {
    signed: SignedPeerRecord,
    lease: RecordLease,
    was_live: bool,
    refresh: RecordRefreshSchedule,
}

/// Candidate freshness is checked at every read. Expired metadata retained for an
/// established session restricts that session's capabilities, never new discovery.
#[derive(Debug)]
pub(super) struct PeerRecordCache {
    records: HashMap<PeerId, CachedRecord>,
    clock: DiscoveryClock,
    refresh_random: RandomState,
    dial_policy: super::peer_record_dial_gate::DiscoveryDialPolicy,
    receiver: super::receiver_floor::SharedPeerRecordReceiver,
}

impl Default for PeerRecordCache {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            clock: DiscoveryClock::default(),
            refresh_random: RandomState::new(),
            dial_policy: Default::default(),
            receiver: super::receiver_floor::PeerRecordReceiver::legacy(),
        }
    }
}

impl PeerRecordCache {
    #[cfg(test)]
    pub(super) fn new(clock: DiscoveryClock) -> Self {
        Self {
            records: HashMap::new(),
            clock,
            refresh_random: RandomState::new(),
            dial_policy: Default::default(),
            receiver: super::receiver_floor::PeerRecordReceiver::legacy(),
        }
    }
    #[cfg(test)]
    pub(super) fn with_dial_policy(
        clock: DiscoveryClock,
        dial_policy: super::peer_record_dial_gate::DiscoveryDialPolicy,
    ) -> Self {
        Self::with_dial_policy_and_receiver(
            clock,
            dial_policy,
            super::receiver_floor::PeerRecordReceiver::legacy(),
        )
    }

    pub(super) fn with_dial_policy_and_receiver(
        clock: DiscoveryClock,
        dial_policy: super::peer_record_dial_gate::DiscoveryDialPolicy,
        receiver: super::receiver_floor::SharedPeerRecordReceiver,
    ) -> Self {
        Self {
            records: HashMap::new(),
            clock,
            refresh_random: RandomState::new(),
            dial_policy,
            receiver,
        }
    }
    pub(super) fn clock(&self) -> &DiscoveryClock {
        &self.clock
    }
    pub(super) fn get(&self, peer: &PeerId) -> Option<&SignedPeerRecord> {
        let cached = self.records.get(peer)?;
        cached.lease.validate().ok()?;
        Some(&cached.signed)
    }
    /// Return a live cache candidate or, when one exists, the current bounded
    /// receiver-floor tombstone. Tombstones are forwarding evidence only: they
    /// are time-checked on every read and never enter the discovery cache.
    pub(super) fn get_for_forwarding(&self, peer: &PeerId) -> Option<SignedPeerRecord> {
        if let Some(record) = self.get(peer)
            && record
                .publication
                .as_ref()
                .map(|publication| publication.state)
                != Some(PeerRecordPublicationStateV2::Tombstone)
        {
            return Some(record.clone());
        }
        let tombstone = self
            .receiver
            .current_tombstone_for_forwarding(*peer)
            .ok()??;
        self.clock.check(&tombstone.record).ok()?;
        self.receiver.lease_guard(&tombstone).ok()?;
        Some(tombstone)
    }
    pub(super) fn session_record(&self, peer: &PeerId) -> Option<&SignedPeerRecord> {
        self.records.get(peer).map(|cached| &cached.signed)
    }
    pub(super) fn contains_key(&self, peer: &PeerId) -> bool {
        self.get(peer).is_some()
    }
    pub(super) fn values(&self) -> impl Iterator<Item = &SignedPeerRecord> {
        self.records
            .values()
            .filter(|cached| cached.lease.validate().is_ok())
            .map(|cached| &cached.signed)
    }
    pub(super) fn keys(&self) -> impl Iterator<Item = &PeerId> {
        self.records
            .iter()
            .filter(|(_, cached)| cached.lease.validate().is_ok())
            .map(|(peer, _)| peer)
    }
    pub(super) fn prepare(
        &self,
        peer: PeerId,
        signed: &SignedPeerRecord,
    ) -> Result<RecordLease, WorldError> {
        if signed.record.peer_id != peer.to_string()
            || serde_cbor::to_vec(&signed)?.len() > MAX_PAYLOAD_BYTES
        {
            return Err(rejected("peer record identity or payload capacity invalid"));
        }
        let mut lease = self.clock.check(&signed.record)?;
        if let Some(old) = self.records.get(&peer) {
            if super::receiver_floor::same_publication_identity(&old.signed, signed) {
                old.lease.validate()?;
                lease = old.lease.clone();
            }
        } else if self.records.len() >= MAX_RECORDS {
            return Err(rejected("peer record cache capacity exhausted"));
        }
        Ok(lease)
    }
    pub(super) fn install(&mut self, peer: PeerId, signed: SignedPeerRecord, lease: RecordLease) {
        if self.records.get(&peer).is_some_and(|old| {
            super::receiver_floor::same_publication_identity(&old.signed, &signed)
        }) {
            return;
        }
        let mut hash = self.refresh_random.build_hasher();
        peer.hash(&mut hash);
        signed.signature_hex.hash(&mut hash);
        let refresh =
            RecordRefreshSchedule::new(lease.deadline(), signed.record.ttl_ms, hash.finish());
        if let Ok(paths) = super::transport_paths::peer_record_transport_paths(&signed) {
            let _ = self.dial_policy.install(
                peer,
                paths.into_iter().map(|path| path.addr),
                lease.clone(),
            );
        }
        self.records.insert(
            peer,
            CachedRecord {
                signed,
                lease,
                was_live: true,
                refresh,
            },
        );
    }

    pub(super) fn receiver(&self) -> super::receiver_floor::SharedPeerRecordReceiver {
        Arc::clone(&self.receiver)
    }

    /// Revoke discovery and autonomous dial authority without discarding metadata
    /// still needed to constrain an already established session. The receiver
    /// lease makes candidate reads fail immediately; `prune` removes the record
    /// once no active transport path depends on its session restrictions.
    pub(super) fn invalidate_candidate(&mut self, peer: PeerId) {
        self.dial_policy.remove_record_paths(peer);
    }
    pub(super) fn take_due_refreshes(&mut self, now: Instant) -> Vec<PeerId> {
        let mut due: Vec<_> = self
            .records
            .iter()
            .filter(|(_, cached)| {
                cached.refresh.eligible_at() <= now
                    && now < cached.lease.deadline()
                    && cached.lease.is_valid()
            })
            .map(|(peer, cached)| (cached.refresh.eligible_at(), peer.to_string(), *peer))
            .collect();
        due.sort_unstable();
        due.truncate(REFRESH_BATCH);
        for (_, _, peer) in &due {
            if let Some(cached) = self.records.get_mut(peer) {
                cached.refresh.retry_not_before = now
                    .checked_add(REFRESH_RETRY)
                    .unwrap_or(cached.lease.deadline());
            }
        }
        due.into_iter().map(|(_, _, peer)| peer).collect()
    }
    pub(super) fn prune(&mut self, active: &HashMap<PeerId, TransportPath>) -> bool {
        let mut changed = false;
        self.records.retain(|peer, cached| {
            let live = cached.lease.is_valid();
            changed |= live != cached.was_live;
            cached.was_live = live;
            active.contains_key(peer) || live
        });
        changed
    }
    #[cfg(test)]
    pub(super) fn insert(&mut self, peer: PeerId, record: SignedPeerRecord) {
        let lease = self
            .prepare(peer, &record)
            .expect("valid live fixture record");
        self.install(peer, record, lease);
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.values().next().is_none()
    }
}
fn rejected(message: &str) -> WorldError {
    WorldError::NetworkProtocolUnavailable {
        protocol: message.to_string(),
    }
}

#[cfg(test)]
impl<const N: usize> From<[(PeerId, SignedPeerRecord); N]> for PeerRecordCache {
    fn from(records: [(PeerId, SignedPeerRecord); N]) -> Self {
        let mut cache = Self::default();
        for (peer, record) in records {
            let lease = cache
                .prepare(peer, &record)
                .expect("valid fixture peer record");
            cache.install(peer, record, lease);
        }
        cache
    }
}
#[cfg(test)]
impl std::ops::Index<&PeerId> for PeerRecordCache {
    type Output = SignedPeerRecord;
    fn index(&self, peer: &PeerId) -> &Self::Output {
        self.get(peer).expect("live fixture record")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libp2p_net::tests::signed_discovery_peer_record;
    use crate::libp2p_net::transport_paths::peer_record_transport_paths;
    use libp2p::identity::Keypair;

    #[test]
    fn candidate_capacity_is_bounded_and_active_expired_metadata_is_retained() {
        let key = Keypair::generate_ed25519();
        let original =
            signed_discovery_peer_record(&key, vec![], crate::libp2p_net::utils::now_ms());
        let mut cache = PeerRecordCache::default();
        let mut peers = Vec::new();
        for _ in 0..MAX_RECORDS {
            // Cache tests exercise capacity after signature verification at ingress.
            let peer = PeerId::random();
            let mut record = original.clone();
            record.record.peer_id = peer.to_string();
            let lease = cache.prepare(peer, &record).unwrap();
            cache.install(peer, record, lease);
            peers.push(peer);
        }
        assert_eq!(cache.records.len(), MAX_RECORDS);
        let overflow = PeerId::from(key.public());
        assert!(cache.prepare(overflow, &original).is_err());
        let active_peer = peers[0];
        let mut active_record = original.clone();
        active_record.record.peer_id = active_peer.to_string();
        let path = peer_record_transport_paths(&active_record)
            .unwrap()
            .remove(0);
        cache
            .clock
            .observe(
                original.record.expires_at_ms().unwrap(),
                std::time::Instant::now(),
            )
            .unwrap();
        assert!(cache.prune(&HashMap::from([(active_peer, path)])));
        assert_eq!(cache.records.len(), 1);
        assert!(cache.get(&active_peer).is_none());
        assert!(cache.session_record(&active_peer).is_some());
        cache.prune(&HashMap::new());
        assert!(cache.records.is_empty());
    }
    #[test]
    fn identical_reinstall_preserves_refresh_attempt_and_original_deadline() {
        let key = Keypair::generate_ed25519();
        let peer = PeerId::from(key.public());
        let signed = signed_discovery_peer_record(&key, vec![], crate::libp2p_net::now_ms());
        let mut cache = PeerRecordCache::default();
        cache.insert(peer, signed.clone());
        let deadline = cache.records[&peer].lease.deadline();
        let due = cache.records[&peer].refresh.due;
        assert_eq!(cache.take_due_refreshes(due), vec![peer]);
        let attempted = cache.records[&peer].refresh;
        let mut pending = super::super::kad_queries::PendingDhtQuery::DiscoverPeerRecord {
            peer_id: peer,
            upstream: None,
            record: None,
            error: None,
        };
        let action = super::super::kad_queries::handle_dht_progress(
            &mut pending,
            libp2p::kad::QueryResult::GetRecord(Ok(libp2p::kad::GetRecordOk::FoundRecord(
                libp2p::kad::PeerRecord {
                    peer: Some(PeerId::random()),
                    record: libp2p::kad::Record {
                        key: libp2p::kad::RecordKey::new(&"replayed-peer-record"),
                        value: crate::util::to_canonical_cbor(&signed).unwrap(),
                        publisher: None,
                        expires: None,
                    },
                },
            ))),
            true,
            cache.clock(),
        );
        let super::super::kad_queries::DhtProgressAction::DiscoverPeerRecord { result, .. } =
            action
        else {
            panic!("DHT refresh must decode the original signed response");
        };
        let replayed = result.unwrap().unwrap();
        let source = replayed.source();
        let replayed = replayed
            .consume(
                source,
                super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::KadDiscoveryPeerRecordResult,
            )
            .unwrap();
        let lease = cache.prepare(peer, &replayed).unwrap();
        cache.install(peer, replayed, lease);
        assert_eq!(cache.records[&peer].lease.deadline(), deadline);
        assert_eq!(cache.records[&peer].refresh, attempted);
        assert!(cache.take_due_refreshes(due).is_empty());
        let mut renewed = signed.record;
        renewed.ttl_ms = 120_000;
        let renewed = crate::libp2p_net::peer_record::sign_peer_record(&renewed, &key).unwrap();
        cache.insert(peer, renewed);
        assert!(cache.records[&peer].lease.deadline() > deadline);
        assert!(cache.records[&peer].refresh.due > attempted.due);
    }

    #[test]
    fn refresh_batch_is_fair_bounded_and_expiry_never_hot_loops() {
        let key = Keypair::generate_ed25519();
        let record = signed_discovery_peer_record(&key, vec![], crate::libp2p_net::now_ms());
        let mut cache = PeerRecordCache::default();
        for _ in 0..10 {
            let peer = PeerId::random();
            let mut record = record.clone();
            record.record.peer_id = peer.to_string();
            cache.insert(peer, record);
        }
        let deadline = cache
            .records
            .values()
            .map(|record| record.lease.deadline())
            .max()
            .unwrap();
        let now = deadline - std::time::Duration::from_secs(1);
        let mut seen = std::collections::HashSet::new();
        for count in [4, 4, 2, 0] {
            let batch = cache.take_due_refreshes(now);
            assert_eq!(batch.len(), count);
            assert!(batch.into_iter().all(|peer| seen.insert(peer)));
        }
        assert_eq!(seen.len(), 10);
        assert!(cache.take_due_refreshes(deadline).is_empty());
        assert!(
            cache
                .take_due_refreshes(deadline + std::time::Duration::from_secs(5))
                .is_empty()
        );
    }
}
