//! Real covered Source -> Installed/Adopted -> Active carry -> fresh Some/QC.
//! The first failing stage is retained as evidence; no production gate is relaxed.
use super::super::super::super::super::formal_header::{
    ActualBftPrivateSourceCreationVerifierV1, FreshBftPrivateSourceInputsV1,
};
use super::*;
#[path = "driver_bft_v3_source_active_cold_session_tests.rs"]
mod cold_session;
#[path = "driver_bft_v3_source_active_pending_cold_tests.rs"]
mod pending_cold;
#[path = "driver_bft_v3_source_active_prepare_fault_tests.rs"]
mod prepare_fault;
#[path = "driver_bft_v3_source_active_ready_fault_tests.rs"]
mod ready_fault;
#[path = "driver_bft_v3_source_sealed_reopen_fault_tests.rs"]
mod reopen_faults;
#[path = "driver_bft_v3_source_active_shared_budget_tests.rs"]
mod shared_budget;
use std::cell::Cell;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CaseMode {
    Normal,
    ColdLocal,
    ColdReceived,
    SharedBudget,
}
thread_local! { static MODE: Cell<CaseMode> = const { Cell::new(CaseMode::Normal) }; }

thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        ENABLED.with(|v| v.set(false));
        MODE.with(|v| v.set(CaseMode::Normal));
    }
}
#[test]
fn actual_source_installed_adopted_active_carry_then_new_protected_some_qc() {
    run_case(CaseMode::Normal);
}
pub(super) fn run_case(mode: CaseMode) {
    // Same caller stack and original fixture budgets for every independent case.
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            ENABLED.with(|v| assert!(!v.replace(true)));
            MODE.with(|v| v.set(mode));
            let _reset = Reset;
            install_case_with_retention(
                true,
                false,
                8,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

pub(super) struct Prepared {
    hook: oasis7_node::NodeBftSharedExecutionHookV3,
    request: FreshBftPrivateSourceInputsV1,
    role: BftPrivateRetentionAttestorV1,
    backend: BftPrivateRetentionMemoryBackendV1,
    before: BftPrivateRetentionCurrentV1,
    namespace: String,
    store: LocalCasStore,
    carry_height: u64,
    cold: Option<Box<cold_session::Reopen>>,
}
#[inline(never)]
pub(super) fn prepare<E>(
    case: &mut active_tests::ActiveExerciseV1<'_>,
    actual: Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    source: SourceFixture,
    old_egress: E,
    readback_signatures: Arc<std::sync::atomic::AtomicUsize>,
) -> Prepared {
    eprintln!("actual protected Active stage: authenticated Install/Adopt complete");
    let active = case
        .lease
        .verified_active_window()
        .unwrap()
        .expect("authenticated Active C0");
    let installed = case
        .original
        .reconstruct_installed_pending(case.binding, case.pin, case.record)
        .unwrap();
    let base = case
        .verifier
        .recover_private_base(
            case.record,
            case.original,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    let (request, role, mut backend, _) = source.cold_parts_for_test();
    assert!(
        role.allow_authenticated_successor_origins,
        "Fresh Active origins must retain the original true capability"
    );
    let before = backend
        .load_private_retention_current(&case.binding.digest().unwrap())
        .unwrap();
    let namespace = active.active_segment_origin().digest.clone();
    let weak = Arc::downgrade(&actual);
    drop(source);
    drop(old_egress);
    drop(actual);
    assert!(
        weak.upgrade().is_none(),
        "old Source hook/Noise must release its writer"
    );
    let open_deadline = Instant::now() + Duration::from_secs(120);
    let window = Box::new(
        FormalBftHeaderActiveExecutionV1::open(
            &case.root.path().join("ledger"),
            case.store.clone(),
            base,
            &active,
            case.binding,
            case.pin,
            &installed,
            open_deadline,
        )
        .expect("actual protected Active stage: open original installed namespace"),
    );
    let carry_height = window.schedule().height();
    // This fixed Local11/Received12 fixture intentionally exercises nonempty
    // C0 carry. Empty-carry remains valid outside this fixture.
    {
        let original_c0_carry = window
            .pending_candidate()
            .unwrap()
            .expect("fixed Active cold fixture requires a nonempty original C0 carry");
        assert_eq!(
            original_c0_carry.public_value.execution_commitment.height,
            carry_height
        );
    }
    let (hook, cold) = if MODE.with(Cell::get) != CaseMode::Normal {
        let reopen = cold_session::Reopen::prepare(case, window);
        (
            pending_cold::ColdHookSlot::new().hook(),
            Some(Box::new(reopen)),
        )
    } else {
        let hook = window.into_shared_node_hook_for_test(case.binding).unwrap();
        let base_path = case
            .root
            .path()
            .join("ledger/successor-active-v1")
            .join(case.record.request.intent.body.checkpoint.digest().unwrap())
            .join("base.cbor");
        prepare_fault::check(
            &hook,
            case.binding,
            case.lease,
            &mut backend,
            &base_path,
            &readback_signatures,
            open_deadline,
        );
        (hook, None)
    };

    Prepared {
        hook,
        request,
        role,
        backend,
        before,
        namespace,
        store: case.store.clone(),
        carry_height,
        cold,
    }
}
impl Prepared {
    #[inline(never)]
    pub(super) fn run(
        mut self,
        h: &QualifiedH<BftProfileTargetV2>,
        root: &Scratch,
        binding: &BftSignerBindingV3,
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
        public: Arc<LocalBftSuccessorArchiveStoreV1>,
    ) {
        if let Some(cold) = self.cold.take() {
            cold_session::run(self, *cold, h, root, binding, archive, public);
            return;
        }
        reopen_faults::check(binding, &self.request, &self.role, &mut self.backend);
        let hook = self.hook.clone();
        {
            let mut unconfigured = hook.lock().unwrap();
            unconfigured
                .validate_bft_v3_action_payload(&payload("active-protected-prestart-shape"))
                .expect("unconfigured Active accepts a valid closed RuntimeAction payload");
            assert!(
                unconfigured
                    .validate_bft_v3_action_payload(&[0xff])
                    .is_err(),
                "unconfigured Active must still reject malformed payloads"
            );
            assert_eq!(
                unconfigured
                    .validate_bft_v3_action_batch(binding.initial_schedule(), &[])
                    .unwrap_err(),
                "active Node is not configured",
                "prestart shape admission cannot configure actual parent execution"
            );
        }
        let mut noise = source_retention::ActiveNoiseFixtureForTest::new(binding, self.hook);
        let verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
            h.anchor.clone(),
            h.target.clone(),
            archive,
        )
        .unwrap()
        .with_installation_root(root.path().join("ledger"))
        .unwrap()
        .with_active_private_store(self.store.clone())
        .unwrap()
        .with_original_retention_reader(self.role.clone(), noise.original_input_reader())
        .unwrap()
        .with_original_retention_binding(binding.clone())
        .unwrap();
        let verifier = Arc::new(verifier);
        let support = BftSuccessorWindowSupportV1::new(public, Duration::from_secs(120))
            .unwrap()
            .with_active_window_rebase()
            .with_driver_verifier(verifier.clone());
        let signs = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let service = noise.service(
            binding,
            self.request,
            self.role.clone(),
            self.backend.clone(),
            vec![Box::new(ReadbackOnlyKey(signs.clone()))],
            support,
            verifier,
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
                        assert!(Instant::now() < deadline, "Active owner connection");
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("Active accept: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            service.serve_tcp_connection(&mut stream).unwrap();
        });
        let server = reserved_archive::ScopedArchiveServerV1::new(stop, server);
        let mut node = noise.runtime(root.path(), binding, address, self.role);
        node.submit_consensus_action_payload(34002, payload("active-protected-fresh-action"))
            .unwrap();
        eprintln!("actual protected Active stage: real Node carry/fresh Some hooks");
        node.start().unwrap();
        assert!(
            hook.lock()
                .unwrap()
                .validate_bft_v3_action_batch(binding.initial_schedule(), &[])
                .is_err(),
            "configured Active must reject the old H parent schedule"
        );
        let committed = wait_batch(&node, 34002);
        assert!(
            committed.height > self.carry_height,
            "new action cannot replace original carry"
        );
        node.stop().unwrap();
        drop(node);
        server.join().unwrap();
        let after = self
            .backend
            .load_private_retention_current(&binding.digest().unwrap())
            .unwrap();
        let before = self.before.retention.as_ref().unwrap();
        let ledger = after.retention.as_ref().unwrap();
        for entry in &before.map.entries {
            assert!(
                ledger.map.entries.contains(entry),
                "carry facts remain exact, without reReserve"
            );
        }
        for (digest, original) in &self.before.inputs {
            assert!(
                Arc::ptr_eq(
                    original,
                    after.inputs.get(digest).expect("original carry input")
                ),
                "carry keeps its original backend allocation"
            );
        }
        let fresh = ledger
            .map
            .entries
            .iter()
            .find(|entry| {
                entry.key.ledger_origin_digest == self.namespace
                    && entry.key.height == committed.height
            })
            .expect("fresh Active Some uses authenticated new owner namespace");
        assert_eq!(fresh.lifecycle, BftPrivateRetentionLifecycleV1::Ready);
        let BftPrivateRetentionFactV1::Some {
            admission_digest,
            input_ref: Some(_),
            ..
        } = &fresh.fact
        else {
            panic!("fresh Active proposal cannot be None or generic retention");
        };
        let input = after
            .inputs
            .get(admission_digest)
            .expect("fresh original input under original admission identity");
        let reference =
            BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, fresh)
                .expect("fresh exact original source/key/fact reference");
        reference
            .validate_input(input)
            .expect("fresh original input matches its full descriptor");
        let original_data = input
            .objects
            .iter()
            .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
            .expect("original Data object");
        let data: serde_cbor::Value = serde_cbor::from_slice(&original_data.bytes).unwrap();
        let serde_cbor::Value::Map(fields) = data else {
            panic!("fresh Data map");
        };
        assert_eq!(
            fields.get(&serde_cbor::Value::Text("schema_version".into())),
            Some(&serde_cbor::Value::Integer(11)),
            "fresh Active Local requires its distinct domain"
        );
        assert!(
            signs.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "real carry/new QC signed"
        );
    }
}
