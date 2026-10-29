//! Real authenticated provider/CAS and archive transport; explicit memory custody
//! and private verifier doubles do not attest hardware or a real World driver.
use super::*;
use oasis7_consensus::tendermint_v3::{BftConsensusActionV3, BftProposalIntentV3, BftVoteIntentV3};
use oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3;
struct Verifier(Arc<Archive>);
impl BftSuccessorWindowDriverVerifierV1 for Verifier {
    fn verify_prepared(
        &self,
        record: &BftSuccessorWindowStoredRequestV1,
        state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if Instant::now() >= deadline
            || self.0.load_public_state(&record.original_public_state)? != *state
        {
            return Err(BftSignerErrorV3("test closure mismatch".into()));
        }
        Ok(())
    }
    fn verify_adopted(
        &self,
        record: &BftSuccessorWindowStoredRequestV1,
        state: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let original = self.0.load_public_state(&record.original_public_state)?;
        if Instant::now() >= deadline
            || original.history != state.history
            || original.journal != state.journal
            || evidence.readback_state_digest != state.state_digest
        {
            return Err(BftSignerErrorV3("test immutable readback mismatch".into()));
        }
        Ok(())
    }
}
fn native_active(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    archive: Arc<Archive>,
    calls: Arc<AtomicUsize>,
) -> (RemoteBftSignerProviderV3, thread::JoinHandle<()>) {
    native_successor_counted_mode(
        binding,
        snapshot,
        archive.clone(),
        Some(Arc::new(Verifier(archive))),
        Arc::new(AtomicUsize::new(0)),
        calls,
        true,
    )
}
fn replay(
    cp: &BftSignerCheckpointV3,
    schedule: &BftHeaderHeightScheduleV3,
    binding: &BftSignerBindingV3,
) -> (
    oasis7_consensus::tendermint_v3::BftConsensusMachineV3,
    Vec<BftConsensusActionV3>,
) {
    oasis7_consensus::tendermint_v3::replay_bft_signing_journal_with_effects_v3(
        cp.state.journal.as_ref().unwrap(),
        schedule.active_set(),
        schedule,
        binding.timeouts,
    )
    .unwrap()
}
fn value(
    previous: &FormalBftCertifiedHeaderV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> BftHeaderValueV3 {
    let mut commitment = previous.execution_commitment.clone();
    commitment.activation_digest = schedule.activation_digest().into();
    commitment.scope = schedule.scope().clone();
    commitment.height = schedule.height();
    commitment.parent_commit_ref = schedule.parent_commit_ref().into();
    commitment.parent_tick = schedule.parent_tick();
    commitment.parent_semantic_state_root = schedule.parent_state_root().into();
    commitment.manifest_hash = schedule.parent_manifest_hash().into();
    commitment.authority_epoch = schedule.authority_epoch();
    commitment.active_validator_set_hash = schedule.active_set().set_hash().into();
    commitment.schedule_policy_hash = schedule.policy_hash().into();
    BftHeaderValueV3 {
        header: BftBlockHeaderV3 {
            protocol_version: 3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
        },
        execution_commitment: commitment,
    }
}
fn proposal_and_prevote(
    lease: &mut dyn BftSignerLeaseV3,
    binding: &BftSignerBindingV3,
    schedule: &BftHeaderHeightScheduleV3,
    value: BftHeaderValueV3,
) -> (BftSignerCheckpointV3, BftProposalIntentV3, BftVoteIntentV3) {
    let cp = lease.checkpoint().unwrap();
    let machine = oasis7_consensus::tendermint_v3::replay_bft_signing_journal_v3(
        cp.state.journal.as_ref().unwrap(),
        schedule.active_set(),
        schedule,
        binding.timeouts,
    )
    .unwrap();
    let intent = machine.proposal_intent(value, "validator-a").unwrap();
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_proposal_intent = Some(intent.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (_, cp) = lease.sign_proposal(cp.state.revision, &intent).unwrap();
    let vote = replay(&cp, schedule, binding)
        .1
        .into_iter()
        .find_map(|e| {
            if let BftConsensusActionV3::SignVote(v) = e {
                Some(v)
            } else {
                None
            }
        })
        .unwrap();
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_vote_intent = Some(vote.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (_, cp) = lease.sign_vote(cp.state.revision, &vote).unwrap();
    let precommit = replay(&cp, schedule, binding)
        .1
        .into_iter()
        .find_map(|e| {
            if let BftConsensusActionV3::SignVote(v) = e {
                Some(v)
            } else {
                None
            }
        })
        .unwrap();
    (cp, intent, precommit)
}
fn install(
    lease: &mut dyn BftSignerLeaseV3,
    binding: &BftSignerBindingV3,
) -> (BftSuccessorWindowStoredRequestV1, BftSignerCheckpointV3) {
    let cp = lease.checkpoint().unwrap();
    let schedule = lease.current_verified_schedule().unwrap();
    let (mut body, mut manifest) = request_with_schedule(binding, &cp, &schedule);
    manifest.first_height = binding.handoff_position + 1;
    if let Some(window) = &cp.state.successor_window {
        let previous = window.requests.last().unwrap();
        body.checkpoint.sequence = previous.request.intent.body.checkpoint.sequence + 1;
        body.checkpoint.predecessor_checkpoint_digest =
            previous.request.intent.body.checkpoint.digest().unwrap();
        body.checkpoint.request_id = format!("active-window-{}", body.checkpoint.sequence);
        manifest.segment_sequence = body.checkpoint.sequence;
        manifest.predecessor_root = Some(previous.manifest.digest().unwrap());
        manifest.first_height = previous.request.intent.body.checkpoint.cut.height + 1;
        manifest.usage.segments = body.checkpoint.sequence;
        manifest.usage.objects += window.usage.objects;
        manifest.usage.total_bytes += window.usage.total_bytes;
    }
    index_whole_state_signed_results_v1(&mut manifest, &cp.state).unwrap();
    body.checkpoint.usage = manifest.usage;
    body.checkpoint.public_archive_root = manifest.digest().unwrap();
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    lease
        .prepare_successor_window(body.clone(), manifest)
        .unwrap();
    let (record, cp) = lease.commit_successor_window(&id, &digest).unwrap();
    assert!(
        cp.state
            .active_window
            .as_ref()
            .is_none_or(|a| a.checkpoint_digest != digest)
    );
    let evidence = BftSuccessorWindowAdoptedEvidenceV1 {
        active_segment_origin: Some(reference(b"explicit-memory-window-origin-test-double")),
        checkpoint_digest: digest,
        driver_install_digest: body.checkpoint.cut.driver_install_digest,
        schedule_digest: body.checkpoint.successor_schedule_digest,
        journal_digest: body.checkpoint.current_journal_digest,
        readback_state_digest: cp.state.state_digest.clone(),
    };
    let mut missing_origin = evidence.clone();
    missing_origin.active_segment_origin = None;
    assert!(
        lease
            .confirm_successor_window_adopted(
                &id,
                &record.request.intent.body.checkpoint.digest().unwrap(),
                missing_origin
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, cp.state);
    let (_, after) = lease
        .confirm_successor_window_adopted(
            &id,
            &record.request.intent.body.checkpoint.digest().unwrap(),
            evidence,
        )
        .unwrap();
    (record, after)
}
fn activated() -> (
    BftSignerBindingV3,
    Arc<Mutex<BftSignerBackendSnapshotV3>>,
    Arc<Archive>,
    Arc<AtomicUsize>,
) {
    let (binding, snapshot) = fixture_kind(true);
    snapshot.lock().unwrap().successor_archive_transfer =
        Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
    let archive = Arc::new(Archive::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_active(&binding, &snapshot, archive.clone(), calls.clone());
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    lease.start_height_timer(cp.state.revision).unwrap();
    install(lease.as_mut(), &binding);
    drop(lease);
    server.join().unwrap();
    (binding, snapshot, archive, calls)
}
#[test]
fn active_window_cold_open_rejects_disabled_rebase_mode() {
    let (binding, snapshot, archive, _) = activated();
    let service = BftSignerServiceV3::open_with_successor_window(
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
        Box::new(Backend {
            binding: binding.digest().unwrap(),
            snapshot,
        }),
        BftSuccessorWindowSupportV1::new(archive, Duration::from_secs(5)).unwrap(),
    );
    assert!(
        service.is_err(),
        "active custody cannot reopen with rebase capability disabled"
    );
}
#[test]
fn active_window_adopt_preserves_locked_journal_then_cold_continues_and_rebases_again() {
    let (binding, snapshot) = fixture_kind(true);
    snapshot.lock().unwrap().successor_archive_transfer =
        Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
    let archive = Arc::new(Archive::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_active(&binding, &snapshot, archive.clone(), calls.clone());
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let old_proposal = cp.state.completed_journals[0]
        .as_ref()
        .unwrap()
        .events
        .iter()
        .find_map(|e| {
            if let BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } = e
            {
                Some(proposal.as_ref().clone())
            } else {
                None
            }
        })
        .unwrap();
    let mut old_header = old_proposal.header.clone();
    old_header.signature_hex.clear();
    let old_intent = BftProposalIntentV3 {
        header: old_header,
        block: old_proposal.block.clone(),
        valid_round_evidence: old_proposal.valid_round_evidence.clone(),
    };
    let template = cp.state.history.last().unwrap().clone();
    lease.start_height_timer(cp.state.revision).unwrap();
    let schedule = lease.current_verified_schedule().unwrap();
    let (locked, _, precommit) = proposal_and_prevote(
        lease.as_mut(),
        &binding,
        &schedule,
        value(&template, &schedule),
    );
    assert!(replay(&locked, &schedule, &binding).0.locked().is_some());
    let journal = locked.state.journal.clone();
    let count = calls.load(Ordering::SeqCst);
    let (_, after) = install(lease.as_mut(), &binding);
    assert!(after.state.history.is_empty());
    assert!(after.state.completed_journals.is_empty());
    assert!(after.state.applied_installs.is_empty());
    assert_eq!(after.state.journal, journal);
    assert_eq!(calls.load(Ordering::SeqCst), count);
    assert_eq!(lease.sign_proposal(0, &old_intent).unwrap().0, old_proposal);
    assert_eq!(calls.load(Ordering::SeqCst), count);
    drop(lease);
    server.join().unwrap();
    let before_transfer = snapshot
        .lock()
        .unwrap()
        .successor_archive_transfer
        .unwrap()
        .reserved_response_bytes;
    let (provider, server) = native_active(&binding, &snapshot, archive.clone(), calls.clone());
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(
        snapshot
            .lock()
            .unwrap()
            .successor_archive_transfer
            .unwrap()
            .reserved_response_bytes
            > before_transfer
    );
    let cp = lease.checkpoint().unwrap();
    assert_eq!(cp.state.journal, journal);
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_vote_intent = Some(precommit.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (vote, cp) = lease.sign_vote(cp.state.revision, &precommit).unwrap();
    assert_eq!(vote.height, schedule.height());
    let certificate = replay(&cp, &schedule, &binding)
        .1
        .into_iter()
        .find_map(|e| {
            if let BftConsensusActionV3::Commit(c) = e {
                Some(*c)
            } else {
                None
            }
        })
        .unwrap();
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
    let cp = lease
        .confirm_applied(
            cp.state.revision,
            &cp.state.state_digest,
            certificate.execution_commitment.height,
            &certificate.header.block_id().unwrap(),
            &hash(87),
        )
        .unwrap();
    assert_eq!(cp.state.history.len(), 1);
    assert_eq!(
        lease.current_verified_schedule().unwrap().height(),
        schedule.height() + 1
    );
    let (_, twice) = install(lease.as_mut(), &binding);
    assert!(twice.state.history.is_empty());
    assert_eq!(
        twice
            .state
            .successor_window
            .as_ref()
            .unwrap()
            .minimum
            .as_ref()
            .unwrap()
            .sequence,
        2
    );
    let capability = lease.verified_active_window().unwrap().unwrap();
    assert_eq!(capability.base_schedule().height(), schedule.height() + 1);
    assert_eq!(capability.minimum().sequence, 2);
    let record = twice
        .state
        .successor_window
        .as_ref()
        .unwrap()
        .requests
        .last()
        .unwrap();
    let original = lease
        .read_successor_original_state(
            &record.request.intent.body.checkpoint.request_id,
            &record.request.intent.body.checkpoint.digest().unwrap(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    let installed = original
        .reconstruct_installed_pending(&binding, &public(33), record)
        .unwrap();
    assert_eq!(
        installed.installed_state().revision,
        record
            .request
            .receipt
            .as_ref()
            .unwrap()
            .body
            .committed_revision
    );
    assert_eq!(
        installed.installed_state().state_digest,
        record
            .request
            .adopted_evidence
            .as_ref()
            .unwrap()
            .readback_state_digest
    );
    let mut bad = record.clone();
    bad.request
        .adopted_evidence
        .as_mut()
        .unwrap()
        .readback_state_digest = hash(79);
    assert!(
        original
            .reconstruct_installed_pending(&binding, &public(33), &bad)
            .is_err()
    );
    assert!(
        original
            .reconstruct_installed_pending(&binding, &public(34), record)
            .is_err()
    );
    assert_eq!(lease.sign_proposal(0, &old_intent).unwrap().0, old_proposal);
    drop(lease);
    server.join().unwrap();
}

fn commit_value(
    lease: &mut dyn BftSignerLeaseV3,
    binding: &BftSignerBindingV3,
    schedule: &BftHeaderHeightScheduleV3,
    value: BftHeaderValueV3,
) -> BftSignerCheckpointV3 {
    let cp = lease.checkpoint().unwrap();
    lease.start_height_timer(cp.state.revision).unwrap();
    let (cp, _, precommit) = proposal_and_prevote(lease, binding, schedule, value);
    let mut candidate = cp.state.candidate();
    candidate.journal.as_mut().unwrap().pending_vote_intent = Some(precommit.clone());
    let cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, candidate)
        .unwrap();
    let (_, cp) = lease.sign_vote(cp.state.revision, &precommit).unwrap();
    let certificate = replay(&cp, schedule, binding)
        .1
        .into_iter()
        .find_map(|e| {
            if let BftConsensusActionV3::Commit(c) = e {
                Some(*c)
            } else {
                None
            }
        })
        .unwrap();
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
    lease
        .confirm_applied(
            cp.state.revision,
            &cp.state.state_digest,
            certificate.execution_commitment.height,
            &certificate.header.block_id().unwrap(),
            &hash(87),
        )
        .unwrap()
}
#[test]
fn active_window_h_plus_one_rebase_keeps_h_plus_two_membership_ack_and_new_key() {
    use oasis7_formal_protocol::bft::BftValidatorSetV2;
    use oasis7_proto::distributed_bft::v2::{BftValidatorTransitionV2, BftValidatorV2};
    let binding = super::super::binding_with_epoch_length(4);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        successor_archive_transfer: Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let archive = Arc::new(Archive::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_active(&binding, &snapshot, archive, calls);
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    lease
        .enable_membership(cp.state.revision, &cp.state.state_digest)
        .unwrap();
    let template = fixture_kind(true).1.lock().unwrap().state.history[0].clone();
    for height in 3..=7 {
        let schedule = lease.current_verified_schedule().unwrap();
        assert_eq!(schedule.height(), height);
        let mut block = value(&template, &schedule);
        if height == 6 {
            let validators = vec![BftValidatorV2 {
                validator_id: "validator-a".into(),
                public_key_hex: public(32),
                voting_power: 1,
            }];
            let new_set =
                BftValidatorSetV2::new(schedule.scope().clone(), validators.clone()).unwrap();
            block.execution_commitment.transition = Some(BftValidatorTransitionV2 {
                source_commit_ref: schedule.parent_commit_ref().into(),
                source_state_root: schedule.parent_state_root().into(),
                source_profile: schedule.policy().profile_id.clone(),
                source_epoch: schedule.parent_tick() / 4,
                target_epoch: 2,
                epoch_length_ticks: 4,
                mapping_ruleset_hash: schedule.policy_hash().into(),
                scheduled_height: 6,
                activation_height: 8,
                old_validator_set_hash: schedule.active_set().set_hash().into(),
                new_validator_set_hash: new_set.set_hash().into(),
                new_validators: validators,
                governance_action_digest: hash(61),
                projected_registry_digest: hash(62),
            });
        }
        block.execution_commitment.transition_ref = schedule.pending_transition().cloned();
        block.header.execution_commitment_hash = block.execution_commitment.digest().unwrap();
        commit_value(lease.as_mut(), &binding, &schedule, block);
    }
    let (_, rebased) = install(lease.as_mut(), &binding);
    assert!(rebased.state.applied_installs.is_empty());
    assert_eq!(lease.current_verified_schedule().unwrap().height(), 8);
    let prepared = lease
        .prepare_transition_install(
            rebased.state.revision,
            &rebased.state.state_digest,
            "after-rebase-key-install".into(),
            6,
            binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV3::Pending { intent, checkpoint } = prepared else {
        panic!("fresh install must prepare")
    };
    let b = &intent.body;
    let evidence = BftLocalInstallEvidenceV3 {
        schema_version: 3,
        runtime_instance_id: b.runtime_instance_id.clone(),
        installed_head_height: b.install_head_height,
        installed_head_commit_ref: b.install_parent_commit_ref.clone(),
        installed_state_root: b.install_state_root.clone(),
        installed_manifest_hash: b.install_manifest_hash.clone(),
        installed_active_set_hash: b.install_active_set_hash.clone(),
        installed_authority_epoch: b.target_authority_epoch,
        local_validator_id: b.local_validator_id.clone(),
        local_signing_public_key: b.local_signing_public_key.clone(),
        wal_install_digest: checkpoint.state.journal.as_ref().unwrap().digest().unwrap(),
        driver_install_digest: hash(87),
    };
    let mut bad = evidence.clone();
    bad.driver_install_digest = hash(88);
    assert!(
        lease
            .complete_transition_install(intent.clone(), bad)
            .is_err()
    );
    let mut bad = evidence.clone();
    bad.wal_install_digest = hash(89);
    assert!(
        lease
            .complete_transition_install(intent.clone(), bad)
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, checkpoint.state);
    let completed = lease.complete_transition_install(intent, evidence).unwrap();
    assert!(
        completed
            .checkpoint
            .state
            .membership_install
            .as_ref()
            .unwrap()
            .pending
            .is_none()
    );
    let cp = completed.checkpoint;
    lease.start_height_timer(cp.state.revision).unwrap();
    let schedule = lease.current_verified_schedule().unwrap();
    let (cp, _, _) = proposal_and_prevote(
        lease.as_mut(),
        &binding,
        &schedule,
        value(&template, &schedule),
    );
    assert_eq!(
        cp.state
            .journal
            .as_ref()
            .unwrap()
            .local_signing_public_key(),
        public(32)
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn active_window_enabled_without_transfer_capability_rejects_before_prepare() {
    let (binding, snapshot) = fixture_kind(true);
    let before = snapshot.lock().unwrap().clone();
    let service = BftSignerServiceV3::open_with_successor_window(
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
        Box::new(Backend {
            binding: binding.digest().unwrap(),
            snapshot: snapshot.clone(),
        }),
        BftSuccessorWindowSupportV1::new(Arc::new(Archive::default()), Duration::from_secs(5))
            .unwrap()
            .with_active_window_rebase(),
    );
    assert!(
        service.is_err(),
        "unsupported transfer custody must reject before any minimum reservation"
    );
    assert_eq!(*snapshot.lock().unwrap(), before);
}
#[test]
fn active_window_acquire_refuses_checkpoint_below_newer_authenticated_archive_chunks() {
    use crate::protected_bft_v3::wire::{
        BftSignerReplyKindV3, SignedBftSignerReplyV3, SignedBftSignerRequestV3, encode_frame,
        read_frame,
    };
    let (binding, snapshot, archive, calls) = activated();
    let (_, upstream, upstream_address) = native_successor_counted_mode_endpoint(
        &binding,
        &snapshot,
        archive.clone(),
        Some(Arc::new(Verifier(archive))),
        Arc::new(AtomicUsize::new(0)),
        calls,
        true,
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let proxy = thread::spawn(move || {
        let (mut downstream, _) = listener.accept().unwrap();
        let mut upstream = std::net::TcpStream::connect(upstream_address).unwrap();
        while let Ok(request) = read_frame::<SignedBftSignerRequestV3>(&mut downstream) {
            std::io::Write::write_all(&mut upstream, &encode_frame(&request).unwrap()).unwrap();
            let mut reply: SignedBftSignerReplyV3 = read_frame(&mut upstream).unwrap();
            if let BftSignerReplyKindV3::ArchiveChunk { chunk } = &reply.body.result {
                let mut body = chunk.body.clone();
                body.actual_revision += 1;
                body.actual_state_digest = hash(201);
                let key = Handle {
                    key: key(33),
                    disabled: None,
                };
                let chunk =
                    crate::protected_bft_v3::archive_query::BftSuccessorArchiveChunkV1::sign(
                        body, &key,
                    )
                    .unwrap();
                reply.body.state_revision = chunk.body.actual_revision;
                reply.body.state_digest = chunk.body.actual_state_digest.clone();
                reply.body.result = BftSignerReplyKindV3::ArchiveChunk {
                    chunk: Box::new(chunk),
                };
                reply = SignedBftSignerReplyV3::sign(reply.body, &key).unwrap();
            }
            if std::io::Write::write_all(&mut downstream, &encode_frame(&reply).unwrap()).is_err() {
                break;
            }
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
    assert!(
        provider.acquire(&binding).is_err(),
        "cold acquire must not expose a checkpoint older than a signed archive observation"
    );
    proxy.join().unwrap();
    upstream.join().unwrap();
}

fn resigned_record(
    record: &BftSuccessorWindowStoredRequestV1,
    signer: u8,
    change: impl FnOnce(&mut BftSuccessorWindowCheckpointBodyV1),
) -> BftSuccessorWindowStoredRequestV1 {
    let mut forged = record.clone();
    let key = Handle {
        key: key(signer),
        disabled: None,
    };
    let mut body = record.request.intent.body.clone();
    change(&mut body.checkpoint);
    forged.request.intent = BftSuccessorWindowInstallIntentV1::sign(body, &key).unwrap();
    let mut receipt = record.request.receipt.as_ref().unwrap().body.clone();
    receipt.checkpoint_digest = forged.request.intent.body.checkpoint.digest().unwrap();
    receipt.intent_digest = forged.request.intent.digest().unwrap();
    receipt.minimum = forged.request.intent.body.checkpoint.minimum().unwrap();
    forged.request.receipt = Some(BftSuccessorWindowInstallReceiptV1::sign(receipt, &key).unwrap());
    forged.request.status = BftSuccessorWindowStatusV1::InstalledPending;
    forged.request.adopted_evidence = None;
    forged
}
#[test]
fn active_window_historical_reconstruction_rejects_coherent_signed_wrong_cut_schedule_and_journal()
{
    let (binding, snapshot, archive, calls) = activated();
    let (provider, server) = native_active(&binding, &snapshot, archive, calls);
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let record = cp
        .state
        .successor_window
        .as_ref()
        .unwrap()
        .requests
        .last()
        .unwrap();
    let original = lease
        .read_successor_original_state(
            &record.request.intent.body.checkpoint.request_id,
            &record.request.intent.body.checkpoint.digest().unwrap(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    for mode in 0..3 {
        let forged = resigned_record(record, 33, |checkpoint| match mode {
            0 => checkpoint.cut.header_id = hash(201),
            1 => checkpoint.successor_schedule_digest = hash(202),
            _ => checkpoint.current_journal_digest = Some(hash(203)),
        });
        forged.validate(&binding, &public(33)).unwrap();
        assert!(
            original
                .reconstruct_installed_pending(&binding, &public(33), &forged)
                .is_err(),
            "provider-signed metadata mode {mode} is not an actual original cut"
        );
    }
    drop(lease);
    server.join().unwrap();
}
#[test]
fn active_window_historical_reconstruction_keeps_original_provider_provenance() {
    let (binding, snapshot, archive, calls) = activated();
    let (provider, server) = native_active(&binding, &snapshot, archive, calls);
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let record = cp
        .state
        .successor_window
        .as_ref()
        .unwrap()
        .requests
        .last()
        .unwrap();
    let original = lease
        .read_successor_original_state(
            &record.request.intent.body.checkpoint.request_id,
            &record.request.intent.body.checkpoint.digest().unwrap(),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    let other_provider = resigned_record(record, 51, |_| {});
    other_provider.validate(&binding, &public(51)).unwrap();
    assert!(
        original
            .reconstruct_installed_pending(&binding, &public(51), &other_provider)
            .is_err(),
        "caller-selected provider must not replace authenticated original provenance"
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn active_window_unresolved_first_install_cannot_select_legacy_base() {
    let (binding, snapshot) = fixture_kind(true);
    snapshot.lock().unwrap().successor_archive_transfer =
        Some(BftSuccessorArchiveTransferLedgerV1::provisioned());
    let archive = Arc::new(Archive::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native_active(&binding, &snapshot, archive, calls);
    let mut lease = provider.acquire(&binding).unwrap();
    let cp = lease.checkpoint().unwrap();
    let (mut body, mut manifest) = request(&binding, &cp);
    index_whole_state_signed_results_v1(&mut manifest, &cp.state).unwrap();
    body.checkpoint.public_archive_root = manifest.digest().unwrap();
    let id = body.checkpoint.request_id.clone();
    let digest = body.checkpoint.digest().unwrap();
    let (_, prepared) = lease.prepare_successor_window(body, manifest).unwrap();
    assert!(prepared.state.active_window.is_none());
    assert!(
        lease.verified_active_window().is_err(),
        "first Prepared must not be treated as an available legacy base"
    );
    let (_, installed) = lease.commit_successor_window(&id, &digest).unwrap();
    assert!(installed.state.active_window.is_none());
    assert!(
        lease.verified_active_window().is_err(),
        "first InstalledPending must not be treated as an available legacy base"
    );
    drop(lease);
    server.join().unwrap();
}

#[path = "signed_result_tests.rs"]
mod signed_result_tests;
