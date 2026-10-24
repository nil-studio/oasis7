//! Apply application peer-record validation before Kademlia stores or forwards records.
use std::borrow::Cow;
use std::collections::HashMap;

use libp2p::PeerId;
use libp2p::kad::store::{self, MemoryStore, MemoryStoreConfig, RecordStore};
use libp2p::kad::{ProviderRecord, Record, RecordKey};
use oasis7_proto::distributed::{DHT_PEER_RECORDS_SUFFIX, DHT_WORLD_PREFIX, dht_peer_record_key};
use oasis7_proto::distributed_dht::SignedPeerRecord;

use super::peer_record::decode_peer_record;
#[cfg(test)]
use super::peer_record_admission::PeerRecordAdmissionBudget;
use super::peer_record_admission::{
    PeerRecordAdmissionPermit, PeerRecordAdmissionWorkUnit, SharedPeerRecordAdmissionBudget,
};
use super::peer_record_time::{DiscoveryClock, RecordLease};
#[cfg(test)]
use super::receiver_floor::PeerRecordReceiver;
use super::receiver_floor::{ReceiverAdmission, ReceiverLeaseGuard, SharedPeerRecordReceiver};

pub(super) struct PeerRecordStore {
    inner: MemoryStore,
    clock: DiscoveryClock,
    // Exactly one entry per stored peer record; removals and pruning clear it.
    leases: HashMap<RecordKey, RecordLease>,
    admission_budget: SharedPeerRecordAdmissionBudget,
    receiver: SharedPeerRecordReceiver,
    max_records: usize,
    max_value_bytes: usize,
}

impl PeerRecordStore {
    #[cfg(test)]
    pub(super) fn with_config(peer: PeerId, config: MemoryStoreConfig) -> Self {
        Self::with_admission_budget(peer, config, PeerRecordAdmissionBudget::shared())
    }

    #[cfg(test)]
    pub(super) fn with_admission_budget(
        peer: PeerId,
        config: MemoryStoreConfig,
        admission_budget: SharedPeerRecordAdmissionBudget,
    ) -> Self {
        Self::with_admission_budget_and_receiver(
            peer,
            config,
            admission_budget,
            PeerRecordReceiver::legacy(),
        )
    }

