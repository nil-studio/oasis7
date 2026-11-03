//! Actual covered source fixture. The protected backend and issuer are explicit
//! software test doubles; real Noise transport is not physical fault isolation.
use super::super::super::super::super::formal_header::{
    ActivateBftPrivateSourceV1, ActualBftPrivateSourceCreationVerifierV1,
    FreshBftPrivateSourceInputsV1, PreparedBftPrivateSourceOriginV1,
};
use super::super::source_creation_tests;
use super::*;
#[cfg(feature = "node-libp2p")]
#[path = "driver_bft_v3_source_budget_authority_tests.rs"]
mod budget_authority;

#[derive(Clone)]
pub(super) enum BackendState {
    Legacy(Arc<Mutex<BftSignerBackendSnapshotV3>>),
    Protected(BftPrivateRetentionMemoryBackendV1, String),
}
impl BackendState {
    pub(super) fn lock(&self) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        match self {
            Self::Legacy(state) => Ok(state.lock().unwrap().clone()),
            Self::Protected(backend, digest) => backend.clone().load_provisioned(digest),
        }
    }
    pub(super) fn legacy(&self) -> Arc<Mutex<BftSignerBackendSnapshotV3>> {
        match self {
            Self::Legacy(state) => state.clone(),
            Self::Protected(..) => panic!("protected source must not enter a legacy fixture"),
        }
    }
}

#[cfg(feature = "node-libp2p")]
#[path = "driver_bft_v3_successor_source_noise_tests.rs"]
mod noise;

