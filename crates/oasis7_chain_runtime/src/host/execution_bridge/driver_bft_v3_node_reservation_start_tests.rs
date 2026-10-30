//! Real protected reservation crash seam; no production startup guard is changed.
use super::*;
use oasis7_consensus::tendermint_v3::{BftHeaderValueV3, VerifiedBftHeaderCertificateV3};
use oasis7_distfs::protected_bft_v3::{
    BftSignerLeaseV3, BftSignerStateV3, BftSuccessorArchiveStoreV1, BftSuccessorWindowArchiveRefV1,
};
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionCommitResultV3, NodeBftExecutionContextV3,
    NodeBftExecutionHeadV3, NodeBftPrivateCandidatePackageV3, NodeBftPrivateCandidateV3,
    NodeExecutionHook,
};
use std::sync::atomic::{AtomicBool, Ordering};
pub(super) struct PauseFirstMaterial {
    inner: Arc<LocalBftSuccessorArchiveStoreV1>,
    armed: AtomicBool,
}
impl PauseFirstMaterial {
    pub(super) fn new(inner: Arc<LocalBftSuccessorArchiveStoreV1>) -> Self {
        Self {
            inner,
            armed: AtomicBool::new(true),
        }
    }
}
impl BftSuccessorArchiveStoreV1 for PauseFirstMaterial {
    fn persist_public_state(
        &self,
        state: &BftSignerStateV3,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        if self.armed.swap(false, Ordering::SeqCst) {
            return Err(BftSignerErrorV3(
                "injected material failure after real reservation CAS".into(),
            ));
        }
        self.inner.persist_public_state(state)
    }
    fn load_public_state(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3> {
        self.inner.load_public_state(reference)
    }
}
struct DefaultGate;
impl NodeExecutionHook for DefaultGate {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("unused legacy commit".into())
    }
}
struct ObserveReady {
    inner: Box<dyn NodeExecutionHook>,
    ready: Arc<AtomicBool>,
    configured: Arc<AtomicBool>,
    bypass: bool,
}
impl NodeExecutionHook for ObserveReady {
    fn on_commit(
        &mut self,
        context: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        self.inner.on_commit(context)
    }
    fn prepare_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<(), String> {
        if self.bypass {
            return Ok(());
        }
        let result = self
            .inner
            .prepare_bft_v3_execution(binding, lease, deadline);
        if result.is_ok() {
            self.ready.store(true, Ordering::SeqCst);
        }
        result
    }
    fn enable_bft_v3_membership_execution(&mut self) -> Result<(), String> {
        self.inner.enable_bft_v3_membership_execution()
    }
    fn validate_bft_v3_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        self.inner.validate_bft_v3_action_payload(payload)
    }
    fn configure_bft_v3_execution(
        &mut self,
        binding: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        self.configured.store(true, Ordering::SeqCst);
        self.inner.configure_bft_v3_execution(binding)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        self.inner.current_bft_v3_execution_context(schedule)
    }
    fn build_bft_v3_candidate(
        &mut self,
        context: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.inner.build_bft_v3_candidate(context)
    }
    fn preview_bft_v3_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        self.inner.preview_bft_v3_candidate(context, candidate)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        self.inner
            .retain_bft_v3_private_candidate(context, candidate)
    }
    fn export_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        self.inner.export_bft_v3_private_candidate(value)
    }
    fn retain_received_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
        package: &NodeBftPrivateCandidatePackageV3,
    ) -> Result<String, String> {
        self.inner
            .retain_received_bft_v3_private_candidate(context, package)
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        context: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.inner.pending_bft_v3_private_candidate(context)
    }
    fn lookup_bft_v3_public_value(
        &mut self,
        height: u64,
        id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        self.inner.lookup_bft_v3_public_value(height, id)
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.inner.load_bft_v3_private_candidate(value)
    }
    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        self.inner.commit_bft_v3_certificate(token)
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        binding: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        self.inner
            .load_bft_v3_commit_history_page(binding, after, max)
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
        actions: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        self.inner.validate_bft_v3_action_batch(schedule, actions)
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn exercise(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    address: SocketAddr,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
    signatures: Arc<std::sync::atomic::AtomicUsize>,
    body: BftSuccessorWindowInstallIntentBodyV1,
    manifest: BftSuccessorWindowArchiveManifestV1,
) {
    let before = snapshot.lock().unwrap().state.clone();
    let extension = before.successor_window.as_ref().unwrap();
    assert!(before.active_window.is_none());
    assert!(extension.requests.is_empty());
    let reservation = extension
        .reservation
        .as_ref()
        .expect("real first CAS reservation must survive material failure")
        .clone();
    assert_eq!(reservation.body, body);
    assert_eq!(reservation.manifest, manifest);
    let records = root.path().join("ledger");
    let frozen = tree(&records.join("formal-bft-header-prefix-v1"));
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(120),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: pin.into(),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap(),
    );
    let mut cold = provider
        .acquire(binding)
        .expect("reservation-only State must authenticate on cold Acquire");
    assert_eq!(cold.checkpoint().unwrap().state, before);
    assert!(
        DefaultGate
            .prepare_bft_v3_execution(
                binding,
                cold.as_mut(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err(),
        "default hook must reject the real reservation before readiness"
    );
    drop(cold);
    let ready = Arc::new(AtomicBool::new(false));
    let mut rejected_all = true;
    for bypass in [false, true] {
        let (hook, _, _, _) = FormalBftHeaderNodeDriverV3::deferred_existing(
            &records,
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            h.reader.clone(),
            Some(archive.clone()),
            pin.into(),
            false,
        )
        .unwrap();
        let configured = Arc::new(AtomicBool::new(false));
        let hook = ObserveReady {
            inner: hook,
            ready: ready.clone(),
            configured: configured.clone(),
            bypass,
        };
        let config = NodeConfig::new(
            validator_id(0),
            binding.scope.world_id.clone(),
            NodeRole::Sequencer,
        )
        .unwrap()
        .with_replication(
            NodeReplicationConfig::new(root.path().join(format!("reservation-cold-node-{bypass}")))
                .unwrap(),
        );
        let mut node = NodeRuntime::new(config)
            .with_replication_network_consensus_enabled(false)
            .with_execution_hook(hook)
            .with_tendermint_v3(binding.clone(), provider.clone())
            .unwrap();
        let result = node.start();
        rejected_all &= result.is_err();
        if result.is_ok() {
            let _ = node.stop();
        }
        drop(node);
        assert!(
            !configured.load(Ordering::SeqCst),
            "typed base gate must reject a reservation even when a custom preparation hook returns Ok"
        );
        assert_eq!(
            snapshot.lock().unwrap().state,
            before,
            "cold reservation must not mutate global State"
        );
        assert_eq!(
            signatures.load(Ordering::SeqCst),
            0,
            "no new consensus key call during blocked startup"
        );
        assert_eq!(tree(&records.join("formal-bft-header-prefix-v1")), frozen);
    }
    // An auto-generated WAL may continue only the exact authenticated reservation.
    // Tampering cannot pick a new request; restoring the original bytes is enough
    // for the normal Node.start production preparation path to finish it.
    let identity = OriginalHeaderIdentityV1::open(
        &records,
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
    )
    .unwrap();
    identity
        .persist_auto_prepare_for_test(&records, binding, pin, body.clone(), manifest.clone())
        .unwrap();
    let wal = records.join("successor-auto-prepares-v1").join(format!(
        "{}.prepare.cbor",
        body.checkpoint.digest().unwrap()
    ));
    let bytes = fs::read(&wal).unwrap();
    drop(identity);
    let value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
    let alternate = oasis7_cas::to_canonical_cbor(&value).unwrap();
    assert_ne!(
        alternate, bytes,
        "CAS protocol encoding differs from local WAL codec"
    );
    for damaged in [b"changed original auto Prepare".to_vec(), alternate] {
        fs::write(&wal, damaged).unwrap();
        let (mut tampered, _, _, _) = FormalBftHeaderNodeDriverV3::deferred_existing(
            &records,
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            h.reader.clone(),
            Some(archive.clone()),
            pin.into(),
            false,
        )
        .unwrap();
        let mut tamper_lease = provider.acquire(binding).unwrap();
        assert!(
            tampered
                .prepare_bft_v3_execution(
                    binding,
                    tamper_lease.as_mut(),
                    Instant::now() + Duration::from_secs(120)
                )
                .is_err()
        );
        assert_eq!(snapshot.lock().unwrap().state, before);
        drop(tamper_lease);
        drop(tampered);
    }
    fs::write(&wal, &bytes).unwrap();

    // Full production Deferred cold Node.start must retry the authenticated
    // original reservation, install/read back, and Adopt before serving.
    let (hook, _, _, _) = FormalBftHeaderNodeDriverV3::deferred_existing(
        &records,
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
        Some(archive.clone()),
        pin.into(),
        false,
    )
    .unwrap();
    let config = NodeConfig::new(
        validator_id(0),
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_replication(
        NodeReplicationConfig::new(root.path().join("reservation-auto-cold-positive")).unwrap(),
    );
    let mut node = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .try_with_shared_execution_hook(Arc::new(Mutex::new(hook)))
        .unwrap()
        .with_tendermint_v3(binding.clone(), provider.clone())
        .unwrap();
    let started = node.start();
    if started.is_ok() {
        node.stop().unwrap();
    }
    drop(node);
    started.expect("matching original WAL must complete actual Deferred cold Node.start");
    let mut lease = provider.acquire(binding).unwrap();
    let (prepared, after) = lease
        .get_successor_window(
            &body.checkpoint.request_id,
            &body.checkpoint.digest().unwrap(),
        )
        .unwrap();
    let prepared = prepared.expect("same original auto Prepare must remain queryable");
    assert_eq!(
        prepared.request.status,
        oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Adopted
    );
    let archived = lease
        .read_successor_original_state(
            &body.checkpoint.request_id,
            &body.checkpoint.digest().unwrap(),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    assert_eq!(prepared.request.intent.body, body);
    assert_eq!(prepared.manifest, manifest);
    assert_eq!(
        prepared.original_public_state,
        reservation.original_public_state
    );
    assert!(
        after
            .state
            .successor_window
            .as_ref()
            .unwrap()
            .reservation
            .is_none()
    );
    assert_eq!(
        after.state.successor_window.as_ref().unwrap().usage,
        extension.usage
    );
    assert_eq!(archived.original_state().history, before.history);
    assert_eq!(archived.original_state().journal, before.journal);
    assert!(after.state.active_window.is_some());
    assert_eq!(
        fs::read(&wal).unwrap(),
        bytes,
        "original auto Prepare WAL remains byte-identical"
    );
    assert_eq!(tree(&records.join("formal-bft-header-prefix-v1")), frozen);
    drop(lease);
    assert!(
        !ready.load(Ordering::SeqCst),
        "reservation-only cold startup exposed a writable legacy driver before provider rejection"
    );
    assert!(
        rejected_all,
        "reservation-only cold Node.start must reject before serving"
    );
}
fn tree(path: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &entry.path(), out)
            } else {
                out.insert(
                    entry.path().strip_prefix(root).unwrap().into(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(path, path, &mut out);
    out
}
