//! A genuine earlier Ready publication cannot gain budget authority from a
//! rewritten local Data/pin pair. The protected backend remains unchanged.
use super::*;
use oasis7_node::{NodeBftAdmissionInputPublicationIoV1, NodeBftAdmissionInputPublicationSlotV1};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
type Input = BftPrivateRetentionAdmissionInputV1;
type InputRef = BftPrivateRetentionAdmissionInputRefV1;

// Deliberately reproduce the original codec, including field order, rather
// than trusting a production helper to authorize the attack's replacement.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalData {
    schema_version: u32,
    domain: String,
    origin: bool,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    value: Option<BftHeaderValueV3>,
    actions_cbor: Vec<u8>,
    references: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_reservation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_original_data_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_activation: Option<serde_cbor::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protected_source_origin_ref: Option<String>,
}
struct Attack {
    store: LocalCasStore,
    records: PathBuf,
    backend: BftPrivateRetentionMemoryBackendV1,
    binding: String,
    calls: AtomicUsize,
    last_io_error: Mutex<Option<String>>,
    before: Mutex<Option<BftPrivateRetentionCurrentV1>>,
}
impl Attack {
    fn replace_old_refs(&self, input: &mut Input) -> Result<(), String> {
        let current = self
            .backend
            .clone()
            .load_private_retention_current(&self.binding)
            .map_err(|e| e.to_string())?;
        let ledger = current
            .retention
            .as_ref()
            .ok_or("attack needs genuine covered Some")?;
        let entry = ledger
            .map
            .entries
            .first()
            .ok_or("attack needs genuine earlier Some")?;
        if ledger.map.entries.len() != 1
            || entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready
            || current.signer.state.history.len() != 1
        {
            return Err(
                "attack must follow exactly one successful actual Ready publication".into(),
            );
        }
        let dir = self
            .records
            .join("formal-bft-header-prefix-v1/node-private-v1");
        let stem = format!("candidate-{:020}", entry.key.height);
        let path = dir.join(format!("{stem}.cbor"));
        let original = fs::read(&path).map_err(|_| "attack read genuine original Data")?;
        let mut data: LocalData =
            serde_cbor::from_slice(&original).map_err(|_| "attack decode original Data")?;
        if serde_cbor::to_vec(&data).map_err(|_| "attack original codec")? != original
            || data.schema_version != 9
            || data.source_activation.is_some()
        {
            return Err("attack must preserve actual original Data codec".into());
        }
        let added = input
            .objects
            .iter()
            .find(|object| {
                object.kind == BftPrivateRetentionInputObjectKindV1::Dependency
                    && !data.references.contains(&object.reference.digest)
            })
            .ok_or("second genuine candidate must introduce a new dependency")?;
        data.references.push(added.reference.digest.clone());
        data.references.sort();
        let forged = serde_cbor::to_vec(&data).map_err(|_| "attack encode changed refs")?;
        assert_ne!(forged, original);
        for suffix in [".cbor", ".reservation.cbor", ".intent.cbor"] {
            let path = dir.join(format!("{stem}{suffix}"));
            if path.exists() {
                fs::write(path, &forged).map_err(|_| "attack replace local slot")?;
            }
        }
        let digest = self
            .store
            .put_bytes(&forged)
            .map_err(|_| "attack store replaced Data")?;
        self.store
            .put_bytes(&added.bytes)
            .map_err(|_| "attack store extra dependency")?;
        let pins = data
            .references
            .iter()
            .cloned()
            .chain(data.publication_reservation_ref.iter().cloned())
            .chain(data.publication_original_data_ref.iter().cloned())
            .chain(data.protected_source_origin_ref.iter().cloned())
            .chain([digest])
            .collect::<BTreeSet<_>>();
        let shard = format!(
            "ledger-{}-height-{:020}",
            data.ledger_origin_digest, data.height
        );
        self.store
            .replace_pin_scope_shard("formal-bft-header-node-private-v3", &shard, &pins)
            .map_err(|_| "attack recompute local pin")?;
        self.store
            .verify_pin_scope_shard("formal-bft-header-node-private-v3", &shard, &pins)
            .map_err(|_| "attack verify recomputed pin")?;
        // The old flawed helper would discount this forged retained reference.
        input.publication_reservation_bytes = input
            .publication_reservation_bytes
            .checked_sub(added.bytes.len() as u64)
            .ok_or("attack budget subtraction")?;
        *self.before.lock().map_err(|_| "attack result lock")? = Some(current);
        Ok(())
    }
}
struct AttackIo {
    inner: Arc<dyn NodeBftAdmissionInputPublicationIoV1>,
    attack: Arc<Attack>,
}
struct AttackSlot {
    inner: Box<dyn NodeBftAdmissionInputPublicationSlotV1>,
    attack: Arc<Attack>,
}
impl NodeBftAdmissionInputPublicationSlotV1 for AttackSlot {
    fn publish(self: Box<Self>, mut input: Input) -> Result<InputRef, String> {
        let Self { inner, attack } = *self;
        let result = (|| {
            if attack.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                attack.replace_old_refs(&mut input)?;
            }
            inner.publish(input)
        })();
        if let Err(error) = &result {
            *attack.last_io_error.lock().unwrap() = Some(error.clone());
        }
        result
    }
}
impl NodeBftAdmissionInputPublicationIoV1 for AttackIo {
    fn reserve(&self) -> Result<Box<dyn NodeBftAdmissionInputPublicationSlotV1>, String> {
        Ok(Box::new(AttackSlot {
            inner: self.inner.reserve()?,
            attack: self.attack.clone(),
        }))
    }
    fn contains(&self, reference: &InputRef, deadline: Instant) -> Result<bool, String> {
        self.inner.contains(reference, deadline)
    }
    fn clear(&self, reference: &InputRef) -> Result<(), String> {
        self.inner.clear(reference)
    }
    fn read_original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, String> {
        self.inner.read_original(reference, deadline)
    }
}