#[cfg(feature = "node-libp2p")]
pub(super) struct SourceFixture {
    original_marker: BftPrivateRetentionCreationMarkerV1,
    backend: BftPrivateRetentionMemoryBackendV1,
    request: FreshBftPrivateSourceInputsV1,
    role: BftPrivateRetentionAttestorV1,
    noise: noise::NoiseFixture,
    archive_reader: Arc<noise::DeferredOriginalReader>,
}
#[cfg(not(feature = "node-libp2p"))]
pub(super) struct SourceFixture;
#[cfg(not(feature = "node-libp2p"))]
impl SourceFixture {
    pub(super) fn prepare(
        _: &QualifiedH<BftProfileTargetV2>,
        _: &Scratch,
        _: &LocalCasStore,
    ) -> (
        Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
        BftSignerBindingV3,
        BackendState,
        Self,
    ) {
        panic!("real Some admission fixture requires node-libp2p");
    }
    pub(super) fn service(
        &self,
        _: &BftSignerBindingV3,
        _: Vec<Box<dyn BftSigningKeyHandleV3>>,
        _: Option<BftSuccessorWindowSupportV1>,
    ) -> BftSignerServiceV3 {
        unreachable!()
    }
    pub(super) fn runtime(&self, _: &Path, _: &BftSignerBindingV3, _: SocketAddr) -> NodeRuntime {
        unreachable!()
    }
    pub(super) fn provider_input_egress(&self, _: &BftSignerBindingV3, _: Arc<BftSignerServiceV3>) {
        unreachable!()
    }
    pub(super) fn original_input_reader(
        &self,
    ) -> Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1> {
        unreachable!()
    }
    pub(super) fn original_input_read_count(&self) -> usize {
        unreachable!()
    }
    pub(super) fn fixed_attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        unreachable!()
    }
    pub(super) fn provider(&self, _: RemoteBftSignerProviderV3) -> RemoteBftSignerProviderV3 {
        unreachable!()
    }
}
#[cfg(feature = "node-libp2p")]
impl SourceFixture {
    pub(super) fn prepare(
        h: &QualifiedH<BftProfileTargetV2>,
        root: &Scratch,
        store: &LocalCasStore,
    ) -> (
        Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
        BftSignerBindingV3,
        BackendState,
        Self,
    ) {
        let request = source_creation_tests::inputs(h, root, store);
        let prepared = Box::new(
            PreparedBftPrivateSourceOriginV1::prepare(
                request.clone(),
                BftPrivateRetentionFreshSeedV1::generate().unwrap(),
            )
            .unwrap(),
        );
        let mut binding = source_creation_tests::binding(&prepared);
        binding.timeouts.propose_ms = 30_000;
        binding.timeouts.prevote_ms = 30_000;
        binding.timeouts.precommit_ms = 30_000;
        let namespace = prepared.finalize_namespace(&binding).unwrap();
        let marker = namespace.marker().clone();
        let role = source_creation_tests::role(&binding, &marker);
        let backend = BftPrivateRetentionMemoryBackendV1::create_fresh(
            namespace,
            role.clone(),
            Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
            100,
        )
        .unwrap();
        let (provider, server) = source_creation_tests::endpoint(
            &binding,
            backend.clone(),
            role.clone(),
            prepared.source_verifier(),
        );
        let mut lease = provider.acquire(&binding).unwrap();
        lease
            .provision_private_retention(Instant::now() + Duration::from_secs(30))
            .unwrap();
        let authentication_started = Instant::now();
        let opening = prepared
            .authenticate_activation(ActivateBftPrivateSourceV1 {
                binding: &binding,
                fixed_attestor: &role,
                original_marker: &marker,
                lease: lease.as_mut(),
                deadline: Instant::now() + Duration::from_secs(120),
            })
            .unwrap();
        eprintln!(
            "source activation authenticated after {:?}",
            authentication_started.elapsed()
        );
        let closing_started = Instant::now();
        drop(lease);
        server.join().unwrap();
        eprintln!(
            "source activation original TCP EOF joined after {:?}",
            closing_started.elapsed()
        );
        // The original observation connection ends normally before purely local
        // H restoration/publication. The original operation deadline is retained.
        let local_open_started = Instant::now();
        let driver = opening.open();
        eprintln!(
            "source activation local opening completed after {:?}",
            local_open_started.elapsed()
        );
        let driver = driver.unwrap();
        let actual = Arc::new(Mutex::new(*driver));
        let noise = noise::NoiseFixture::new(&binding, actual.clone());
        let state = BackendState::Protected(backend.clone(), binding.digest().unwrap());
        let fixture = Self {
            original_marker: marker,
            backend,
            request,
            role,
            noise,
            archive_reader: Arc::new(noise::DeferredOriginalReader::default()),
        };
        (actual, binding, state, fixture)
    }
    pub(super) fn service(
        &self,
        binding: &BftSignerBindingV3,
        keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        successor: Option<BftSuccessorWindowSupportV1>,
    ) -> BftSignerServiceV3 {
        BftSignerServiceV3::open_with_private_retention(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            keys,
            Box::new(self.backend.clone()),
            successor,
            BftPrivateRetentionSupportV1::new(
                self.role.clone(),
                Arc::new(
                    ActualBftPrivateSourceCreationVerifierV1::for_original_source(
                        self.request.clone(),
                    )
                    .unwrap(),
                ),
                Duration::from_secs(120),
            )
            .unwrap()
            .with_admission_input_reader(self.noise.reader.clone()),
        )
        .unwrap()
    }
    pub(super) fn runtime(
        &self,
        root: &Path,
        binding: &BftSignerBindingV3,
        address: SocketAddr,
    ) -> NodeRuntime {
        self.noise
            .runtime(root, binding, address, self.role.clone())
    }
    pub(super) fn provider_input_egress(
        &self,
        binding: &BftSignerBindingV3,
        service: Arc<BftSignerServiceV3>,
    ) -> noise::ProviderEgressFixture {
        self.noise
            .provider_egress(binding, self.backend.clone(), service, &self.archive_reader)
    }
    pub(super) fn original_input_reader(
        &self,
    ) -> Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1> {
        self.archive_reader.clone()
    }
    pub(super) fn original_input_read_count(&self) -> usize {
        self.archive_reader.read_calls()
    }
    pub(super) fn fixed_attestor(&self) -> &BftPrivateRetentionAttestorV1 {
        &self.role
    }
    pub(super) fn provider(
        &self,
        provider: RemoteBftSignerProviderV3,
    ) -> RemoteBftSignerProviderV3 {
        provider
            .with_private_retention_attestor(self.role.clone())
            .unwrap()
    }
}

