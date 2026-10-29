//! Real authenticated TCP and protected CAS; memory backend/archive and explicit
//! driver verifier doubles are transaction tests, not real World/private custody.
use super::*;
use crate::protected_bft_v3::state::validate_state;
use crate::protected_bft_v3::successor_archive_store::BftSuccessorArchiveStoreV1;
use oasis7_consensus::tendermint_v3::{
    BftHeaderValueV3, BftPublicProposalV3, BftSigningJournalEventV3, BftSigningJournalV3,
};
use oasis7_formal_protocol::bft::{
    BFT_FORMAL_EXECUTION_DOMAIN_V1, FormalBftCertifiedHeaderV3, FormalBftExecutionCommitmentV1,
};
use oasis7_proto::distributed_bft::v3::*;
use std::time::Instant;

#[derive(Default)]
struct Archive {
    states: Mutex<BTreeMap<String, BftSignerStateV3>>,
    fail_write: AtomicUsize,
}
impl BftSuccessorArchiveStoreV1 for Archive {
    fn persist_public_state(
        &self,
        state: &BftSignerStateV3,
    ) -> Result<BftSuccessorWindowArchiveRefV1, BftSignerErrorV3> {
        if self.fail_write.load(Ordering::SeqCst) != 0 {
            return Err(BftSignerErrorV3("test archive unavailable".into()));
        }
        let reference = reference(&oasis7_cas::to_canonical_cbor(state).unwrap());
        self.states
            .lock()
            .unwrap()
            .insert(reference.digest.clone(), state.clone());
        Ok(reference)
    }
    fn load_public_state(
        &self,
        reference: &BftSuccessorWindowArchiveRefV1,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3> {
        self.states
            .lock()
            .unwrap()
            .get(&reference.digest)
            .cloned()
            .ok_or_else(|| BftSignerErrorV3("test archive missing".into()))
    }
}
fn reference(bytes: &[u8]) -> BftSuccessorWindowArchiveRefV1 {
    BftSuccessorWindowArchiveRefV1 {
        digest: blake3::hash(bytes).to_hex().to_string(),
        bytes: bytes.len() as u64,
    }
}
struct DriverVerifier {
    expected: BftSignerStateV3,
    reject: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}
impl BftSuccessorWindowDriverVerifierV1 for DriverVerifier {
    fn verify_prepared(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if Instant::now() >= deadline
            || self.reject.load(Ordering::SeqCst) != 0
            || state != &self.expected
            || original.request().intent.body.expected_state_digest != state.state_digest
        {
            return Err(BftSignerErrorV3(
                "test independent private closure rejected".into(),
            ));
        }
        Ok(())
    }
    fn verify_adopted(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        state: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if Instant::now() >= deadline
            || self.reject.load(Ordering::SeqCst) != 0
            || state.history != self.expected.history
            || evidence.checkpoint_digest != original.request().intent.body.checkpoint.digest()?
            || evidence.readback_state_digest != state.state_digest
        {
            return Err(BftSignerErrorV3("test independent reopen rejected".into()));
        }
        Ok(())
    }
}
struct CountHandle {
    key: SigningKey,
    calls: Arc<AtomicUsize>,
}
impl BftSigningKeyHandleV3 for CountHandle {
    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.key.sign(bytes).to_bytes())
    }
}
struct UncertainBackend {
    inner: Backend,
    fail_at: Arc<AtomicUsize>,
}
impl BftSignerBackendV3 for UncertainBackend {
    fn load_provisioned(
        &mut self,
        b: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.load_provisioned(b)
    }
    fn acquire_lease(
        &mut self,
        b: &str,
        g: u64,
        t: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.inner.acquire_lease(b, g, t)
    }
    fn release_lease(
        &mut self,
        g: u64,
        t: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.release_lease(g, t)
    }
    fn read_for_lease(
        &mut self,
        g: u64,
        t: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.read_for_lease(g, t)
    }
    fn protected_time_ms(&mut self, g: u64, t: &str) -> Result<i64, BftSignerErrorV3> {
        self.inner.protected_time_ms(g, t)
    }
    fn compare_and_swap(
        &mut self,
        g: u64,
        t: &str,
        r: u64,
        d: &str,
        e: Option<i64>,
        n: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let result = self.inner.compare_and_swap(g, t, r, d, e, n)?;
        if self
            .fail_at
            .compare_exchange((r + 1) as usize, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(BftSignerErrorV3("test committed CAS reply lost".into()));
        }
        Ok(result)
    }
}
fn native_successor(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    archive: Arc<Archive>,
    verifier: Option<Arc<dyn BftSuccessorWindowDriverVerifierV1>>,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    native_successor_fault(
        binding,
        snapshot,
        archive,
        verifier,
        Arc::new(AtomicUsize::new(0)),
    )
}
fn native_successor_fault(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    archive: Arc<Archive>,
    verifier: Option<Arc<dyn BftSuccessorWindowDriverVerifierV1>>,
    fail_at: Arc<AtomicUsize>,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    native_successor_counted(
        binding,
        snapshot,
        archive,
        verifier,
        fail_at,
        Arc::new(AtomicUsize::new(0)),
    )
}
fn native_successor_counted(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    archive: Arc<Archive>,
    verifier: Option<Arc<dyn BftSuccessorWindowDriverVerifierV1>>,
    fail_at: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    let support = BftSuccessorWindowSupportV1::new(archive, Duration::from_secs(5)).unwrap();
    let support = match verifier {
        Some(v) => support.with_driver_verifier(v),
        None => support,
    };
    let service = BftSignerServiceV3::open_with_successor_window(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(CountHandle {
                key: key(31),
                calls: calls.clone(),
            }),
            Box::new(CountHandle {
                key: key(32),
                calls,
            }),
        ],
        Box::new(UncertainBackend {
            inner: Backend {
                binding: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
            },
            fail_at,
        }),
        support,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        if let Err(error) = service.serve_connection(&mut stream) {
            assert_eq!(error.0, "BFT V3 provider is poisoned");
        }
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    (provider, server)
}
// Nonactive local observer deliberately has no signing journal. The cut is an
// actually verified public QC; private execution is only the explicit verifier.
fn fixture_kind(active: bool) -> (BftSignerBindingV3, Arc<Mutex<BftSignerBackendSnapshotV3>>) {
    let mut binding = if active {
        binding_with_epoch_length(6)
    } else {
        binding()
    };
    if !active {
        binding.local_validator_id = "observer".into();
        for identity in &mut binding.provisioned_signing_keys {
            identity.validator_id = "observer".into();
        }
    }
    binding.validate().unwrap();
    let schedule = binding.initial_schedule();
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: schedule.activation_digest().into(),
        scope: schedule.scope().clone(),
        height: schedule.height(),
        parent_commit_ref: schedule.parent_commit_ref().into(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root().into(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        manifest_hash: schedule.parent_manifest_hash().into(),
        post_manifest_hash: hash(80),
        action_root: hash(81),
        event_root: hash(82),
        execution_state_root: hash(83),
        state_root: hash(84),
        projection_root: hash(85),
        result_root: hash(86),
        transition: None,
        transition_ref: None,
    };
    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: commitment.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
    };
    let id = header.block_id().unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        "validator-a".into(),
        id.clone(),
        None,
        &key(31),
    )
    .unwrap();
    let vote = BftVoteV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        0,
        BftVotePhaseV3::Precommit,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        BftVoteValueV3::Block {
            block_id: id.clone(),
        },
        "validator-a".into(),
        &key(31),
    )
    .unwrap();
    let certificate = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits: vec![vote],
        },
        header,
        commitment,
    )
    .unwrap();
    let mut candidate = BftSignerStateV3::from_handoff(&binding)
        .unwrap()
        .candidate();
    if active {
        let mut journal = candidate.journal.take().unwrap();
        journal.height_started_at_ms = Some(1);
        let prevote = BftVoteV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV3::Prevote,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            BftVoteValueV3::Block {
                block_id: id.clone(),
            },
            "validator-a".into(),
            &key(31),
        )
        .unwrap();
        journal.events = vec![
            BftSigningJournalEventV3::Proposal {
                proposal: Box::new(BftPublicProposalV3 {
                    header: certificate.certificate.proposal.clone(),
                    block: BftHeaderValueV3 {
                        header: certificate.header.clone(),
                        execution_commitment: certificate.execution_commitment.clone(),
                    },
                    valid_round_evidence: None,
                }),
                local: true,
                now_ms: 2,
            },
            BftSigningJournalEventV3::Vote {
                vote: Box::new(prevote),
                local: true,
                now_ms: 3,
            },
            BftSigningJournalEventV3::Vote {
                vote: Box::new(certificate.certificate.precommits[0].clone()),
                local: true,
                now_ms: 4,
            },
        ];
        journal.pending_commit = Some(certificate.clone());
        let next = oasis7_formal_protocol::bft::verify_formal_bft_header_certificate_v3(
            schedule,
            &certificate,
        )
        .unwrap();
        candidate.journal = Some(
            BftSigningJournalV3::for_schedule(
                next.next_height_schedule(),
                "validator-a",
                public(31),
            )
            .unwrap(),
        );
        candidate.completed_journals.push(Some(journal));
    } else {
        candidate.completed_journals.push(None);
    }
    candidate.history.push(certificate);
    candidate.applied_installs.push(BftDriverAppliedReceiptV3 {
        height: schedule.height(),
        header_id: id,
        driver_install_digest: hash(87),
    });
    let state = BftSignerStateV3::from_candidate(&binding, 0, candidate).unwrap();
    state.validate(&binding, &public(33)).unwrap();
    (
        binding,
        Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
            state,
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 100,
        })),
    )
}
fn fixture() -> (BftSignerBindingV3, Arc<Mutex<BftSignerBackendSnapshotV3>>) {
    fixture_kind(false)
}
fn request(
    binding: &BftSignerBindingV3,
    checkpoint: &BftSignerCheckpointV3,
) -> (
    BftSuccessorWindowInstallIntentBodyV1,
    BftSuccessorWindowArchiveManifestV1,
) {
    let state = &checkpoint.state;
    let verified = validate_state(binding, Some(&public(33)), state).unwrap();
    let cert = state.history.last().unwrap();
    let c = &cert.execution_commitment;
    let source = BftSuccessorWindowSourceV1::from_binding(binding).unwrap();
    let object = reference(&oasis7_cas::to_canonical_cbor(state).unwrap());
    let usage = BftSuccessorWindowArchiveUsageV1 {
        segments: 1,
        objects: 4,
        total_bytes: object.bytes + 1024,
    };
    let manifest = BftSuccessorWindowArchiveManifestV1 {
        schema_version: 1,
        source: source.clone(),
        kind: BftSuccessorWindowArchiveKindV1::Public,
        segment_sequence: 1,
        predecessor_root: None,
        first_height: c.height,
        last_height: c.height,
        entries: vec![BftSuccessorWindowArchiveEntryV1 {
            identity_digest: hash(89),
            object,
        }],
        usage,
    };
    let body = BftSuccessorWindowCheckpointBodyV1 {
        schema_version: 1,
        source: source.clone(),
        request_id: "successor-native-original".into(),
        sequence: 1,
        predecessor_checkpoint_digest: source.origin_digest().unwrap(),
        cut: BftSuccessorWindowCutV1 {
            height: c.height,
            header_id: cert.header.block_id().unwrap(),
            certificate_digest: blake3::hash(&cert.canonical_bytes().unwrap())
                .to_hex()
                .to_string(),
            execution_commitment_digest: c.digest().unwrap(),
            snapshot_ref: BftSuccessorWindowArchiveRefV1 {
                digest: c.execution_state_root.clone(),
                bytes: 1,
            },
            journal_ref: reference(&[1]),
            semantic_state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            business_result_root: c.result_root.clone(),
            driver_record_digest: hash(90),
            driver_install_digest: hash(87),
        },
        successor_schedule_cbor: oasis7_cas::to_canonical_cbor(&verified.schedule).unwrap(),
        successor_schedule_digest: verified.schedule.digest().unwrap(),
        current_journal_ref: state
            .journal
            .as_ref()
            .map(|j| reference(&oasis7_cas::to_canonical_cbor(j).unwrap())),
        current_journal_digest: state.journal.as_ref().map(|j| j.digest().unwrap()),
        public_archive_root: manifest.digest().unwrap(),
        execution_archive_root: hash(91),
        evidence_refs: vec![],
        usage,
    };
    (
        BftSuccessorWindowInstallIntentBodyV1 {
            schema_version: 1,
            checkpoint: body,
            expected_lease_generation: checkpoint.lease_generation,
            expected_revision: state.revision,
            expected_state_digest: state.state_digest.clone(),
            driver_evidence_digest: hash(92),
        },
        manifest,
    )
}
#[test]
fn successor_window_remote_original_cas_private_gate_and_cold_owner() {
    let (binding, snapshot) = fixture();
    let original = snapshot.lock().unwrap().state.clone();
    let archive = Arc::new(Archive::default());
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    let checkpoint = lease.checkpoint().unwrap();
    let (body, manifest) = request(&binding, &checkpoint);
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    let (prepared, cp) = lease
        .prepare_successor_window(body.clone(), manifest.clone())
        .unwrap();
    assert_eq!(prepared.prepared_revision, body.expected_revision + 2);
    assert_eq!(cp.state.history, original.history);
    assert!(
        cp.state
            .successor_window
            .as_ref()
            .unwrap()
            .minimum
            .is_none()
    );
    assert_eq!(
        lease.get_successor_window(&id, &digest).unwrap().0,
        Some(prepared.clone())
    );
    assert!(lease.commit_successor_window(&id, &digest).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    assert!(
        lease
            .advance(
                cp.state.revision,
                &cp.state.state_digest,
                cp.state.candidate()
            )
            .is_err()
    );
    let mut changed = body.clone();
    changed.driver_evidence_digest = hash(93);
    assert!(
        lease
            .prepare_successor_window(changed, manifest.clone())
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    drop(lease);
    server.join().unwrap();
    let reject = Arc::new(AtomicUsize::new(1));
    let calls = Arc::new(AtomicUsize::new(0));
    let verifier = Arc::new(DriverVerifier {
        expected: original.clone(),
        reject: reject.clone(),
        calls: calls.clone(),
    });
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), Some(verifier));
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(
        lease.get_successor_window(&id, &digest).unwrap().0,
        Some(prepared.clone())
    );
    assert!(lease.commit_successor_window(&id, &digest).is_err());
    assert!(
        snapshot
            .lock()
            .unwrap()
            .state
            .successor_window
            .as_ref()
            .unwrap()
            .minimum
            .is_none()
    );
    reject.store(0, Ordering::SeqCst);
    let (installed, cp) = lease.commit_successor_window(&id, &digest).unwrap();
    let receipt = installed.request().receipt.clone().unwrap();
    assert_eq!(
        receipt.body.committed_revision,
        prepared.prepared_revision + 1
    );
    assert_eq!(receipt.body.lease_generation, cp.lease_generation);
    assert!(cp.lease_generation > body.expected_lease_generation);
    assert_eq!(
        installed.request().status,
        BftSuccessorWindowStatusV1::InstalledPending
    );
    let mut evidence = BftSuccessorWindowAdoptedEvidenceV1 {
        checkpoint_digest: digest.clone(),
        driver_install_digest: body.checkpoint.cut.driver_install_digest.clone(),
        schedule_digest: body.checkpoint.successor_schedule_digest.clone(),
        journal_digest: None,
        readback_state_digest: cp.state.state_digest.clone(),
    };
    let mut bad = evidence.clone();
    bad.driver_install_digest = hash(94);
    assert!(
        lease
            .confirm_successor_window_adopted(&id, &digest, bad)
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    reject.store(1, Ordering::SeqCst);
    assert!(
        lease
            .confirm_successor_window_adopted(&id, &digest, evidence.clone())
            .is_err()
    );
    reject.store(0, Ordering::SeqCst);
    evidence.readback_state_digest = cp.state.state_digest.clone();
    let (adopted, after) = lease
        .confirm_successor_window_adopted(&id, &digest, evidence)
        .unwrap();
    assert_eq!(
        adopted.request().status,
        BftSuccessorWindowStatusV1::Adopted
    );
    assert_eq!(adopted.request().receipt.as_ref(), Some(&receipt));
    assert_eq!(after.state.history, original.history);
    assert!(calls.load(Ordering::SeqCst) >= 4);
    drop(lease);
    server.join().unwrap();
    let verifier = Arc::new(DriverVerifier {
        expected: original,
        reject,
        calls,
    });
    let (provider, server) = native_successor(&binding, &snapshot, archive, Some(verifier));
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(
        lease.get_successor_window(&id, &digest).unwrap().0,
        Some(adopted)
    );
    let (retry, current) = lease.commit_successor_window(&id, &digest).unwrap();
    assert_eq!(retry.request().receipt.as_ref(), Some(&receipt));
    assert!(current.lease_generation > receipt.body.lease_generation);
    drop(lease);
    server.join().unwrap();
}
#[test]
fn successor_window_reservation_cold_retry_restores_exact_original_not_reserved_state() {
    let (binding, snapshot) = fixture();
    let original = snapshot.lock().unwrap().state.clone();
    let archive = Arc::new(Archive::default());
    archive.fail_write.store(1, Ordering::SeqCst);
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    let (body, manifest) = request(&binding, &lease.checkpoint().unwrap());
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    assert!(
        lease
            .prepare_successor_window(body.clone(), manifest.clone())
            .is_err()
    );
    let reserved = snapshot.lock().unwrap().state.clone();
    assert_eq!(reserved.revision, body.expected_revision + 1);
    assert!(
        reserved
            .successor_window
            .as_ref()
            .unwrap()
            .reservation
            .is_some()
    );
    assert!(
        reserved
            .successor_window
            .as_ref()
            .unwrap()
            .requests
            .is_empty()
    );
    assert_eq!(
        reserved.successor_window.as_ref().unwrap().usage,
        body.checkpoint.usage
    );
    assert_eq!(reserved.history, original.history);
    drop(lease);
    server.join().unwrap();
    archive.fail_write.store(0, Ordering::SeqCst);
    let (provider, server) = native_successor(&binding, &snapshot, archive.clone(), None);
    let mut lease = provider.acquire(&binding).unwrap();
    let (record, cp) = lease.get_successor_window(&id, &digest).unwrap();
    let record = record.unwrap();
    assert_eq!(record.request().intent.body, body);
    assert_eq!(
        archive
            .load_public_state(&record.original_public_state)
            .unwrap(),
        original
    );
    assert!(
        cp.state
            .successor_window
            .as_ref()
            .unwrap()
            .reservation
            .is_none()
    );
    assert_eq!(cp.state.revision, reserved.revision + 1);
    assert_eq!(
        lease.prepare_successor_window(body, manifest).unwrap().0,
        record
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn successor_window_unknown_committed_cas_poison_then_new_owner_queries_original() {
    let (binding, snapshot) = fixture();
    let original = snapshot.lock().unwrap().state.clone();
    let archive = Arc::new(Archive::default());
    let reject = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let verifier = Arc::new(DriverVerifier {
        expected: original,
        reject,
        calls,
    });
    let fail_at = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_successor_fault(
        &binding,
        &snapshot,
        archive.clone(),
        Some(verifier.clone()),
        fail_at.clone(),
    );
    let mut lease = provider.acquire(&binding).unwrap();
    let (body, manifest) = request(&binding, &lease.checkpoint().unwrap());
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    let (prepared, _) = lease.prepare_successor_window(body, manifest).unwrap();
    fail_at.store((prepared.prepared_revision + 1) as usize, Ordering::SeqCst);
    assert!(lease.commit_successor_window(&id, &digest).is_err());
    let installed = snapshot.lock().unwrap().state.clone();
    let record = installed
        .successor_window
        .as_ref()
        .unwrap()
        .request(&id)
        .unwrap()
        .clone();
    assert_eq!(
        record.request().status,
        BftSuccessorWindowStatusV1::InstalledPending
    );
    assert!(
        lease.get_successor_window(&id, &digest).is_err(),
        "unknown outcome must poison this service"
    );
    assert_eq!(snapshot.lock().unwrap().state, installed);
    drop(lease);
    server.join().unwrap();
    // Explicit memory-backend fencing evidence for crashed owner, not recovery
    // from an untrusted public archive. Production backend must supply this.
    {
        let mut durable = snapshot.lock().unwrap();
        durable.lease_generation += 1;
        durable.lease_token_digest.clear();
    }
    let (provider, server) = native_successor(&binding, &snapshot, archive, Some(verifier));
    let mut lease = provider.acquire(&binding).unwrap();
    let (actual, cp) = lease.get_successor_window(&id, &digest).unwrap();
    assert_eq!(actual, Some(record.clone()));
    assert_eq!(cp.state, installed);
    assert_eq!(
        lease.commit_successor_window(&id, &digest).unwrap().0,
        record
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn successor_window_pending_transaction_never_calls_new_proof_key_but_retrieves_original() {
    let (binding, snapshot) = fixture_kind(true);
    let archive = Arc::new(Archive::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_successor_counted(
        &binding,
        &snapshot,
        archive,
        None,
        Arc::new(AtomicUsize::new(0)),
        calls.clone(),
    );
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let cp = lease
        .enable_membership(cp.state.revision, &cp.state.state_digest)
        .unwrap();
    let schedule = validate_state(&binding, Some(&public(33)), &cp.state)
        .unwrap()
        .schedule;
    let mut proof_body = body(&binding);
    proof_body.authorization.parent_position = schedule.parent_tick();
    proof_body.authorization.parent_record_id = schedule.parent_commit_ref().into();
    proof_body.authorization.parent_state_root = schedule.parent_state_root().into();
    proof_body.authorization.valid_from_position = schedule.height();
    proof_body.authorization.valid_through_position = schedule.height();
    proof_body.decision_height = schedule.height();
    proof_body.activation_height = schedule.height() + 2;
    let (original, cp) = lease
        .sign_finality_rotation_proof(
            cp.state.revision,
            &cp.state.state_digest,
            &proof_body,
            FinalityKeyRotationRoleV1::OldKeyAuthorization,
        )
        .unwrap();
    let cp = lease.start_height_timer(cp.state.revision).unwrap();
    let journal = cp.state.journal.as_ref().unwrap();
    let mut commitment = cp
        .state
        .history
        .last()
        .unwrap()
        .execution_commitment
        .clone();
    commitment.height = schedule.height();
    commitment.parent_commit_ref = schedule.parent_commit_ref().into();
    commitment.parent_tick = schedule.parent_tick();
    commitment.parent_semantic_state_root = schedule.parent_state_root().into();
    commitment.manifest_hash = schedule.parent_manifest_hash().into();
    let value = BftHeaderValueV3 {
        header: BftBlockHeaderV3 {
            protocol_version: 3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
        },
        execution_commitment: commitment,
    };
    let machine = oasis7_consensus::tendermint_v3::replay_bft_signing_journal_v3(
        journal,
        schedule.active_set(),
        &schedule,
        binding.timeouts,
    )
    .unwrap();
    let proposal_intent = machine.proposal_intent(value, "validator-a").unwrap();
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_proposal_intent = Some(proposal_intent.clone());
    let reserved = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (proposal, cp) = lease
        .sign_proposal(reserved.state.revision, &proposal_intent)
        .unwrap();
    let (_, effects) = oasis7_consensus::tendermint_v3::replay_bft_signing_journal_with_effects_v3(
        cp.state.journal.as_ref().unwrap(),
        schedule.active_set(),
        &schedule,
        binding.timeouts,
    )
    .unwrap();
    let vote_intent = effects
        .into_iter()
        .find_map(|e| match e {
            oasis7_consensus::tendermint_v3::BftConsensusActionV3::SignVote(i) => Some(i),
            _ => None,
        })
        .unwrap();
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_vote_intent = Some(vote_intent.clone());
    let reserved = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (vote, cp) = lease
        .sign_vote(reserved.state.revision, &vote_intent)
        .unwrap();
    let (_, effects) = oasis7_consensus::tendermint_v3::replay_bft_signing_journal_with_effects_v3(
        cp.state.journal.as_ref().unwrap(),
        schedule.active_set(),
        &schedule,
        binding.timeouts,
    )
    .unwrap();
    let fresh_vote = effects
        .into_iter()
        .find_map(|e| match e {
            oasis7_consensus::tendermint_v3::BftConsensusActionV3::SignVote(i) => Some(i),
            _ => None,
        })
        .unwrap();
    let origin = cp.state.journal.as_ref().unwrap().height_started_at_ms();
    let (body, manifest) = request(&binding, &cp);
    let (_, cp) = lease.prepare_successor_window(body, manifest).unwrap();
    let before = calls.load(Ordering::SeqCst);
    assert_eq!(
        lease
            .sign_proposal(cp.state.revision, &proposal_intent)
            .unwrap()
            .0,
        proposal
    );
    assert_eq!(
        lease.sign_vote(cp.state.revision, &vote_intent).unwrap().0,
        vote
    );
    assert!(lease.sign_vote(cp.state.revision, &fresh_vote).is_err());
    assert_eq!(
        lease.start_height_timer(cp.state.revision).unwrap().state,
        cp.state
    );
    assert_eq!(
        cp.state.journal.as_ref().unwrap().height_started_at_ms(),
        origin
    );

    assert_eq!(
        lease
            .sign_finality_rotation_proof(
                cp.state.revision,
                &cp.state.state_digest,
                &proof_body,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .unwrap()
            .0,
        original
    );
    assert_eq!(calls.load(Ordering::SeqCst), before);
    assert!(
        lease
            .sign_finality_rotation_proof(
                cp.state.revision,
                &cp.state.state_digest,
                &proof_body,
                FinalityKeyRotationRoleV1::NewKeyPossession
            )
            .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "reject before invoking a new proof key handle"
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn successor_window_public_manifest_chain_is_checked_before_any_reservation() {
    let (binding, snapshot) = fixture();
    let (provider, server) =
        native_successor(&binding, &snapshot, Arc::new(Archive::default()), None);
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let (body, manifest) = request(&binding, &cp);
    for change in 0..2 {
        let mut bad_manifest = manifest.clone();
        let mut bad_body = body.clone();
        if change == 0 {
            bad_manifest.first_height += 1;
            bad_manifest.last_height += 1;
            bad_body.checkpoint.cut.height += 1;
        } else {
            bad_manifest.segment_sequence = 2;
            bad_manifest.predecessor_root = Some(hash(95));
            bad_manifest.usage.segments = 2;
            bad_body.checkpoint.usage.segments = 2;
        }
        bad_body.checkpoint.public_archive_root = bad_manifest.digest().unwrap();
        assert!(
            lease
                .prepare_successor_window(bad_body, bad_manifest)
                .is_err()
        );
        assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    }
    drop(lease);
    server.join().unwrap();
}

#[test]
fn successor_window_pending_transaction_cannot_start_an_unstarted_active_timer() {
    let (binding, snapshot) = fixture_kind(true);
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_successor_counted(
        &binding,
        &snapshot,
        Arc::new(Archive::default()),
        None,
        Arc::new(AtomicUsize::new(0)),
        calls.clone(),
    );
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    assert_eq!(
        cp.state.journal.as_ref().unwrap().height_started_at_ms(),
        None
    );
    let (body, manifest) = request(&binding, &cp);
    let (_, cp) = lease.prepare_successor_window(body, manifest).unwrap();
    assert!(lease.start_height_timer(cp.state.revision).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(lease);
    server.join().unwrap();
}