    pub(super) fn with_admission_budget_and_receiver(
        peer: PeerId,
        config: MemoryStoreConfig,
        admission_budget: SharedPeerRecordAdmissionBudget,
        receiver: SharedPeerRecordReceiver,
    ) -> Self {
        let max_records = config.max_records;
        let max_value_bytes = config.max_value_bytes;
        Self {
            inner: MemoryStore::with_config(peer, config),
            clock: DiscoveryClock::default(),
            leases: HashMap::new(),
            admission_budget,
            receiver,
            max_records,
            max_value_bytes,
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

    fn checked_record(&self, record: &Record) -> store::Result<(SignedPeerRecord, RecordLease)> {
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
        let lease = self
            .clock
            .check(&signed.record)
            .map_err(|_| store::Error::ValueTooLarge)?;
        Ok((signed, lease))
    }

    fn eligible(&self, record: &Record) -> bool {
        if !Self::peer_record_key(&record.key) {
            return true;
        }
        self.leases
            .get(&record.key)
            .is_some_and(RecordLease::is_valid)
            && self.checked_record(record).is_ok()
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
        let receiver = std::sync::Arc::clone(&self.receiver);
        let _transaction = receiver
            .begin_ingress()
            .map_err(|_| store::Error::ValueTooLarge)?;
        let (signed, mut lease) = self.checked_record(&record)?;
        self.receiver
            .preflight_record(&signed)
            .map_err(|_| store::Error::ValueTooLarge)?;
        let same_sequence_conflict = self
            .receiver
            .would_conflict(&signed)
            .map_err(|_| store::Error::ValueTooLarge)?;
        let previous_same_identity = if !same_sequence_conflict {
            if let Some(previous) = self.inner.get(&record.key) {
                let same_identity = if signed.publication.is_some() {
                    super::receiver_floor::same_publication_identity(
                        &decode_peer_record(&previous.value)
                            .map_err(|_| store::Error::ValueTooLarge)?,
                        &signed,
                    )
                } else {
                    previous.value == record.value
                };
                if same_identity {
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
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !same_sequence_conflict {
            if record.value.len() >= self.max_value_bytes {
                return Err(store::Error::ValueTooLarge);
            }
            self.prune();
            if self.inner.get(&record.key).is_none()
                && self.inner.records().count() >= self.max_records
            {
                return Err(store::Error::MaxRecords);
            }
        }
        permit
            .consume(None, PeerRecordAdmissionWorkUnit::StorePut, &record.value)
            .map_err(|_| store::Error::ValueTooLarge)?;
        let admission = self
            .receiver
            .admit_serialized(&signed)
            .map_err(|_| store::Error::ValueTooLarge)?;
        if admission == ReceiverAdmission::Conflict {
            self.inner.remove(&record.key);
            self.leases.remove(&record.key);
            return Err(store::Error::ValueTooLarge);
        }
        if admission == ReceiverAdmission::Duplicate && previous_same_identity {
            return Ok(());
        }
        let receiver_guard: ReceiverLeaseGuard = self
            .receiver
            .lease_guard(&signed)
            .map_err(|_| store::Error::ValueTooLarge)?;
        lease = lease.with_receiver_guard(receiver_guard);
        record.expires = Some(
            record
                .expires
                .map_or(lease.deadline(), |expiry| expiry.min(lease.deadline())),
        );
        let key = record.key.clone();
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
    use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};
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

    #[cfg(unix)]
    #[test]
    fn durable_receiver_store_rejects_rollback_and_persists_same_sequence_conflict() {
        use super::super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
        use super::super::receiver_floor::PeerRecordReceiver;
        use std::os::unix::fs::PermissionsExt;

        let local_key = Keypair::generate_ed25519();
        let local_template = super::super::tests::signed_discovery_peer_record(
            &local_key,
            vec![],
            super::super::utils::now_ms(),
        )
        .record;
        let temp = tempfile::tempdir_in(
            std::env::temp_dir()
                .canonicalize()
                .expect("canonical temp root"),
        )
        .expect("receiver temp directory");
        let directory = temp.path().join("receiver");
        std::fs::create_dir(&directory).expect("create receiver directory");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .expect("set private receiver directory");
        let config = PeerRecordReceiverConfig {
            mode: PeerRecordReceiverMode::DurableCompatible,
            directory: Some(directory),
        };
        PeerRecordReceiver::initialize(&config, &local_key, &local_template)
            .expect("initialize durable receiver");
        let receiver = PeerRecordReceiver::open(&config, &local_key, Some(&local_template))
            .expect("open durable receiver");
        let mut store = PeerRecordStore::with_admission_budget_and_receiver(
            PeerId::random(),
            MemoryStoreConfig::default(),
            PeerRecordAdmissionBudget::shared(),
            receiver.clone(),
        );
        let remote_key = Keypair::generate_ed25519();
        let peer = PeerId::from(remote_key.public());
        let mut materialized = super::super::tests::signed_discovery_peer_record(
            &remote_key,
            vec![],
            super::super::utils::now_ms(),
        )
        .record;
        materialized.direct_addrs.clear();
        let sign = |record: &oasis7_proto::distributed_dht::PeerRecord, sequence| {
            super::super::peer_record::sign_peer_record_v2(
                record,
                PeerRecordPublicationV2 {
                    schema_version: 2,
                    sequence,
                    state: PeerRecordPublicationStateV2::Live,
                },
                &remote_key,
            )
            .expect("sign V2 record")
        };
        let make_store_record = |signed: &oasis7_proto::distributed_dht::SignedPeerRecord| Record {
            key: RecordKey::new(&dht_peer_record_key(
                &signed.record.world_id,
                &signed.record.peer_id,
            )),
            value: crate::util::to_canonical_cbor(signed).expect("encode peer record"),
            publisher: None,
            expires: None,
        };
        let high = sign(&materialized, 2);
        let key = RecordKey::new(&dht_peer_record_key("world-a", &peer.to_string()));
        store
            .put(make_store_record(&high))
            .expect("accept first durable floor");
        let lower = sign(&materialized, 1);
        assert!(store.put(make_store_record(&lower)).is_err());
        assert_eq!(
            decode_peer_record(&store.get(&key).expect("stored high record").value)
                .expect("decode stored record")
                .publication
                .expect("V2 publication")
                .sequence,
            2
        );

        materialized.node_id.push_str("-fork");
        let conflict = sign(&materialized, 2);
        assert!(store.put(make_store_record(&conflict)).is_err());
        assert!(store.get(&key).is_none(), "conflict must isolate old value");
        drop(store);
        drop(receiver);

        let reopened = PeerRecordReceiver::open(&config, &local_key, Some(&local_template))
            .expect("reopen durable receiver after conflict");
        assert!(
            reopened.preflight_record(&high).is_err(),
            "same-sequence conflict evidence must keep the peer isolated after restart"
        );
    }

    #[cfg(unix)]
    #[test]
    fn store_capacity_failures_do_not_advance_the_durable_floor() {
        use super::super::config::{PeerRecordReceiverConfig, PeerRecordReceiverMode};
        use super::super::receiver_floor::PeerRecordReceiver;
        use std::os::unix::fs::PermissionsExt;

        let local_key = Keypair::generate_ed25519();
        let local_template = super::super::tests::signed_discovery_peer_record(
            &local_key,
            vec![],
            super::super::utils::now_ms(),
        )
        .record;
        let temp = tempfile::tempdir_in(
            std::env::temp_dir()
                .canonicalize()
                .expect("canonical temp root"),
        )
        .expect("receiver temp directory");
        let directory = temp.path().join("receiver");
        std::fs::create_dir(&directory).expect("create receiver directory");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .expect("set private receiver directory");
        let receiver_config = PeerRecordReceiverConfig {
            mode: PeerRecordReceiverMode::DurableCompatible,
            directory: Some(directory),
        };
        PeerRecordReceiver::initialize(&receiver_config, &local_key, &local_template)
            .expect("initialize durable receiver");
        let receiver =
            PeerRecordReceiver::open(&receiver_config, &local_key, Some(&local_template))
                .expect("open durable receiver");

        let remote_key = Keypair::generate_ed25519();
        let mut remote_record = super::super::tests::signed_discovery_peer_record(
            &remote_key,
            vec![],
            super::super::utils::now_ms(),
        )
        .record;
        remote_record.direct_addrs.clear();
        let sign = |record: &oasis7_proto::distributed_dht::PeerRecord, sequence| {
            super::super::peer_record::sign_peer_record_v2(
                record,
                PeerRecordPublicationV2 {
                    schema_version: 2,
                    sequence,
                    state: PeerRecordPublicationStateV2::Live,
                },
                &remote_key,
            )
            .expect("sign V2 record")
        };
        let first = sign(&remote_record, 1);
        let first_bytes = crate::util::to_canonical_cbor(&first).expect("encode first V2 record");
        let max_value_bytes = first_bytes.len() + 1;
        let store_config = MemoryStoreConfig {
            max_records: 1,
            max_value_bytes,
            ..MemoryStoreConfig::default()
        };
        let mut store = PeerRecordStore::with_admission_budget_and_receiver(
            PeerId::random(),
            store_config,
            PeerRecordAdmissionBudget::shared(),
            receiver.clone(),
        );
        let occupied = Record {
            key: RecordKey::new(&"occupied-non-peer-key"),
            value: vec![1],
            publisher: None,
            expires: None,
        };
        store
            .put(occupied.clone())
            .expect("fill the only MemoryStore record slot");
        let v2_key = RecordKey::new(&dht_peer_record_key(
            &first.record.world_id,
            &first.record.peer_id,
        ));
        let first_record = Record {
            key: v2_key.clone(),
            value: first_bytes.clone(),
            publisher: None,
            expires: None,
        };
        assert!(store.put(first_record).is_err());
        assert_eq!(store.get(&occupied.key).as_deref(), Some(&occupied));
        assert!(
            receiver.lease_guard(&first).is_err(),
            "a full store must not commit the first V2 floor"
        );

        store.remove(&occupied.key);
        store
            .put(Record {
                key: v2_key.clone(),
                value: first_bytes.clone(),
                publisher: None,
                expires: None,
            })
            .expect("commit first V2 record after releasing capacity");
        assert!(receiver.lease_guard(&first).is_ok());

        remote_record
            .node_id
            .push_str("-larger-value-that-exceeds-store-capacity");
        let higher = sign(&remote_record, 2);
        let higher_bytes =
            crate::util::to_canonical_cbor(&higher).expect("encode higher V2 record");
        assert!(higher_bytes.len() >= max_value_bytes);
        assert!(
            store
                .put(Record {
                    key: v2_key.clone(),
                    value: higher_bytes,
                    publisher: None,
                    expires: None,
                })
                .is_err(),
            "MemoryStore value capacity must be checked before advancing the floor"
        );
        assert_eq!(
            store.get(&v2_key).expect("retain previous record").value,
            first_bytes
        );
        assert!(
            receiver.lease_guard(&higher).is_err(),
            "a rejected higher record must not advance the durable floor"
        );
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