#[cfg(feature = "node-libp2p")]
impl SourceFixture {
    // Original bootstrap handles only; none retains the live driver or Noise.
    pub(super) fn cold_parts_for_test(
        &self,
    ) -> (
        FreshBftPrivateSourceInputsV1,
        BftPrivateRetentionAttestorV1,
        BftPrivateRetentionMemoryBackendV1,
        BftPrivateRetentionCreationMarkerV1,
    ) {
        (
            self.request.clone(),
            self.role.clone(),
            self.backend.clone(),
            self.original_marker.clone(),
        )
    }
}

// Same real Noise wiring and original fixed role, now owned by the Active hook.
#[cfg(feature = "node-libp2p")]
pub(super) struct ActiveNoiseFixtureForTest {
    noise: noise::NoiseFixture,
    reader: Arc<noise::DeferredOriginalReader>,
    egress: Option<noise::ProviderEgressFixture>,
}
#[cfg(feature = "node-libp2p")]
impl ActiveNoiseFixtureForTest {
    pub(super) fn publish_from_lease_for_test(
        &self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
    ) {
        self.noise.publish_from_lease_for_test(binding, lease);
    }

    pub(super) fn new(
        binding: &BftSignerBindingV3,
        hook: oasis7_node::NodeBftSharedExecutionHookV3,
    ) -> Self {
        Self {
            noise: noise::NoiseFixture::new_with_hook(binding, hook),
            reader: Arc::new(noise::DeferredOriginalReader::default()),
            egress: None,
        }
    }
    pub(super) fn service(
        &mut self,
        binding: &BftSignerBindingV3,
        request: FreshBftPrivateSourceInputsV1,
        role: BftPrivateRetentionAttestorV1,
        backend: BftPrivateRetentionMemoryBackendV1,
        keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        successor: BftSuccessorWindowSupportV1,
        actual: Arc<ActualBftSuccessorWindowDriverVerifierV1>,
    ) -> Arc<BftSignerServiceV3> {
        let service = Arc::new(
            BftSignerServiceV3::open_with_private_retention(
                binding.clone(),
                Box::new(MemoryTestSignerKey(fixtures::key(116))),
                keys,
                Box::new(backend.clone()),
                Some(successor),
                BftPrivateRetentionSupportV1::new(
                    role,
                    Arc::new(
                        ActualBftPrivateSourceCreationVerifierV1::for_original_source(request)
                            .unwrap()
                            .with_active_admission_verifier(actual)
                            .unwrap(),
                    ),
                    Duration::from_secs(120),
                )
                .unwrap()
                .with_admission_input_reader(self.noise.reader.clone()),
            )
            .unwrap(),
        );
        self.egress =
            Some(
                self.noise
                    .provider_egress(binding, backend, service.clone(), &self.reader),
            );
        service
    }
    pub(super) fn original_input_reader(
        &self,
    ) -> Arc<dyn BftPrivateRetentionAdmissionInputReaderV1> {
        self.reader.clone()
    }
    pub(super) fn runtime(
        &self,
        root: &Path,
        binding: &BftSignerBindingV3,
        address: SocketAddr,
        role: BftPrivateRetentionAttestorV1,
    ) -> NodeRuntime {
        self.noise.runtime(root, binding, address, role)
    }
}

pub(super) fn install(
    driver: &mut FormalBftHeaderNodeDriverV3,
    request: SuccessorPrivateInstallRequestV1<'_>,
    source: Option<&SourceFixture>,
) -> Result<BftSuccessorWindowAdoptedEvidenceV1, String> {
    #[cfg(feature = "node-libp2p")]
    if let Some(source) = source {
        let retention =
            super::super::super::super::super::formal_header::ConfiguredRetentionInstallV1::new(
                request.binding,
                source.fixed_attestor().clone(),
                source.original_input_reader(),
            )?;
        return driver.install_successor_window_with_retention(request, &retention);
    }
    #[cfg(not(feature = "node-libp2p"))]
    let _ = source;
    driver.install_successor_window(request)
}
