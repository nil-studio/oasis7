//! Real same-root cold lifecycle. Only public recovery descriptors survive release.
use super::*;
use oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3;
use oasis7_node::{
    NodeBftBuildContextV3, NodeBftExecutionContextV3, NodeBftPrivateCandidatePackageV3,
};
use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;

pub(super) struct Reopen {
    original: ValidatedBftSuccessorOriginalStateV1,
    record: BftSuccessorWindowStoredRequestV1,
    pin: String,
    window: Box<FormalBftHeaderActiveExecutionV1>,
}
struct ReadyReopen {
    original: ValidatedBftSuccessorOriginalStateV1,
    record: BftSuccessorWindowStoredRequestV1,
    pin: String,
    context: NodeBftExecutionContextV3,
    received: NodeBftPrivateCandidatePackageV3,
    baseline: BTreeSet<String>,
}
fn actions(unique: &str) -> Vec<oasis7_node::NodeConsensusAction> {
    ["active-shared-real-agent", unique]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            oasis7_node::NodeConsensusAction::from_payload(
                35000 + i as u64,
                validator_id(0),
                payload(name),
            )
            .unwrap()
        })
        .collect()
}
impl Reopen {
    pub(super) fn prepare(
        case: &active_tests::ActiveExerciseV1<'_>,
        window: Box<FormalBftHeaderActiveExecutionV1>,
    ) -> Self {
        Self {
            original: case.original.clone(),
            record: case.record.clone(),
            pin: case.pin.into(),
            window,
        }
    }
    fn advance(
        mut self,
        lease: &mut dyn BftSignerLeaseV3,
        deadline: Instant,
    ) -> (Box<FormalBftHeaderActiveExecutionV1>, ReadyReopen) {
        // Only the explicitly Active-configured real service may sign carry.
        active_tests::commit_original_carry_for_test(&mut self.window, lease, deadline);
        let context = NodeBftExecutionContextV3::from_schedule(self.window.schedule());
        let baseline = self.window.retained_baseline_refs_for_test().unwrap();
        let received = self
            .window
            .preview_external_package_for_test(
                serde_cbor::to_vec(&actions("active-shared-alternative-b")).unwrap(),
                deadline,
            )
            .unwrap();
        (
            self.window,
            ReadyReopen {
                original: self.original,
                record: self.record,
                pin: self.pin,
                context,
                received,
                baseline,
            },
        )
    }
}

