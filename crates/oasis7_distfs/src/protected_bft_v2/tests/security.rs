use super::*;
use oasis7_consensus::tendermint_v2::{BftVoteIntentV2, verify_commit_certificate_token_v2};
use oasis7_proto::distributed_bft::v2::{BftVotePhaseV2, BftVoteValueV2};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use crate::protected_bft_v2::install::BftSignedTransitionInstallAckV2;
use crate::protected_bft_v2::state::BftSignerCheckpointV2;

fn acquire_direct(
    stream: &mut TcpStream,
    fixture: &Harness,
    challenge: u8,
) -> (BftSignerLeaseCredentialV2, BftSignerCheckpointV2) {
    let (_, prepared) = super::direct_exchange(
        stream,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
        challenge,
        &fixture.trust,
    );
    let BftSignerReplyKindV2::Prepared {
        expected_generation,
        prepare_nonce,
    } = prepared.body.result
    else {
        panic!("provider should prepare a lease");
    };
    let (_, acquired) = super::direct_exchange(
        stream,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::Acquire {
            binding: Box::new(fixture.binding.clone()),
            expected_generation,
            prepare_nonce,
        },
        challenge + 1,
        &fixture.trust,
    );
    assert!(matches!(
        acquired.body.result,
        BftSignerReplyKindV2::Acquired
    ));
    let token = acquired
        .body
        .lease_token
        .expect("acquire reply should carry a lease token");
    let credential = BftSignerLeaseCredentialV2 {
        generation: acquired.body.lease_generation,
        token,
    };
    let checkpoint = acquired
        .body
        .checkpoint
        .expect("acquire reply should carry a checkpoint");
    (credential, *checkpoint)
}

fn wait_for_released_generation(fixture: &Harness, generation: u64) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let snapshot = fixture.backend.snapshot();
        if snapshot.lease_generation == generation && snapshot.lease_token_digest.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "lease release did not reach backend"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn spawn_tcp_server_allowing_poison(
    service: Arc<BftSignerServiceV2>,
) -> (std::net::SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let result = service.serve_connection(&mut stream);
        assert!(
            result.is_err_and(|error| error.0.contains("provider is poisoned")),
            "only the expected fail-closed poison may escape connection cleanup"
        );
    });
    (address, join)
}

fn first_certificate(
    binding: &BftSignerBindingV2,
    validator_key: &SigningKey,
) -> BftCommitCertificateV2 {
    let schedule = BftHeightProposerScheduleV2::genesis(
        binding.validate().unwrap(),
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
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
        post_manifest_hash: super::hash(111),
        actions_cbor: vec![0x80],
        action_root: super::hash(112),
        event_root: super::hash(113),
        execution_block_hash: super::hash(114),
        execution_state_root: super::hash(115),
        state_root: super::hash(116),
        transition: None,
        transition_ref: None,
    };
    let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
    let block_id = block.block_id().unwrap();
    let proposal = BftProposalV2 {
        header: BftProposalHeaderV2::sign(
            block.scope.clone(),
            height,
            0,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id.clone(),
            block_id.clone(),
            None,
            validator_key,
        )
        .unwrap(),
        block,
        valid_round_evidence: None,
    };
    let precommit = BftVoteV2::sign(
        schedule.scope().clone(),
        height,
        0,
        BftVotePhaseV2::Precommit,
        schedule.active_set().set_hash().to_string(),
        schedule.policy_hash().to_string(),
        BftVoteValueV2::Block { block_id },
        proposer_id,
        validator_key,
    )
    .unwrap();
    let certificate = BftCommitCertificateV2 {
        proposal: Box::new(proposal),
        precommits: vec![precommit],
    };
    verify_commit_certificate_token_v2(&schedule, &certificate).unwrap();
    certificate
}

