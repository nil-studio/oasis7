use super::*;
use oasis7_consensus::tendermint_v2::{
    BftConsensusActionV2, BftConsensusMachineV2, BftProposalIntentV2, BftSigningJournalV2,
    BftVoteIntentV2, replay_bft_signing_journal_with_effects_v2,
    verify_bft_signing_journal_transition_v2,
};
use oasis7_distfs::protected_bft_v2::{
    BftSignerCheckpointV2, BftSignerLeaseV2, BftSignerProviderV2,
};

#[test]
fn restart_recovers_commit_effect_from_completed_precommit_journal() {
    let root = PrivateRoot::new();
    let identity = solo_identity(91);
    let set = identity.binding.validate().expect("valid solo set");
    let schedule = BftHeightProposerScheduleV2::genesis(
        set.clone(),
        identity.binding.schedule_policy.clone(),
        identity.binding.genesis_authority_epoch,
        identity.binding.genesis_state_root.clone(),
        identity.binding.genesis_manifest_hash.clone(),
    )
    .expect("valid solo schedule");
    // Model a process crash after the final signed precommit event was durably
    // appended, but before its returned Commit effect could reserve the QC.
    let mut lease = identity
        .provider
        .provider
        .acquire(&identity.binding)
        .expect("acquire protected signer for crash fixture");
    let mut checkpoint = lease.checkpoint().expect("read signer checkpoint");
    checkpoint = lease
        .start_height_timer(checkpoint.state.revision)
        .expect("start durable height timer");
    let event_time = lease.protected_time_ms().expect("read protected time");
    let mut machine = BftConsensusMachineV2::new(
        schedule.clone(),
        identity.id.clone(),
        event_time,
        TEST_TIMEOUTS,
    )
    .expect("construct same-height consensus machine");
    let hook = TransitionTestHook {
        ledger: Arc::clone(&identity.driver),
        new_validators: validators(&[11, 12, 13, 15, 16]),
    };
    let block = hook
        .make_block(&schedule, &[])
        .expect("build valid idle block");
    let proposal_intent = machine
        .proposal_intent(block, &identity.id)
        .expect("construct deterministic solo proposal intent");
    reserve_proposal(
        &mut *lease,
        &mut checkpoint,
        &proposal_intent,
        &set,
        &schedule,
    );
    let (proposal, signed_checkpoint) = lease
        .sign_proposal(checkpoint.state.revision, &proposal_intent)
        .expect("sign and durably record the proposal");
    checkpoint = signed_checkpoint;
    let prevote_intent = take_vote_intent(
        machine
            .receive_proposal(proposal.clone(), true, event_time)
            .expect("accept signed proposal"),
        oasis7_proto::distributed_bft::v2::BftVotePhaseV2::Prevote,
    );
    reserve_vote(
        &mut *lease,
        &mut checkpoint,
        &prevote_intent,
        &set,
        &schedule,
    );
    let (prevote, signed_checkpoint) = lease
        .sign_vote(checkpoint.state.revision, &prevote_intent)
        .expect("sign and durably record the prevote");
    checkpoint = signed_checkpoint;
    let precommit_intent = take_vote_intent(
        machine
            .record_local_vote(prevote, event_time)
            .expect("record signed prevote"),
        oasis7_proto::distributed_bft::v2::BftVotePhaseV2::Precommit,
    );
    reserve_vote(
        &mut *lease,
        &mut checkpoint,
        &precommit_intent,
        &set,
        &schedule,
    );
    let (precommit, _signed_checkpoint) = lease
        .sign_vote(checkpoint.state.revision, &precommit_intent)
        .expect("sign and durably record the precommit");
    let effects = machine
        .record_local_vote(precommit.clone(), event_time)
        .expect("precommit quorum produces the commit effect");
    let certificate = effects
        .into_iter()
        .find_map(|effect| match effect {
            BftConsensusActionV2::Commit(certificate) => Some(certificate),
            _ => None,
        })
        .expect("single-validator precommit quorum produces a QC");
    let crashed_state = identity.provider.backend.snapshot().state;
    let crashed_journal = crashed_state
        .journal
        .as_ref()
        .expect("durable same-height journal exists");
    assert_eq!(crashed_journal.events().len(), 3);
    assert!(crashed_journal.pending_commit().is_none());
    let (replayed_machine, replayed_effects) =
        replay_bft_signing_journal_with_effects_v2(crashed_journal, &set, &schedule, TEST_TIMEOUTS)
            .expect("replay completed precommit journal");
    assert_eq!(replayed_machine.height(), schedule.height());
    assert_eq!(
        replayed_effects,
        vec![BftConsensusActionV2::Commit(certificate.clone())],
        "the durable precommit must restore its exact unreserved QC effect"
    );
    let mut already_reserved = crashed_journal.clone();
    already_reserved.pending_commit = Some(certificate.clone());
    let (_, reserved_effects) = replay_bft_signing_journal_with_effects_v2(
        &already_reserved,
        &set,
        &schedule,
        TEST_TIMEOUTS,
    )
    .expect("replay exact reserved QC");
    assert!(
        reserved_effects
            .iter()
            .all(|effect| !matches!(effect, BftConsensusActionV2::Commit(_))),
        "an exact pending_commit is recovered only through its reserved path"
    );
    drop(lease);

    let deadline = Instant::now() + Duration::from_secs(3);
    while !identity
        .provider
        .backend
        .snapshot()
        .lease_token_digest
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "old signer lease did not release"
        );
        thread::sleep(Duration::from_millis(5));
    }

    let addresses = reserve_udp_addresses(1);
    let mut node = make_node(&identity, 0, &addresses, root.path());
    node.start()
        .expect("reopen worker from the durable completed-precommit journal");
    wait_node_height(&node, 1, "replayed precommit quorum commit recovery");

    let recovered_state = identity.provider.backend.snapshot().state;
    assert_eq!(recovered_state.history, vec![certificate.clone()]);
    assert_eq!(
        identity
            .driver
            .lock()
            .expect("test driver ledger")
            .certificates,
        vec![certificate],
        "the recovered exact QC must be durably installed by the execution hook"
    );
    node.stop().expect("stop recovered solo worker");
}

