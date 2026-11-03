//! Actual World/native-provider min CAS and independently read-back adoption.
//! Keys and protected backend are explicit memory doubles. Private rolling and
//! actual Node startup have separate behavioral cases below.
use super::super::super::super::formal_header::{
    ActualBftSuccessorWindowDriverVerifierV1, FormalBftHeaderActiveExecutionV1,
    OriginalHeaderIdentityV1,
};
use super::super::super::super::formal_header::{
    SuccessorPrivateExportRequestV1, SuccessorPrivateInstallRequestV1,
};
use super::*;

#[path = "driver_bft_v3_pending_commit_archive_tests.rs"]
mod pending_commit_archive_tests;

#[path = "driver_bft_v3_successor_source_test_support.rs"]
mod source_retention;
use source_retention::{BackendState, SourceFixture};
#[path = "driver_bft_v3_source_commit_actual_tests.rs"]
mod source_commit_actual;
#[path = "driver_bft_v3_source_continuity_tests.rs"]
mod source_continuity;
#[path = "driver_bft_v3_source_permission_order_tests.rs"]
mod source_permission_order;
#[path = "driver_bft_v3_source_seal_actual_tests.rs"]
mod source_seal_actual;

#[cfg(feature = "node-libp2p")]
#[path = "driver_bft_v3_source_active_some_tests.rs"]
mod source_active_some;
#[path = "driver_bft_v3_source_selected_archive_tests.rs"]
mod source_selected_archive;

