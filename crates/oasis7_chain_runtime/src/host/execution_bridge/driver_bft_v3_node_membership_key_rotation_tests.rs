//! Actual admitted-node key rotation, including the Node's queued typed proof
//! consumer and native protected signer. Backend/issuer/clock remain explicit
//! test doubles; no physical custody or independent persistence is claimed.
use super::*;
use oasis7_agent_api::world_action::{
    FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
};
use oasis7_consensus::tendermint_v3::{
    BftPublicProposalV3, replay_bft_signing_journal_with_effects_v3,
};
use oasis7_proto::distributed_bft::v3::{
    BftHeaderCommitCertificateV3, BftProposalHeaderV3, BftVotePhaseV3, BftVoteV3,
};
use std::sync::atomic::{AtomicUsize, Ordering};

type KeyProofs = (FinalityKeyRotationProofV1, FinalityKeyRotationProofV1);

fn native_service(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    disabled: Option<Arc<AtomicUsize>>,
) -> Arc<BftSignerServiceV3> {
    struct Handle {
        public: String,
        calls: Arc<AtomicUsize>,
    }
    impl BftSigningKeyHandleV3 for Handle {
        fn public_key_hex(&self) -> String {
            self.public.clone()
        }
        fn sign_preimage(&self, _: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(BftSignerErrorV3(
                "test original finality handles disabled".into(),
            ))
        }
    }
    let keys: Vec<Box<dyn BftSigningKeyHandleV3>> = [123, 124]
        .into_iter()
        .map(|seed| match &disabled {
            Some(calls) => Box::new(Handle {
                public: hex::encode(fixtures::key(seed).verifying_key().to_bytes()),
                calls: calls.clone(),
            }) as Box<dyn BftSigningKeyHandleV3>,
            None => {
                Box::new(MemoryTestSignerKey(fixtures::key(seed))) as Box<dyn BftSigningKeyHandleV3>
            }
        })
        .collect();
    Arc::new(
        BftSignerServiceV3::open_with_candidate_read_proofs(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(203))),
            keys,
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
                fail_confirm_after_commit: None,
            }),
            BftCandidateReadPolicyV3::STANDARD,
        )
        .unwrap(),
    )
}
fn three_sessions(service: Arc<BftSignerServiceV3>) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(120)))
                .unwrap();
            service.serve_connection(&mut stream).unwrap();
        }
    });
    (address, server)
}
fn make_body(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    tip: &VerifiedBftHeaderCertificateV3,
) -> FinalityKeyRotationBodyV1 {
    let driver = actual.lock().unwrap();
    let mut world = driver.world().clone();
    world
        .attach_formal_bft_header_successor_parent_v3(driver.handoff(), tip)
        .unwrap();
    let authorization = world.governance_authorization_for_next_commit(4).unwrap();
    FinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization,
        rotation_id: "actual-node4-finality-generation1".into(),
        candidate_id: "new-formal-validator".into(),
        node_id: validator_id(3),
        expected_generation: 0,
        old_public_key_hex: hex::encode(fixtures::key(123).verifying_key().to_bytes()),
        new_public_key_hex: hex::encode(fixtures::key(124).verifying_key().to_bytes()),
        decision_height: 7,
        activation_height: 9,
        target_epoch: 3,
    }
}
fn preview_world(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    schedule: &BftHeaderHeightScheduleV3,
    payloads: Vec<(u64, Vec<u8>)>,
) -> Result<RuntimeWorld, String> {
    actual
        .lock()
        .unwrap()
        .preview_candidate_world_for_test(NodeBftBuildContextV3 {
            execution: NodeBftExecutionContextV3::from_schedule(schedule),
            actions: payloads
                .into_iter()
                .map(|(id, payload)| {
                    oasis7_node::NodeConsensusAction::from_payload(id, validator_id(0), payload)
                        .unwrap()
                })
                .collect(),
        })
}

