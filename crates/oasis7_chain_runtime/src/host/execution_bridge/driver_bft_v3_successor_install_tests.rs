//! Actual World/native-provider min CAS and independently read-back adoption.
//! Key and protected backend are explicit memory doubles. No hot-window reset,
//! automatic startup continuation or height-65 support is asserted.
use super::super::super::super::formal_header::ActualBftSuccessorWindowDriverVerifierV1;
use super::*;

struct StopNextPrecommit {
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
    calls: Arc<std::sync::atomic::AtomicUsize>,
}
impl BftSigningKeyHandleV3 for StopNextPrecommit {
    fn public_key_hex(&self) -> String {
        hex::encode(fixtures::key(120).verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if bytes
            .windows(b"oasis7.bft.vote.v3.sign".len())
            .any(|w| w == b"oasis7.bft.vote.v3.sign")
            && bytes.windows(b"precommit".len()).any(|w| w == b"precommit")
            && !self.snapshot.lock().unwrap().state.history.is_empty()
        {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(BftSignerErrorV3(
                "injected original T+1 PC interruption".into(),
            ));
        }
        Ok(fixtures::key(120).sign(bytes).to_bytes())
    }
}
struct ReadbackOnlyKey(Arc<std::sync::atomic::AtomicUsize>);
impl BftSigningKeyHandleV3 for ReadbackOnlyKey {
    fn public_key_hex(&self) -> String {
        hex::encode(fixtures::key(120).verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(fixtures::key(120).sign(bytes).to_bytes())
    }
}
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
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let interrupted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let first_service = Arc::new(
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(StopNextPrecommit {
                snapshot: snapshot.clone(),
                calls: interrupted.clone(),
            })],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshot.clone(),
            }),
        )
        .unwrap(),
    );
    let (address, server) = tcp(first_service);
    let mut node = runtime(root.path(), actual.clone(), &binding, address);
    node.submit_consensus_action_payload(34001, payload("successor-installed-original-action"))
        .unwrap();
    node.start().unwrap();
    let receipt = wait_batch(&node, 34001);
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let state = snapshot.lock().unwrap().state.clone();
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
    let verifier = Arc::new(
        ActualBftSuccessorWindowDriverVerifierV1::new(
            h.anchor.clone(),
            h.target.clone(),
            private_archive.clone(),
        )
        .unwrap()
        .with_installation_root(root.path().join("ledger"))
        .unwrap(),
    );
    let support = BftSuccessorWindowSupportV1::new(public_archive, Duration::from_secs(120))
        .unwrap()
        .with_driver_verifier(verifier);
    let readback_signatures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let successor_service = Arc::new(
        BftSignerServiceV3::open_with_successor_window(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(ReadbackOnlyKey(readback_signatures.clone()))],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshot.clone(),
            }),
            support,
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(180);
            let mut stream = loop {
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
            successor_service.serve_connection(&mut stream).unwrap();
        }
    });
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
    let mut lease = provider.acquire(&binding).unwrap();
    let original = lease.checkpoint().unwrap();
    let certificate = original.state.history.last().unwrap();
    let height = certificate.execution_commitment.height;
    assert!(height >= receipt.height);
    let id = certificate.header.block_id().unwrap();
    let source = BftSuccessorWindowSourceV1::from_binding(&binding).unwrap();
    let exported = actual
        .lock()
        .unwrap()
        .export_successor_private_closure(
            &private_archive,
            &source,
            &original.state,
            BftSuccessorWindowArchiveUsageV1::default(),
            height,
            &id,
            1,
            None,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
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
        1,
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
    detached_verifier
        .verify_closure(
            &body,
            &original.state,
            exported.public_state_ref(),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
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
    let (prepared, _) = lease
        .prepare_successor_window(
            BftSuccessorWindowInstallIntentBodyV1 {
                schema_version: 1,
                checkpoint: body.clone(),
                expected_lease_generation: original.lease_generation,
                expected_revision: original.state.revision,
                expected_state_digest: original.state.state_digest.clone(),
                driver_evidence_digest: body.cut.driver_install_digest.clone(),
            },
            public_manifest,
        )
        .unwrap();
    assert_eq!(
        prepared.request.status,
        BftSuccessorWindowStatusV1::Prepared
    );
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
    let premature = BftSuccessorWindowAdoptedEvidenceV1 {
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
    let evidence = actual
        .lock()
        .unwrap()
        .install_successor_window(
            &binding,
            &provider_pin,
            lease.as_mut(),
            &request_id,
            &checkpoint_digest,
            private_archive.clone(),
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
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
    let retried = actual
        .lock()
        .unwrap()
        .install_successor_window(
            &binding,
            &provider_pin,
            lease.as_mut(),
            &request_id,
            &checkpoint_digest,
            private_archive,
            Instant::now() + Duration::from_secs(120),
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
        final_state.state.history, original.state.history,
        "this slice never clears hot history"
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
    drop(lease);
    server.join().unwrap();
}
