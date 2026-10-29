//! Bootstrap-source integration through actual World/driver, Node proof worker,
//! native TCP provider and decisive protected-key votes. Installation below uses
//! the provider API directly: this is not a continuously running Node-install test.
//! Other quorum signatures, issuer witness, backend and clock are test fixtures;
//! no independent physical fault domains or non-exportable custody are claimed.
use super::key_rotation_cases::{commit_with_provisioned_key, preview_world};
use super::*;
use oasis7_agent_api::world_action::{
    BootstrapFinalityKeyRotationBodyV1 as Body, BootstrapFinalityKeyRotationProofV1 as Proof,
    BootstrapFinalityKeyRotationRoleV1 as Role,
};
use std::sync::atomic::{AtomicUsize, Ordering};
type Proofs = (Proof, Proof);
fn public(seed: u8) -> String {
    hex::encode(fixtures::key(seed).verifying_key().to_bytes())
}
pub(super) fn handoff_bootstrap() -> QualifiedH<BftProfileTargetV2> {
    qualified_handoff_v3_with_count_and_setup(4, |world| {
        let mut policy = world.governance_execution_policy().clone();
        policy.epoch_length_ticks = 3;
        world.set_governance_execution_policy(policy).unwrap();
        world
            .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
                slot_id: "governance.finality.v1".into(),
                threshold: 3,
                signer_bindings: (0..4).map(|i| (validator_id(i), public(120 + i))).collect(),
                validator_stakes: (0..4).map(|i| (validator_id(i), 1)).collect(),
                ..Default::default()
            })
            .unwrap();
        assert!(world.state().governance_validator_admissions.is_empty());
    })
}
fn service(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    disabled: Option<Arc<AtomicUsize>>,
) -> Arc<BftSignerServiceV3> {
    struct Disabled(String, Arc<AtomicUsize>);
    impl BftSigningKeyHandleV3 for Disabled {
        fn public_key_hex(&self) -> String {
            self.0.clone()
        }
        fn sign_preimage(&self, _: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
            self.1.fetch_add(1, Ordering::SeqCst);
            Err(BftSignerErrorV3("disabled cold-reopen handle".into()))
        }
    }
    let keys =
        [123, 124, 125]
            .into_iter()
            .map(|seed| match &disabled {
                Some(calls) => Box::new(Disabled(public(seed), calls.clone()))
                    as Box<dyn BftSigningKeyHandleV3>,
                None => Box::new(MemoryTestSignerKey(fixtures::key(seed)))
                    as Box<dyn BftSigningKeyHandleV3>,
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
pub(super) fn make_body(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    tip: &VerifiedBftHeaderCertificateV3,
    generation: u64,
    decision: u64,
) -> Body {
    let driver = actual.lock().unwrap();
    let mut world = driver.world().clone();
    world
        .attach_formal_bft_header_successor_parent_v3(driver.handoff(), tip)
        .unwrap();
    let raw = world
        .state()
        .governance_finality_signer_registry
        .as_ref()
        .unwrap();
    Body {
        schema_version: 1,
        authorization: world
            .governance_authorization_for_next_commit(generation + 1)
            .unwrap(),
        rotation_id: format!("bootstrap-node4-generation-{}", generation + 1),
        node_id: validator_id(3),
        bootstrap_registry_digest_hex: oasis7_world_state::bootstrap_finality_registry_digest_v1(
            raw,
        )
        .unwrap(),
        bootstrap_public_key_hex: public(123),
        expected_generation: generation,
        old_public_key_hex: public(123 + generation as u8),
        new_public_key_hex: public(124 + generation as u8),
        decision_height: decision,
        activation_height: decision + 2,
        target_epoch: (decision + 2) / 3,
    }
}
pub(super) fn signed(body: &Body, (old, new): Proofs, id: u64) -> (u64, Vec<u8>) {
    let action = RuntimeAction::RotateGovernanceBootstrapFinalityKeyV1 {
        controller_account_id: body.authorization.controller_account_id.clone(),
        body: body.clone(),
        old_key_proof: old,
        new_key_proof: new,
    };
    let mut auth = body.authorization.clone();
    let key = fixtures::key(102);
    auth.signatures.insert(
        public(102),
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
fn node_proofs(
    root: &Scratch,
    h: &QualifiedH<BftProfileTargetV2>,
    actual: Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    address: SocketAddr,
    body: &Body,
) -> Proofs {
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
        status.committed_height + 1 == body.decision_height
            && state
                .view()
                .unwrap()
                .is_some_and(|view| view.schedule.height() == body.decision_height)
    });
    let view = state.view().unwrap().unwrap();
    assert_eq!(view.schedule.height(), body.decision_height);
    assert_ne!(
        view.schedule.expected_proposer_for_round(0).unwrap(),
        validator_id(3),
        "manual fixture must not race an original local proposal"
    );

    let result = node
        .request_bft_v3_bootstrap_finality_key_rotation_proofs(
            body.clone(),
            Duration::from_secs(30),
        )
        .unwrap();
    let retry = node
        .request_bft_v3_bootstrap_finality_key_rotation_proofs(
            body.clone(),
            Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(retry, result);
    assert!(node.bft_v3_status().unwrap().running);
    node.stop().unwrap();
    result
}

#[test]
fn wire3_bootstrap_two_generations_real_node_proofs_driver_install_decisive_qc_and_cold_continuation()
 {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(actual_bootstrap_rotation)
        .unwrap()
        .join()
        .unwrap();
}
fn actual_bootstrap_rotation() {
    let h = handoff_bootstrap();
    let root = Scratch::new();
    let store = cas(root.path());
    let mut actual = open_actual(&h, &root, &store);
    let mut binding = binding_for(&actual, 3);
    for seed in [124, 125] {
        binding
            .provisioned_signing_keys
            .push(BftSignerKeyIdentityV3 {
                validator_id: validator_id(3),
                public_key_hex: public(seed),
            });
    }
    binding
        .provisioned_signing_keys
        .sort_by(|a, b| a.public_key_hex.cmp(&b.public_key_hex));
    binding.validate().unwrap();
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let immutable = {
        let d = actual.lock().unwrap();
        let w = d.world();
        assert!(w.state().governance_validator_admissions.is_empty());
        (
            serde_cbor::to_vec(&w.state().governance_finality_signer_registry).unwrap(),
            serde_cbor::to_vec(&w.state().governance_validator_admissions).unwrap(),
            w.node_identity_public_key(&validator_id(3))
                .map(str::to_owned),
            w.node_main_token_account(&validator_id(3))
                .map(str::to_owned),
        )
    };
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        successor_archive_transfer: None,
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    // Five bounded sessions: initial producer, two Node proof workers and two
    // resumed producers. The same backend clock floor survives every reopen.
    let (address, server) = provider_sessions(service(&binding, &snapshot, None), 5);
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    let initial = lease.checkpoint().unwrap();
    let mut cp = lease
        .enable_membership(initial.state.revision, &initial.state.state_digest)
        .unwrap();
    let mut schedule = binding.initial_schedule().clone();
    assert_eq!(schedule.height(), 3);
    let mut tip = None;
    while schedule.height() < 7 {
        let height = schedule.height();
        tip = Some(
            commit(
                &actual,
                &binding,
                &mut *lease,
                &mut cp,
                &mut schedule,
                vec![(
                    21000 + height,
                    payload(&format!("bootstrap-before-{height}")),
                )],
            )
            .1,
        );
    }
    let body = make_body(&actual, &tip.unwrap(), 0, 7);
    body.validate().unwrap();
    drop(lease);
    let proofs = node_proofs(&root, &h, actual.clone(), &binding, address, &body);
    let mut lease = provider.acquire(&binding).unwrap();
    cp = lease.checkpoint().unwrap();
    let original_provider = cp.state.clone();
    let original_identity = actual
        .lock()
        .unwrap()
        .world()
        .execution_state_identity()
        .unwrap();
    let original_journal_len = actual.lock().unwrap().world().journal().events.len();
    // Authentic controller signatures over bad input do not make that input
    // authoritative. Detached previews exercise the actual frozen-parent path.
    for field in 0..4 {
        let mut bad = body.clone();
        match field {
            0 => bad.bootstrap_registry_digest_hex = "c".repeat(64),
            1 => bad.bootstrap_public_key_hex = public(122),
            2 => bad.expected_generation = 9,
            _ => bad.new_public_key_hex = public(121),
        }
        // Sign each changed full typed body with the exact fixture old/new key;
        // this isolates World source/generation/key checks from proof tampering.
        let pair = (
            Proof {
                schema_version: 1,
                role: Role::OldKeyAuthorization,
                signature_hex: hex::encode(
                    fixtures::key(123)
                        .sign(&bad.signing_bytes(Role::OldKeyAuthorization).unwrap())
                        .to_bytes(),
                ),
            },
            Proof {
                schema_version: 1,
                role: Role::NewKeyPossession,
                signature_hex: hex::encode(
                    fixtures::key(if field == 3 { 121 } else { 124 })
                        .sign(&bad.signing_bytes(Role::NewKeyPossession).unwrap())
                        .to_bytes(),
                ),
            },
        );
        let preview =
            preview_world(&actual, &schedule, vec![signed(&bad, pair, 21100 + field)]).unwrap();
        assert!(
            preview
                .state()
                .governance_bootstrap_finality_key_rotations
                .is_empty()
        );
        assert_eq!(
            preview
                .state()
                .governance_authorization_nonces
                .get("formal-bft-controller"),
            None
        );
        assert!(
            preview.journal().events[original_journal_len..]
                .iter()
                .any(|e| matches!(
                    &e.body,
                    oasis7_runtime::WorldEventBody::Domain(
                        oasis7_runtime::DomainEvent::ActionRejected { .. }
                    )
                ))
        );
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .world()
                .execution_state_identity()
                .unwrap(),
            original_identity
        );
        assert_eq!(lease.checkpoint().unwrap().state, original_provider);
    }
    let accepted = signed(&body, proofs.clone(), 21207);
    assert!(
        preview_world(
            &actual,
            &schedule,
            vec![accepted.clone(), signed(&body, proofs.clone(), 21208)]
        )
        .unwrap_err()
        .contains("repeated or descending nonce")
    );
    assert_eq!(
        actual
            .lock()
            .unwrap()
            .world()
            .execution_state_identity()
            .unwrap(),
        original_identity
    );
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
        public(123)
    );
    let (installed, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(21208, payload("bootstrap-confirm-first"))],
    );
    assert_eq!(schedule.height(), 9);
    assert_eq!(
        schedule
            .active_set()
            .validator(&validator_id(3))
            .unwrap()
            .public_key_hex,
        public(124)
    );
    assert!(lease.start_height_timer(cp.state.revision).is_err());
    // Actual provider install/ACK from the real driver receipt, not a claim
    // that the stopped Node's membership worker performed this installation.
    install(&binding, &mut *lease, &mut cp, &schedule, &installed, 7);
    let tip9 = commit_with_provisioned_key(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        21209,
        "bootstrap-first-new-key",
    );
    assert!(
        tip9.certificate()
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(3) && v.public_key_hex == public(124))
    );
    assert_eq!(tip9.certificate().precommits.len(), 3);
    assert!(
        !tip9
            .certificate()
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(0))
    );
    // The proof-only worker must not race its own original proposal. Choose
    // the next exact epoch boundary whose round-zero proposer is another node,
    // advancing real canonical headers rather than delaying a decision in place.
    let mut second_tip = tip9;
    for _ in 0..4 {
        if schedule.expected_proposer_for_round(0).unwrap() != validator_id(3) {
            break;
        }
        for _ in 0..3 {
            let height = schedule.height();
            second_tip = commit_with_provisioned_key(
                &actual,
                &binding,
                &mut *lease,
                &mut cp,
                &mut schedule,
                22000 + height,
                &format!("bootstrap-between-{height}"),
            );
        }
    }
    assert_ne!(
        schedule.expected_proposer_for_round(0).unwrap(),
        validator_id(3)
    );
    let second_height = schedule.height();
    let second = make_body(&actual, &second_tip, 1, second_height);
    assert_eq!(
        second.bootstrap_registry_digest_hex,
        body.bootstrap_registry_digest_hex
    );
    assert_eq!(
        second.bootstrap_public_key_hex,
        body.bootstrap_public_key_hex
    );
    assert_eq!(second.old_public_key_hex, body.new_public_key_hex);
    drop(lease);
    let second_proofs = node_proofs(&root, &h, actual.clone(), &binding, address, &second);
    let mut lease = provider.acquire(&binding).unwrap();
    cp = lease.checkpoint().unwrap();
    commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![signed(&second, second_proofs.clone(), 21210)],
    );
    assert_eq!(
        schedule
            .active_set()
            .validator(&validator_id(3))
            .unwrap()
            .public_key_hex,
        public(124)
    );
    let (installed, _) = commit(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        vec![(21211, payload("bootstrap-confirm-second"))],
    );
    assert_eq!(schedule.height(), second_height + 2);
    assert_eq!(
        schedule
            .active_set()
            .validator(&validator_id(3))
            .unwrap()
            .public_key_hex,
        public(125)
    );
    install(
        &binding,
        &mut *lease,
        &mut cp,
        &schedule,
        &installed,
        second_height,
    );
    let tip12 = commit_with_provisioned_key(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        21212,
        "bootstrap-second-new-key",
    );
    assert_eq!(tip12.certificate().precommits.len(), 3);
    assert!(
        tip12
            .certificate()
            .precommits
            .iter()
            .any(|v| v.validator_id == validator_id(3) && v.public_key_hex == public(125))
    );
    {
        let d = actual.lock().unwrap();
        let w = d.world();
        assert_eq!(
            w.state().governance_bootstrap_finality_key_rotations.len(),
            2
        );
        assert_eq!(
            serde_cbor::to_vec(&w.state().governance_finality_signer_registry).unwrap(),
            immutable.0
        );
        assert_eq!(
            serde_cbor::to_vec(&w.state().governance_validator_admissions).unwrap(),
            immutable.1
        );
        assert_eq!(
            w.node_identity_public_key(&validator_id(3)),
            immutable.2.as_deref()
        );
        assert_eq!(
            w.node_main_token_account(&validator_id(3)),
            immutable.3.as_deref()
        );
        assert_eq!(
            w.state().governance_authorization_nonces["formal-bft-controller"],
            2
        );
    }
    for journal in cp
        .state
        .completed_journals
        .iter()
        .flatten()
        .filter(|j| j.height >= 9)
    {
        assert_eq!(
            journal.local_signing_public_key(),
            public(if journal.height >= second_height + 2 {
                125
            } else {
                124
            })
        );
        for event in &journal.events {
            if let BftSigningJournalEventV3::Vote {
                vote, local: true, ..
            } = event
            {
                assert_ne!(vote.public_key_hex, public(123));
            }
        }
    }
    let current = cp.state.clone();
    drop(lease);
    server.join().unwrap().unwrap();
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
        assert_eq!(
            current
                .applied_installs
                .iter()
                .find(|r| r.height == result.height)
                .unwrap()
                .driver_install_digest,
            result.driver_install_digest
        );
        replay = token.next_height_schedule().clone();
    }
    assert_eq!(replay, schedule);
    let calls = Arc::new(AtomicUsize::new(0));
    let (address, server) = provider_sessions(service(&binding, &snapshot, Some(calls.clone())), 1);
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    for (original, pair) in [(&body, proofs), (&second, second_proofs)] {
        for (role, expected) in [
            (Role::OldKeyAuthorization, pair.0),
            (Role::NewKeyPossession, pair.1),
        ] {
            let (found, after) = lease
                .sign_bootstrap_finality_rotation_proof(
                    current.revision,
                    &current.state_digest,
                    original,
                    role,
                )
                .unwrap();
            assert_eq!(found, expected);
            assert_eq!(after.state, current);
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    drop(lease);
    server.join().unwrap().unwrap();
    // Re-enable current custody and perform a new legal canonical commit after
    // cold recovery, using the retained clock floor and original WAL state.
    let (address, server) = provider_sessions(service(&binding, &snapshot, None), 1);
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    cp = lease.checkpoint().unwrap();
    let tip13 = commit_with_provisioned_key(
        &actual,
        &binding,
        &mut *lease,
        &mut cp,
        &mut schedule,
        21213,
        "bootstrap-cold-continued",
    );
    assert_eq!(tip13.certificate().proposal.height, second_height + 3);
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("bootstrap-cold-continued")
    );
    drop(lease);
    server.join().unwrap().unwrap();
}
