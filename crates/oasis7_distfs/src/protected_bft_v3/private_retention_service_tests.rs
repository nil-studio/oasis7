//! Actual authenticated TCP/atomic backend gates with an explicit material-verifier double.
//! These tests do not attest World material admission or physical custody.
use super::*;
use crate::protected_bft_v3::private_retention::*;
#[path = "private_retention_metadata_chunk_tests.rs"]
mod metadata_chunks;
#[path = "private_retention_metadata_fault_tests.rs"]
mod metadata_faults;
#[path = "private_retention_readback_faults.rs"]
mod readback_faults;
#[path = "private_retention_response_budget_tests.rs"]
mod response_budget;

struct InputReaderDouble(Arc<BftPrivateRetentionAdmissionInputV1>, Arc<AtomicUsize>);
struct InputHold(Arc<AtomicUsize>);
impl Drop for InputHold {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl BftPrivateRetentionAdmissionInputReaderV1 for InputReaderDouble {
    fn read_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        _: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3> {
        reference.validate_input(&self.0)?;
        self.1.fetch_add(1, Ordering::SeqCst);
        Ok(BftPrivateRetentionAdmissionInputReadV1::new(
            self.0.clone(),
            Box::new(InputHold(self.1.clone())),
        ))
    }
}

struct MaterialDouble {
    expected: BftPrivateRetentionEntryV1,
    ready: Arc<AtomicUsize>,
    creation_identity: String,
    input: Arc<BftPrivateRetentionAdmissionInputV1>,
}
impl BftPrivateRetentionMaterialVerifierV1 for MaterialDouble {
    fn verify_source_creation(
        &self,
        _: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        creation: &ValidatedBftPrivateRetentionCreationV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let marker = creation.marker();
        if marker.creation_identity != self.creation_identity
            || marker.ledger_origin_digest != self.expected.key.ledger_origin_digest
            || marker.initial_state_digest != state.state_digest
        {
            return Err(BftSignerErrorV3(
                "verifier double source creation identity mismatch".into(),
            ));
        }
        Ok(())
    }
    fn verify_reservation(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        entry: &BftPrivateRetentionEntryV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if entry.key != self.expected.key || entry.fact != self.expected.fact {
            return Err(BftSignerErrorV3("verifier double original mismatch".into()));
        }
        Ok(())
    }
    fn verify_reservation_input(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if input != self.input.as_ref() {
            return Err(BftSignerErrorV3(
                "input verifier double original mismatch".into(),
            ));
        }
        self.verify_reservation(
            binding,
            state,
            &BftPrivateRetentionEntryV1 {
                key: input.key.clone(),
                fact: input.fact()?,
                lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
            },
            deadline,
        )
    }
    fn verify_ready_input(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &BftPrivateRetentionAdmissionInputV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_reservation_input(binding, state, input, deadline)?;
        self.verify_ready(binding, state, entry, deadline)
    }
    // This remains an explicit transport/backend test double. Production
    // ActualSource implementations independently reproduce original inventory.
    fn verify_reservation_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        context: &BftPrivateRetentionVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_reservation_input(binding, &context.signer().state, input, deadline)
    }
    fn verify_ready_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &BftPrivateRetentionAdmissionInputV1,
        context: &BftPrivateRetentionVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_ready_input(binding, &context.signer().state, entry, input, deadline)
    }
    fn verify_ready(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        entry: &BftPrivateRetentionEntryV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_reservation(binding, state, entry, deadline)?;
        if self.ready.load(Ordering::SeqCst) == 0 {
            return Err(BftSignerErrorV3(
                "verifier double material not published".into(),
            ));
        }
        Ok(())
    }
}

#[test]
fn private_retention_tcp_original_reserved_ready_and_external_qc_gate() {
    run_retention_case(0, false);
}

#[test]
fn private_retention_tcp_malformed_original_readback_poisons_after_fault_removed() {
    run_retention_case(1, false);
}

#[test]
fn private_retention_tcp_foreign_original_readback_poisons_after_fault_removed() {
    run_retention_case(2, false);
}

#[test]
fn private_retention_tcp_uncovered_unsigned_some_cannot_provision_or_reserve() {
    run_retention_case(0, true);
}

fn run_retention_case(fault_mode: usize, legacy: bool) {
    run_retention_case_with_metadata_fault(fault_mode, legacy, None);
}

