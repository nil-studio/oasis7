use super::*;

use super::super::state::BftSignerCheckpointV2;
use oasis7_consensus::tendermint_v2::{
    BftValidatorSetV2, BftVoteIntentV2, verify_bft_signing_journal_transition_v2,
};

fn late_join_harness(tip_height: u64) -> Harness {
    let mut fixture = super::harness();
    fixture.binding.schedule_policy = BftSchedulePolicyV2::new(4, "profile-test").unwrap();
    let (mut history, mut schedule) =
        super::transition_history(&fixture.binding, &fixture.validator);
    while schedule.height() <= tip_height {
        let height = schedule.height();
        let block = BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height,
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: super::hash(height as u8 + 20),
            actions_cbor: vec![0x80],
            action_root: super::hash(height as u8 + 30),
            event_root: super::hash(height as u8 + 31),
            execution_block_hash: super::hash(height as u8 + 32),
            execution_state_root: super::hash(height as u8 + 33),
            state_root: super::hash(height as u8 + 34),
            transition: None,
            transition_ref: None,
        };
        let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
        assert_eq!(proposer_id, fixture.binding.local_validator_id);
        let block_id = block.block_id().unwrap();
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            height,
            0,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id,
            block_id.clone(),
            None,
            &fixture.validator,
        )
        .unwrap();
        let proposal = BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        };
        let vote = BftVoteV2::sign(
            schedule.scope().clone(),
            height,
            0,
            BftVotePhaseV2::Precommit,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            BftVoteValueV2::Block { block_id },
            fixture.binding.local_validator_id.clone(),
            &fixture.validator,
        )
        .unwrap();
        let certificate = BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits: vec![vote],
        };
        let token = verify_commit_certificate_token_v2(&schedule, &certificate).unwrap();
        history.push(certificate);
        schedule = token.next_height_schedule().clone();
    }

    let local_key = fixture.validator.clone();
    install_history_state(&mut fixture, history, schedule, vec![local_key]);
    fixture
}

fn install_history_state(
    fixture: &mut Harness,
    history: Vec<BftCommitCertificateV2>,
    schedule: BftHeightProposerScheduleV2,
    signing_keys: Vec<SigningKey>,
) {
    let journal = BftSigningJournalV2::for_schedule(
        &schedule,
        fixture.binding.local_validator_id.clone(),
        schedule
            .active_set()
            .validator(&fixture.binding.local_validator_id)
            .expect("local signer active in fixture history")
            .public_key_hex
            .clone(),
    )
    .unwrap();
    let state = BftSignerStateV2::from_candidate(
        &fixture.binding,
        0,
        history,
        Some(journal),
        None,
        Vec::new(),
        Some(&hex::encode(fixture.provider.verifying_key().to_bytes())),
    )
    .unwrap();
    let binding_digest = fixture.binding.digest().unwrap();
    fixture.backend.0.lock().unwrap().binding_digest = binding_digest.clone();
    fixture.backend.0.lock().unwrap().snapshot.state = state;
    fixture.trust.binding_digest = binding_digest;
    fixture.service = Arc::new(
        BftSignerServiceV2::open(
            fixture.binding.clone(),
            Box::new(TestKey(fixture.provider.clone())),
            signing_keys
                .into_iter()
                .map(|key| Box::new(TestKey(key)) as Box<dyn BftSigningKeyHandleV2>)
                .collect(),
            Box::new(fixture.backend.clone()),
        )
        .unwrap(),
    );
}

