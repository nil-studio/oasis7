use super::peer_record_time::{DiscoveryClock, RecordLease};
use super::transport_paths::TransportPath;
use crate::error::WorldError;
use libp2p::PeerId;
use oasis7_proto::distributed_dht::SignedPeerRecord;
use std::collections::HashMap;

const MAX_RECORDS: usize = 1024;
const MAX_PAYLOAD_BYTES: usize = crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES;

#[derive(Debug)]
struct CachedRecord {
    signed: SignedPeerRecord,
    lease: RecordLease,
    was_live: bool,
}

/// Candidate freshness is checked at every read. Expired metadata retained for an
/// established session restricts that session's capabilities, never new discovery.
#[derive(Debug, Default)]
pub(super) struct PeerRecordCache {
    records: HashMap<PeerId, CachedRecord>,
    clock: DiscoveryClock,
    dial_policy: super::peer_record_dial_gate::DiscoveryDialPolicy,
}

impl PeerRecordCache {
    #[cfg(test)]
    pub(super) fn new(clock: DiscoveryClock) -> Self {
        Self {
            records: HashMap::new(),
            clock,
            dial_policy: Default::default(),
        }
    }
    pub(super) fn with_dial_policy(
        clock: DiscoveryClock,
        dial_policy: super::peer_record_dial_gate::DiscoveryDialPolicy,
    ) -> Self {
        Self {
            records: HashMap::new(),
            clock,
            dial_policy,
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
            if &old.signed == signed {
                old.lease.validate()?;
                lease = old.lease.clone();
            }
        } else if self.records.len() >= MAX_RECORDS {
            return Err(rejected("peer record cache capacity exhausted"));
        }
        Ok(lease)
    }
    pub(super) fn install(&mut self, peer: PeerId, signed: SignedPeerRecord, lease: RecordLease) {
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
            },
        );
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
}
