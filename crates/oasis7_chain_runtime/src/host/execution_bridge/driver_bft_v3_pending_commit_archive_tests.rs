//! Actual pending-QC recovery and the original layer capacity boundary. Legacy None publication is explicit;
//! this does not prove protected Some completeness or automatic Node rolling.
use super::*;
use oasis7_consensus::tendermint_v3::{BftHeaderValueV3, verify_commit_certificate_token_v3};
use oasis7_proto::distributed_bft::v3::{
    BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
};

#[test]
fn wire3_original_layer_reports_capacity_before_64_without_mutating_authenticated_prefix() {
    original_pending_case(true);
}

#[test]
fn wire3_early_cut_original_pending_commit_external_qc_without_proposal_event() {
    original_pending_case(false);
}

fn original_pending_case(capacity_boundary: bool) {
    // Use the test harness default stack and existing 64-height/RPC bounds.
    let h = Box::new(weighted_handoff());
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = Arc::new(Mutex::new(
        FormalBftHeaderNodeDriverV3::open_from_handoff_with_peer_network(
            &root.path().join("ledger"),
            store.clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            h.reader.clone(),
        )
        .unwrap(),
    ));
    let binding = signer_binding(&actual);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let private_root = Scratch::new();
    let archive = Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(private_root.path())).unwrap());
    let public_root = Scratch::new();
    let public = Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(public_root.path())).unwrap());
    let verifier = Arc::new(
        ActualBftSuccessorWindowDriverVerifierV1::new(
            h.anchor.clone(),
            h.target.clone(),
            archive.clone(),
        )
        .unwrap()
        .with_installation_root(root.path().join("ledger"))
        .unwrap()
        .with_active_private_store(store.clone())
        .unwrap(),
    );
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        successor_archive_transfer: Some(
            oasis7_distfs::protected_bft_v3::BftSuccessorArchiveTransferLedgerV1::provisioned(),
        ),
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let consensus_signatures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let service = Arc::new(
        BftSignerServiceV3::open_with_successor_window(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(ReadbackOnlyKey(consensus_signatures.clone()))],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshot.clone(),
            }),
            BftSuccessorWindowSupportV1::new(public, Duration::from_secs(120))
                .unwrap()
                .with_active_window_rebase()
                .with_driver_verifier(verifier.clone()),
        )
        .unwrap(),
    );
    let (address, server) = tcp(service);
    let server = ProviderJoin(Some(server));
    let pin = hex::encode(fixtures::key(116).verifying_key().to_bytes());
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(120),
        binding.clone(),
        fixtures::key(117),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: pin.clone(),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    let mut schedule = binding.initial_schedule().clone();
    // A single layer cannot promise 64 heights: the unchanged 4096-node
    // resource bound may require an earlier roll. The previous full64 fixture
    // failed before pending recovery at height 16 with genuine NeedsRoll
    // (4111 nodes); that failure is not a pending-recovery failure.
    let cut_count = if capacity_boundary { 64 } else { 1 };
    for ordinal in 0..cut_count {
        if capacity_boundary {
            let build = NodeBftBuildContextV3 {
                execution: NodeBftExecutionContextV3::from_schedule(&schedule),
                actions: Vec::new(),
            };
            let before = lease.checkpoint().unwrap();
            let before_schedule = lease.current_verified_schedule().unwrap();
            let admission = actual
                .lock()
                .unwrap()
                .preflight_bft_v3_capacity(&build)
                .unwrap();
            let after = lease.checkpoint().unwrap();
            assert_eq!(after.state, before.state);
            assert_eq!(after.lease_generation, before.lease_generation);
            assert_eq!(after.provider_public_key, before.provider_public_key);
            assert_eq!(lease.current_verified_schedule().unwrap(), before_schedule);
            match admission {
                oasis7_node::NodeBftCapacityAdmissionV3::Fits => {}
                oasis7_node::NodeBftCapacityAdmissionV3::NeedsRoll => {
                    assert!(ordinal > 0 && ordinal < 64);
                    assert_eq!(before.state.history.len(), ordinal);
                    assert!(before.state.pending_driver_apply.is_none());
                    assert!(
                        before
                            .state
                            .journal
                            .as_ref()
                            .is_none_or(|journal| journal.pending_commit.is_none())
                    );
                    let mut driver = actual.lock().unwrap();
                    assert_eq!(
                        driver.preflight_bft_v3_capacity(&build).unwrap(),
                        oasis7_node::NodeBftCapacityAdmissionV3::NeedsRoll
                    );
                    assert!(
                        driver
                            .pending_bft_v3_private_candidate(&build.execution)
                            .unwrap()
                            .is_none()
                    );
                    drop(driver);
                    let after = lease.checkpoint().unwrap();
                    assert_eq!(after.state, before.state);
                    assert_eq!(after.lease_generation, before.lease_generation);
                    assert_eq!(after.provider_public_key, before.provider_public_key);
                    assert_eq!(lease.current_verified_schedule().unwrap(), before_schedule);
                    assert_eq!(
                        consensus_signatures.load(std::sync::atomic::Ordering::SeqCst),
                        0
                    );
                    // This proves pure admission only. Do not create an
                    // over-capacity pending package or claim automatic rolling.
                    drop(lease);
                    server.join().unwrap().unwrap();
                    return;
                }
                blocked => panic!("valid original prefix unexpectedly blocked: {blocked:?}"),
            }
        }
        eprintln!(
            "pending fixture prefix_build ordinal={} height={} parent_tick={}",
            ordinal + 1,
            schedule.height(),
            schedule.parent_tick()
        );
        schedule = append_original_empty(&actual, lease.as_mut(), &schedule);
    }
    assert!(
        !capacity_boundary,
        "expected the real private material bound before a single-layer 64-height prefix"
    );
    let context = NodeBftExecutionContextV3::from_schedule(&schedule);
    let package = actual
        .lock()
        .unwrap()
        .preview_actual_package_for_test(NodeBftBuildContextV3 {
            execution: context.clone(),
            actions: vec![
                oasis7_node::NodeConsensusAction::from_payload(
                    67001,
                    validator_id(0),
                    payload("external-original-pending-qc"),
                )
                .unwrap(),
            ],
        })
        .unwrap();
    actual
        .lock()
        .unwrap()
        .retain_received_bft_v3_private_candidate(&context, &package)
        .unwrap();
    assert!(
        actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap()
            .is_none(),
        "received-only original is not a RetainedLocal fallback"
    );
    let original_private = package.candidate;
    let certificate = external_certificate(&schedule, &original_private.public_value, true);
    assert!(
        certificate
            .certificate
            .precommits
            .iter()
            .all(|vote| vote.validator_id != binding.local_validator_id)
    );
    assert_ne!(
        certificate.certificate.proposal.proposer_id,
        binding.local_validator_id
    );
    let original_qc = certificate.canonical_bytes().unwrap();
    let cp = lease.checkpoint().unwrap();
    let cp = lease.start_height_timer(cp.state.revision).unwrap();
    let mut candidate = cp.state.candidate();
    assert!(candidate.journal.as_ref().unwrap().events.is_empty());
    candidate.journal.as_mut().unwrap().pending_commit = Some(certificate.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let original = Box::new(cp);
    assert_eq!(original.state.history.len(), cut_count);
    assert_eq!(original.state.applied_installs.len(), cut_count);
    assert!(original.state.pending_driver_apply.is_none());
    let original_journal = original.state.journal.clone().unwrap();
    assert!(
        original_journal.events.is_empty(),
        "no Proposal event may accidentally supply this package role"
    );
    assert_eq!(original_journal.pending_commit.as_ref(), Some(&certificate));
    let cut = original.state.history.last().unwrap();
    let source = BftSuccessorWindowSourceV1::from_binding(&binding).unwrap();
    let exported = actual
        .lock()
        .unwrap()
        .export_successor_private_closure(SuccessorPrivateExportRequestV1 {
            archive: &archive,
            source: &source,
            public_state: &original.state,
            previous_usage: BftSuccessorWindowArchiveUsageV1::default(),
            height: cut.execution_commitment.height,
            header_id: &cut.header.block_id().unwrap(),
            sequence: 1,
            predecessor: None,
            deadline: deadline(),
        })
        .unwrap();
    let mut public_manifest = BftSuccessorWindowArchiveManifestV1 {
        public_index: None,
        schema_version: 1,
        source: source.clone(),
        kind: BftSuccessorWindowArchiveKindV1::Public,
        segment_sequence: 1,
        predecessor_root: None,
        first_height: binding.initial_schedule().height(),
        last_height: exported.cut().height,
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
    let body = BftSuccessorWindowCheckpointBodyV1 {
        schema_version: 1,
        source: source.clone(),
        request_id: "original-external-pending-commit".into(),
        sequence: 1,
        predecessor_checkpoint_digest: source.origin_digest().unwrap(),
        cut: exported.cut().clone(),
        successor_schedule_cbor: oasis7_cas::to_canonical_cbor(&schedule).unwrap(),
        successor_schedule_digest: schedule.digest().unwrap(),
        current_journal_ref: Some(
            archive
                .persist_object(
                    &oasis7_cas::to_canonical_cbor(&original_journal).unwrap(),
                    512 * 1024 * 1024,
                )
                .unwrap(),
        ),
        current_journal_digest: Some(original_journal.digest().unwrap()),
        public_archive_root: public_manifest.digest().unwrap(),
        execution_archive_root: exported.manifest().digest().unwrap(),
        execution_manifest_ref: exported.execution_manifest_ref().clone(),
        evidence_refs: vec![exported.metadata_ref().clone()],
        usage: exported.manifest().usage,
    };
    let detached = ActualBftSuccessorWindowDriverVerifierV1::new(
        h.anchor.clone(),
        h.target.clone(),
        archive.clone(),
    )
    .unwrap();
    detached
        .verify_closure(
            &body,
            &original.state,
            exported.public_state_ref(),
            deadline(),
        )
        .unwrap();
    assert_eq!(exported.pending_actions().len(), 1);
    let reference = &exported.pending_actions()[0];
    let path = archive
        .root()
        .join("blobs")
        .join(format!("{}.blob", reference.digest));
    let bytes = fs::read(&path).unwrap();
    let permissions = fs::metadata(&path).unwrap().permissions();
    fs::remove_file(&path).unwrap();
    assert!(
        detached
            .verify_closure(
                &body,
                &original.state,
                exported.public_state_ref(),
                deadline()
            )
            .is_err(),
        "the same original QC/journal/roots cannot recover without its original pending actions"
    );
    assert_eq!(certificate.canonical_bytes().unwrap(), original_qc);
    assert_eq!(original.state.journal.as_ref(), Some(&original_journal));
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, permissions).unwrap();
    detached
        .verify_closure(
            &body,
            &original.state,
            exported.public_state_ref(),
            deadline(),
        )
        .unwrap();
    let digest = body.digest().unwrap();
    let intent = BftSuccessorWindowInstallIntentBodyV1 {
        schema_version: 1,
        checkpoint: body.clone(),
        expected_lease_generation: original.lease_generation,
        expected_revision: original.state.revision,
        expected_state_digest: original.state.state_digest.clone(),
        driver_evidence_digest: body.cut.driver_install_digest.clone(),
    };
    lease
        .prepare_successor_window(intent, public_manifest)
        .unwrap();
    let (record, installed) = lease
        .commit_successor_window(&body.request_id, &digest)
        .unwrap();
    assert_eq!(installed.state.journal.as_ref(), Some(&original_journal));
    let evidence = actual
        .lock()
        .unwrap()
        .install_successor_window(SuccessorPrivateInstallRequestV1 {
            binding: &binding,
            provider_pin: &pin,
            lease: lease.as_mut(),
            request_id: &body.request_id,
            checkpoint_digest: &digest,
            archive: archive.clone(),
            deadline: deadline(),
        })
        .unwrap();
    let (adopted_record, _) = lease
        .confirm_successor_window_adopted(&body.request_id, &digest, evidence)
        .unwrap();
    assert_eq!(adopted_record.request.intent, record.request.intent);
    assert_eq!(adopted_record.request.receipt, record.request.receipt);
    assert_eq!(
        adopted_record.original_public_state,
        record.original_public_state
    );
    let active = lease.verified_active_window().unwrap().unwrap();
    let adopted = lease.checkpoint().unwrap();
    assert!(adopted.state.history.is_empty());
    assert_eq!(adopted.state.journal.as_ref(), Some(&original_journal));
    assert_eq!(active.base_schedule(), &schedule);
    let archived = lease
        .read_successor_original_state(&body.request_id, &digest, deadline())
        .unwrap();
    let installed = archived
        .reconstruct_installed_pending(&binding, &pin, &adopted_record)
        .unwrap();
    let base = verifier
        .recover_private_base(&adopted_record, &archived, deadline())
        .unwrap();
    drop(actual);
    let mut window = FormalBftHeaderActiveExecutionV1::open(
        &root.path().join("ledger"),
        store,
        base,
        &active,
        &binding,
        &pin,
        &installed,
        deadline(),
    )
    .unwrap();
    assert!(
        window.pending_candidate().unwrap().is_none(),
        "pending QC survives as its original received alternative, never a new local selection"
    );
    let cp = lease
        .append_committed_certificate(
            adopted.state.revision,
            &adopted.state.state_digest,
            certificate.clone(),
        )
        .unwrap();
    window
        .append_certified(certificate.clone(), deadline())
        .unwrap();
    lease
        .confirm_applied(
            cp.state.revision,
            &cp.state.state_digest,
            certificate.execution_commitment.height,
            &certificate.header.block_id().unwrap(),
            &window.current_install_digest().unwrap(),
        )
        .unwrap();
    assert!(
        window
            .world()
            .state()
            .agents
            .contains_key("external-original-pending-qc")
    );
    let final_cp = lease.checkpoint().unwrap();
    assert_eq!(final_cp.state.history.len(), 1);
    assert_eq!(
        final_cp.state.history[0].canonical_bytes().unwrap(),
        original_qc
    );
    assert!(final_cp.state.pending_driver_apply.is_none());
    assert_eq!(
        consensus_signatures.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "original external QC recovery never requests another consensus signature"
    );
    drop(lease);
    server.join().unwrap().unwrap();
}