fn run_retention_case_with_metadata_fault(
    fault_mode: usize,
    legacy: bool,
    metadata_fault: Option<usize>,
) {
    let (binding, old) = fixture_kind(false);
    let certificate = old.lock().unwrap().state.history[0].clone();
    let initial = BftSignerStateV3::from_handoff(&binding).unwrap();
    let seed = BftPrivateRetentionFreshSeedV1::create(&binding).unwrap();
    let recovery = seed.recovery_handle();
    let creation_identity = seed.creation_identity().to_string();
    let namespace = seed.bind_origin(hash(91)).unwrap();
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: BftPrivateRetentionSourceV1 {
            binding_digest: binding.digest().unwrap(),
            handoff_digest: binding.handoff_record_id.clone(),
            material_source_digest: binding.material_identity.clone(),
        },
        creation_marker_digest: namespace.marker().digest().unwrap(),
        coverage_start_height: binding.initial_schedule().height(),
        allow_authenticated_successor_origins: true,
    };
    let snapshot = BftSignerBackendSnapshotV3 {
        state: initial,
        successor_archive_transfer: Some(BftSuccessorArchiveTransferLedgerV1 {
            schema_version: 1,
            reserved_response_bytes: 0,
        }),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 1000,
    };
    let another_namespace = BftPrivateRetentionFreshSeedV1::create(&binding)
        .unwrap()
        .bind_origin(hash(91))
        .unwrap();
    assert!(
        BftPrivateRetentionMemoryBackendV1::create_fresh(
            another_namespace,
            role.clone(),
            snapshot.successor_archive_transfer,
            snapshot.protected_time_floor_ms
        )
        .is_err(),
        "a second factory cannot reuse the original external creation capability"
    );
    let backend = if legacy {
        BftPrivateRetentionMemoryBackendV1::restore_legacy(&binding, snapshot, role.clone())
            .unwrap()
    } else {
        BftPrivateRetentionMemoryBackendV1::create_fresh(
            namespace,
            role.clone(),
            snapshot.successor_archive_transfer,
            snapshot.protected_time_floor_ms,
        )
        .unwrap()
    };
    let mut retained = backend.clone();
    let fault = Arc::new(AtomicUsize::new(0));
    let mut entry = BftPrivateRetentionEntryV1 {
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: hash(91),
            height: certificate.execution_commitment.height,
            header_id: certificate.header.block_id().unwrap(),
        },
        fact: BftPrivateRetentionFactV1::Some {
            original_data_ref: hash(92),
            metadata_ref: hash(93),
            admission_digest: hash(94),
            input_ref: None,
        },
        lifecycle: BftPrivateRetentionLifecycleV1::ReservedPending,
    };
    let input =
        crate::protected_bft_v3::private_retention_tests::input_for(&role.source, &entry.key);
    entry.fact = input.fact().unwrap();
    let ready = Arc::new(AtomicUsize::new(0));
    let input_hold = Arc::new(AtomicUsize::new(0));
    let support = BftPrivateRetentionSupportV1::new(
        role.clone(),
        Arc::new(MaterialDouble {
            expected: entry.clone(),
            ready: ready.clone(),
            creation_identity,
            input: input.clone(),
        }),
        Duration::from_secs(10),
    )
    .unwrap()
    .with_admission_input_reader(Arc::new(InputReaderDouble(
        input.clone(),
        input_hold.clone(),
    )));
    let metadata_mode = Arc::new(AtomicUsize::new(0));
    let service = BftSignerServiceV3::open_with_private_retention(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(Handle {
                key: key(31),
                disabled: None,
            }),
            Box::new(Handle {
                key: key(32),
                disabled: None,
            }),
        ],
        Box::new(metadata_faults::MetadataFault::new(
            Box::new(readback_faults::ReadbackFault {
                inner: backend,
                mode: fault.clone(),
                input_hold: input_hold.clone(),
                role: role.clone(),
            }),
            metadata_mode.clone(),
        )),
        None,
        support,
    )
    .unwrap();
    let service = Arc::new(service);
    let serving = service.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        serving.serve_tcp_connection(&mut stream)
    });
    let response_fault = Arc::new(AtomicUsize::new(0));
    let (address, proxy) = if fault_mode >= 3 {
        let (address, proxy) =
            response_budget::proxy(address, binding.clone(), response_fault.clone());
        (address, Some(proxy))
    } else {
        (address, None)
    };
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
    .unwrap()
    .with_private_retention_attestor(role.clone())
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    let deadline = || Instant::now() + Duration::from_secs(10);
    let initial = lease.checkpoint().unwrap();
    assert!(initial.state.private_retention.is_none());
    let mut unsigned_header = certificate.certificate.proposal.clone();
    unsigned_header.signature_hex.clear();
    let proposal_intent = oasis7_consensus::tendermint_v3::BftProposalIntentV3 {
        header: unsigned_header,
        block: BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        },
        valid_round_evidence: certificate.certificate.valid_round_evidence.clone(),
    };
    let vote = &certificate.certificate.precommits[0];
    let vote_intent = oasis7_consensus::tendermint_v3::BftVoteIntentV3 {
        height: vote.height,
        round: vote.round,
        phase: vote.phase,
        value: vote.value.clone(),
        validator_id: vote.validator_id.clone(),
        schedule_policy_hash: vote.schedule_policy_hash.clone(),
    };
    for error in [
        lease
            .sign_proposal(initial.state.revision, &proposal_intent)
            .unwrap_err(),
        lease
            .sign_vote(initial.state.revision, &vote_intent)
            .unwrap_err(),
    ] {
        assert!(
            error
                .to_string()
                .contains("requires explicit initial provision"),
            "configured mode cannot bypass Ready through uncovered None: {error}"
        );
    }
    assert!(
        lease
            .append_committed_certificate(
                initial.state.revision,
                &initial.state.state_digest,
                certificate.clone()
            )
            .is_err(),
        "configured mode must never interpret uncovered None as fresh legacy permission"
    );
    assert_eq!(lease.checkpoint().unwrap().state, initial.state);
    if legacy {
        // The private material double already retains the unsigned Some entry;
        // its public State is byte-identical to the fresh H State. It has no
        // original namespace marker: neither entry refs nor an empty journal
        // are a proof that this source started covered.
        assert!(matches!(entry.fact, BftPrivateRetentionFactV1::Some { .. }));
        for error in [
            lease.provision_private_retention(deadline()).unwrap_err(),
            lease
                .reserve_private_retention_input(input.descriptor().unwrap(), deadline())
                .unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("legacy source is uncovered"),
                "both first-head paths must reject absent original creation, not merely unsupported material: {error}"
            );
        }
        assert_eq!(lease.checkpoint().unwrap().state, initial.state);
        drop(lease);
        if let Some(proxy) = proxy {
            proxy.join().unwrap();
        }
        server.join().unwrap().unwrap();
        let state = retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap();
        assert_eq!(state.state, initial.state);
        assert_eq!(
            state
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes,
            0
        );
        assert!(
            retained
                .load_private_retention_current(&binding.digest().unwrap())
                .unwrap()
                .retention
                .is_none()
        );
        return;
    }
    let provisioned = lease.provision_private_retention(deadline()).unwrap();
    assert!(provisioned.map.entries.is_empty());
    let cp = lease.checkpoint().unwrap();
    assert!(
        lease
            .append_committed_certificate(
                cp.state.revision,
                &cp.state.state_digest,
                certificate.clone()
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    let reserved = lease
        .reserve_private_retention_input(input.descriptor().unwrap(), deadline())
        .unwrap();
    assert_eq!(reserved.map.entries, vec![entry.clone()]);
    assert_eq!(
        input_hold.load(Ordering::SeqCst),
        0,
        "receiving permit released only after original CAS/readback response"
    );
    let cp = lease.checkpoint().unwrap();
    assert!(
        lease
            .append_committed_certificate(
                cp.state.revision,
                &cp.state.state_digest,
                certificate.clone()
            )
            .is_err()
    );
    assert!(
        lease
            .ready_private_retention(entry.key.clone(), deadline())
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    ready.store(1, Ordering::SeqCst);
    let published = lease
        .ready_private_retention(entry.key.clone(), deadline())
        .unwrap();
    assert_eq!(
        published.map.entries[0].lifecycle,
        BftPrivateRetentionLifecycleV1::Ready
    );
    let same = lease
        .ready_private_retention(entry.key, deadline())
        .unwrap();
    assert_eq!(
        same, published,
        "retry returns original receipt, never re-signs"
    );
    let cp = lease.checkpoint().unwrap();
    let appended = lease
        .append_committed_certificate(
            cp.state.revision,
            &cp.state.state_digest,
            certificate.clone(),
        )
        .unwrap();
    assert_eq!(appended.state.history, vec![certificate]);
    assert_eq!(
        appended.state.private_retention.as_ref(),
        Some(&published.receipt.body.head)
    );
    if fault_mode == 0 {
        let current = retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap();
        let originals = retained
            .read_private_retention_sealed_current(
                current.lease_generation,
                &current.lease_token_digest,
            )
            .unwrap();
        let mut before_counter = current
            .successor_archive_transfer
            .unwrap()
            .reserved_response_bytes;
        for (digest, original) in &originals.history {
            let proof = lease
                .read_private_retention_operation_proof(digest.clone(), deadline())
                .unwrap();
            assert_eq!(proof.state, original.signer.state);
            assert_eq!(proof.ledger, original.retention);
            let after = retained
                .load_provisioned(&binding.digest().unwrap())
                .unwrap();
            assert_eq!(
                after.state, current.state,
                "historical metadata must not become the live checkpoint"
            );
            let counter = after
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes;
            assert!(
                counter > before_counter,
                "each metadata response must reserve transfer"
            );
            before_counter = counter;
            let same = lease
                .read_private_retention_operation_proof(digest.clone(), deadline())
                .unwrap();
            assert_eq!(
                same.canonical_bytes().unwrap(),
                proof.canonical_bytes().unwrap(),
                "repeat reads preserve exact original receipt/signature bytes"
            );
            let repeated = retained
                .load_provisioned(&binding.digest().unwrap())
                .unwrap()
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes;
            assert!(
                repeated > before_counter,
                "replay does not get a free metadata response"
            );
            before_counter = repeated;
        }
        let error = lease
            .read_private_retention_operation_proof(hash(243), deadline())
            .unwrap_err();
        assert!(
            error.to_string().contains("unavailable"),
            "missing original must fail explicitly: {error}"
        );
        assert!(
            lease
                .read_private_retention_seal_proof(hash(244), deadline())
                .is_err()
        );
        let after = retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap();
        assert_eq!(after.state, current.state);
        assert_eq!(
            after
                .successor_archive_transfer
                .unwrap()
                .reserved_response_bytes,
            before_counter,
            "no payload release means no successful proof charge"
        );
        assert_eq!(lease.checkpoint().unwrap().state, current.state);
    }
    if let Some(mode) = metadata_fault {
        metadata_faults::assert_guard_revocation(
            mode,
            &binding,
            &mut retained,
            lease.as_mut(),
            &service,
            metadata_mode.as_ref(),
            &deadline,
        );
    }
    if fault_mode >= 3 {
        let before = lease.checkpoint().unwrap();
        response_fault.store(fault_mode, Ordering::SeqCst);
        let observed = lease.observe_private_retention(deadline());
        if fault_mode == 5 {
            assert_eq!(
                observed.unwrap().reserved_response_bytes(),
                BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
                    - before
                        .state
                        .private_retention
                        .as_ref()
                        .unwrap()
                        .retained_material_bytes
                        .unwrap()
            );
        } else {
            let error = observed
                .err()
                .expect("valid provider signature cannot authorize over-cap transfer");
            assert!(
                error.to_string().contains("capacity"),
                "client must reject budget shape: {error}"
            );
            response_fault.store(0, Ordering::SeqCst);
            assert_eq!(lease.checkpoint().unwrap().state, before.state);
            assert!(
                lease.observe_private_retention(deadline()).is_ok(),
                "invalid response must not advance cached counter"
            );
        }
        response_fault.store(0, Ordering::SeqCst);
    }
    if (1..=2).contains(&fault_mode) {
        fault.store(fault_mode, Ordering::SeqCst);
        assert!(lease.get_private_retention(deadline()).is_err());
        fault.store(0, Ordering::SeqCst);
        let error = lease.checkpoint().unwrap_err();
        assert!(
            error.to_string().contains("fail-closed"),
            "malformed readback must remain poisoned after fault removal: {error}"
        );
    }
    drop(lease);
    if let Some(proxy) = proxy {
        proxy.join().unwrap();
    }
    let outcome = server.join().unwrap();
    if !(1..=2).contains(&fault_mode) {
        outcome.unwrap();
    } else {
        assert_eq!(
            outcome,
            Err(BftSignerErrorV3("BFT V3 provider is poisoned".into())),
            "only the expected poisoned-session teardown may fail"
        );
    }
    let cold = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    assert!(
        recovery
            .recover_seed()
            .bind_source(&binding, hash(95))
            .is_err(),
        "the original preparation cannot choose another Origin"
    );
    let namespace = recovery
        .recover_seed()
        .bind_source(&binding, hash(91))
        .unwrap();
    let mut same_namespace = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        role,
        Some(BftSuccessorArchiveTransferLedgerV1 {
            schema_version: 1,
            reserved_response_bytes: 0,
        }),
        1000,
    )
    .unwrap();
    assert_eq!(
        same_namespace
            .load_provisioned(&binding.digest().unwrap())
            .unwrap(),
        cold,
        "recovering a factory result must preserve original State, lease, receipt and charged counter"
    );
    assert!(
        cold.successor_archive_transfer
            .unwrap()
            .reserved_response_bytes
            > 0
    );
    assert_eq!(cold.state, appended.state);
    assert!(
        BftSignerServiceV3::open(
            binding,
            Box::new(Handle {
                key: key(33),
                disabled: None
            }),
            vec![
                Box::new(Handle {
                    key: key(31),
                    disabled: None
                }),
                Box::new(Handle {
                    key: key(32),
                    disabled: None
                })
            ],
            Box::new(retained)
        )
        .is_err(),
        "ordinary service must not silently accept enabled head"
    );
}