fn signed_rotation(
    body: &FinalityKeyRotationBodyV1,
    old: FinalityKeyRotationProofV1,
    new: FinalityKeyRotationProofV1,
    id: u64,
) -> (u64, Vec<u8>) {
    let action = RuntimeAction::RotateGovernanceValidatorFinalityKeyV1 {
        controller_account_id: body.authorization.controller_account_id.clone(),
        body: body.clone(),
        old_key_proof: old,
        new_key_proof: new,
    };
    let mut auth = body.authorization.clone();
    let key = fixtures::key(102);
    auth.signatures.insert(
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(key.sign(&auth.signing_bytes(&action).unwrap()).to_bytes()),
    );
    (
        id,
        encode_consensus_action_payload(
            &ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
                action,
                ConsensusActionAuthEnvelope::GovernanceAction(auth),
            ),
        )
        .unwrap(),
    )
}
/// Only Node3 runs consensus. Other real Noise endpoints accept public frames,
/// but have no Node worker, cannot vote and cannot advance the current parent.
fn node_proofs(
    root: &Scratch,
    h: &QualifiedH<BftProfileTargetV2>,
    actual: Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    address: SocketAddr,
    body: &FinalityKeyRotationBodyV1,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
) -> (KeyProofs, KeyProofs) {
    assert_eq!(body.decision_height, 7);
    let networks = (0..4)
        .map(|_| {
            Arc::new(oasis7_net::Libp2pNetwork::new(
                oasis7_net::Libp2pNetworkConfig {
                    listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
                    ..Default::default()
                },
            ))
        })
        .collect::<Vec<_>>();
    network_wait("rotation Noise listeners", || {
        networks.iter().all(|n| !n.listening_addrs().is_empty())
    });
    for index in 0..3 {
        networks[3]
            .dial(networks[index].listening_addrs()[0].clone())
            .unwrap();
    }
    network_wait("rotation Noise peers", || {
        networks[3].connected_peers().len() == 3
    });
    let context =
        oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(binding.handoff());
    let transports = networks
        .iter()
        .enumerate()
        .map(|(index, n)| {
            Arc::new(
                NodeBftLibp2pPeerTransportV3::new(
                    n.clone(),
                    context.clone(),
                    networks
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != index)
                        .map(|(_, n)| n.peer_id().to_string())
                        .collect(),
                )
                .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let state = Arc::new(NodeBftPeerExecutionStateV3::default());
    let hook: NodeBftSharedExecutionHookV3 = Arc::new(Mutex::new(Box::new(ActualHook(actual))));
    let gates = crate::node_lifecycle::network_gates_for_count(h.trust.initial_trust.clone(), 4);
    let exchange = Arc::new(
        NodeBftLibp2pPrivateCandidateExchangeV3::new(
            networks[3].clone(),
            NodeBftPrivateExchangeConfigV3 {
                binding: binding.clone(),
                candidate_policy_digest: gates.policy_digest,
                candidate_grant_generation: 1,
                peers: networks[..3]
                    .iter()
                    .map(|n| n.peer_id().to_string())
                    .collect(),
                operation_timeout: Duration::from_secs(60),
            },
            state.clone(),
            hook.clone(),
            gates.gates[3].clone(),
            Some(Arc::new(Credentials {
                grant: gates.committed_grants[3].clone(),
                key: fixtures::key(133),
            })),
        )
        .unwrap(),
    );
    let config = NodeConfig::new(
        validator_id(3),
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_secs(2))
    .unwrap()
    .with_replication(
        NodeReplicationConfig::new(root.path().join("rotation-replication")).unwrap(),
    );
    let mut node = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .try_with_shared_execution_hook(hook)
        .unwrap()
        .with_tendermint_v3_membership_network(
            binding.clone(),
            Arc::new(remote_at(binding, address)),
            transports[3].clone(),
            exchange,
            state.clone(),
        )
        .unwrap();
    node.start().unwrap();
    network_wait("rotation Node publishes its actual replayed parent", || {
        let status = node.bft_v3_status().unwrap();
        assert!(status.running, "{status:?}");
        status.committed_height == 6
            && state
                .view()
                .unwrap()
                .is_some_and(|view| view.schedule.height() == 7)
    });
    let view = state.view().unwrap().unwrap();
    assert_eq!(view.schedule.height(), 7);
    assert_ne!(
        view.schedule.expected_proposer_for_round(0).unwrap(),
        validator_id(3),
        "manual fixture must not race an original local proposal"
    );
    let mut wrong = body.clone();
    wrong.rotation_id = "wrong-actual-parent".into();
    wrong.authorization.parent_record_id = "a".repeat(64);
    assert!(
        node.request_bft_v3_finality_key_rotation_proofs(wrong, Duration::from_secs(120))
            .is_err()
    );
    assert!(node.bft_v3_status().unwrap().running);
    let mut duplicate = body.clone();
    duplicate.rotation_id = "duplicate-active-finality-key".into();
    duplicate.new_public_key_hex = hex::encode(fixtures::key(121).verifying_key().to_bytes());
    assert!(
        node.request_bft_v3_finality_key_rotation_proofs(duplicate, Duration::from_secs(120))
            .is_err()
    );
    assert!(node.bft_v3_status().unwrap().running);
    let valid = node
        .request_bft_v3_finality_key_rotation_proofs(body.clone(), Duration::from_secs(120))
        .unwrap();
    // A caller conflict is a local rejection, not an unknown provider outcome.
    // Neither the worker nor the protected original result may be poisoned.
    let original_state_bytes = serde_cbor::to_vec(&snapshot.lock().unwrap().state).unwrap();
    let mut conflict = body.clone();
    conflict.candidate_id = "same-rotation-id-conflicting-admission".into();
    conflict.validate().unwrap();
    assert!(
        node.request_bft_v3_finality_key_rotation_proofs(conflict, Duration::from_secs(120))
            .is_err()
    );
    assert!(node.bft_v3_status().unwrap().running);
    assert_eq!(
        serde_cbor::to_vec(&snapshot.lock().unwrap().state).unwrap(),
        original_state_bytes
    );
    let retried = node
        .request_bft_v3_finality_key_rotation_proofs(body.clone(), Duration::from_secs(120))
        .unwrap();
    assert_eq!(retried, valid);
    assert!(node.bft_v3_status().unwrap().running);
    assert_eq!(
        serde_cbor::to_vec(&snapshot.lock().unwrap().state).unwrap(),
        original_state_bytes
    );

    let mut wrong_generation = body.clone();
    wrong_generation.rotation_id = "rejected-generation-candidate".into();
    wrong_generation.expected_generation = 9;
    let incorrect = node
        .request_bft_v3_finality_key_rotation_proofs(wrong_generation, Duration::from_secs(120))
        .unwrap();
    assert_eq!(node.bft_v3_status().unwrap().committed_height, 6);
    node.stop().unwrap();
    (valid, incorrect)
}
fn reserve_vote(
    lease: &mut dyn BftSignerLeaseV3,
    cp: &mut BftSignerCheckpointV3,
    intent: BftVoteIntentV3,
) -> BftVoteV3 {
    let mut next = cp.state.candidate();
    next.journal.as_mut().unwrap().pending_vote_intent = Some(intent.clone());
    *cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, next)
        .unwrap();
    let (vote, after) = lease.sign_vote(cp.state.revision, &intent).unwrap();
    *cp = after;
    vote
}
fn next_vote(
    binding: &BftSignerBindingV3,
    cp: &BftSignerCheckpointV3,
    schedule: &BftHeaderHeightScheduleV3,
    phase: BftVotePhaseV3,
) -> BftVoteIntentV3 {
    let (_, effects) = replay_bft_signing_journal_with_effects_v3(
        cp.state.journal.as_ref().unwrap(),
        schedule.active_set(),
        schedule,
        binding.timeouts,
    )
    .unwrap();
    effects
        .into_iter()
        .find_map(|e| match e {
            BftConsensusActionV3::SignVote(v) if v.phase == phase => Some(v),
            _ => None,
        })
        .expect("real proposal/tally must derive the exact new-key vote")
}
/// Produce the decisive precommit with the provider's actually active key,
/// rather than a fixture-only key.sign call. Two other validators sign separately.
fn commit_with_provisioned_key(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    cp: &mut BftSignerCheckpointV3,
    schedule: &mut BftHeaderHeightScheduleV3,
    action_id: u64,
    marker: &str,
) -> VerifiedBftHeaderCertificateV3 {
    let expected_public = schedule
        .active_set()
        .validator(&binding.local_validator_id)
        .unwrap()
        .public_key_hex
        .clone();
    let private = candidate(actual, schedule, vec![(action_id, payload(marker))]).unwrap();
    let execution = NodeBftExecutionContextV3::from_schedule(schedule);
    actual
        .lock()
        .unwrap()
        .retain_bft_v3_private_candidate(&execution, &private)
        .unwrap();
    *cp = lease.start_height_timer(cp.state.revision).unwrap();
    let journal = cp.state.journal.as_ref().unwrap();
    let machine =
        replay_bft_signing_journal_v3(journal, schedule.active_set(), schedule, binding.timeouts)
            .unwrap();
    let proposer = schedule.expected_proposer_for_round(0).unwrap();
    let block_id = private.public_value.block_id().unwrap();
    let proposal = if proposer == validator_id(3) {
        let intent = machine
            .proposal_intent(private.public_value.clone(), &binding.local_validator_id)
            .unwrap();
        let mut next = cp.state.candidate();
        next.journal.as_mut().unwrap().pending_proposal_intent = Some(intent.clone());
        *cp = lease
            .advance(cp.state.revision, &cp.state.state_digest, next)
            .unwrap();
        let (p, after) = lease.sign_proposal(cp.state.revision, &intent).unwrap();
        *cp = after;
        p
    } else {
        let index = (0..3).find(|i| validator_id(*i) == proposer).unwrap();
        let header = BftProposalHeaderV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            proposer.clone(),
            block_id.clone(),
            None,
            &fixtures::key(120 + index),
        )
        .unwrap();
        let p = BftPublicProposalV3 {
            header,
            block: private.public_value.clone(),
            valid_round_evidence: None,
        };
        let now = lease.protected_time_ms().unwrap();
        let mut next = cp.state.candidate();
        next.journal
            .as_mut()
            .unwrap()
            .events
            .push(BftSigningJournalEventV3::Proposal {
                proposal: Box::new(p.clone()),
                local: false,
                now_ms: now,
            });
        *cp = lease
            .advance(cp.state.revision, &cp.state.state_digest, next)
            .unwrap();
        p
    };
    let intent = next_vote(binding, cp, schedule, BftVotePhaseV3::Prevote);
    let prevote = reserve_vote(lease, cp, intent);
    assert_eq!(prevote.public_key_hex, expected_public);
    for index in [1, 2] {
        let vote = BftVoteV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV3::Prevote,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            BftVoteValueV3::Block {
                block_id: block_id.clone(),
            },
            validator_id(index),
            &fixtures::key(120 + index),
        )
        .unwrap();
        let now = lease.protected_time_ms().unwrap();
        let mut next = cp.state.candidate();
        next.journal
            .as_mut()
            .unwrap()
            .events
            .push(BftSigningJournalEventV3::Vote {
                vote: Box::new(vote),
                local: false,
                now_ms: now,
            });
        *cp = lease
            .advance(cp.state.revision, &cp.state.state_digest, next)
            .unwrap();
    }
    let intent = next_vote(binding, cp, schedule, BftVotePhaseV3::Precommit);
    let decisive = reserve_vote(lease, cp, intent);
    assert_eq!(decisive.public_key_hex, expected_public);
    decisive.verify_signature().unwrap();
    let mut precommits = [1, 2]
        .into_iter()
        .map(|index| {
            BftVoteV3::sign(
                schedule.scope().clone(),
                schedule.height(),
                0,
                BftVotePhaseV3::Precommit,
                schedule.active_set().set_hash().into(),
                schedule.policy_hash().into(),
                BftVoteValueV3::Block {
                    block_id: block_id.clone(),
                },
                validator_id(index),
                &fixtures::key(120 + index),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    precommits.push(decisive);
    let certified = FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal: proposal.header,
            valid_round_evidence: None,
            precommits,
        },
        private.public_value.header,
        private.public_value.execution_commitment,
    )
    .unwrap();
    let token = verify_commit_certificate_token_v3(schedule, &certified).unwrap();
    let mut next = cp.state.candidate();
    next.journal.as_mut().unwrap().pending_commit = Some(certified.clone());
    *cp = lease
        .advance(cp.state.revision, &cp.state.state_digest, next)
        .unwrap();
    *cp = lease
        .append_committed_certificate(cp.state.revision, &cp.state.state_digest, certified)
        .unwrap();
    let result = actual
        .lock()
        .unwrap()
        .commit_bft_v3_certificate(token.clone())
        .unwrap();
    *cp = lease
        .confirm_applied(
            cp.state.revision,
            &cp.state.state_digest,
            result.height,
            &result.header_id,
            &result.driver_install_digest,
        )
        .unwrap();
    *schedule = token.next_height_schedule().clone();
    token
}
#[test]
fn wire3_actual_node_typed_rotation_changes_only_finality_key_and_installs_new_key_before_decisive_qc()
 {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(actual_rotation)
        .unwrap()
        .join()
        .unwrap();
}
fn actual_rotation() {
    let h = handoff();
    let root = Scratch::new();
    let store = cas(root.path());
    let mut actual = open_actual(&h, &root, &store);
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
    let (address, server) = three_sessions(native_service(&binding, &snapshot, None));
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
        vec![(10404, payload("actual-rotation-agent-4"))],
    );
    let (result, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10405, payload("actual-rotation-agent-5"))],
    );
    install(&binding, &mut *lease, &mut cp, &schedule, &result, 4);
    let tip6 = commit_with_provisioned_key(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        10406,
        "actual-rotation-agent-6",
    );
    let original_old_vote = cp
        .state
        .completed_journals
        .iter()
        .flatten()
        .flat_map(|j| j.events.iter())
        .find_map(|event| match event {
            BftSigningJournalEventV3::Vote {
                vote, local: true, ..
            } if vote.public_key_hex
                == hex::encode(fixtures::key(123).verifying_key().to_bytes()) =>
            {
                Some(vote.as_ref().clone())
            }
            _ => None,
        })
        .expect("native old key must have a durable original consensus signature");
    let original_old_intent = BftVoteIntentV3 {
        height: original_old_vote.height,
        round: original_old_vote.round,
        phase: original_old_vote.phase,
        value: original_old_vote.value.clone(),
        validator_id: original_old_vote.validator_id.clone(),
        schedule_policy_hash: original_old_vote.schedule_policy_hash.clone(),
    };
    assert_eq!(schedule.height(), 7);
    assert_ne!(
        schedule.expected_proposer_for_round(0).unwrap(),
        validator_id(3)
    );
    let body = make_body(&actual, &tip6);
    body.validate().unwrap();
    let immutable = {
        let d = actual.lock().unwrap();
        (
            d.world().state().governance_validator_admissions["new-formal-validator"].clone(),
            d.world()
                .node_identity_public_key(&validator_id(3))
                .unwrap()
                .to_owned(),
            d.world()
                .node_main_token_account(&validator_id(3))
                .unwrap()
                .to_owned(),
        )
    };
    drop(lease);
    let ((old, new), (bad_generation_old, bad_generation_new)) = node_proofs(
        &root,
        &h,
        actual.clone(),
        &binding,
        address,
        &body,
        &snapshot,
    );
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    cp = lease.checkpoint().unwrap();
    assert_eq!(
        cp.state.history.last().unwrap().execution_commitment.height,
        6
    );
    assert_eq!(
        cp.state
            .finality_rotation_proofs
            .as_ref()
            .unwrap()
            .records
            .len(),
        4
    );
    let mut wrong_generation = body.clone();
    wrong_generation.rotation_id = "rejected-generation-candidate".into();
    wrong_generation.expected_generation = 9;
    let mut wrong_old = old.clone();
    wrong_old.signature_hex = "0".repeat(128);
    let rejected_gen = signed_rotation(
        &wrong_generation,
        bad_generation_old,
        bad_generation_new,
        10470,
    );
    let rejected_proof = signed_rotation(&body, wrong_old, new.clone(), 10471);
    let accepted = signed_rotation(&body, old.clone(), new.clone(), 10472);
    let (journal_len, original_world_identity) = {
        let driver = actual.lock().unwrap();
        (
            driver.world().journal().events.len(),
            driver.world().execution_state_identity().unwrap(),
        )
    };
    let original_provider = lease.checkpoint().unwrap().state;
    // These are detached previews of the actual qualified-parent pipeline,
    // not claims that the rejected actions were canonically committed.
    for (negative, category) in [
        (rejected_gen.clone(), "rotation generation"),
        (rejected_proof.clone(), "rotation key proof rejected"),
    ] {
        let preview = preview_world(&actual, &schedule, vec![negative]).unwrap();
        assert!(preview.state().governance_finality_key_rotations.is_empty());
        assert_eq!(
            preview.state().governance_authorization_nonces["formal-bft-controller"],
            3
        );
        assert_eq!(
            serde_cbor::to_vec(
                &preview.state().governance_validator_admissions["new-formal-validator"]
            )
            .unwrap(),
            serde_cbor::to_vec(&immutable.0).unwrap()
        );
        assert_eq!(
            preview.node_identity_public_key(&validator_id(3)),
            Some(immutable.1.as_str())
        );
        assert_eq!(
            preview.node_main_token_account(&validator_id(3)),
            Some(immutable.2.as_str())
        );
        let reasons = preview.journal().events[journal_len..]
            .iter()
            .filter_map(|event| match &event.body {
                oasis7_runtime::WorldEventBody::Domain(
                    oasis7_runtime::DomainEvent::ActionRejected { reason, .. },
                ) => Some(format!("{reason:?}")),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            reasons.iter().any(|reason| reason.contains(category)),
            "actual detached rejection category {category}: {reasons:?}"
        );
        let driver = actual.lock().unwrap();
        assert_eq!(
            driver.world().execution_state_identity().unwrap(),
            original_world_identity
        );
        assert!(
            driver
                .world()
                .state()
                .governance_finality_key_rotations
                .is_empty()
        );
        assert_eq!(
            driver.world().state().governance_authorization_nonces["formal-bft-controller"],
            3
        );
        drop(driver);
        assert_eq!(lease.checkpoint().unwrap().state, original_provider);
    }
    let repeated = preview_world(
        &actual,
        &schedule,
        vec![rejected_gen, rejected_proof, accepted.clone()],
    )
    .unwrap_err();
    assert!(
        repeated.contains("governance action batch has a repeated or descending nonce"),
        "{repeated}"
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .world()
            .execution_state_identity()
            .unwrap(),
        original_world_identity
    );
    assert_eq!(lease.checkpoint().unwrap().state, original_provider);
    let (_, tip7) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![accepted],
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
    assert_eq!(
        schedule
            .active_set()
            .validator(&validator_id(3))
            .unwrap()
            .public_key_hex,
        body.old_public_key_hex
    );
    {
        let d = actual.lock().unwrap();
        let w = d.world();
        assert_eq!(w.state().governance_finality_key_rotations.len(), 1);
        let record = &w.state().governance_finality_key_rotations[&body.rotation_id];
        let (b, o, n) = record.typed_parts().unwrap();
        assert_eq!(b, &body);
        assert_eq!(o, &old);
        assert_eq!(n, &new);
        assert_eq!(b.next_generation().unwrap(), 1);
        assert_eq!(
            w.state().governance_authorization_nonces["formal-bft-controller"],
            4
        );
    }
    let (result, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(10408, payload("actual-rotation-agent-8"))],
    );
    assert_eq!(schedule.height(), 9);
    assert_eq!(
        schedule
            .active_set()
            .validator(&validator_id(3))
            .unwrap()
            .public_key_hex,
        body.new_public_key_hex
    );
    assert_eq!(
        cp.state
            .journal
            .as_ref()
            .unwrap()
            .local_signing_public_key(),
        body.new_public_key_hex
    );
    assert!(lease.start_height_timer(cp.state.revision).is_err());
    install(&binding, &mut *lease, &mut cp, &schedule, &result, 7);
    let tip9 = commit_with_provisioned_key(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        10409,
        "actual-finality-key124-agent",
    );
    assert_eq!(tip9.certificate().precommits.len(), 3);
    assert!(
        tip9.certificate()
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(3)
                && v.public_key_hex == body.new_public_key_hex)
    );
    assert!(
        !tip9
            .certificate()
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(0))
    );
    {
        let d = actual.lock().unwrap();
        let w = d.world();
        assert_eq!(
            serde_cbor::to_vec(&w.state().governance_validator_admissions["new-formal-validator"])
                .unwrap(),
            serde_cbor::to_vec(&immutable.0).unwrap()
        );
        assert_eq!(
            w.node_identity_public_key(&validator_id(3)),
            Some(immutable.1.as_str())
        );
        assert_eq!(
            w.node_main_token_account(&validator_id(3)),
            Some(immutable.2.as_str())
        );
        let effective = w
            .resolve_governance_effective_finality_signer_registry()
            .unwrap()
            .unwrap();
        assert_eq!(
            effective.signer_bindings[&validator_id(3)],
            body.new_public_key_hex
        );
        assert!(
            w.state()
                .agents
                .contains_key("actual-finality-key124-agent")
        );
    }
    let current = cp.state.clone();
    let (retried, unchanged) = lease
        .sign_vote(current.revision, &original_old_intent)
        .unwrap();
    assert_eq!(retried, original_old_vote);
    assert_eq!(unchanged.state, current);

    let mut fresh = make_body(&actual, &tip9);
    fresh.rotation_id = "fresh-exited-old-key".into();
    fresh.authorization.nonce = 5;
    fresh.decision_height = 10;
    fresh.activation_height = 12;
    fresh.target_epoch = 4;
    assert!(
        lease
            .sign_finality_rotation_proof(
                current.revision,
                &current.state_digest,
                &fresh,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, current);
    drop(lease);
    server.join().unwrap();
    drop(actual);
    actual = open_actual(&h, &root, &store);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let mut replay = binding.initial_schedule().clone();
    for certified in &current.history {
        let token = verify_commit_certificate_token_v3(&replay, certified).unwrap();
        let result = actual
            .lock()
            .unwrap()
            .commit_bft_v3_certificate(token.clone())
            .unwrap();
        let receipt = current
            .applied_installs
            .iter()
            .find(|r| r.height == result.height)
            .unwrap();
        assert_eq!(receipt.driver_install_digest, result.driver_install_digest);
        replay = token.next_height_schedule().clone();
    }
    assert_eq!(replay, schedule);
    let calls = Arc::new(AtomicUsize::new(0));
    let (address, server) = tcp(native_service(&binding, &snapshot, Some(calls.clone())));
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    for (role, expected) in [
        (FinalityKeyRotationRoleV1::OldKeyAuthorization, old),
        (FinalityKeyRotationRoleV1::NewKeyPossession, new),
    ] {
        let (proof, after) = lease
            .sign_finality_rotation_proof(current.revision, &current.state_digest, &body, role)
            .unwrap();
        assert_eq!(proof, expected);
        assert_eq!(after.state, current);
    }
    let (retried, unchanged) = lease
        .sign_vote(current.revision, &original_old_intent)
        .unwrap();
    assert_eq!(retried, original_old_vote);
    assert_eq!(unchanged.state, current);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        lease
            .sign_finality_rotation_proof(
                current.revision,
                &current.state_digest,
                &fresh,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "exited old key must be rejected before private-key access"
    );
    drop(lease);
    server.join().unwrap().unwrap();
}
