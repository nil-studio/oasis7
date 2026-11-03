//! Actual pending original assertions shared by the two full cold lifecycles.
use super::*;

pub(super) struct PendingColdWitness {
    original: BftPrivateRetentionCurrentV1,
    reference: BftPrivateRetentionAdmissionInputRefV1,
    domain: u32,
    signatures: usize,
}
impl PendingColdWitness {
    pub(super) fn capture(
        binding: &BftSignerBindingV3,
        backend: &mut BftPrivateRetentionMemoryBackendV1,
        namespace: &str,
        candidate: &oasis7_node::NodeBftPrivateCandidateV3,
        domain: u32,
        signs: &std::sync::atomic::AtomicUsize,
    ) -> Self {
        assert!(matches!(domain, 11 | 12));
        let original = backend
            .load_private_retention_current(&binding.digest().unwrap())
            .unwrap();
        let ledger = original.retention.as_ref().unwrap();
        let entry = ledger
            .map
            .entries
            .iter()
            .find(|entry| {
                entry.key.ledger_origin_digest == namespace
                    && entry.key.height == candidate.public_value.execution_commitment.height
                    && entry.key.header_id == candidate.public_value.block_id().unwrap()
            })
            .expect("real Pending Active entry");
        assert_eq!(
            entry.lifecycle,
            BftPrivateRetentionLifecycleV1::ReservedPending
        );
        let reference =
            BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, entry).unwrap();
        let input = original
            .inputs
            .get(&reference.admission_digest)
            .expect("original input under admission identity");
        reference.validate_input(input).unwrap();
        let data = input
            .objects
            .iter()
            .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
            .unwrap();
        let value: serde_cbor::Value = serde_cbor::from_slice(&data.bytes).unwrap();
        let serde_cbor::Value::Map(fields) = value else {
            panic!("actual Active input Data map")
        };
        assert_eq!(
            fields.get(&serde_cbor::Value::Text("schema_version".into())),
            Some(&serde_cbor::Value::Integer(domain.into()))
        );
        Self {
            original,
            reference,
            domain,
            signatures: signs.load(std::sync::atomic::Ordering::SeqCst),
        }
    }
    pub(super) fn recover(
        self,
        hook: &oasis7_node::NodeBftSharedExecutionHookV3,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        backend: &mut BftPrivateRetentionMemoryBackendV1,
        signs: &std::sync::atomic::AtomicUsize,
        deadline: Instant,
    ) {
        // Acquire legitimately changes the lease; the original admission must
        // survive that fence before invoking any recovery operation.
        let acquired = backend
            .load_private_retention_current(&binding.digest().unwrap())
            .unwrap();
        assert_eq!(acquired.retention, self.original.retention);
        assert_eq!(acquired.inputs.len(), self.original.inputs.len());
        for (key, input) in &self.original.inputs {
            assert!(Arc::ptr_eq(input, acquired.inputs.get(key).unwrap()));
        }
        let mut lease = super::ready_fault::NoReserveOnRecovery::new(lease);
        hook.lock()
            .unwrap()
            .prepare_bft_v3_execution(binding, &mut lease, deadline)
            .expect("same namespace original Pending resumes after full writer/Noise release");
        let after = backend
            .load_private_retention_current(&binding.digest().unwrap())
            .unwrap();
        let ledger = after.retention.as_ref().unwrap();
        let before = acquired.retention.as_ref().unwrap();
        let mut expected_map = before.map.clone();
        expected_map
            .entries
            .iter_mut()
            .find(|entry| entry.key == self.reference.key)
            .unwrap()
            .lifecycle = BftPrivateRetentionLifecycleV1::Ready;
        assert_eq!(
            ledger.map, expected_map,
            "the exact original map changes only Pending to Ready"
        );
        assert_eq!(ledger.map.entries.len(), before.map.entries.len());
        for entry in &before.map.entries {
            let actual = ledger
                .map
                .entries
                .iter()
                .find(|actual| actual.key == entry.key)
                .unwrap();
            let mut expected = entry.clone();
            if entry.key == self.reference.key {
                expected.lifecycle = BftPrivateRetentionLifecycleV1::Ready;
            }
            assert_eq!(
                actual, &expected,
                "cold recovery preserves every original fact/key and other alternative"
            );
        }
        assert_eq!(
            after.signer.state.revision,
            acquired.signer.state.revision.checked_add(1).unwrap(),
            "one original Ready operation, no repeated admission"
        );
        if self.domain == 12 {
            assert_eq!(
                ledger.selection_index, before.selection_index,
                "Received recovery cannot invent a Local choice"
            );
        } else {
            let selected = ledger
                .selection_index
                .as_ref()
                .expect("Local11 Ready authenticates its original choice");
            let old_sequence = before
                .selection_index
                .as_ref()
                .map_or(0, |index| index.last_sequence);
            let old_choices = before
                .selection_index
                .as_ref()
                .map_or(0, |index| index.selections.len());
            assert_eq!(selected.last_sequence, old_sequence.checked_add(1).unwrap());
            assert_eq!(selected.selections.len(), old_choices + 1);
            if let Some(previous) = &before.selection_index {
                for choice in &previous.selections {
                    assert!(selected.selections.contains(choice));
                }
            }
            let choice = selected
                .selections
                .iter()
                .find(|choice| choice.key == self.reference.key)
                .unwrap();
            assert_eq!(choice.reference, self.reference);
            assert_eq!(choice.source, self.reference.source);
            assert_eq!(
                choice.kind,
                BftPrivateRetentionSelectionKindV1::OriginalLocal
            );
        }
        for (key, input) in &self.original.inputs {
            assert!(
                Arc::ptr_eq(input, after.inputs.get(key).unwrap()),
                "cold recovery never replaces or reprices original input"
            );
        }
        assert_eq!(after.inputs.len(), self.original.inputs.len());
        assert_eq!(
            after.signer.lease_generation,
            acquired.signer.lease_generation
        );
        assert_eq!(
            after.signer.lease_token_digest,
            acquired.signer.lease_token_digest
        );
        let mut normalized = after.signer.state.clone();
        normalized.revision = acquired.signer.state.revision;
        normalized.state_digest = acquired.signer.state.state_digest.clone();
        normalized.private_retention = acquired.signer.state.private_retention.clone();
        assert_eq!(
            normalized, acquired.signer.state,
            "only protected Ready/choice metadata may change State"
        );
        assert_eq!(
            signs.load(std::sync::atomic::Ordering::SeqCst),
            self.signatures,
            "cold recovery cannot sign"
        );
        assert!(matches!(self.domain, 11 | 12));
    }
}