#[path = "driver_bft_v3_successor_install_keys_tests.rs"]
mod install_keys;
use install_keys::{ReadbackOnlyKey, StopNextPrecommit};
#[cfg(feature = "node-libp2p")]
#[path = "driver_bft_v3_source_batch_noise_tests.rs"]
mod source_batch_noise;
#[test]
fn wire3_successor_actual_minimum_pointer_and_new_owner_adoption() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(install_case)
        .unwrap()
        .join()
        .unwrap();
}
fn install_case() {
    install_case_with_active(false);
}
#[test]
fn wire3_private_active_segment_executes_height65_and_cold_reopens() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active(true))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn wire3_native_node_two_phase_active_start_replays_before_serving() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, 0))
        .unwrap()
        .join()
        .unwrap();
}
#[path = "driver_bft_v3_window_execution_tests.rs"]
mod active_tests;
#[test]
fn wire3_native_node_resumes_original_prepared_window_before_serving() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, u64::MAX - 1))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn wire3_native_node_resumes_original_installed_pending_before_serving() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, u64::MAX))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn wire3_shipping_cli_two_phase_active_start_commits_nonempty_business() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, u64::MAX - 2))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn wire3_native_node_reservation_only_cold_start_never_exposes_legacy_execution() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, u64::MAX - 3))
        .unwrap()
        .join()
        .unwrap();
}
#[path = "driver_bft_v3_node_active_start_tests.rs"]
mod native_active_start;
#[path = "driver_bft_v3_node_reservation_start_tests.rs"]
mod reservation_start;
#[test]
fn wire3_private_active_segment_recovers_pre_pin_reservation() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, true, 66))
        .unwrap()
        .join()
        .unwrap();
}
fn install_case_with_active(exercise_active: bool) {
    install_case_with_active_and_pre_pin(exercise_active, false, 66);
}
#[test]
fn wire3_private_active_two_windows_replay_and_cold_open() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| install_case_with_active_and_pre_pin(true, false, 8))
        .unwrap()
        .join()
        .unwrap();
}
fn install_case_with_active_and_pre_pin(exercise_active: bool, pre_pin: bool, target_offset: u64) {
    install_case_with_retention(
        exercise_active,
        pre_pin,
        target_offset,
        CandidateRetentionProfile::LegacyRecovery,
    );
}
fn install_case_with_retention(
    exercise_active: bool,
    pre_pin: bool,
    target_offset: u64,
    retention: CandidateRetentionProfile,
) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let (actual, binding, snapshot, mut source_fixture) = if matches!(
        retention,
        CandidateRetentionProfile::PublicationReserved
    ) {
        let (actual, binding, state, source) = SourceFixture::prepare(&h, &root, &store);
        (actual, binding, state, Some(source))
    } else {
        let actual = driver(&h, root.path(), &store);
        let binding = signer_binding(&actual);
        let state = BackendState::Legacy(Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
            successor_archive_transfer: Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
            state: BftSignerStateV3::from_handoff(&binding).unwrap(),
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 100,
        })));
        (actual, binding, state, None)
    };
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let selected_signatures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let selected_probe = source_selected_archive::register(
        &actual,
        &binding,
        &snapshot,
        &root.path().join("ledger"),
        selected_signatures.clone(),
    );
    let interrupted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let keys: Vec<Box<dyn BftSigningKeyHandleV3>> = if selected_probe.is_some() {
        vec![Box::new(ReadbackOnlyKey(selected_signatures.clone()))]
    } else {
        vec![Box::new(StopNextPrecommit {
            snapshot: snapshot.clone(),
            calls: interrupted.clone(),
        })]
    };
    let first_service = Arc::new(if let Some(source) = &source_fixture {
        source.service(&binding, keys, None)
    } else {
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            keys,
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshot.legacy(),
            }),
        )
        .unwrap()
    });
    let (address, server) = tcp(first_service);
    let mut node = if let Some(source) = &source_fixture {
        source.runtime(root.path(), &binding, address)
    } else {
        runtime_with_retention(root.path(), actual.clone(), &binding, address, retention)
    };
    node.submit_consensus_action_payload(34001, payload("successor-installed-original-action"))
        .unwrap();
    node.start().unwrap();
    let receipt = wait_batch(&node, 34001);
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let state = snapshot.lock().unwrap().state.clone();
        if selected_probe
            .as_ref()
            .is_some_and(|probe| probe.finished())
        {
            break;
        }
        if interrupted.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            let journal = state.journal.as_ref().unwrap();
            assert_eq!(
                journal.height(),
                state.history.last().unwrap().execution_commitment.height + 1
            );
            assert!(journal.pending_commit().is_none());
            assert!(journal.pending_vote_intent().is_some());
            assert!(journal.events().iter().any(|e| matches!(
                e,
                oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Proposal {
                    local: true,
                    ..
                }
            )));
            assert!(journal.events().iter().any(|e| matches!(e, oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Vote { vote, local: true, .. } if vote.phase == oasis7_proto::distributed_bft::v3::BftVotePhaseV3::Prevote)));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "T+1 PC fault did not preserve original Proposal/PV"
        );
        thread::sleep(Duration::from_millis(10));
    }
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("successor-installed-original-action")
    );
    let private_root = Scratch::new();
    let private_archive =
        Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(private_root.path())).unwrap());
    let public_root = Scratch::new();
    let public_archive =
        Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(public_root.path())).unwrap());
    let verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
        h.anchor.clone(),
        h.target.clone(),
        private_archive.clone(),
    )
    .unwrap()
    .with_installation_root(root.path().join("ledger"))
    .unwrap()
    .with_active_private_store(store.clone())
    .unwrap();
    let verifier = Arc::new(if let Some(source) = &source_fixture {
        verifier
            .with_original_retention_reader(
                source.fixed_attestor().clone(),
                source.original_input_reader(),
            )
            .unwrap()
            .with_original_retention_binding(binding.clone())
            .unwrap()
    } else {
        verifier
    });
    #[cfg(feature = "node-libp2p")]
    let active_public_material = public_archive.clone();
    let public_material: Arc<dyn oasis7_distfs::protected_bft_v3::BftSuccessorArchiveStoreV1> =
        if exercise_active && target_offset == u64::MAX - 3 {
            Arc::new(reservation_start::PauseFirstMaterial::new(public_archive))
        } else {
            public_archive
        };
    let support = BftSuccessorWindowSupportV1::new(public_material, Duration::from_secs(120))
        .unwrap()
        .with_active_window_rebase()
        .with_driver_verifier(verifier);
    let readback_signatures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let keys: Vec<Box<dyn BftSigningKeyHandleV3>> =
        vec![Box::new(ReadbackOnlyKey(readback_signatures.clone()))];
    let successor_service = Arc::new(if let Some(source) = &source_fixture {
        source.service(&binding, keys, Some(support))
    } else {
        BftSignerServiceV3::open_with_successor_window(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            keys,
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshot.legacy(),
            }),
            support,
        )
        .unwrap()
    });
    let _provider_input_egress = source_fixture
        .as_ref()
        .map(|source| source.provider_input_egress(&binding, successor_service.clone()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let connections = if exercise_active {
        match target_offset {
            0 => 6,
            n if n == u64::MAX - 2 => 6,
            n if n == u64::MAX - 3 => usize::MAX,
            u64::MAX => 4,
            n if n == u64::MAX - 1 => 3,
            _ => 2,
        }
    } else {
        2
    };
    // This scenario adds independent negative owners and a full cold Node.
    // Its server lifetime is explicit; connection count is not a hidden budget.
    let server_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stopping = server_stop.clone();
    #[cfg(feature = "node-libp2p")]
    let batch_noise_service = source_batch_noise::requested().then(|| successor_service.clone());
    let server = thread::spawn(move || {
        for _ in 0..connections {
            if server_stopping.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            let deadline = Instant::now() + Duration::from_secs(180);
            let mut stream = loop {
                if server_stopping.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "original successor owner did not reconnect"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("successor accept: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            successor_service.serve_tcp_connection(&mut stream).unwrap();
        }
    });
    let server = reserved_archive::ScopedArchiveServerV1::new(server_stop.clone(), server);
    let provider_pin = hex::encode(fixtures::key(116).verifying_key().to_bytes());
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(120),
        binding.clone(),
        fixtures::key(117),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: provider_pin.clone(),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    let provider = if let Some(source) = &source_fixture {
        source.provider(provider)
    } else {
        provider
    };
    let mut lease = provider.acquire(&binding).unwrap();
    let mut original = lease.checkpoint().unwrap();
    let certificate = original.state.history.last().unwrap();
    let height = certificate.execution_commitment.height;
    assert!(height >= receipt.height);
    let id = certificate.header.block_id().unwrap();
    let source = BftSuccessorWindowSourceV1::from_binding(&binding).unwrap();
    if source_seal_actual::requested() {
        let cut = actual
            .lock()
            .unwrap()
            .verified_private_retention_cut(
                &source,
                height,
                &id,
                Instant::now() + Duration::from_secs(120),
            )
            .unwrap();
        source_seal_actual::exercise(lease.as_mut(), &original.state, cut);
        drop(lease);
        server.join().unwrap();
        return;
    }
    #[cfg(feature = "node-libp2p")]
    if source_batch_noise::requested() {
        source_batch_noise::exercise(
            source_fixture.as_ref().unwrap(),
            batch_noise_service.unwrap(),
            &binding,
        );
        drop(lease);
        server_stop.store(true, std::sync::atomic::Ordering::SeqCst);
        server.join().unwrap();
        return;
    }
    if source_fixture.is_some() {
        let cut = actual
            .lock()
            .unwrap()
            .verified_private_retention_cut(
                &source,
                height,
                &id,
                Instant::now() + Duration::from_secs(120),
            )
            .unwrap();
        original = lease
            .seal_private_retention(
                original.state.revision,
                original.state.state_digest.clone(),
                cut,
                Instant::now() + Duration::from_secs(120),
            )
            .unwrap()
            .checkpoint()
            .clone();
    }
    let request = SuccessorPrivateExportRequestV1 {
        archive: &private_archive,
        source: &source,
        public_state: &original.state,
        previous_usage: BftSuccessorWindowArchiveUsageV1::default(),
        height,
        header_id: &id,
        sequence: 1,
        predecessor: None,
        deadline: Instant::now() + Duration::from_secs(120),
    };
    let exported = if let Some(source) = &source_fixture {
        actual
            .lock()
            .unwrap()
            .export_successor_private_closure_with_retention(
                request,
                lease.as_mut(),
                source.original_input_reader().as_ref(),
            )
    } else {
        actual
            .lock()
            .unwrap()
            .export_successor_private_closure(request)
    }
    .unwrap();
    #[cfg(feature = "node-libp2p")]
    if let Some(provider_inputs) = &_provider_input_egress {
        provider_inputs.assert_material_released();
    }
    if matches!(retention, CandidateRetentionProfile::PublicationReserved) {
        reserved_archive::assert_original_promises_exported(&root, &store, exported.manifest());
    }
    let mut schedule = binding.initial_schedule().clone();
    for certificate in &original.state.history {
        schedule = oasis7_consensus::tendermint_v3::verify_commit_certificate_token_v3(
            &schedule,
            certificate,
        )
        .unwrap()
        .next_height_schedule()
        .clone();
    }
    let mut public_manifest = BftSuccessorWindowArchiveManifestV1 {
        public_index: None,
        schema_version: 1,
        source: source.clone(),
        kind: BftSuccessorWindowArchiveKindV1::Public,
        segment_sequence: 1,
        predecessor_root: None,
        first_height: binding.initial_schedule().height(),
        last_height: height,
        entries: vec![BftSuccessorWindowArchiveEntryV1 {
            identity_digest: original.state.state_digest.clone(),
            object: exported.public_state_ref().clone(),
        }],
        usage: exported.manifest().usage,
    };
    oasis7_distfs::protected_bft_v3::index_whole_state_signed_results_v1(
        &mut public_manifest,
        &original.state,
    )
    .unwrap();
    let journal_ref = original.state.journal.as_ref().map(|j| {
        private_archive
            .persist_object(
                &oasis7_cas::to_canonical_cbor(j).unwrap(),
                512 * 1024 * 1024,
            )
            .unwrap()
    });
    let body = BftSuccessorWindowCheckpointBodyV1 {
        schema_version: 1,
        source: source.clone(),
        request_id: "actual-successor-install-1".into(),
        sequence: 1,
        predecessor_checkpoint_digest: source.origin_digest().unwrap(),
        cut: exported.cut().clone(),
        successor_schedule_cbor: oasis7_cas::to_canonical_cbor(&schedule).unwrap(),
        successor_schedule_digest: schedule.digest().unwrap(),
        current_journal_ref: journal_ref,
        current_journal_digest: original.state.journal.as_ref().map(|j| j.digest().unwrap()),
        public_archive_root: public_manifest.digest().unwrap(),
        execution_archive_root: exported.manifest().digest().unwrap(),
        execution_manifest_ref: exported.execution_manifest_ref().clone(),
        evidence_refs: vec![exported.metadata_ref().clone()],
        usage: exported.manifest().usage,
    };
    assert_eq!(
        exported.pending_actions().len(),
        selected_probe
            .as_ref()
            .map_or(1, |probe| probe.expected_pending_count()),
        "the original T+1 Proposal/PV requires its retained package"
    );
    let pending_ref = &exported.pending_actions()[0];
    let pending_path = private_root
        .path()
        .join("cas/blobs")
        .join(format!("{}.blob", pending_ref.digest));
    let pending_bytes = fs::read(&pending_path).unwrap();
    let detached_verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
        h.anchor.clone(),
        h.target.clone(),
        private_archive.clone(),
    )
    .unwrap();
    let detached_verifier = if let Some(source) = &source_fixture {
        detached_verifier
            .with_original_retention_reader(
                source.fixed_attestor().clone(),
                source.original_input_reader(),
            )
            .unwrap()
            .with_original_retention_binding(binding.clone())
            .unwrap()
    } else {
        detached_verifier
    };
    if source_continuity::requested() {
        detached_verifier
            .verify_retention_continuity_mutations_for_test(
                &body,
                &original.state,
                exported.public_state_ref(),
                &store,
                Instant::now() + Duration::from_secs(120),
            )
            .unwrap();
        drop(lease);
        server.join().unwrap();
        return;
    }
    detached_verifier
        .verify_closure(
            &body,
            &original.state,
            exported.public_state_ref(),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    if let Some(probe) = &selected_probe {
        probe.assert_after_archive(&original.state, exported.manifest(), &readback_signatures);
        probe.check_purposes(
            &detached_verifier,
            &body,
            &original.state,
            exported.public_state_ref(),
            &readback_signatures,
        );
        drop(lease);
        server.join().unwrap();
        return;
    }
    if source_permission_order::requested() {
        let source_fixture = source_fixture
            .as_ref()
            .expect("real protected source required");
        let deny = Arc::new(source_permission_order::DenyReader::default());
        let denied_verifier = ActualBftSuccessorWindowDriverVerifierV1::new(
            h.anchor.clone(),
            h.target.clone(),
            private_archive.clone(),
        )
        .unwrap()
        .with_original_retention_reader(source_fixture.fixed_attestor().clone(), deny.clone())
        .unwrap()
        .with_original_retention_binding(binding.clone())
        .unwrap();
        source_permission_order::exercise(source_permission_order::Check {
            archive: &private_archive,
            private_root: private_root.path(),
            metadata: exported.metadata_ref(),
            public_state_ref: exported.public_state_ref(),
            body: &body,
            state: &original.state,
            verifier: &denied_verifier,
            reader: &deny,
        });
        drop(lease);
        server.join().unwrap();
        return;
    }
    if reserved_completeness::requested() {
        let omitted = exported
            .omit_pending_publication_for_test(&private_archive, height + 1)
            .unwrap();
        reserved_completeness::exercise(
            reserved_completeness::AttackInput {
                verifier: &detached_verifier,
                archive: &private_archive,
                body: &body,
                original_state: &original.state,
                manifest: exported.manifest(),
                public_manifest: &public_manifest,
                public_ref: exported.public_state_ref(),
                metadata_ref: exported.metadata_ref(),
                signed_checkpoint: &oasis7_cas::to_canonical_cbor(&original).unwrap(),
            },
            omitted,
        );
        return;
    }
    fs::remove_file(&pending_path).unwrap();
    assert!(
        detached_verifier
            .verify_closure(
                &body,
                &original.state,
                exported.public_state_ref(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err(),
        "missing T+1 original actions must reject"
    );
    fs::write(&pending_path, b"wrong original T+1 candidate body").unwrap();
    assert!(
        detached_verifier
            .verify_closure(
                &body,
                &original.state,
                exported.public_state_ref(),
                Instant::now() + Duration::from_secs(120)
            )
            .is_err(),
        "wrong T+1 original actions must reject even after prior read"
    );
    fs::write(&pending_path, pending_bytes).unwrap();
    let checkpoint_digest = body.digest().unwrap();
    let request_id = body.request_id.clone();
    let prepare_body = BftSuccessorWindowInstallIntentBodyV1 {
        schema_version: 1,
        checkpoint: body.clone(),
        expected_lease_generation: original.lease_generation,
        expected_revision: original.state.revision,
        expected_state_digest: original.state.state_digest.clone(),
        driver_evidence_digest: body.cut.driver_install_digest.clone(),
    };
    let prepared_result =
        lease.prepare_successor_window(prepare_body.clone(), public_manifest.clone());
    if exercise_active && target_offset == u64::MAX - 3 {
        assert!(
            prepared_result.is_err(),
            "material failure must interrupt original Prepare after reservation CAS"
        );
        drop(actual);
        drop(lease);
        reservation_start::exercise(
            &h,
            &root,
            store,
            &binding,
            &provider_pin,
            address,
            private_archive,
            snapshot.legacy(),
            readback_signatures.clone(),
            prepare_body,
            public_manifest,
        );
        server_stop.store(true, std::sync::atomic::Ordering::SeqCst);
        server.join().unwrap();
        return;
    }
    let (prepared, _) = prepared_result.unwrap();
    assert_eq!(
        prepared.request.status,
        BftSuccessorWindowStatusV1::Prepared
    );
    if exercise_active && target_offset == u64::MAX - 1 {
        drop(actual);
        drop(lease);
        native_active_start::run(
            &h,
            &root,
            store,
            &binding,
            &provider_pin,
            address,
            private_archive,
            snapshot.legacy(),
            readback_signatures.clone(),
            false,
            false,
        );
        server.join().unwrap();
        return;
    }
    let original_input_reads_before_commit = source_fixture
        .as_ref()
        .map(|source| source.original_input_read_count());
    let (committed, installed) = lease
        .commit_successor_window(&request_id, &checkpoint_digest)
        .unwrap();
    let original_receipt = committed.request.receipt.clone().unwrap();
    assert_eq!(
        committed.request.status,
        BftSuccessorWindowStatusV1::InstalledPending
    );
    assert_eq!(
        installed.state.successor_window.as_ref().unwrap().minimum,
        Some(body.minimum().unwrap())
    );
    if source_commit_actual::requested() {
        let source_fixture = source_fixture
            .as_ref()
            .expect("real covered source required");
        assert_eq!(
            Some(source_fixture.original_input_read_count()),
            original_input_reads_before_commit,
            "provider Commit must borrow its own protected originals without calling its network reader"
        );
        assert_eq!(installed.state.history, original.state.history);
        assert_eq!(installed.state.journal, original.state.journal);
        assert_eq!(
            installed.state.private_retention,
            original.state.private_retention
        );
        assert_eq!(
            readback_signatures.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "archive validation must not sign a new validator event"
        );
        drop(lease);
        server.join().unwrap();
        return;
    }
    let premature = BftSuccessorWindowAdoptedEvidenceV1 {
        active_segment_origin: None,
        checkpoint_digest: checkpoint_digest.clone(),
        driver_install_digest: body.cut.driver_install_digest.clone(),
        schedule_digest: body.successor_schedule_digest.clone(),
        journal_digest: body.current_journal_digest.clone(),
        readback_state_digest: installed.state.state_digest.clone(),
    };
    assert!(
        lease
            .confirm_successor_window_adopted(&request_id, &checkpoint_digest, premature)
            .is_err(),
        "minimum before pointer must not adopt"
    );
    assert_eq!(snapshot.lock().unwrap().state, installed.state);
    drop(lease); // An unknown/abandoned original attempt never becomes a new request.
    let mut lease = provider.acquire(&binding).unwrap();
    let (recovered, current) = lease
        .get_successor_window(&request_id, &checkpoint_digest)
        .unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered, committed);
    assert_eq!(recovered.request.receipt.as_ref(), Some(&original_receipt));
    assert!(current.lease_generation > original_receipt.body.lease_generation);
    #[cfg(feature = "node-libp2p")]
    if source_active_some::requested() {
        eprintln!(
            "actual protected Active stage: installer must independently recover fixed Source continuity"
        );
    }
    let evidence = source_retention::install(
        &mut actual.lock().unwrap(),
        SuccessorPrivateInstallRequestV1 {
            binding: &binding,
            provider_pin: &provider_pin,
            lease: lease.as_mut(),
            request_id: &request_id,
            checkpoint_digest: &checkpoint_digest,
            archive: private_archive.clone(),
            deadline: Instant::now() + Duration::from_secs(120),
        },
        source_fixture.as_ref(),
    )
    .unwrap();
    let signing_calls_before_reads = interrupted.load(std::sync::atomic::Ordering::SeqCst);
    let archived = lease
        .read_successor_original_state(
            &request_id,
            &checkpoint_digest,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    assert_eq!(archived.original_state(), &original.state);
    if matches!(retention, CandidateRetentionProfile::PublicationReserved) {
        reserved_archive::assert_archive_material_failure_and_exact_restore(
            &root,
            &private_archive,
            exported.manifest(),
            || {
                detached_verifier
                    .recover_private_base(
                        &committed,
                        &archived,
                        Instant::now() + Duration::from_secs(120),
                    )
                    .map(|_| ())
            },
        );
    }

    let vote = original
        .state
        .journal
        .as_ref()
        .unwrap()
        .events
        .iter()
        .find_map(|event| match event {
            oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Vote {
                vote,
                local: true,
                ..
            } => Some(vote.clone()),
            _ => None,
        })
        .expect("actual pending journal must retain its original signed prevote");
    let identity =
        oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveIdentityV1::SigningSlot {
            public_key_hex: vote.public_key_hex.clone(),
            height: vote.height,
            round: vote.round,
            phase: "prevote".into(),
            preimage_digest: blake3::hash(&vote.signing_payload().unwrap())
                .to_hex()
                .to_string(),
        };
    let result = lease
        .query_successor_archive(
            &request_id,
            &checkpoint_digest,
            &identity,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap()
        .expect("actual original vote must be recoverable through authenticated TCP");
    assert_eq!(
        result,
        oasis7_distfs::protected_bft_v3::BftSuccessorWindowOriginalSignedResultV1::Vote(vote)
    );
    assert_eq!(
        interrupted.load(std::sync::atomic::Ordering::SeqCst),
        signing_calls_before_reads,
        "archive reads must not call the original consensus key"
    );
    {
        let observed = snapshot.lock().unwrap();
        assert!(
            observed
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes
                > 0
        );
        assert_eq!(
            observed.state, installed.state,
            "read charging must preserve installed WAL State"
        );
    }
    let pointer = root
        .path()
        .join("ledger/successor-installs-v1")
        .join(format!("{checkpoint_digest}.installed.cbor"));
    let bytes = fs::read(&pointer).unwrap();
    let mut old = bytes.clone();
    let digest = installed.state.state_digest.as_bytes();
    let offset = old.windows(digest.len()).position(|w| w == digest).unwrap();
    old[offset..offset + digest.len()].copy_from_slice("dd".repeat(32).as_bytes());
    fs::write(&pointer, &old).unwrap();
    assert!(
        lease
            .confirm_successor_window_adopted(&request_id, &checkpoint_digest, evidence.clone())
            .is_err(),
        "old canonical pointer digest must reject"
    );
    assert_eq!(snapshot.lock().unwrap().state, installed.state);
    fs::write(&pointer, b"corrupt pointer").unwrap();
    assert!(
        lease
            .confirm_successor_window_adopted(&request_id, &checkpoint_digest, evidence.clone())
            .is_err()
    );
    fs::write(&pointer, &bytes).unwrap();
    let retried = source_retention::install(
        &mut actual.lock().unwrap(),
        SuccessorPrivateInstallRequestV1 {
            binding: &binding,
            provider_pin: &provider_pin,
            lease: lease.as_mut(),
            request_id: &request_id,
            checkpoint_digest: &checkpoint_digest,
            archive: private_archive.clone(),
            deadline: Instant::now() + Duration::from_secs(120),
        },
        source_fixture.as_ref(),
    )
    .unwrap();
    assert_eq!(retried, evidence);
    assert_eq!(
        fs::read(&pointer).unwrap(),
        bytes,
        "original pointer retry is byte-identical"
    );
    // A valid archive/pointer cannot attest an installed driver after its
    // actual stable carrier changes. Restore originals before each assertion.
    for relative in [
        "ledger/bft-format.lock",
        "ledger/formal-bft-header-prefix-v1/writer.lock",
    ] {
        let carrier = root.path().join(relative);
        let original_bytes = fs::read(&carrier).unwrap();
        let mut corrupt = original_bytes.clone();
        corrupt[0] ^= 1;
        fs::write(&carrier, &corrupt).unwrap();
        let rejected = lease.confirm_successor_window_adopted(
            &request_id,
            &checkpoint_digest,
            evidence.clone(),
        );
        fs::write(&carrier, &original_bytes).unwrap();
        assert!(
            rejected.is_err(),
            "changed live carrier token must prevent Adopt: {relative}"
        );
        assert_eq!(snapshot.lock().unwrap().state, installed.state);

        let retained = carrier.with_extension("retained-original");
        fs::rename(&carrier, &retained).unwrap();
        fs::copy(&retained, &carrier).unwrap();
        let rejected = lease.confirm_successor_window_adopted(
            &request_id,
            &checkpoint_digest,
            evidence.clone(),
        );
        fs::remove_file(&carrier).unwrap();
        fs::rename(&retained, &carrier).unwrap();
        assert!(
            rejected.is_err(),
            "same bytes in a replacement inode must prevent Adopt: {relative}"
        );
        assert_eq!(snapshot.lock().unwrap().state, installed.state);
    }
    // Preserve every child inode and byte while replacing only a configured
    // directory. Authentic original carriers cannot authorize a different root.
    for relative in ["ledger/formal-bft-header-prefix-v1", "ledger"] {
        let directory = root.path().join(relative);
        let retained = directory.with_extension("retained-original-directory");
        let permissions = fs::metadata(&directory).unwrap().permissions();
        fs::rename(&directory, &retained).unwrap();
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, permissions).unwrap();
        let names = fs::read_dir(&retained)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        for name in &names {
            fs::rename(retained.join(name), directory.join(name)).unwrap();
        }
        let rejected = lease.confirm_successor_window_adopted(
            &request_id,
            &checkpoint_digest,
            evidence.clone(),
        );
        for name in &names {
            fs::rename(directory.join(name), retained.join(name)).unwrap();
        }
        fs::remove_dir(&directory).unwrap();
        fs::rename(&retained, &directory).unwrap();
        assert!(
            rejected.is_err(),
            "replacement live directory must prevent Adopt: {relative}"
        );
        assert_eq!(snapshot.lock().unwrap().state, installed.state);
    }
    if exercise_active && target_offset == u64::MAX {
        drop(actual);
        drop(lease);
        native_active_start::run(
            &h,
            &root,
            store,
            &binding,
            &provider_pin,
            address,
            private_archive,
            snapshot.legacy(),
            readback_signatures.clone(),
            false,
            false,
        );
        server.join().unwrap();
        return;
    }
    let calls_before_adoption = interrupted.load(std::sync::atomic::Ordering::SeqCst);
    let (adopted, final_state) = lease
        .confirm_successor_window_adopted(&request_id, &checkpoint_digest, evidence.clone())
        .unwrap();
    assert_eq!(adopted.request.status, BftSuccessorWindowStatusV1::Adopted);
    assert_eq!(adopted.request.receipt.as_ref(), Some(&original_receipt));
    assert_eq!(adopted.request.adopted_evidence.as_ref(), Some(&evidence));
    assert_eq!(
        final_state.state.successor_window.as_ref().unwrap().minimum,
        installed.state.successor_window.as_ref().unwrap().minimum
    );
    assert_eq!(
        final_state
            .state
            .active_window
            .as_ref()
            .unwrap()
            .checkpoint_digest,
        checkpoint_digest
    );
    assert!(
        final_state.state.history.is_empty(),
        "Adopted active base archives the old public suffix"
    );
    assert_eq!(
        interrupted.load(std::sync::atomic::Ordering::SeqCst),
        calls_before_adoption,
        "detached closure verification never signs T+1"
    );
    let (again, again_state) = lease
        .confirm_successor_window_adopted(&request_id, &checkpoint_digest, evidence)
        .unwrap();
    assert_eq!(again, adopted);
    assert_eq!(again_state.state, final_state.state);
    assert_eq!(
        readback_signatures.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "prepare/minimum/install/readback/adopt never sign a new T+1 proposal or vote"
    );
    #[cfg(feature = "node-libp2p")]
    if source_active_some::requested() {
        let prepared = source_active_some::prepare(
            &mut active_tests::ActiveExerciseV1 {
                h: &h,
                root: &root,
                store: store.clone(),
                binding: &binding,
                pin: &provider_pin,
                verifier: &detached_verifier,
                original: &archived,
                record: &adopted,
                lease: lease.as_mut(),
                pre_pin,
                archive: private_archive.clone(),
                target_offset,
                reserved_publication: true,
            },
            actual,
            source_fixture.take().unwrap(),
            _provider_input_egress,
            readback_signatures.clone(),
        );
        drop(lease);
        server.join().unwrap();
        prepared.run(&h, &root, &binding, private_archive, active_public_material);
        return;
    }
    if exercise_active && (target_offset == 0 || target_offset == u64::MAX - 2) {
        drop(actual);
        drop(lease);
        native_active_start::run(
            &h,
            &root,
            store,
            &binding,
            &provider_pin,
            address,
            private_archive,
            snapshot.legacy(),
            readback_signatures.clone(),
            true,
            target_offset == u64::MAX - 2,
        );
        server.join().unwrap();
        return;
    }
    if exercise_active {
        drop(actual);
        active_tests::exercise(active_tests::ActiveExerciseV1 {
            h: &h,
            root: &root,
            store: store.clone(),
            binding: &binding,
            pin: &provider_pin,
            verifier: &detached_verifier,
            original: &archived,
            record: &adopted,
            lease: lease.as_mut(),
            pre_pin,
            archive: private_archive.clone(),
            target_offset,
            reserved_publication: matches!(
                retention,
                CandidateRetentionProfile::PublicationReserved
                    | CandidateRetentionProfile::ActivePublicationReserved
            ),
        });
    }
    drop(lease);
    server.join().unwrap();
}

#[path = "driver_bft_v3_reserved_archive_tests.rs"]
mod reserved_archive;
#[test]
fn wire3_reserved_original_record_and_t1_cross_actual_successor_archive() {
    install_case_with_retention(
        true,
        false,
        8,
        CandidateRetentionProfile::PublicationReserved,
    );
}

#[test]
fn wire3_active_node_reserved_publication_uses_authenticated_base_and_original_qc() {
    install_case_with_retention(
        true,
        false,
        8,
        CandidateRetentionProfile::ActivePublicationReserved,
    );
}

#[path = "driver_bft_v3_reserved_completeness_tests.rs"]
mod reserved_completeness;

#[cfg(feature = "node-libp2p")]
#[path = "driver_bft_v3_source_received_alternative_tests.rs"]
mod source_received_alternative;
#[cfg(feature = "node-libp2p")]
pub(super) fn intercept_received_alternative(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    candidate: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Option<Result<String, String>> {
    source_received_alternative::intercept(actual, binding, lease, context, candidate, deadline)
}

pub(super) fn intercept_selected_archive(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    candidate: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Option<Result<String, String>> {
    source_selected_archive::intercept(actual, binding, lease, context, candidate, deadline)
}
