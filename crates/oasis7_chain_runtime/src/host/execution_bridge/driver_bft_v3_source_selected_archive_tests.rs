//! Real committed prefix, unsigned selected Received10, then authorized archive replay.
use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
#[path = "driver_bft_v3_source_carry_purpose_tests.rs"]
mod purpose_matrix;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
const FINISHED: &str = "selected10 archive target stopped before proposal signing";
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get) || purpose_matrix::requested()
}
struct Summary {
    entry: BftPrivateRetentionEntryV1,
    reference: BftPrivateRetentionAdmissionInputRefV1,
    input: Arc<BftPrivateRetentionAdmissionInputV1>,
    state: BftSignerStateV3,
    originals: Vec<BftPrivateRetentionEntryV1>,
    selection: BftSuccessorWindowArchiveRefV1,
    validator_signatures: usize,
}
struct Probe {
    target: u64,
    purpose_matrix: bool,
    backend: BftPrivateRetentionMemoryBackendV1,
    binding: String,
    private: std::path::PathBuf,
    signatures: Arc<AtomicUsize>,
    result: Mutex<Option<Result<Summary, String>>>,
}
fn registry() -> &'static Mutex<BTreeMap<usize, Arc<Probe>>> {
    static PROBES: OnceLock<Mutex<BTreeMap<usize, Arc<Probe>>>> = OnceLock::new();
    PROBES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