#[inline(never)]
fn build_original(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    schedule: &BftHeaderHeightScheduleV3,
    actions: Vec<oasis7_node::NodeConsensusAction>,
) -> oasis7_node::NodeBftPrivateCandidateV3 {
    let context = NodeBftExecutionContextV3::from_schedule(schedule);
    let mut driver = actual.lock().unwrap();
    let build = NodeBftBuildContextV3 {
        execution: context.clone(),
        actions,
    };
    let private = driver.build_bft_v3_candidate(build.clone()).unwrap_or_else(|_| {
        let diagnostic = driver.diagnose_candidate_capacity_for_test(&build);
        panic!("original prefix build failed height={} parent_tick={} action_count={} {diagnostic}",
            context.height, context.parent_tick, build.actions.len());
    });
    driver
        .retain_bft_v3_private_candidate(&context, &private)
        .unwrap();
    private
}

#[inline(never)]
fn append_original_empty(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    lease: &mut dyn BftSignerLeaseV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> BftHeaderHeightScheduleV3 {
    let private = build_original(actual, schedule, Vec::new());
    let certificate = external_certificate(schedule, &private.public_value, false);
    let cp = lease.checkpoint().unwrap();
    let cp = lease.start_height_timer(cp.state.revision).unwrap();
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_commit = Some(certificate.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let cp = lease
        .append_committed_certificate(
            cp.state.revision,
            &cp.state.state_digest,
            certificate.clone(),
        )
        .unwrap();
    let token = verify_commit_certificate_token_v3(schedule, &certificate).unwrap();
    let applied = actual
        .lock()
        .unwrap()
        .commit_bft_v3_certificate(token.clone())
        .unwrap();
    lease
        .confirm_applied(
            cp.state.revision,
            &cp.state.state_digest,
            applied.height,
            &applied.header_id,
            &applied.driver_install_digest,
        )
        .unwrap();
    token.next_height_schedule().clone()
}

fn external_certificate(
    schedule: &BftHeaderHeightScheduleV3,
    value: &BftHeaderValueV3,
    external_proposer: bool,
) -> FormalBftCertifiedHeaderV3 {
    let round = (0..4)
        .find(|round| {
            !external_proposer
                || schedule.expected_proposer_for_round(*round).unwrap() != validator_id(0)
        })
        .unwrap();
    let proposer = schedule.expected_proposer_for_round(round).unwrap();
    let key_index = (0..3).find(|i| validator_id(*i) == proposer).unwrap();
    let block = value.block_id().unwrap();
    let proposal = BftProposalHeaderV3::sign(
        schedule.scope().clone(),
        schedule.height(),
        round,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        proposer,
        block.clone(),
        None,
        &fixtures::key(120 + key_index),
    )
    .unwrap();
    let precommits = [1, 2]
        .into_iter()
        .map(|i| {
            BftVoteV3::sign(
                schedule.scope().clone(),
                schedule.height(),
                round,
                BftVotePhaseV3::Precommit,
                schedule.active_set().set_hash().into(),
                schedule.policy_hash().into(),
                BftVoteValueV3::Block {
                    block_id: block.clone(),
                },
                validator_id(i),
                &fixtures::key(120 + i),
            )
            .unwrap()
        })
        .collect();
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits,
        },
        value.header.clone(),
        value.execution_commitment.clone(),
    )
    .unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(120)
}

