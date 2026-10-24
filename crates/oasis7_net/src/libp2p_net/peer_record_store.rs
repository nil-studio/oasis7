//! Apply application peer-record validation before Kademlia stores or forwards records.
use std::borrow::Cow;
use std::collections::HashMap;

use libp2p::PeerId;
use libp2p::kad::store::{self, MemoryStore, MemoryStoreConfig, RecordStore};
use libp2p::kad::{ProviderRecord, Record, RecordKey};
use oasis7_proto::distributed::{DHT_PEER_RECORDS_SUFFIX, DHT_WORLD_PREFIX, dht_peer_record_key};

use super::peer_record::decode_peer_record;
#[cfg(test)]
use super::peer_record_admission::PeerRecordAdmissionBudget;
use super::peer_record_admission::{
    PeerRecordAdmissionPermit, PeerRecordAdmissionWorkUnit, SharedPeerRecordAdmissionBudget,
};
use super::peer_record_time::{DiscoveryClock, RecordLease};

pub(super) struct PeerRecordStore {
    inner: MemoryStore,
    clock: DiscoveryClock,
    // Exactly one entry per stored peer record; removals and pruning clear it.
    leases: HashMap<RecordKey, RecordLease>,
    admission_budget: SharedPeerRecordAdmissionBudget,
}

impl PeerRecordStore {
    #[cfg(test)]
    pub(super) fn with_config(peer: PeerId, config: MemoryStoreConfig) -> Self {
        Self::with_admission_budget(peer, config, PeerRecordAdmissionBudget::shared())
    }

    pub(super) fn with_admission_budget(
        peer: PeerId,
        config: MemoryStoreConfig,
        admission_budget: SharedPeerRecordAdmissionBudget,
    ) -> Self {
        Self {
            inner: MemoryStore::with_config(peer, config),
            clock: DiscoveryClock::default(),
            leases: HashMap::new(),
            admission_budget,
        }
    }

    pub(super) fn clock(&self) -> &DiscoveryClock {
        &self.clock
    }

    pub(super) fn peer_record_key(key: &RecordKey) -> bool {
        let Ok(text) = std::str::from_utf8(key.as_ref()) else {
            return false;
        };
        text.strip_prefix(DHT_WORLD_PREFIX)
            .and_then(|rest| rest.strip_prefix('/'))
            .and_then(|rest| rest.rsplit_once(&format!("/{DHT_PEER_RECORDS_SUFFIX}/")))
            .is_some_and(|(_, peer)| peer.parse::<PeerId>().is_ok())
    }

    fn checked_lease(&self, record: &Record) -> store::Result<RecordLease> {
        // RecordStore's error enum has no invalid-record variant. Reject using its
        // existing refusal error; no malformed application record reaches MemoryStore.
        if record.value.len() > crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES {
            return Err(store::Error::ValueTooLarge);
        }
        let signed = decode_peer_record(&record.value).map_err(|_| store::Error::ValueTooLarge)?;
        let expected = dht_peer_record_key(&signed.record.world_id, &signed.record.peer_id);
        if record.key != RecordKey::new(&expected) {
            return Err(store::Error::ValueTooLarge);
        }
        self.clock
            .check(&signed.record)
            .map_err(|_| store::Error::ValueTooLarge)
    }

    fn eligible(&self, record: &Record) -> bool {
        if !Self::peer_record_key(&record.key) {
            return true;
        }
        self.leases
            .get(&record.key)
            .is_some_and(RecordLease::is_valid)
            && self.checked_lease(record).is_ok()
            && record
                .expires
                .is_none_or(|expiry| std::time::Instant::now() < expiry)
    }

    fn prune(&mut self) {
        let leases = &self.leases;
        self.inner.retain(|key, record| {
            !Self::peer_record_key(key)
                || (leases.get(key).is_some_and(RecordLease::is_valid)
                    && record
                        .expires
                        .is_none_or(|expiry| std::time::Instant::now() < expiry))
        });
        self.leases.retain(|key, _| self.inner.get(key).is_some());
    }
}

impl RecordStore for PeerRecordStore {
    type RecordsIter<'a> = std::vec::IntoIter<Cow<'a, Record>>;
    type ProvidedIter<'a> = <MemoryStore as RecordStore>::ProvidedIter<'a>;