pub(super) struct Registration {
    key: usize,
    probe: Arc<Probe>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let mut map = registry().lock().unwrap_or_else(|p| p.into_inner());
        if map
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.probe))
        {
            map.remove(&self.key);
        }
    }
}
pub(super) fn register(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    state: &BackendState,
    records: &Path,
    signatures: Arc<AtomicUsize>,
) -> Option<Registration> {
    if !requested() {
        return None;
    }
    let BackendState::Protected(backend, digest) = state else {
        panic!("selected10 needs real covered source");
    };
    let key = Arc::as_ptr(actual) as usize;
    let probe = Arc::new(Probe {
        purpose_matrix: purpose_matrix::requested(),
        target: binding.initial_schedule().height().checked_add(1).unwrap(),
        backend: backend.clone(),
        binding: digest.clone(),
        private: records.join("formal-bft-header-prefix-v1/node-private-v1"),
        signatures,
        result: Mutex::new(None),
    });
    let mut map = registry().lock().unwrap();
    assert!(!map.contains_key(&key));
    map.insert(key, probe.clone());
    Some(Registration { key, probe })
}
pub(super) fn intercept(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    candidate: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Option<Result<String, String>> {
    let probe = registry()
        .lock()
        .unwrap()
        .get(&(Arc::as_ptr(actual) as usize))
        .cloned()?;
    if context.height < probe.target {
        return None;
    }
    if probe.result.lock().unwrap().is_some() {
        return Some(Err(FINISHED.into()));
    }
    let result = exercise(
        &probe,
        &mut actual.lock().unwrap(),
        binding,
        lease,
        context,
        candidate,
        deadline,
    );
    *probe.result.lock().unwrap() = Some(result);
    Some(Err(FINISHED.into()))
}
fn exercise(
    probe: &Probe,
    driver: &mut FormalBftHeaderNodeDriverV3,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    candidate: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Result<Summary, String> {
    if probe.purpose_matrix {
        return purpose_matrix::exercise(
            probe, driver, binding, lease, context, candidate, deadline,
        );
    }
    if context.height != probe.target {
        return Err("selected10 target height skipped".into());
    }
    let public = lease.checkpoint().map_err(|e| e.to_string())?.state;
    if public.history.last().is_none_or(|certificate| {
        certificate.execution_commitment.height.checked_add(1) != Some(context.height)
    }) {
        return Err("selected10 archive requires its real committed QC prefix".into());
    }
    let signatures = probe.signatures.load(Ordering::SeqCst);
    if signatures == 0 {
        return Err("selected10 prefix has no genuine validator signatures".into());
    }
    let package = driver.preview_received_package_for_test(context, &candidate.actions_cbor)?;
    if package.candidate != *candidate {
        return Err("selected10 actual original package differs".into());
    }
    match driver
        .retain_received_bft_v3_protected_candidate(binding, lease, context, &package, deadline)?
    {
        oasis7_node::NodeBftProtectedCandidateAdmissionV3::Retained(_) => {}
        other => return Err(format!("selected10 actual received rejection: {other:?}")),
    }
    if driver.pending_bft_v3_private_candidate(context)?.is_some() {
        return Err("received target selected local proposal".into());
    }
    let current = probe
        .backend
        .clone()
        .load_private_retention_current(&probe.binding)
        .map_err(|e| e.to_string())?;
    let header = candidate.public_value.block_id()?;
    let ledger = current
        .retention
        .as_ref()
        .ok_or("selected10 ledger absent")?;
    let entry = ledger
        .map
        .entries
        .iter()
        .find(|entry| entry.key.height == context.height && entry.key.header_id == header)
        .ok_or("selected10 original fact absent")?
        .clone();
    if entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready {
        return Err("selected10 target is not Ready".into());
    }
    let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, &entry)
        .map_err(|e| e.to_string())?;
    let input = current
        .inputs
        .get(&reference.admission_digest)
        .ok_or("selected10 original input absent")?
        .clone();
    #[derive(serde::Deserialize)]
    struct Kind {
        schema_version: u32,
    }
    let original = input
        .objects
        .iter()
        .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
        .ok_or("selected10 original Data absent")?;
    let kind: Kind = serde_cbor::from_slice(&original.bytes).map_err(|e| e.to_string())?;
    if kind.schema_version != 10 {
        return Err("archive target must be real original Received10".into());
    }
    driver.retain_bft_v3_protected_candidate(binding, lease, context, candidate, deadline)?;
    if driver.pending_bft_v3_private_candidate(context)?.as_ref() != Some(candidate)
        || driver.export_bft_v3_private_candidate(&candidate.public_value)? != package
    {
        return Err("selected10 original local selection/package differs".into());
    }
    let after = probe
        .backend
        .clone()
        .load_private_retention_current(&probe.binding)
        .map_err(|e| e.to_string())?;
    if current.signer.state != after.signer.state
        || current.retention != after.retention
        || !Arc::ptr_eq(
            &input,
            after
                .inputs
                .get(&reference.admission_digest)
                .ok_or("selected10 original input lost")?,
        )
        || probe.signatures.load(Ordering::SeqCst) != signatures
    {
        return Err("selected10 local selection changed fact/charge/state/input or signed".into());
    }
    let bytes = fs::read(
        probe
            .private
            .join(format!("selection-{:020}.cbor", context.height)),
    )
    .map_err(|e| e.to_string())?;
    Ok(Summary {
        entry,
        reference,
        input,
        state: after.signer.state,
        originals: ledger.map.entries.clone(),
        selection: BftSuccessorWindowArchiveRefV1 {
            digest: blake3_hex(&bytes),
            bytes: bytes.len() as u64,
        },
        validator_signatures: signatures,
    })
}
impl Registration {
    pub(super) fn finished(&self) -> bool {
        let result = self.probe.result.lock().unwrap();
        match result.as_ref() {
            None => false,
            Some(Ok(_)) => true,
            Some(Err(reason)) => panic!("actual selected10 target failed: {reason}"),
        }
    }
    pub(super) fn expected_pending_count(&self) -> usize {
        if self.probe.purpose_matrix { 2 } else { 1 }
    }
    pub(super) fn check_purposes(
        &self,
        verifier: &ActualBftSuccessorWindowDriverVerifierV1,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
        state: &BftSignerStateV3,
        reference: &BftSuccessorWindowArchiveRefV1,
        signatures: &AtomicUsize,
    ) {
        if self.probe.purpose_matrix {
            let before_signs = self.probe.signatures.load(Ordering::SeqCst);
            verifier
                .verify_carry_purpose_matrix_for_test(
                    checkpoint,
                    state,
                    reference,
                    &mut self.probe.backend.clone(),
                    &self.probe.binding,
                    signatures,
                    Instant::now() + Duration::from_secs(120),
                )
                .unwrap();
            assert_eq!(self.probe.signatures.load(Ordering::SeqCst), before_signs);
        }
    }
    pub(super) fn assert_after_archive(
        &self,
        state: &BftSignerStateV3,
        manifest: &BftSuccessorWindowArchiveManifestV1,
        readback_signatures: &AtomicUsize,
    ) {
        let result = self.probe.result.lock().unwrap();
        let original = result
            .as_ref()
            .expect("selected10 target never completed")
            .as_ref()
            .unwrap_or_else(|reason| panic!("selected10 target failed: {reason}"));
        assert_eq!(original.state.history, state.history);
        assert_eq!(original.state.journal, state.journal);
        assert_eq!(original.state.applied_installs, state.applied_installs);
        assert_eq!(
            self.probe.signatures.load(Ordering::SeqCst),
            original.validator_signatures
        );
        assert_eq!(readback_signatures.load(Ordering::SeqCst), 0);
        let current = self
            .probe
            .backend
            .clone()
            .load_private_retention_current(&self.probe.binding)
            .unwrap();
        assert_eq!(
            &current.signer.state, state,
            "export/replay changed post-Seal State"
        );
        let ledger = current.retention.as_ref().unwrap();
        assert_eq!(
            ledger
                .map
                .entries
                .iter()
                .find(|entry| entry.key == original.entry.key),
            Some(&original.entry)
        );
        let input = self
            .probe
            .backend
            .clone()
            .load_private_retention_input(&self.probe.binding, &original.reference)
            .unwrap();
        assert!(Arc::ptr_eq(&original.input, &input));
        original.reference.validate_input(&input).unwrap();
        assert!(
            original.originals.len() >= 2,
            "archive needs committed Some plus unsigned selected Some"
        );
        for entry in &original.originals {
            let BftPrivateRetentionFactV1::Some {
                original_data_ref,
                metadata_ref,
                input_ref: Some(input_ref),
                ..
            } = &entry.fact
            else {
                panic!("all original covered archive entries must be Some inputs");
            };
            for digest in [original_data_ref, metadata_ref, &input_ref.digest] {
                assert!(
                    manifest
                        .entries
                        .iter()
                        .any(|object| object.object.digest == *digest),
                    "archive omitted an original Some object"
                );
            }
            assert!(
                manifest
                    .entries
                    .iter()
                    .any(|object| &object.object == input_ref)
            );
        }
        let selection = fs::read(
            self.probe
                .private
                .join(format!("selection-{:020}.cbor", original.entry.key.height)),
        )
        .unwrap();
        original.selection.validate_bytes(&selection).unwrap();
    }
}
#[cfg(feature = "node-libp2p")]
#[test]
fn wire3_actual_selected_received10_archive_replays_with_original_purpose() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|enabled| enabled.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
