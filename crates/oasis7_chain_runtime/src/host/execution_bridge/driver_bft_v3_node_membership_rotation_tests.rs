//! Actual exit and retained original-key signatures. Different-key reentry is
//! intentionally rejected by current World identity ownership; this does NOT
//! claim an implemented governance rotation. Backend/clock are memory doubles.
use super::super::super::super::super::super::formal_header::FormalBftHeaderCandidateV3;
use super::super::super::super::execution_tests::certify;
use super::fault_cases::{open_actual, remote_at};
use super::*;
use oasis7_consensus::tendermint_v3::{
    BftConsensusActionV3, BftSigningJournalEventV3, BftVoteIntentV3, replay_bft_signing_journal_v3,
    verify_bft_signing_journal_transition_v3, verify_commit_certificate_token_v3,
};
use oasis7_proto::distributed_bft::v3::BftVoteValueV3;

fn signed_governance(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    tip: &VerifiedBftHeaderCertificateV3,
    action: RuntimeAction,
    nonce: u64,
    id: u64,
) -> Vec<(u64, Vec<u8>)> {
    let driver = actual.lock().unwrap();
    let mut world = driver.world().clone();
    world
        .attach_formal_bft_header_successor_parent_v3(driver.handoff(), tip)
        .unwrap();
    let mut auth = world
        .governance_authorization_for_next_commit(nonce)
        .unwrap();
    let key = fixtures::key(102);
    auth.signatures.insert(
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(key.sign(&auth.signing_bytes(&action).unwrap()).to_bytes()),
    );
    vec![(
        id,
        encode_consensus_action_payload(
            &ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
                action,
                ConsensusActionAuthEnvelope::GovernanceAction(auth),
            ),
        )
        .unwrap(),
    )]
}
fn candidate(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    schedule: &BftHeaderHeightScheduleV3,
    payloads: Vec<(u64, Vec<u8>)>,
) -> Result<NodeBftPrivateCandidateV3, String> {
    actual
        .lock()
        .unwrap()
        .build_bft_v3_candidate(NodeBftBuildContextV3 {
            execution: NodeBftExecutionContextV3::from_schedule(schedule),
            actions: payloads
                .into_iter()
                .map(|(id, p)| {
                    oasis7_node::NodeConsensusAction::from_payload(id, validator_id(0), p).unwrap()
                })
                .collect(),
        })
}
fn commit(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    checkpoint: &mut BftSignerCheckpointV3,
    schedule: &mut BftHeaderHeightScheduleV3,
    payloads: Vec<(u64, Vec<u8>)>,
) -> (
    NodeBftExecutionCommitResultV3,
    VerifiedBftHeaderCertificateV3,
) {
    let context = NodeBftExecutionContextV3::from_schedule(schedule);
    let private = candidate(actual, schedule, payloads).unwrap();
    actual
        .lock()
        .unwrap()
        .retain_bft_v3_private_candidate(&context, &private)
        .unwrap();
    let certificate = certify(
        schedule,
        &FormalBftHeaderCandidateV3 {
            header: private.public_value.header.clone(),
            commitment: private.public_value.execution_commitment.clone(),
        },
    );
    let token = verify_commit_certificate_token_v3(schedule, &certificate).unwrap();
    if checkpoint.state.journal.is_some() {
        *checkpoint = lease.start_height_timer(checkpoint.state.revision).unwrap();
        let old = checkpoint.state.journal.as_ref().unwrap();
        let mut journal = old.clone();
        journal.pending_commit = Some(certificate.clone());
        verify_bft_signing_journal_transition_v3(
            old,
            &journal,
            schedule.active_set(),
            schedule,
            binding.timeouts,
        )
        .unwrap();
        let mut next = checkpoint.state.candidate();
        next.journal = Some(journal);
        *checkpoint = lease
            .advance(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                next,
            )
            .unwrap();
    }
    *checkpoint = lease
        .append_committed_certificate(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            certificate,
        )
        .unwrap();
    let result = actual
        .lock()
        .unwrap()
        .commit_bft_v3_certificate(token.clone())
        .unwrap();
    *checkpoint = lease
        .confirm_applied(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            result.height,
            &result.header_id,
            &result.driver_install_digest,
        )
        .unwrap();
    *schedule = token.next_height_schedule().clone();
    (result, token)
}
fn install(
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    checkpoint: &mut BftSignerCheckpointV3,
    schedule: &BftHeaderHeightScheduleV3,
    tip: &NodeBftExecutionCommitResultV3,
    source: u64,
) {
    let prepared = lease
        .prepare_transition_install(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            format!("actual-install-source-{source}"),
            source,
            binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV3::Pending {
        intent,
        checkpoint: pending,
    } = prepared
    else {
        panic!("new installation must have original pending transaction")
    };
    let evidence = BftLocalInstallEvidenceV3 {
        schema_version: 3,
        runtime_instance_id: binding.operator_instance_id.clone(),
        installed_head_height: tip.height,
        installed_head_commit_ref: tip.header_id.clone(),
        installed_state_root: tip.state_root.clone(),
        installed_manifest_hash: tip.post_manifest_hash.clone(),
        installed_active_set_hash: schedule.active_set().set_hash().into(),
        installed_authority_epoch: schedule.authority_epoch(),
        local_validator_id: binding.local_validator_id.clone(),
        local_signing_public_key: schedule
            .active_set()
            .validator(&binding.local_validator_id)
            .unwrap()
            .public_key_hex
            .clone(),
        wal_install_digest: pending.state.journal.as_ref().unwrap().digest().unwrap(),
        driver_install_digest: tip.driver_install_digest.clone(),
    };
    *checkpoint = lease
        .complete_transition_install(intent, evidence)
        .unwrap()
        .checkpoint;
}
fn sign_original_nil(
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    checkpoint: &mut BftSignerCheckpointV3,
    schedule: &BftHeaderHeightScheduleV3,
) -> (
    BftVoteIntentV3,
    oasis7_proto::distributed_bft::v3::BftVoteV3,
) {
    *checkpoint = lease.start_height_timer(checkpoint.state.revision).unwrap();
    let old = checkpoint.state.journal.as_ref().unwrap().clone();
    let mut machine =
        replay_bft_signing_journal_v3(&old, schedule.active_set(), schedule, binding.timeouts)
            .unwrap();
    let origin = old.height_started_at_ms.unwrap();
    let mut now = lease.protected_time_ms().unwrap();
    for _ in 0..16 {
        if now >= origin + binding.timeouts.propose_ms as i64 {
            break;
        }
        now = lease.protected_time_ms().unwrap();
    }
    let effects = machine
        .on_timeout(now, &binding.local_validator_id)
        .unwrap();
    let intent = effects
        .into_iter()
        .find_map(|effect| {
            if let BftConsensusActionV3::SignVote(v) = effect {
                Some(v)
            } else {
                None
            }
        })
        .expect("actual expired propose timer derives a nil intent");
    assert_eq!(intent.value, BftVoteValueV3::Nil);
    let mut journal = old;
    journal
        .events
        .push(BftSigningJournalEventV3::Timeout { now_ms: now });
    let mut next = checkpoint.state.candidate();
    next.journal = Some(journal);
    *checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            next,
        )
        .unwrap();
    let mut next = checkpoint.state.candidate();
    next.journal.as_mut().unwrap().pending_vote_intent = Some(intent.clone());
    *checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            next,
        )
        .unwrap();
    let (vote, after) = lease.sign_vote(checkpoint.state.revision, &intent).unwrap();
    *checkpoint = after;
    vote.verify_signature().unwrap();
    assert_eq!(
        vote.public_key_hex,
        hex::encode(fixtures::key(123).verifying_key().to_bytes())
    );
    (intent, vote)
}
#[test]
fn wire3_membership_actual_exit_preserves_old_nil_original_and_rejects_unowned_replacement_key() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(exit_case)
        .unwrap()
        .join()
        .unwrap();
}
fn exit_case() {
    let h = handoff();
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = open_actual(&h, &root, &store);
    let mut binding = binding_for(&actual, 3);
    binding
        .provisioned_signing_keys
        .push(BftSignerKeyIdentityV3 {
            validator_id: validator_id(3),
            public_key_hex: hex::encode(fixtures::key(124).verifying_key().to_bytes()),
        });
    binding
        .provisioned_signing_keys
        .sort_by(|a, b| a.public_key_hex.cmp(&b.public_key_hex));
    binding.timeouts = oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
        propose_ms: 2,
        prevote_ms: 2,
        precommit_ms: 2,
        round_delta_ms: 1,
    };
    binding.validate().unwrap();
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
    let service = Arc::new(
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(203))),
            vec![
                Box::new(MemoryTestSignerKey(fixtures::key(123))),
                Box::new(MemoryTestSignerKey(fixtures::key(124))),
            ],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
                fail_confirm_after_commit: None,
            }),
        )
        .unwrap(),
    );
    let (address, server) = tcp(service);
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    let initial = lease.checkpoint().unwrap();
    let mut cp = lease
        .enable_membership(initial.state.revision, &initial.state.state_digest)
        .unwrap();
    let mut schedule = binding.initial_schedule().clone();
    let admission = admission_payloads(&actual.lock().unwrap());
    commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        admission,
    );
    commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10104, payload("actual-exit-agent-4"))],
    );
    let (result, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10105, payload("actual-exit-agent-5"))],
    );
    assert_eq!(schedule.height(), 6);
    install(&binding, &mut *lease, &mut cp, &schedule, &result, 4);
    let (nil_intent, nil_vote) = sign_original_nil(&binding, &mut *lease, &mut cp, &schedule);
    let (_, tip6) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10106, payload("actual-exit-agent-6"))],
    );
    let revoke = signed_governance(
        &actual,
        &tip6,
        RuntimeAction::RevokeGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "new-formal-validator".into(),
            node_id: validator_id(3),
            reason: "actual membership exit".into(),
        },
        4,
        10107,
    );
    let (_, tip7) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        revoke,
    );
    assert_eq!(
        tip7.bound_header()
            .commitment()
            .transition
            .as_ref()
            .unwrap()
            .activation_height,
        9
    );
    let (_, tip8) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10108, payload("actual-exit-agent-8"))],
    );
    assert_eq!(schedule.height(), 9);
    assert!(schedule.active_set().validator(&validator_id(3)).is_none());
    assert!(cp.state.journal.is_none());
    let exited = cp.state.clone();
    assert!(lease.start_height_timer(exited.revision).is_err());
    let mut forbidden = nil_intent.clone();
    forbidden.height = 9;
    forbidden.schedule_policy_hash = schedule.policy_hash().into();
    assert!(lease.sign_vote(exited.revision, &forbidden).is_err());
    let (retried, unchanged) = lease.sign_vote(exited.revision, &nil_intent).unwrap();
    assert_eq!(retried, nil_vote);
    assert_eq!(unchanged.state, exited);
    let replacement = signed_governance(
        &actual,
        &tip8,
        RuntimeAction::SubmitGovernanceValidatorAdmission {
            controller_account_id: "formal-bft-controller".into(),
            candidate_id: "replacement-validator-key124".into(),
            node_id: validator_id(3),
            finality_signer_public_key: hex::encode(fixtures::key(124).verifying_key().to_bytes()),
            stake: 1,
            operator_owner: "same-owner-new-finality-key".into(),
            public_manifest_hash: "replacement-manifest".into(),
        },
        5,
        10109,
    );
    let (old_identity, old_account, journal_len) = {
        let driver = actual.lock().unwrap();
        (
            driver
                .world()
                .node_identity_public_key(&validator_id(3))
                .unwrap()
                .to_owned(),
            driver
                .world()
                .node_main_token_account(&validator_id(3))
                .unwrap()
                .to_owned(),
            driver.world().journal().events.len(),
        )
    };
    let original_receipts = cp
        .state
        .membership_install
        .as_ref()
        .unwrap()
        .receipts
        .clone();
    let (rejected_result, rejected_token) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        replacement,
    );
    assert_eq!(rejected_result.height, 9);
    assert!(
        rejected_token
            .bound_header()
            .commitment()
            .transition
            .is_none()
    );
    assert!(
        rejected_token
            .bound_header()
            .commitment()
            .transition_ref
            .is_none()
    );
    {
        let driver = actual.lock().unwrap();
        let world = driver.world();
        let reasons = world.journal().events[journal_len..]
            .iter()
            .filter_map(|event| match &event.body {
                oasis7_runtime::WorldEventBody::Domain(
                    oasis7_runtime::DomainEvent::ActionRejected { reason, .. },
                ) => Some(format!("{reason:?}")),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            reasons
                .iter()
                .any(|reason| reason.contains("node identity binding mismatch")),
            "actual rejected replacement reason: {reasons:?}"
        );
        assert_eq!(
            world.node_identity_public_key(&validator_id(3)),
            Some(old_identity.as_str())
        );
        assert_eq!(
            world.node_main_token_account(&validator_id(3)),
            Some(old_account.as_str())
        );
        assert!(
            !world
                .state()
                .governance_validator_admissions
                .contains_key("replacement-validator-key124")
        );
        assert_eq!(
            world.state().governance_validator_admissions["new-formal-validator"].status,
            GovernanceValidatorAdmissionStatus::Revoked
        );
    }
    assert!(schedule.active_set().validator(&validator_id(3)).is_none());
    assert!(cp.state.journal.is_none());
    assert_eq!(
        cp.state.membership_install.as_ref().unwrap().receipts,
        original_receipts
    );
    let (continued, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10110, payload("actual-follower-after-exit"))],
    );
    assert_eq!(continued.height, 10);
    assert!(cp.state.journal.is_none());
    assert_eq!(
        cp.state.membership_install.as_ref().unwrap().receipts.len(),
        1
    );
    let mut verified = binding.initial_schedule().clone();
    for qc in &cp.state.history {
        verified = verify_commit_certificate_token_v3(&verified, qc)
            .unwrap()
            .next_height_schedule()
            .clone();
    }
    assert_eq!(verified, schedule);
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("actual-follower-after-exit")
    );
    let (retried, after) = lease.sign_vote(cp.state.revision, &nil_intent).unwrap();
    assert_eq!(retried, nil_vote);
    assert_eq!(after.state, cp.state);
    drop(lease);
    server.join().unwrap().unwrap();
}