    fn get(&self, key: &RecordKey) -> Option<Cow<'_, Record>> {
        self.inner.get(key).filter(|record| self.eligible(record))
    }

    fn put(&mut self, mut record: Record) -> store::Result<()> {
        if !Self::peer_record_key(&record.key) {
            self.prune();
            return self.inner.put(record);
        }
        let permit = PeerRecordAdmissionPermit::admit_payload(
            &self.admission_budget,
            None,
            PeerRecordAdmissionWorkUnit::StorePut,
            &record.value,
            crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
        )
        .map_err(|_| store::Error::ValueTooLarge)?;
        let mut lease = self.checked_lease(&record)?;
        permit
            .consume(None, PeerRecordAdmissionWorkUnit::StorePut, &record.value)
            .map_err(|_| store::Error::ValueTooLarge)?;
        if let Some(previous) = self.inner.get(&record.key)
            && previous.value == record.value
        {
            let old = self
                .leases
                .get(&record.key)
                .ok_or(store::Error::ValueTooLarge)?;
            if !old.is_valid()
                || previous
                    .expires
                    .is_some_and(|expiry| std::time::Instant::now() >= expiry)
            {
                return Err(store::Error::ValueTooLarge);
            }
            if old.deadline() < lease.deadline() {
                lease = old.clone();
            }
            record.expires = match (record.expires, previous.expires) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
        record.expires = Some(
            record
                .expires
                .map_or(lease.deadline(), |expiry| expiry.min(lease.deadline())),
        );
        let key = record.key.clone();
        self.prune();
        self.inner.put(record)?;
        self.leases.insert(key, lease);
        Ok(())
    }

    fn remove(&mut self, key: &RecordKey) {
        self.inner.remove(key);
        self.leases.remove(key);
    }

    fn records(&self) -> Self::RecordsIter<'_> {
        self.inner
            .records()
            .filter(|record| self.eligible(record))
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn add_provider(&mut self, record: ProviderRecord) -> store::Result<()> {
        self.inner.add_provider(record)
    }
    fn providers(&self, key: &RecordKey) -> Vec<ProviderRecord> {
        self.inner.providers(key)
    }
    fn provided(&self) -> Self::ProvidedIter<'_> {
        self.inner.provided()
    }
    fn remove_provider(&mut self, key: &RecordKey, peer: &PeerId) {
        self.inner.remove_provider(key, peer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use std::time::Instant;

    fn store() -> PeerRecordStore {
        PeerRecordStore::with_config(PeerId::random(), MemoryStoreConfig::default())
    }

    fn signed_record() -> Record {
        let signed = super::super::tests::signed_discovery_peer_record(
            &Keypair::generate_ed25519(),
            vec![],
            super::super::utils::now_ms(),
        );
        Record {
            key: RecordKey::new(&dht_peer_record_key(
                &signed.record.world_id,
                &signed.record.peer_id,
            )),
            value: crate::util::to_canonical_cbor(&signed).unwrap(),
            publisher: None,
            expires: None,
        }
    }

    #[test]
    fn inbound_peer_store_rejects_malformed_wrong_key_tampered_and_oversized_values() {
        let mut store = store();
        let valid = signed_record();
        let mut malformed = valid.clone();
        malformed.value = vec![0xff];
        assert!(store.put(malformed).is_err());
        let mut wrong_key = valid.clone();
        wrong_key.key = RecordKey::new(&dht_peer_record_key(
            "wrong-world",
            &PeerId::random().to_string(),
        ));
        assert!(store.put(wrong_key).is_err());
        let mut tampered = valid.clone();
        let mut signed = decode_peer_record(&tampered.value).unwrap();
        signed.record.node_id.push_str("-tampered");
        tampered.value = crate::util::to_canonical_cbor(&signed).unwrap();
        assert!(store.put(tampered).is_err());
        let mut oversized = valid;
        oversized.value = vec![0; crate::libp2p_net::peer_record::MAX_SIGNED_PEER_RECORD_BYTES + 1];
        assert!(store.put(oversized).is_err());
        assert_eq!(store.records().count(), 0);
        assert!(store.leases.is_empty());
    }

    #[test]
    fn inbound_store_enforces_time_and_never_extends_duplicate_lease() {
        let mut store = store();
        let record = signed_record();
        store.put(record.clone()).unwrap();
        let deadline = store.get(&record.key).unwrap().expires.unwrap();
        store.put(record.clone()).unwrap();
        let repeated_deadline = store.get(&record.key).unwrap().expires.unwrap();
        // Forward wall samples may shorten the lease, including millisecond
        // quantization advances. Retransmission must never extend it.
        assert!(repeated_deadline <= deadline);
        assert!(repeated_deadline <= store.leases[&record.key].deadline());
        for published in [
            super::super::utils::now_ms() - 60_001,
            super::super::utils::now_ms() + 60_000,
        ] {
            let signed = super::super::tests::signed_discovery_peer_record(
                &Keypair::generate_ed25519(),
                vec![],
                published,
            );
            let rejected = Record {
                key: RecordKey::new(&dht_peer_record_key(
                    &signed.record.world_id,
                    &signed.record.peer_id,
                )),
                value: crate::util::to_canonical_cbor(&signed).unwrap(),
                publisher: None,
                expires: None,
            };
            assert!(store.put(rejected).is_err());
        }
        // Simulate Kademlia's shorter monotonic expiry without sleeping or changing signed bytes.
        store.inner.retain(|_, stored| {
            stored.expires = Some(Instant::now());
            true
        });
        assert!(store.get(&record.key).is_none());
        assert_eq!(store.records().count(), 0);
        assert!(store.put(record).is_err());
    }

    #[test]
    fn store_rechecks_signature_before_get_and_replication_iteration() {
        let mut store = store();
        let record = signed_record();
        store.put(record.clone()).unwrap();
        store.inner.retain(|_, stored| {
            stored.value = vec![0xff];
            true
        });
        assert!(store.get(&record.key).is_none());
        assert_eq!(store.records().count(), 0);
    }

    #[test]
    fn unrelated_records_providers_and_metadata_capacity_are_preserved() {
        let local = PeerId::random();
        let mut store = PeerRecordStore::with_config(
            local,
            MemoryStoreConfig {
                max_records: 1,
                ..Default::default()
            },
        );
        for _ in 0..16 {
            let record = signed_record();
            store.put(record.clone()).unwrap();
            assert_eq!(store.leases.len(), 1);
            store.remove(&record.key);
            assert!(store.leases.is_empty());
        }
        let unrelated = Record {
            key: RecordKey::new(&"/aw/world/peer-records/head"),
            value: vec![0xff],
            publisher: None,
            expires: None,
        };
        store.put(unrelated.clone()).unwrap();
        assert_eq!(store.get(&unrelated.key).unwrap().as_ref(), &unrelated);
        assert_eq!(store.records().count(), 1);
        assert!(store.leases.is_empty());
        let provider = ProviderRecord::new(RecordKey::new(&"content"), local, vec![]);
        store.add_provider(provider.clone()).unwrap();
        assert_eq!(store.providers(&provider.key), vec![provider.clone()]);
        assert_eq!(store.provided().count(), 1);
        store.remove_provider(&provider.key, &local);
        assert!(store.providers(&provider.key).is_empty());
        assert_eq!(store.provided().count(), 0);
    }
    #[test]
    fn nested_world_peer_record_keys_cannot_bypass_store_time_policy() {
        let key = libp2p::identity::Keypair::generate_ed25519();
        for world in ["tenant/world", ""] {
            let mut record = crate::libp2p_net::tests::signed_discovery_peer_record(
                &key,
                vec![],
                crate::libp2p_net::utils::now_ms(),
            )
            .record;
            record.world_id = world.into();
            let mut store =
                PeerRecordStore::with_config(PeerId::random(), MemoryStoreConfig::default());
            let dht_key = RecordKey::new(&dht_peer_record_key(&record.world_id, &record.peer_id));
            let signed = crate::libp2p_net::peer_record::sign_peer_record(&record, &key).unwrap();
            store
                .put(Record {
                    key: dht_key.clone(),
                    value: serde_cbor::to_vec(&signed).unwrap(),
                    publisher: None,
                    expires: None,
                })
                .unwrap();
            assert!(store.get(&dht_key).is_some());
            record.published_at_ms -= record.ttl_ms;
            let signed = crate::libp2p_net::peer_record::sign_peer_record(&record, &key).unwrap();
            assert!(
                store
                    .put(Record {
                        key: dht_key.clone(),
                        value: serde_cbor::to_vec(&signed).unwrap(),
                        publisher: None,
                        expires: None
                    })
                    .is_err()
            );
            assert!(
                store.get(&dht_key).is_some(),
                "invalid update must preserve prior payload"
            );
        }
        let mut store =
            PeerRecordStore::with_config(PeerId::random(), MemoryStoreConfig::default());
        let head = Record {
            key: RecordKey::new(&oasis7_proto::distributed::dht_world_head_key(
                "tenant/peer-records",
            )),
            value: b"ordinary head".to_vec(),
            publisher: None,
            expires: None,
        };
        store.put(head.clone()).unwrap();
        assert_eq!(store.get(&head.key).unwrap().value, head.value);
        assert!(store.leases.is_empty());
    }
}