#[test]
fn restart_resumes_unreserved_vote_effect_from_durable_proposal_event() {
    let root = PrivateRoot::new();
    let identity = solo_identity(92);
    let set = identity.binding.validate().expect("valid solo set");
    let schedule = BftHeightProposerScheduleV2::genesis(
        set.clone(),
        identity.binding.schedule_policy.clone(),
        identity.binding.genesis_authority_epoch,
        identity.binding.genesis_state_root.clone(),
        identity.binding.genesis_manifest_hash.clone(),
    )
    .expect("valid solo schedule");
    let mut lease = identity
        .provider
        .provider
        .acquire(&identity.binding)
        .expect("acquire protected signer for vote replay fixture");
    let mut checkpoint = lease.checkpoint().expect("read signer checkpoint");
    checkpoint = lease
        .start_height_timer(checkpoint.state.revision)
        .expect("start durable height timer");
    let event_time = lease.protected_time_ms().expect("read protected time");
    let machine = BftConsensusMachineV2::new(
        schedule.clone(),
        identity.id.clone(),
        event_time,
        TEST_TIMEOUTS,
    )
    .expect("construct same-height consensus machine");
    let hook = TransitionTestHook {
        ledger: Arc::clone(&identity.driver),
        new_validators: validators(&[11, 12, 13, 15, 16]),
    };
    let block = hook
        .make_block(&schedule, &[])
        .expect("build valid idle block");
    let proposal_intent = machine
        .proposal_intent(block, &identity.id)
        .expect("construct deterministic solo proposal intent");
    reserve_proposal(
        &mut *lease,
        &mut checkpoint,
        &proposal_intent,
        &set,
        &schedule,
    );
    let (proposal, _) = lease
        .sign_proposal(checkpoint.state.revision, &proposal_intent)
        .expect("sign and durably record the proposal");
    let crashed_journal = identity
        .provider
        .backend
        .snapshot()
        .state
        .journal
        .expect("durable proposal journal exists");
    assert_eq!(crashed_journal.events().len(), 1);
    assert!(crashed_journal.pending_vote_intent().is_none());
    let (replayed_machine, effects) = replay_bft_signing_journal_with_effects_v2(
        &crashed_journal,
        &set,
        &schedule,
        TEST_TIMEOUTS,
    )
    .expect("replay durable proposal event");
    assert_eq!(replayed_machine.round(), 0);
    let expected_prevote = oasis7_proto::distributed_bft::v2::BftVoteValueV2::Block {
        block_id: proposal.header.block_id.clone(),
    };
    match effects.as_slice() {
        [BftConsensusActionV2::SignVote(intent)] => {
            assert_eq!(
                intent.phase,
                oasis7_proto::distributed_bft::v2::BftVotePhaseV2::Prevote
            );
            assert_eq!(intent.value, expected_prevote);
        }
        other => panic!("expected one recovered prevote intent, found {other:?}"),
    }
    drop(lease);

    let deadline = Instant::now() + Duration::from_secs(3);
    while !identity
        .provider
        .backend
        .snapshot()
        .lease_token_digest
        .is_empty()
    {
        assert!(
            Instant::now() < deadline,
            "old signer lease did not release"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let addresses = reserve_udp_addresses(1);
    let mut node = make_node(&identity, 0, &addresses, root.path());
    node.start()
        .expect("reopen worker from the durable proposal event");
    wait_node_height(&node, 1, "replayed unreserved prevote recovery");

    let certificate = identity.provider.backend.snapshot().state.history[0].clone();
    assert_eq!(certificate.proposal.header.round, 0);
    assert_eq!(certificate.precommits.len(), 1);
    assert_eq!(certificate.precommits[0].validator_id, identity.id);
    assert_eq!(
        identity
            .driver
            .lock()
            .expect("test driver ledger")
            .certificates,
        vec![certificate],
        "replaying one unreserved prevote must not duplicate the installed QC"
    );
    node.stop().expect("stop recovered solo worker");
}

fn solo_identity(seed: u8) -> NodeIdentity {
    let id = validator_id(seed);
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let requester_seed = seed.wrapping_add(40);
    let provider_seed = seed.wrapping_add(80);
    let requester_key = SigningKey::from_bytes(&[requester_seed; 32]);
    let binding = BftSignerBindingV2 {
        schema_version: 2,
        scope: scope(),
        genesis_authority_epoch: 1,
        genesis_validators: vec![BftValidatorV2 {
            validator_id: id.clone(),
            public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
            voting_power: 1,
        }],
        genesis_state_root: hash_byte(0x22),
        genesis_manifest_hash: hash_byte(0x33),
        schedule_policy: BftSchedulePolicyV2::new(EPOCH_LENGTH, PROFILE_ID)
            .expect("valid solo schedule policy"),
        local_validator_id: id.clone(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: id.clone(),
            public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
        }],
        timeouts: TEST_TIMEOUTS,
        operator_instance_id: format!("node-v2-commit-replay-{seed}"),
        requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
    };
    let provider = ProtectedBftV2TestProvider::start(
        binding.clone(),
        requester_key,
        SigningKey::from_bytes(&[provider_seed; 32]),
        vec![signing_key],
    )
    .expect("start protected solo signer provider");
    NodeIdentity {
        id,
        seed,
        binding,
        provider,
        driver: Arc::new(Mutex::new(DriverLedger::default())),
    }
}

fn reserve_proposal(
    lease: &mut dyn BftSignerLeaseV2,
    checkpoint: &mut BftSignerCheckpointV2,
    intent: &BftProposalIntentV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
) {
    advance_journal(lease, checkpoint, set, schedule, |journal| {
        journal.pending_proposal_intent = Some(intent.clone());
    });
}

fn reserve_vote(
    lease: &mut dyn BftSignerLeaseV2,
    checkpoint: &mut BftSignerCheckpointV2,
    intent: &BftVoteIntentV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
) {
    advance_journal(lease, checkpoint, set, schedule, |journal| {
        journal.pending_vote_intent = Some(intent.clone());
    });
}

fn advance_journal(
    lease: &mut dyn BftSignerLeaseV2,
    checkpoint: &mut BftSignerCheckpointV2,
    set: &BftValidatorSetV2,
    schedule: &BftHeightProposerScheduleV2,
    update: impl FnOnce(&mut BftSigningJournalV2),
) {
    let previous = checkpoint
        .state
        .journal
        .as_ref()
        .expect("active signer journal");
    let mut journal = previous.clone();
    update(&mut journal);
    verify_bft_signing_journal_transition_v2(previous, &journal, set, schedule, TEST_TIMEOUTS)
        .expect("event is an exact legal journal transition");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal = Some(journal);
    *checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            candidate,
        )
        .expect("persist event in the protected signer");
}

fn take_vote_intent(
    effects: Vec<BftConsensusActionV2>,
    phase: oasis7_proto::distributed_bft::v2::BftVotePhaseV2,
) -> BftVoteIntentV2 {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            BftConsensusActionV2::SignVote(intent) if intent.phase == phase => Some(intent),
            _ => None,
        })
        .expect("deterministic consensus vote intent")
}