fn transition_to_target(
    binding: &BftSignerBindingV2,
    old_key: &SigningKey,
    target_validator_id: &str,
    target_key: &SigningKey,
) -> Vec<BftCommitCertificateV2> {
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        binding.validate().unwrap(),
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    let target = BftValidatorV2 {
        validator_id: target_validator_id.to_string(),
        public_key_hex: hex::encode(target_key.verifying_key().to_bytes()),
        voting_power: 2,
    };
    let target_set_hash = BftValidatorSetV2::new(schedule.scope().clone(), vec![target.clone()])
        .unwrap()
        .set_hash()
        .to_string();
    let mut history = Vec::new();
    let mut confirmation_ref: Option<BftValidatorTransitionRefV2> = None;
    while schedule.height() <= 7 {
        let transition = (schedule.height() == 6).then(|| BftValidatorTransitionV2 {
            source_commit_ref: schedule.parent_commit_ref().to_string(),
            source_state_root: schedule.parent_state_root().to_string(),
            source_profile: schedule.policy().profile_id.clone(),
            source_epoch: schedule.parent_tick() / schedule.policy().epoch_length_ticks,
            target_epoch: 2,
            epoch_length_ticks: schedule.policy().epoch_length_ticks,
            mapping_ruleset_hash: schedule.policy_hash().to_string(),
            scheduled_height: 6,
            activation_height: 8,
            old_validator_set_hash: schedule.active_set().set_hash().to_string(),
            new_validators: vec![target.clone()],
            new_validator_set_hash: target_set_hash.clone(),
            governance_action_digest: super::hash(40),
            projected_registry_digest: super::hash(41),
        });
        let transition_ref = if schedule.height() == 7 {
            confirmation_ref.clone()
        } else {
            None
        };
        let height = schedule.height();
        let block = BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height,
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: super::hash(42),
            actions_cbor: vec![0x80],
            action_root: super::hash(height as u8),
            event_root: super::hash(height as u8 + 1),
            execution_block_hash: super::hash(height as u8 + 2),
            execution_state_root: super::hash(height as u8 + 3),
            state_root: super::hash(height as u8 + 4),
            transition: transition.clone(),
            transition_ref,
        };
        if let Some(transition) = transition {
            confirmation_ref = Some(
                transition
                    .committed_reference(block.block_id().unwrap())
                    .unwrap(),
            );
        }
        let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
        assert_eq!(proposer_id, binding.local_validator_id);
        let block_id = block.block_id().unwrap();
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            height,
            0,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id,
            block_id.clone(),
            None,
            old_key,
        )
        .unwrap();
        let proposal = BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        };
        let vote = BftVoteV2::sign(
            schedule.scope().clone(),
            height,
            0,
            BftVotePhaseV2::Precommit,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            BftVoteValueV2::Block { block_id },
            binding.local_validator_id.clone(),
            old_key,
        )
        .unwrap();
        let certificate = BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits: vec![vote],
        };
        let token = verify_commit_certificate_token_v2(&schedule, &certificate).unwrap();
        history.push(certificate);
        schedule = token.next_height_schedule().clone();
    }
    history
}

fn pending_commit_harness(
    target_validator_id: &str,
    target_key: &SigningKey,
    include_pending_commit: bool,
) -> (Harness, BftCommitCertificateV2) {
    let mut fixture = super::harness();
    fixture.binding.schedule_policy = BftSchedulePolicyV2::new(4, "profile-test").unwrap();
    let old_key = fixture.validator.clone();
    let target_public_key = hex::encode(target_key.verifying_key().to_bytes());
    let rotated = target_validator_id == fixture.binding.local_validator_id;
    let mut signing_keys = vec![old_key.clone()];
    if rotated {
        fixture
            .binding
            .provisioned_signing_keys
            .push(BftSignerKeyIdentityV2 {
                validator_id: target_validator_id.to_string(),
                public_key_hex: target_public_key.clone(),
            });
        fixture
            .binding
            .provisioned_signing_keys
            .sort_by(|left, right| left.public_key_hex.cmp(&right.public_key_hex));
        signing_keys.push(target_key.clone());
    }
    let history = transition_to_target(&fixture.binding, &old_key, target_validator_id, target_key);
    let certificate = history[6].clone();
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        fixture.binding.validate().unwrap(),
        fixture.binding.schedule_policy.clone(),
        fixture.binding.genesis_authority_epoch,
        fixture.binding.genesis_state_root.clone(),
        fixture.binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    for item in &history[..6] {
        schedule = verify_commit_certificate_token_v2(&schedule, item)
            .unwrap()
            .next_height_schedule()
            .clone();
    }
    let mut journal = BftSigningJournalV2::for_schedule(
        &schedule,
        fixture.binding.local_validator_id.clone(),
        hex::encode(old_key.verifying_key().to_bytes()),
    )
    .unwrap();
    let mut started = journal.clone();
    started.height_started_at_ms = Some(100);
    verify_bft_signing_journal_transition_v2(
        &journal,
        &started,
        schedule.active_set(),
        &schedule,
        fixture.binding.timeouts,
    )
    .unwrap();
    journal = started;
    if include_pending_commit {
        let mut pending = journal.clone();
        pending.pending_commit = Some(certificate.clone());
        verify_bft_signing_journal_transition_v2(
            &journal,
            &pending,
            schedule.active_set(),
            &schedule,
            fixture.binding.timeouts,
        )
        .unwrap();
        journal = pending;
    }
    install_history_state(&mut fixture, history[..6].to_vec(), schedule, signing_keys);
    let mut candidate = fixture.backend.snapshot().state.candidate();
    candidate.journal = Some(journal);
    let state = BftSignerStateV2::from_candidate(
        &fixture.binding,
        0,
        history[..6].to_vec(),
        candidate.journal,
        None,
        Vec::new(),
        Some(&hex::encode(fixture.provider.verifying_key().to_bytes())),
    )
    .unwrap();
    fixture.backend.0.lock().unwrap().snapshot.state = state;
    fixture.service = Arc::new(
        BftSignerServiceV2::open(
            fixture.binding.clone(),
            Box::new(TestKey(fixture.provider.clone())),
            if rotated {
                vec![
                    Box::new(TestKey(old_key)),
                    Box::new(TestKey(target_key.clone())),
                ]
            } else {
                vec![Box::new(TestKey(old_key))]
            },
            Box::new(fixture.backend.clone()),
        )
        .unwrap(),
    );
    (fixture, certificate)
}