fn weighted_handoff() -> QualifiedH<BftProfileTargetV2> {
    qualified_handoff_with_target_and_setup(
        102,
        |_| {},
        |scope, policy| {
            let validators = (0..3)
                .map(|index| oasis7_proto::distributed_bft::v2::BftValidatorV2 {
                    validator_id: validator_id(index),
                    public_key_hex: hex::encode(
                        fixtures::key(120 + index).verifying_key().to_bytes(),
                    ),
                    voting_power: if index == 1 { 2 } else { 1 },
                })
                .collect();
            let target = BftProfileTargetV2::new(scope, 1, validators, policy)?;
            let handoff = ProfileHandoffV1 {
                next_profile: target.profile.clone(),
                next_profile_version: target.profile_version,
                next_authority_digest: target.target_digest()?,
                valid_from_position: 3,
            };
            Ok((target, handoff))
        },
    )
}

/// Join before archive Scratch roots disappear, including assertion unwinds.
/// The lease is declared after this guard, so its connection closes first.
struct ProviderJoin(Option<thread::JoinHandle<Result<(), String>>>);
impl ProviderJoin {
    fn join(mut self) -> thread::Result<Result<(), String>> {
        self.0.take().unwrap().join()
    }
}
impl Drop for ProviderJoin {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let _ = handle.join();
        }
    }
}
