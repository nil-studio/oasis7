//! Object cold recovery with actual World retention and a queued Noise QC.
//! The weighted other two validators form a real QC without forging Node0's key.
use super::*;
use oasis7_consensus::tendermint_v3::{
    BftPeerContextV3, BftPublicPeerMessageV3, BftSigningJournalEventV3, BftVoteIntentV3,
    verify_commit_certificate_token_v3,
};
use oasis7_node::{NodeBftPeerTransportV3, NodeBftReceivedPublicV3};
use oasis7_proto::distributed_bft::v3::BftVotePhaseV3;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
fn prevote(bytes: &[u8]) -> bool {
    bytes
        .windows(b"oasis7.bft.vote.v3.sign".len())
        .any(|w| w == b"oasis7.bft.vote.v3.sign")
        && bytes.windows(7).any(|w| w == b"prevote")
}
struct FailFirstPrevote {
    key: SigningKey,
    failed: AtomicBool,
}
impl BftSigningKeyHandleV3 for FailFirstPrevote {
    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if prevote(bytes) && !self.failed.swap(true, Ordering::SeqCst) {
            return Err(BftSignerErrorV3(
                "injected original prevote signing interruption".into(),
            ));
        }
        Ok(self.key.sign(bytes).to_bytes())
    }
}
pub(super) fn initial_key(index: u8, pending: bool) -> Box<dyn BftSigningKeyHandleV3> {
    if pending && index == 0 {
        Box::new(FailFirstPrevote {
            key: fixtures::key(120),
            failed: AtomicBool::new(false),
        })
    } else {
        Box::new(MemoryTestSignerKey(fixtures::key(120 + index)))
    }
}
pub(super) fn weighted_handoff() -> QualifiedH<BftProfileTargetV2> {
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
pub(super) fn retain_original(
    actual: &[Arc<Mutex<FormalBftHeaderNodeDriverV3>>],
    bindings: &[BftSignerBindingV3],
) {
    let proposer = bindings[0]
        .initial_schedule()
        .expected_proposer_for_round(0)
        .unwrap();
    let proposer = bindings
        .iter()
        .position(|b| b.local_validator_id == proposer)
        .unwrap();
    assert_ne!(proposer, 0, "failed validator must not be the proposer");
    let context = NodeBftExecutionContextV3::from_schedule(bindings[proposer].initial_schedule());
    let mut d = actual[proposer].lock().unwrap();
    let candidate = d
        .build_bft_v3_candidate(NodeBftBuildContextV3 {
            execution: context.clone(),
            actions: vec![
                oasis7_node::NodeConsensusAction::from_payload(
                    9500,
                    validator_id(proposer as u8),
                    payload("actual-noise-network-agent"),
                )
                .unwrap(),
            ],
        })
        .unwrap();
    d.retain_bft_v3_private_candidate(&context, &candidate)
        .unwrap();
    let package = d
        .export_bft_v3_private_candidate(&candidate.public_value)
        .unwrap();
    drop(d);
    for (i, d) in actual.iter().enumerate() {
        if i != proposer {
            d.lock()
                .unwrap()
                .retain_received_bft_v3_private_candidate(&context, &package)
                .unwrap();
        }
    }
}
struct ColdKey {
    key: SigningKey,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
    intent: BftVoteIntentV3,
    calls: Arc<AtomicUsize>,
}
impl BftSigningKeyHandleV3 for ColdKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        if prevote(bytes) {
            let snapshot = self.snapshot.lock().unwrap();
            if snapshot
                .state
                .journal
                .as_ref()
                .is_some_and(|j| j.height == self.intent.height)
            {
                assert!(
                    snapshot.state.history.is_empty(),
                    "original slot must finish before queued QC append"
                );
                assert_eq!(
                    snapshot
                        .state
                        .journal
                        .as_ref()
                        .unwrap()
                        .pending_vote_intent
                        .as_ref(),
                    Some(&self.intent)
                );
                self.calls.fetch_add(1, Ordering::SeqCst);
            }
        }
        Ok(self.key.sign(bytes).to_bytes())
    }
}
struct BufferedNoise {
    inner: Arc<NodeBftLibp2pPeerTransportV3>,
    received: Mutex<Vec<NodeBftReceivedPublicV3>>,
}
impl NodeBftPeerTransportV3 for BufferedNoise {
    fn context(&self) -> &BftPeerContextV3 {
        self.inner.context()
    }
    fn peers(&self) -> Vec<String> {
        self.inner.peers()
    }
    fn broadcast_public(&self, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        self.inner.broadcast_public(m)
    }
    fn send_public(&self, p: &str, m: &BftPublicPeerMessageV3) -> Result<(), String> {
        self.inner.send_public(p, m)
    }
    fn drain_public(&self, max: usize) -> Result<Vec<NodeBftReceivedPublicV3>, String> {
        let mut original = self.received.lock().unwrap();
        let count = max.min(original.len());
        if count > 0 {
            return Ok(original.drain(..count).collect());
        }
        self.inner.drain_public(max)
    }
}
#[test]
fn wire3_formal_read_cold_pending_prevote_finishes_before_queued_sameheight_noise_qc() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| network_case_pending(false, false, false, true))
        .unwrap()
        .join()
        .unwrap();
}
#[expect(
    clippy::too_many_arguments,
    reason = "cold object replacement keeps independent actual owners explicit"
)]
pub(super) fn recover(
    nodes: &mut Vec<NodeRuntime>,
    servers: &mut Vec<thread::JoinHandle<Result<(), String>>>,
    actual: &mut Vec<Arc<Mutex<FormalBftHeaderNodeDriverV3>>>,
    public: &mut Vec<Arc<NodeBftLibp2pPeerTransportV3>>,
    exchanges: &mut Vec<Arc<NodeBftLibp2pPrivateCandidateExchangeV3>>,
    views: &mut Vec<Arc<NodeBftPeerExecutionStateV3>>,
    networks: &[Arc<oasis7_net::Libp2pNetwork>],
    bindings: &[BftSignerBindingV3],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    roots: &[Scratch],
    stores: &[LocalCasStore],
    h: &QualifiedH<BftProfileTargetV2>,
    gates: &crate::node_lifecycle::NetworkGates,
    barriers: &[Arc<fetch_cases::BarrierGate>],
    delivery: &Arc<AtomicBool>,
    drains: &[Arc<AtomicU64>],
    received: &[Arc<AtomicU64>],
) {
    network_wait(
        "original prevote failure and independent weighted QC",
        || {
            if nodes[0].bft_v3_status().unwrap().running {
                return false;
            }
            !snapshots[1].lock().unwrap().state.history.is_empty()
                && !snapshots[2].lock().unwrap().state.history.is_empty()
        },
    );
    delivery.store(false, Ordering::Release);
    let before = snapshots[0].lock().unwrap().state.clone();
    assert!(before.history.is_empty());
    let original = before
        .journal
        .as_ref()
        .unwrap()
        .pending_vote_intent
        .clone()
        .expect("original pending prevote must survive signing failure");
    assert_eq!(original.phase, BftVotePhaseV3::Prevote);
    let certificate = snapshots[1].lock().unwrap().state.history[0].clone();
    assert_eq!(certificate.execution_commitment.height, original.height);
    let token =
        verify_commit_certificate_token_v3(bindings[0].initial_schedule(), &certificate).unwrap();
    assert_eq!(token.bound_header().commitment().height(), original.height);
    assert!(
        certificate
            .certificate
            .precommits
            .iter()
            .all(|vote| vote.validator_id != validator_id(0)),
        "QC must come from the other native validators"
    );
    let path = roots[0]
        .path()
        .join("ledger/formal-bft-header-prefix-v1/node-private-v1")
        .join(format!(
            "candidate-{:020}-{}.cbor",
            original.height,
            certificate.header.block_id().unwrap()
        ));
    let bytes = fs::read(&path).unwrap();
    let mut failed = nodes.remove(0);
    failed.stop().unwrap();
    drop(failed);
    servers.remove(0).join().unwrap().unwrap();
    drop(exchanges.remove(0));
    drop(public.remove(0));
    drop(views.remove(0));
    drop(actual.remove(0));
    let cold = Arc::new(Mutex::new(
        FormalBftHeaderNodeDriverV3::open_from_handoff_with_peer_network(
            &roots[0].path().join("ledger"),
            stores[0].clone(),
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            Arc::new(EmptyIndexReader),
        )
        .unwrap(),
    ));
    assert_eq!(
        binding_for(&cold, 0).digest().unwrap(),
        bindings[0].digest().unwrap()
    );
    actual.insert(0, cold.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Arc::new(
        BftSignerServiceV3::open_with_candidate_read_proofs(
            bindings[0].clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(200))),
            vec![Box::new(ColdKey {
                key: fixtures::key(120),
                snapshot: snapshots[0].clone(),
                intent: original.clone(),
                calls: calls.clone(),
            })],
            Box::new(MemoryTestSignerBackend {
                binding_digest: bindings[0].digest().unwrap(),
                fail_confirm_after_commit: None,
                snapshot: snapshots[0].clone(),
            }),
            BftCandidateReadPolicyV3::STANDARD,
        )
        .unwrap(),
    );
    let (address, server) = tcp(service);
    servers.insert(0, server);
    let routes = networks
        .iter()
        .skip(1)
        .map(|n| n.peer_id().to_string())
        .collect::<Vec<_>>();
    let transport = Arc::new(
        NodeBftLibp2pPeerTransportV3::new(
            networks[0].clone(),
            BftPeerContextV3::from_handoff(cold.lock().unwrap().handoff()),
            routes.clone(),
        )
        .unwrap(),
    );
    // Obtain the QC through actual Noise before startup. Buffer only the
    // received data/peer identity; the Node must still fully verify it.
    public[0]
        .send_public(
            &networks[0].peer_id().to_string(),
            &BftPublicPeerMessageV3::Commit(Box::new(certificate.clone())),
        )
        .unwrap();
    let mut inbox = Vec::new();
    network_wait("sameheight QC actually received before cold start", || {
        inbox.extend(transport.drain_public(128).unwrap());
        inbox.iter().any(
            |item| matches!(&item.message,BftPublicPeerMessageV3::Commit(c) if **c==certificate),
        )
    });
    assert!(
        inbox
            .iter()
            .any(|item| item.peer_id == networks[1].peer_id().to_string())
    );
    let buffered = Arc::new(BufferedNoise {
        inner: transport.clone(),
        received: Mutex::new(inbox),
    });
    public.insert(0, transport);
    let execution = Arc::new(NodeBftPeerExecutionStateV3::default());
    let hook: NodeBftSharedExecutionHookV3 = Arc::new(Mutex::new(Box::new(ActualHook(cold))));
    let exchange = Arc::new(
        NodeBftLibp2pPrivateCandidateExchangeV3::new(
            networks[0].clone(),
            NodeBftPrivateExchangeConfigV3 {
                binding: bindings[0].clone(),
                candidate_policy_digest: gates.policy_digest.clone(),
                candidate_grant_generation: 1,
                peers: routes,
                operation_timeout: Duration::from_secs(60),
            },
            execution.clone(),
            hook.clone(),
            barriers[0].clone(),
            Some(Arc::new(Credentials {
                grant: gates.committed_grants[0].clone(),
                key: fixtures::key(130),
            })),
        )
        .unwrap(),
    );
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(10),
            bindings[0].clone(),
            fixtures::key(210),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: hex::encode(fixtures::key(200).verifying_key().to_bytes()),
                binding_digest: bindings[0].digest().unwrap(),
            },
        )
        .unwrap(),
    );
    let transport = Arc::new(fetch_cases::PartitionedPublic {
        inner: buffered,
        enabled: delivery.clone(),
        held: Mutex::new(Vec::new()),
        drains: drains[0].clone(),
        received: received[0].clone(),
    });
    let config = NodeConfig::new(
        validator_id(0),
        bindings[0].scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_millis(200))
    .unwrap()
    .with_replication(NodeReplicationConfig::new(roots[0].path().join("replication")).unwrap());
    let mut node = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .try_with_shared_execution_hook(hook)
        .unwrap()
        .with_tendermint_v3_network(
            bindings[0].clone(),
            provider,
            transport,
            exchange.clone(),
            execution.clone(),
        )
        .unwrap();
    node.start().unwrap();
    nodes.insert(0, node);
    exchanges.insert(0, exchange);
    views.insert(0, execution);
    let header_id = certificate.header.block_id().unwrap();
    let mut original_completion: Option<oasis7_node::NodeCommittedActionBatch> = None;
    network_wait(
        "cold original QC installed, confirmed and published",
        || {
            assert!(nodes[0].bft_v3_status().unwrap().running);
            for batch in nodes[0].drain_committed_action_batches() {
                if batch.height == original.height
                    && batch.actions.iter().any(|action| action.action_id == 9500)
                {
                    assert!(
                        original_completion.is_none(),
                        "duplicate original completion"
                    );
                    assert_eq!(batch.block_hash, header_id);
                    assert_eq!(
                        batch.action_root,
                        certificate.execution_commitment.action_root
                    );
                    assert_eq!(
                        batch.epoch,
                        certificate.execution_commitment.authority_epoch
                    );
                    assert_eq!(batch.slot, certificate.certificate.proposal.round);
                    assert_eq!(
                        batch.actions,
                        vec![
                            oasis7_node::NodeConsensusAction::from_payload(
                                9500,
                                certificate.certificate.proposal.proposer_id.clone(),
                                payload("actual-noise-network-agent")
                            )
                            .unwrap()
                        ]
                    );
                    original_completion = Some(batch);
                }
            }
            let state = snapshots[0].lock().unwrap().state.clone();
            let exact_applied = state.applied_installs.iter().any(|receipt| {
                receipt.height == original.height
                    && receipt.header_id == header_id
                    && receipt.driver_install_digest.len() == 64
            });
            let original_not_pending = state
                .pending_driver_apply
                .as_ref()
                .is_none_or(|pending| pending.height > original.height);
            if !exact_applied
                || !original_not_pending
                || original_completion.is_none()
                || !views[0].is_serving()
            {
                return false;
            }
            let Some(view) = views[0].view().unwrap() else {
                return false;
            };
            if view.committed_history.first() != Some(&certificate) {
                return false;
            }
            let world = actual[0].lock().unwrap().world().clone();
            let semantic = world.execution_state_identity().unwrap().state_root_hash;
            world
                .state()
                .agents
                .contains_key("actual-noise-network-agent")
                && world.state().time >= certificate.execution_commitment.height
                && semantic == view.schedule.parent_state_root()
                && views[0].is_serving()
                && views[0]
                    .view()
                    .unwrap()
                    .is_some_and(|current| current.generation == view.generation)
                && views[0].is_serving()
        },
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let restored = snapshots[0].lock().unwrap().state.clone();
    assert_eq!(restored.history[0], certificate);
    assert!(
        restored
            .pending_driver_apply
            .as_ref()
            .is_none_or(|pending| pending.height > original.height)
    );
    let applied = restored
        .applied_installs
        .iter()
        .find(|receipt| receipt.height == original.height && receipt.header_id == header_id)
        .unwrap();
    assert_eq!(
        hex::decode(&applied.driver_install_digest).unwrap().len(),
        32
    );
    assert!(original_completion.is_some());
    let archived = restored.completed_journals[0].as_ref().unwrap();
    assert!(archived.events.iter().any(|event|matches!(event,BftSigningJournalEventV3::Vote {vote,local:true,..} if vote.height==original.height&&vote.phase==original.phase&&vote.value==original.value&&vote.validator_id==original.validator_id&&vote.verify_signature().is_ok())));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    nodes[0]
        .submit_consensus_action_payload(9501, payload("actual-after-cold-pending-agent"))
        .unwrap();
    delivery.store(true, Ordering::Release);
    let mut next = [false; 3];
    network_wait("post-recovery nonempty continuation", || {
        for (index, node) in nodes.iter().enumerate() {
            assert!(node.bft_v3_status().unwrap().running);
            for batch in node.drain_committed_action_batches() {
                if batch.actions.iter().any(|action| action.action_id == 9501) {
                    assert!(!next[index]);
                    next[index] = true;
                }
            }
        }
        next.iter().all(|done| *done)
    });
    for node in nodes.iter_mut() {
        node.stop().unwrap();
    }
    nodes.clear();
    for server in servers.drain(..) {
        server.join().unwrap().unwrap();
    }
    for d in actual.iter() {
        assert!(
            d.lock()
                .unwrap()
                .world()
                .state()
                .agents
                .contains_key("actual-after-cold-pending-agent")
        );
    }
    assert_eq!(fs::read(path).unwrap(), bytes);
}