#[test]
fn actual_source_rejects_local_ref_discount_without_changing_protected_originals() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
            let root = Scratch::new();
            let store = cas(root.path());
            let (actual, binding, _, source) = SourceFixture::prepare(&h, &root, &store);
            let attack = Arc::new(Attack {
                store: store.clone(),
                records: root.path().join("ledger"),
                backend: source.backend.clone(),
                binding: binding.digest().unwrap(),
                calls: AtomicUsize::new(0),
                last_io_error: Mutex::new(None),
                before: Mutex::new(None),
            });
            actual
                .lock()
                .unwrap()
                .configure_bft_v3_admission_input_io(Arc::new(AttackIo {
                    inner: Arc::new(source.noise.publisher.clone()),
                    attack: attack.clone(),
                }))
                .unwrap();
            let service = Arc::new(source.service(
                &binding,
                vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
                None,
            ));
            let (address, server) = tcp(service);
            let mut node = source.runtime(root.path(), &binding, address);
            node.submit_consensus_action_payload(34901, payload("budget-authority-original-agent"))
                .unwrap();
            node.start().unwrap();
            let deadline = Instant::now() + Duration::from_secs(90);
            while attack.before.lock().unwrap().is_none() {
                let status = node.bft_v3_status();
                let stopped = status.as_ref().is_some_and(|s| !s.running || s.blocked_reason.is_some());
                if stopped || Instant::now() >= deadline {
                    let current = source.backend.clone()
                        .load_private_retention_current(&binding.digest().unwrap()).unwrap();
                    let entries = current.retention.as_ref().map(|ledger| ledger.map.entries.iter()
                        .map(|entry| (entry.key.height, &entry.key.header_id, entry.lifecycle))
                        .collect::<Vec<_>>());
                    let first_some_ready = current.retention.as_ref().is_some_and(|ledger|
                        ledger.map.entries.iter().any(|entry|
                            entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready))
                        && !current.signer.state.history.is_empty();
                    eprintln!("source_budget_setup status={status:?} history_heights={:?} map_entries={entries:?} head_sequence={:?} io_publish_calls={} first_some_ready={} io_error={:?}",
                        current.signer.state.history.iter().map(|c| c.execution_commitment.height).collect::<Vec<_>>(),
                        current.signer.state.private_retention.as_ref().map(|head| head.sequence),
                        attack.calls.load(Ordering::SeqCst), first_some_ready,
                        attack.last_io_error.lock().unwrap());
                }
                assert!(
                    !stopped && Instant::now() < deadline,
                    "actual first Some and forged second publication must execute"
                );
                thread::sleep(Duration::from_millis(10));
            }
            // Publishing the inconsistent input is not a rejection receipt.
            // Keep the real Noise endpoint live until the provider finishes
            // validating it; stopping now could itself abort the read.
            loop {
                let status = node.bft_v3_status().expect("actual BFT worker status");
                if let Some(reason) = status.blocked_reason {
                    assert!(
                        reason.contains("local source budget slot differs from protected original Data/metadata"),
                        "expected actual protected-inventory rejection, got: {reason}"
                    );
                    break;
                }
                assert!(
                    status.running && Instant::now() < deadline,
                    "provider must reject the inconsistent original before shutdown"
                );
                thread::sleep(Duration::from_millis(10));
            }
            node.stop().unwrap();
            drop(node);
            server.join().unwrap().unwrap();
            let before = attack.before.lock().unwrap().take().unwrap();
            let after = source
                .backend
                .clone()
                .load_private_retention_current(&binding.digest().unwrap())
                .unwrap();
            assert_eq!(
                after.retention, before.retention,
                "local Data/ref/pin rewrite must reject before a protected reservation CAS"
            );
            assert_eq!(after.inputs.len(), before.inputs.len());
            for (digest, original) in &before.inputs {
                assert!(
                    Arc::ptr_eq(original, &after.inputs[digest]),
                    "protected original input must remain exact"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