/// A single test lifecycle may configure transport before independent cold
/// reconstruction finishes. Until bind, every material/serving operation keeps
/// the trait's denied defaults; only the genuine transport IO is retained.
pub(super) struct ColdHookSlot {
    hook: oasis7_node::NodeBftSharedExecutionHookV3,
    io: Arc<Mutex<Option<Arc<dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1>>>>,
}
struct DeniedColdHook {
    io: Arc<Mutex<Option<Arc<dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1>>>>,
}
impl oasis7_node::NodeExecutionHook for DeniedColdHook {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("cold hook has not completed independent reconstruction".into())
    }
    fn configure_bft_v3_admission_input_io(
        &mut self,
        io: Arc<dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1>,
    ) -> Result<(), String> {
        let mut stored = self.io.lock().map_err(|_| "cold transport lock poisoned")?;
        if stored.as_ref().is_some_and(|old| !Arc::ptr_eq(old, &io)) {
            return Err("cold original IO cannot be replaced".into());
        }
        *stored = Some(io);
        Ok(())
    }
}
impl ColdHookSlot {
    pub(super) fn new() -> Self {
        let io = Arc::new(Mutex::new(None));
        let hook: oasis7_node::NodeBftSharedExecutionHookV3 =
            Arc::new(Mutex::new(Box::new(DeniedColdHook { io: io.clone() })));
        Self { hook, io }
    }
    pub(super) fn hook(&self) -> oasis7_node::NodeBftSharedExecutionHookV3 {
        self.hook.clone()
    }
    pub(super) fn bind(
        self,
        window: Box<FormalBftHeaderActiveExecutionV1>,
        binding: &BftSignerBindingV3,
    ) -> Result<oasis7_node::NodeBftSharedExecutionHookV3, String> {
        let temporary = window.into_shared_node_hook_for_test(binding)?;
        let mutex = Arc::try_unwrap(temporary).map_err(|_| "new cold hook unexpectedly shared")?;
        let mut actual = mutex
            .into_inner()
            .map_err(|_| "new cold hook lock poisoned")?;
        let io = self
            .io
            .lock()
            .map_err(|_| "cold transport lock poisoned")?
            .clone()
            .ok_or("cold transport IO must be configured before binding")?;
        actual.configure_bft_v3_admission_input_io(io)?;
        *self.hook.lock().map_err(|_| "cold hook lock poisoned")? = actual;
        Ok(self.hook)
    }
}

#[test]
fn actual_source_active_local11_pending_reopens_original_after_full_release() {
    super::run_case(super::CaseMode::ColdLocal);
}
#[test]
fn actual_source_active_received12_pending_reopens_original_after_full_release() {
    super::run_case(super::CaseMode::ColdReceived);
}