fn append_over_tcp(
    fixture: &Harness,
    certificate: BftCommitCertificateV2,
) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let result = (|| {
        let mut lease = provider.acquire(&fixture.binding)?;
        let checkpoint = lease.checkpoint()?;
        lease.append_committed_certificate(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            certificate,
        )
    })();
    server.join().unwrap();
    result
}

#[test]
fn late_joiner_can_install_against_a_verified_successor_head() {
    let fixture = late_join_harness(10);
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut lease = provider.acquire(&fixture.binding).unwrap();
    let before = lease.checkpoint().unwrap();
    let BftTransitionInstallPreparationV2::Pending { intent, checkpoint } = lease
        .prepare_transition_install(
            before.state.revision,
            &before.state.state_digest,
            "late-join-source-six".into(),
            6,
            fixture.binding.operator_instance_id.clone(),
        )
        .unwrap()
    else {
        panic!("late-join install should prepare once");
    };
    assert_eq!(intent.body.confirmation_height, 7);
    assert_eq!(intent.body.install_head_height, 10);
    assert_eq!(checkpoint.state.history.len(), 10);
    let evidence = super::install_evidence(&intent);
    let result = lease
        .complete_transition_install(intent.clone(), evidence)
        .unwrap();
    assert_eq!(result.receipt.body.installed_head_height, 10);
    assert_eq!(
        result.receipt.body.installed_head_commit_ref,
        intent.body.install_parent_commit_ref
    );
    assert_eq!(
        result.checkpoint.state.install_receipts,
        vec![result.receipt]
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn removed_local_validator_can_append_exact_reserved_confirmation_then_only_follow() {
    let target_key = super::key(33);
    let (fixture, certificate) = pending_commit_harness("validator-b", &target_key, true);
    let checkpoint = append_over_tcp(&fixture, certificate).unwrap();
    assert_eq!(checkpoint.state.history.len(), 7);
    assert!(checkpoint.state.journal.is_none());
}

#[test]
fn rotated_local_identity_gets_fresh_unacknowledged_journal_but_cannot_sign() {
    let target_key = super::key(34);
    let (fixture, certificate) = pending_commit_harness("validator-a", &target_key, true);
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut lease = provider.acquire(&fixture.binding).unwrap();
    let before = lease.checkpoint().unwrap();
    let installed = lease
        .append_committed_certificate(
            before.state.revision,
            &before.state.state_digest,
            certificate,
        )
        .unwrap();
    let journal = installed.state.journal.as_ref().unwrap();
    assert_eq!(
        journal.local_signing_public_key(),
        hex::encode(target_key.verifying_key().to_bytes())
    );
    let successor = {
        let mut schedule = BftHeightProposerScheduleV2::genesis(
            fixture.binding.validate().unwrap(),
            fixture.binding.schedule_policy.clone(),
            fixture.binding.genesis_authority_epoch,
            fixture.binding.genesis_state_root.clone(),
            fixture.binding.genesis_manifest_hash.clone(),
        )
        .unwrap();
        for item in &installed.state.history {
            schedule = verify_commit_certificate_token_v2(&schedule, item)
                .unwrap()
                .next_height_schedule()
                .clone();
        }
        schedule
    };
    let intent = BftVoteIntentV2 {
        height: successor.height(),
        round: 0,
        phase: BftVotePhaseV2::Prevote,
        value: BftVoteValueV2::Nil,
        validator_id: fixture.binding.local_validator_id.clone(),
        schedule_policy_hash: successor.policy_hash().to_string(),
    };
    assert!(lease.sign_vote(installed.state.revision, &intent).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, installed.state);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn append_without_exact_pending_qc_does_not_advance_removed_or_rotated_identity() {
    let target_key = super::key(35);
    let (fixture, certificate) = pending_commit_harness("validator-b", &target_key, false);
    let initial = fixture.backend.snapshot().state;
    let result = append_over_tcp(&fixture, certificate);
    assert!(result.is_err());
    assert_eq!(fixture.backend.snapshot().state, initial);
}