#[test]
fn old_lease_is_rejected_after_release_and_reacquire() {
    let fixture = harness();
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 3);

    let mut first = TcpStream::connect(address).unwrap();
    let (old_lease, _) = acquire_direct(&mut first, &fixture, 31);
    let mut other_connection = TcpStream::connect(address).unwrap();
    let (_, moved) = super::direct_exchange(
        &mut other_connection,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease.clone()),
        BftSignerCommandV2::Checkpoint,
        41,
        &fixture.trust,
    );
    assert!(matches!(
        moved.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    drop(other_connection);
    drop(first);
    wait_for_released_generation(&fixture, old_lease.generation + 1);

    let mut current = TcpStream::connect(address).unwrap();
    let (new_lease, new_checkpoint) = acquire_direct(&mut current, &fixture, 51);
    assert_eq!(new_lease.generation, old_lease.generation + 2);
    let (_, stale) = super::direct_exchange(
        &mut current,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease),
        BftSignerCommandV2::Checkpoint,
        53,
        &fixture.trust,
    );
    assert!(matches!(
        stale.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    let (_, fresh) = super::direct_exchange(
        &mut current,
        &fixture.binding,
        &fixture.requester,
        Some(new_lease.clone()),
        BftSignerCommandV2::Checkpoint,
        54,
        &fixture.trust,
    );
    assert!(matches!(
        fresh.body.result,
        BftSignerReplyKindV2::Checkpoint
    ));
    assert_eq!(fresh.body.state_digest, new_checkpoint.state.state_digest);
    drop(current);
    server.join().unwrap();
    let stored = fixture.backend.snapshot();
    assert_eq!(stored.lease_generation, new_lease.generation + 1);
    assert!(stored.lease_token_digest.is_empty());
}

#[test]
fn bad_install_ack_preserves_intent_and_install_fences_old_lease() {
    let fixture = installable_harness();
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (old_lease, checkpoint) = acquire_direct(&mut stream, &fixture, 61);
    let (_, prepared) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease.clone()),
        BftSignerCommandV2::PrepareTransitionInstall {
            expected_revision: checkpoint.state.revision,
            expected_state_digest: checkpoint.state.state_digest.clone(),
            request_id: "security-install-ack".into(),
            transition_source_height: 6,
            runtime_instance_id: fixture.binding.operator_instance_id.clone(),
        },
        63,
        &fixture.trust,
    );
    let BftSignerReplyKindV2::InstallPreparedPending { intent } = prepared.body.result else {
        panic!("transition install should persist a pending intent");
    };
    let evidence = super::install_evidence(&intent);
    let ack = BftSignedTransitionInstallAckV2::sign(
        &intent,
        evidence,
        &fixture.binding,
        &fixture.trust.provider_public_key,
        &fixture.requester,
    )
    .unwrap();
    let mut missing_signature = ack.clone();
    missing_signature.requester_signature_hex.clear();
    let before_rejection = fixture.backend.snapshot();
    let (_, rejected) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease.clone()),
        BftSignerCommandV2::CompleteTransitionInstall {
            intent: intent.clone(),
            acknowledgement: Box::new(missing_signature),
        },
        65,
        &fixture.trust,
    );
    assert!(matches!(
        rejected.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    let after_rejection = fixture.backend.snapshot();
    assert_eq!(after_rejection.state, before_rejection.state);
    assert_eq!(after_rejection.state.pending_install, Some(*intent.clone()));

    let (_, installed) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease.clone()),
        BftSignerCommandV2::CompleteTransitionInstall {
            intent: intent.clone(),
            acknowledgement: Box::new(ack),
        },
        67,
        &fixture.trust,
    );
    let BftSignerReplyKindV2::TransitionInstalled { .. } = installed.body.result else {
        panic!("valid local install ACK should complete the transition");
    };
    let rotated = BftSignerLeaseCredentialV2 {
        generation: installed.body.lease_generation,
        token: installed
            .body
            .lease_token
            .expect("installed response rotates the lease token"),
    };
    assert_eq!(rotated.generation, old_lease.generation + 1);
    let (_, stale) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(old_lease),
        BftSignerCommandV2::Checkpoint,
        69,
        &fixture.trust,
    );
    assert!(matches!(
        stale.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    let (_, current) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(rotated.clone()),
        BftSignerCommandV2::Checkpoint,
        70,
        &fixture.trust,
    );
    assert!(matches!(
        current.body.result,
        BftSignerReplyKindV2::Checkpoint
    ));
    drop(stream);
    server.join().unwrap();
    let stored = fixture.backend.snapshot();
    assert_eq!(stored.lease_generation, rotated.generation + 1);
    assert!(stored.lease_token_digest.is_empty());
    assert_eq!(stored.state.install_receipts.len(), 1);
    assert!(stored.state.pending_install.is_none());
}