struct Session {
    hook: oasis7_node::NodeBftSharedExecutionHookV3,
    lease: Box<dyn BftSignerLeaseV3>,
    noise: source_retention::ActiveNoiseFixtureForTest,
    verifier: Arc<ActualBftSuccessorWindowDriverVerifierV1>,
    server: reserved_archive::ScopedArchiveServerV1,
}
impl Session {
    #[allow(clippy::too_many_arguments)]
    fn connect(
        hook: oasis7_node::NodeBftSharedExecutionHookV3,
        prepared: &Prepared,
        h: &QualifiedH<BftProfileTargetV2>,
        root: &Scratch,
        binding: &BftSignerBindingV3,
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
        public: Arc<LocalBftSuccessorArchiveStoreV1>,
        signs: Arc<AtomicUsize>,
    ) -> Self {
        let mut noise = source_retention::ActiveNoiseFixtureForTest::new(binding, hook.clone());
        let verifier = Arc::new(
            ActualBftSuccessorWindowDriverVerifierV1::new(
                h.anchor.clone(),
                h.target.clone(),
                archive,
            )
            .unwrap()
            .with_installation_root(root.path().join("ledger"))
            .unwrap()
            .with_active_private_store(prepared.store.clone())
            .unwrap()
            .with_original_retention_reader(prepared.role.clone(), noise.original_input_reader())
            .unwrap()
            .with_original_retention_binding(binding.clone())
            .unwrap(),
        );
        let support = BftSuccessorWindowSupportV1::new(public, Duration::from_secs(120))
            .unwrap()
            .with_active_window_rebase()
            .with_driver_verifier(verifier.clone());
        let service = noise.service(
            binding,
            prepared.request.clone(),
            prepared.role.clone(),
            prepared.backend.clone(),
            vec![Box::new(ReadbackOnlyKey(signs))],
            support,
            verifier.clone(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(120);
            let mut stream = loop {
                if stopping.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "cold real owner connection");
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("cold owner accept: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            service.serve_tcp_connection(&mut stream).unwrap();
        });
        let server = reserved_archive::ScopedArchiveServerV1::new(stop, server);
        let provider = RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(120),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: prepared.role.provider_public_key.clone(),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap()
        .with_private_retention_attestor(prepared.role.clone())
        .unwrap();
        let mut lease = provider.acquire(binding).unwrap();
        noise.publish_from_lease_for_test(binding, lease.as_mut());
        Self {
            hook,
            lease,
            noise,
            verifier,
            server,
        }
    }
    fn release(self) {
        let Self {
            hook,
            lease,
            noise,
            verifier,
            server,
        } = self;
        let weak = Arc::downgrade(&hook);
        drop(lease);
        server.join().unwrap();
        drop(verifier);
        drop(noise);
        drop(hook);
        assert!(
            weak.upgrade().is_none(),
            "every old Active hook/Noise writer must be released before independent reconstruction"
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    mut prepared: Prepared,
    reopen: Reopen,
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    binding: &BftSignerBindingV3,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    public: Arc<LocalBftSuccessorArchiveStoreV1>,
) {
    let mode = MODE.with(Cell::get);
    let signs = Arc::new(AtomicUsize::new(0));
    // The only actual Window is in Reopen until the new real lease signs carry.
    let slot = pending_cold::ColdHookSlot::new();
    let mut session = Session::connect(
        slot.hook(),
        &prepared,
        h,
        root,
        binding,
        archive.clone(),
        public.clone(),
        signs.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(120);
    let (window, reopen) = reopen.advance(session.lease.as_mut(), deadline);
    session
        .noise
        .publish_from_lease_for_test(binding, session.lease.as_mut());
    let bound = slot.bind(window, binding).unwrap();
    assert!(Arc::ptr_eq(&bound, &session.hook));
    drop(bound);
    session
        .hook
        .lock()
        .unwrap()
        .prepare_bft_v3_execution(binding, session.lease.as_mut(), deadline)
        .unwrap();
    // Follow actual Node startup: configure the original base cursor, then
    // independently verify every leased certificate before replaying its token.
    session
        .hook
        .lock()
        .unwrap()
        .configure_bft_v3_execution(binding)
        .unwrap();
    let history = session.lease.checkpoint().unwrap().state.history;
    let mut schedule = session
        .lease
        .verified_active_window()
        .unwrap()
        .unwrap()
        .base_schedule()
        .clone();
    for certificate in &history {
        let token = verify_commit_certificate_token_v3(&schedule, certificate).unwrap();
        session
            .hook
            .lock()
            .unwrap()
            .commit_bft_v3_certificate(token.clone())
            .unwrap();
        schedule = token.next_height_schedule().clone();
    }
    assert_eq!(
        NodeBftExecutionContextV3::from_schedule(&schedule),
        reopen.context
    );
    let mut pending_price = None;
    let candidate = if mode == CaseMode::ColdReceived {
        let error = session
            .hook
            .lock()
            .unwrap()
            .retain_received_bft_v3_protected_candidate(
                binding,
                &mut ready_fault::StopBeforeReady::new(session.lease.as_mut()),
                &reopen.context,
                &reopen.received,
                deadline,
            )
            .unwrap_err();
        assert!(
            error.contains("stopped before first Ready"),
            "must reach genuine post-Reserve Ready boundary; actual rejection: {error}"
        );
        reopen.received.candidate.clone()
    } else {
        let candidate = session
            .hook
            .lock()
            .unwrap()
            .build_bft_v3_candidate(NodeBftBuildContextV3 {
                execution: reopen.context.clone(),
                actions: actions("active-shared-alternative-a"),
            })
            .unwrap();
        if mode == CaseMode::SharedBudget {
            pending_price = Some(shared_budget::reserve_a_then_ready_b(
                session.hook.lock().unwrap().as_mut(),
                binding,
                session.lease.as_mut(),
                &mut prepared.backend,
                &reopen.context,
                &candidate,
                &reopen.received,
                &prepared.namespace,
                &reopen.baseline,
                deadline,
            ));
        } else {
            let error = session
                .hook
                .lock()
                .unwrap()
                .retain_bft_v3_protected_candidate(
                    binding,
                    &mut ready_fault::StopBeforeReady::new(session.lease.as_mut()),
                    &reopen.context,
                    &candidate,
                    deadline,
                )
                .unwrap_err();
            assert!(
                error.contains("stopped before first Ready"),
                "must reach genuine post-Reserve Ready boundary; actual rejection: {error}"
            );
        }
        candidate
    };
    let witness = pending_cold::PendingColdWitness::capture(
        binding,
        &mut prepared.backend,
        &prepared.namespace,
        &candidate,
        if mode == CaseMode::ColdReceived {
            12
        } else {
            11
        },
        &signs,
    );
    // No old service, transport, lease or execution survives this boundary.
    session.release();
    let slot = pending_cold::ColdHookSlot::new();
    let mut cold = Session::connect(
        slot.hook(),
        &prepared,
        h,
        root,
        binding,
        archive,
        public,
        signs.clone(),
    );
    let base = cold
        .verifier
        .recover_private_base(&reopen.record, &reopen.original, deadline)
        .unwrap();
    let active = cold.lease.verified_active_window().unwrap().unwrap();
    let installed = reopen
        .original
        .reconstruct_installed_pending(binding, &reopen.pin, &reopen.record)
        .unwrap();
    let window = Box::new(
        FormalBftHeaderActiveExecutionV1::open(
            &root.path().join("ledger"),
            prepared.store.clone(),
            base,
            &active,
            binding,
            &reopen.pin,
            &installed,
            deadline,
        )
        .unwrap(),
    );
    let bound = slot.bind(window, binding).unwrap();
    assert!(Arc::ptr_eq(&bound, &cold.hook));
    drop(bound);
    witness.recover(
        &cold.hook,
        binding,
        cold.lease.as_mut(),
        &mut prepared.backend,
        &signs,
        deadline,
    );
    if let Some(original) = &pending_price {
        assert_eq!(
            original.original_deadline(),
            deadline,
            "cold retry retains original operation deadline"
        );
        shared_budget::assert_cold_a_kept_original(&mut prepared.backend, binding, original);
    }
    cold.release();
}