#[test]
fn rolled_back_state_snapshot_poisons_the_live_service() {
    let fixture = harness();
    let (address, server) = spawn_tcp_server_allowing_poison(fixture.service.clone());
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut lease = provider.acquire(&fixture.binding).unwrap();
    let genesis = lease.checkpoint().unwrap().state;
    let advanced = lease.start_height_timer(genesis.revision).unwrap();
    assert_eq!(advanced.state.revision, genesis.revision + 1);

    fixture.backend.replace_state(genesis.clone());
    let rollback = lease.checkpoint().unwrap_err();
    assert!(rollback.0.contains("rejected"), "{rollback}");
    let poisoned = lease.protected_time_ms().unwrap_err();
    assert!(poisoned.0.contains("rejected"), "{poisoned}");
    assert_eq!(fixture.backend.snapshot().state, genesis);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn committed_cas_with_stale_reply_poisons_without_losing_durable_state() {
    let fixture = harness();
    fixture.backend.return_stale_snapshot_after_next_cas();
    let (address, server) = spawn_tcp_server_allowing_poison(fixture.service.clone());
    let provider = RemoteBftSignerProviderV2::new(
        address,
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut lease = provider.acquire(&fixture.binding).unwrap();
    let error = lease.start_height_timer(0).unwrap_err();
    assert!(error.0.contains("rejected"), "{error}");

    let committed = fixture.backend.snapshot();
    assert_eq!(committed.state.revision, 1);
    assert!(
        committed
            .state
            .journal
            .as_ref()
            .and_then(|journal| journal.height_started_at_ms())
            .is_some()
    );
    let poisoned = lease.checkpoint().unwrap_err();
    assert!(poisoned.0.contains("rejected"), "{poisoned}");
    assert_eq!(fixture.backend.snapshot().state, committed.state);
    drop(lease);
    server.join().unwrap();
}

#[test]
fn standalone_qc_cannot_append_without_the_exact_pending_commit() {
    let fixture = harness();
    let certificate = first_certificate(&fixture.binding, &fixture.validator);
    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let mut stream = TcpStream::connect(address).unwrap();
    let (lease, checkpoint) = acquire_direct(&mut stream, &fixture, 81);
    let (_, reply) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(lease.clone()),
        BftSignerCommandV2::AppendCommittedCertificate {
            expected_revision: checkpoint.state.revision,
            expected_state_digest: checkpoint.state.state_digest,
            certificate: Box::new(certificate),
        },
        83,
        &fixture.trust,
    );
    assert!(matches!(
        reply.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    assert_eq!(
        fixture.backend.snapshot().state.revision,
        checkpoint.state.revision
    );
    assert!(fixture.backend.snapshot().state.history.is_empty());
    let (_, current) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(lease),
        BftSignerCommandV2::Checkpoint,
        84,
        &fixture.trust,
    );
    assert!(matches!(
        current.body.result,
        BftSignerReplyKindV2::Checkpoint
    ));
    drop(stream);
    server.join().unwrap();
}

#[test]
fn inactive_follower_appends_valid_history_but_cannot_sign() {
    let mut fixture = harness();
    let certificate = first_certificate(&fixture.binding, &fixture.validator);
    let inactive_key = super::key(17);
    fixture.binding.local_validator_id = "validator-b".into();
    fixture.binding.provisioned_signing_keys = vec![BftSignerKeyIdentityV2 {
        validator_id: fixture.binding.local_validator_id.clone(),
        public_key_hex: hex::encode(inactive_key.verifying_key().to_bytes()),
    }];
    let binding_digest = fixture.binding.digest().unwrap();
    let state = BftSignerStateV2::genesis(&fixture.binding).unwrap();
    {
        let mut backend = fixture.backend.0.lock().unwrap();
        backend.binding_digest = binding_digest.clone();
        backend.snapshot.state = state;
    }
    fixture.trust.binding_digest = binding_digest;
    fixture.service = Arc::new(
        BftSignerServiceV2::open(
            fixture.binding.clone(),
            Box::new(TestKey(fixture.provider.clone())),
            vec![Box::new(TestKey(inactive_key))],
            Box::new(fixture.backend.clone()),
        )
        .unwrap(),
    );

    let (address, server) = super::spawn_tcp_server(fixture.service.clone(), 1);
    let mut stream = TcpStream::connect(address).unwrap();
    let (lease, checkpoint) = acquire_direct(&mut stream, &fixture, 91);
    let (_, appended) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(lease.clone()),
        BftSignerCommandV2::AppendCommittedCertificate {
            expected_revision: checkpoint.state.revision,
            expected_state_digest: checkpoint.state.state_digest,
            certificate: Box::new(certificate.clone()),
        },
        93,
        &fixture.trust,
    );
    assert!(matches!(
        appended.body.result,
        BftSignerReplyKindV2::CertificateAppended
    ));
    let appended_checkpoint = appended.body.checkpoint.unwrap();
    assert_eq!(appended_checkpoint.state.history, vec![certificate.clone()]);
    assert!(appended_checkpoint.state.journal.is_none());

    let genesis_schedule = BftHeightProposerScheduleV2::genesis(
        fixture.binding.validate().unwrap(),
        fixture.binding.schedule_policy.clone(),
        fixture.binding.genesis_authority_epoch,
        fixture.binding.genesis_state_root.clone(),
        fixture.binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    let successor = verify_commit_certificate_token_v2(&genesis_schedule, &certificate)
        .unwrap()
        .next_height_schedule()
        .clone();
    let intent = BftVoteIntentV2 {
        height: successor.height(),
        round: 0,
        phase: BftVotePhaseV2::Prevote,
        value: BftVoteValueV2::Nil,
        validator_id: fixture.binding.local_validator_id.clone(),
        schedule_policy_hash: successor.policy_hash().to_string(),
    };
    let (_, signing) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(lease.clone()),
        BftSignerCommandV2::SignVote {
            expected_revision: appended_checkpoint.state.revision,
            intent: Box::new(intent),
        },
        94,
        &fixture.trust,
    );
    assert!(matches!(
        signing.body.result,
        BftSignerReplyKindV2::Rejected { .. }
    ));
    let stored = fixture.backend.snapshot().state;
    assert_eq!(stored.revision, appended_checkpoint.state.revision);
    assert_eq!(stored.history, vec![certificate]);
    assert!(stored.journal.is_none());
    let (_, current) = super::direct_exchange(
        &mut stream,
        &fixture.binding,
        &fixture.requester,
        Some(lease),
        BftSignerCommandV2::Checkpoint,
        95,
        &fixture.trust,
    );
    assert!(matches!(
        current.body.result,
        BftSignerReplyKindV2::Checkpoint
    ));
    drop(stream);
    server.join().unwrap();
}

#[test]
fn old_install_receipt_does_not_reactivate_replaced_local_key() {
    let mut fixture = installable_harness();
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
    let checkpoint = lease.checkpoint().unwrap();
    let BftTransitionInstallPreparationV2::Pending { intent, .. } = lease
        .prepare_transition_install(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            "historical-receipt-key-removal".into(),
            6,
            fixture.binding.operator_instance_id.clone(),
        )
        .unwrap()
    else {
        panic!("first transition install must be prepared");
    };
    let completed = lease
        .complete_transition_install(intent.clone(), super::install_evidence(&intent))
        .unwrap();
    let receipt = completed.receipt;
    drop(lease);
    server.join().unwrap();

    let previous = fixture.backend.snapshot().state;
    assert_eq!(previous.install_receipts, vec![receipt]);
    let replacement_key = super::key(19);
    let history = append_removing_transition(
        &fixture.binding,
        &fixture.validator,
        &replacement_key,
        previous.history.clone(),
    );
    let provider_public_key = hex::encode(fixture.provider.verifying_key().to_bytes());
    let inactive_state = BftSignerStateV2::from_candidate(
        &fixture.binding,
        previous.revision + 1,
        history,
        None,
        None,
        previous.install_receipts.clone(),
        Some(&provider_public_key),
    )
    .unwrap();
    assert!(inactive_state.journal.is_none());
    assert_eq!(inactive_state.install_receipts.len(), 1);
    fixture.backend.replace_state(inactive_state.clone());
    fixture.service = Arc::new(
        BftSignerServiceV2::open(
            fixture.binding.clone(),
            Box::new(TestKey(fixture.provider.clone())),
            vec![Box::new(TestKey(fixture.validator.clone()))],
            Box::new(fixture.backend.clone()),
        )
        .unwrap(),
    );

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
    let current = lease.checkpoint().unwrap();
    assert_eq!(current.state, inactive_state);
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        fixture.binding.validate().unwrap(),
        fixture.binding.schedule_policy.clone(),
        fixture.binding.genesis_authority_epoch,
        fixture.binding.genesis_state_root.clone(),
        fixture.binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    for certificate in &inactive_state.history {
        schedule = verify_commit_certificate_token_v2(&schedule, certificate)
            .unwrap()
            .next_height_schedule()
            .clone();
    }
    let intent = BftVoteIntentV2 {
        height: schedule.height(),
        round: 0,
        phase: BftVotePhaseV2::Prevote,
        value: BftVoteValueV2::Nil,
        validator_id: fixture.binding.local_validator_id.clone(),
        schedule_policy_hash: schedule.policy_hash().to_string(),
    };
    let signing = lease
        .sign_vote(current.state.revision, &intent)
        .unwrap_err();
    assert!(signing.0.contains("not active"), "{signing}");
    let unchanged = lease.checkpoint().unwrap();
    assert_eq!(unchanged.state, inactive_state);
    assert_eq!(unchanged.state.install_receipts.len(), 1);
    assert!(unchanged.state.journal.is_none());
    drop(lease);
    server.join().unwrap();
}

fn append_removing_transition(
    binding: &BftSignerBindingV2,
    original_validator_key: &SigningKey,
    replacement_key: &SigningKey,
    mut history: Vec<BftCommitCertificateV2>,
) -> Vec<BftCommitCertificateV2> {
    let mut schedule = BftHeightProposerScheduleV2::genesis(
        binding.validate().unwrap(),
        binding.schedule_policy.clone(),
        binding.genesis_authority_epoch,
        binding.genesis_state_root.clone(),
        binding.genesis_manifest_hash.clone(),
    )
    .unwrap();
    for certificate in &history {
        schedule = verify_commit_certificate_token_v2(&schedule, certificate)
            .unwrap()
            .next_height_schedule()
            .clone();
    }
    assert_eq!(schedule.height(), 8);
    let mut transition_reference = None;
    while schedule.height() <= 11 {
        let transition = (schedule.height() == 10).then(|| {
            let replacement = BftValidatorV2 {
                validator_id: "validator-b".into(),
                public_key_hex: hex::encode(replacement_key.verifying_key().to_bytes()),
                voting_power: 1,
            };
            let new_set = oasis7_consensus::tendermint_v2::BftValidatorSetV2::new(
                schedule.scope().clone(),
                vec![replacement.clone()],
            )
            .unwrap();
            let target_epoch = schedule
                .parent_tick()
                .checked_div(schedule.policy().epoch_length_ticks)
                .unwrap()
                + 1;
            BftValidatorTransitionV2 {
                source_commit_ref: schedule.parent_commit_ref().to_string(),
                source_state_root: schedule.parent_state_root().to_string(),
                source_profile: schedule.policy().profile_id.clone(),
                source_epoch: schedule.parent_tick() / schedule.policy().epoch_length_ticks,
                target_epoch,
                epoch_length_ticks: schedule.policy().epoch_length_ticks,
                mapping_ruleset_hash: schedule.policy_hash().to_string(),
                scheduled_height: 10,
                activation_height: 12,
                old_validator_set_hash: schedule.active_set().set_hash().to_string(),
                new_validators: vec![replacement],
                new_validator_set_hash: new_set.set_hash().to_string(),
                governance_action_digest: super::hash(37),
                projected_registry_digest: super::hash(38),
            }
        });
        let block = BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: super::hash(schedule.height() as u8 + 50),
            actions_cbor: vec![0x80],
            action_root: super::hash(schedule.height() as u8 + 1),
            event_root: super::hash(schedule.height() as u8 + 2),
            execution_block_hash: super::hash(schedule.height() as u8 + 3),
            execution_state_root: super::hash(schedule.height() as u8 + 4),
            state_root: super::hash(schedule.height() as u8 + 5),
            transition: transition.clone(),
            transition_ref: transition_reference.clone(),
        };
        if let Some(transition) = transition {
            transition_reference = Some(
                transition
                    .committed_reference(block.block_id().unwrap())
                    .unwrap(),
            );
        }
        let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
        assert_eq!(proposer_id, binding.local_validator_id);
        let block_id = block.block_id().unwrap();
        let proposal = BftProposalV2 {
            header: BftProposalHeaderV2::sign(
                block.scope.clone(),
                block.height,
                0,
                schedule.active_set().set_hash().to_string(),
                schedule.policy_hash().to_string(),
                proposer_id,
                block_id.clone(),
                None,
                original_validator_key,
            )
            .unwrap(),
            block,
            valid_round_evidence: None,
        };
        let vote = BftVoteV2::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV2::Precommit,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            BftVoteValueV2::Block { block_id },
            binding.local_validator_id.clone(),
            original_validator_key,
        )
        .unwrap();
        let certificate = BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits: vec![vote],
        };
        schedule = verify_commit_certificate_token_v2(&schedule, &certificate)
            .unwrap()
            .next_height_schedule()
            .clone();
        history.push(certificate);
    }
    assert_eq!(schedule.height(), 12);
    assert!(
        schedule
            .active_set()
            .validator(&binding.local_validator_id)
            .is_none()
    );
    history
}
