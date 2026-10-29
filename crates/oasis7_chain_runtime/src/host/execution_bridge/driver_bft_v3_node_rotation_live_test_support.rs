//! Shared actual four-Node/Noise/native-provider fixture. Issuer, keys,
//! protected storage and elapsed clock remain explicitly simulated.
use super::*;
use oasis7_consensus::tendermint_v3::BftPublicPeerMessageV3 as Message;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(super) fn public(seed: u8) -> String {
    hex::encode(fixtures::key(seed).verifying_key().to_bytes())
}
struct CountedKey {
    key: SigningKey,
    calls: Arc<AtomicUsize>,
}
impl BftSigningKeyHandleV3 for CountedKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.key.sign(bytes).to_bytes())
    }
}
/// Round zero at the selected rotation height cannot deliver a proposal. All votes still flow, so real
/// elapsed propose/nil vote timeouts must advance the algorithm to round one.
/// From activation, node0 cannot contribute a remotely usable vote/three-vote QC: the
/// other three actual workers, including the new node3 key, become necessary.
struct RotationGate {
    inner: Arc<dyn oasis7_node::NodeBftPeerTransportV3>,
    local: usize,
    healed: Arc<AtomicBool>,
    decision_height: u64,
    activation_height: u64,
}
impl RotationGate {
    fn height(message: &Message) -> Option<u64> {
        match message {
            Message::Proposal(p) => Some(p.header.height),
            Message::Vote(v) => Some(v.height),
            Message::Commit(c) => Some(c.execution_commitment.height),
            Message::HistoryReply(r) => {
                r.certificates.last().map(|c| c.execution_commitment.height)
            }
            Message::HistoryRequest(_) => None,
        }
    }
    fn blocked(&self, message: &Message, incoming: bool) -> bool {
        if matches!(message,Message::Proposal(p) if p.header.height==self.decision_height && p.header.round==0)
        {
            return true;
        }
        if self.healed.load(Ordering::Acquire)
            || Self::height(message).is_none_or(|h| h < self.activation_height)
        {
            return false;
        }
        if self.local == 0 {
            // Node0 may still supply its actual proposal/private candidate if
            // selected; it cannot receive votes or advertise a local QC.
            return incoming || !matches!(message, Message::Proposal(_));
        }
        match message {
            Message::Vote(v) => v.validator_id == validator_id(0),
            Message::Commit(c) => !c
                .certificate
                .precommits
                .iter()
                .any(|v| v.validator_id == validator_id(3)),
            _ => false,
        }
    }
}
impl oasis7_node::NodeBftPeerTransportV3 for RotationGate {
    fn context(&self) -> &oasis7_consensus::tendermint_v3::BftPeerContextV3 {
        self.inner.context()
    }
    fn peers(&self) -> Vec<String> {
        self.inner.peers()
    }
    fn broadcast_public(&self, m: &Message) -> Result<(), String> {
        if self.blocked(m, false) {
            Ok(())
        } else {
            self.inner.broadcast_public(m)
        }
    }
    fn send_public(&self, p: &str, m: &Message) -> Result<(), String> {
        if self.blocked(m, false) {
            Ok(())
        } else {
            self.inner.send_public(p, m)
        }
    }
    fn drain_public(
        &self,
        max: usize,
    ) -> Result<Vec<oasis7_node::NodeBftReceivedPublicV3>, String> {
        Ok(self
            .inner
            .drain_public(max)?
            .into_iter()
            .filter(|m| !self.blocked(&m.message, true))
            .collect())
    }
}

pub(super) fn diagnostics(
    bindings: &[BftSignerBindingV3],
    snapshots: &[Arc<Mutex<BftSignerBackendSnapshotV3>>],
    origin: &std::sync::OnceLock<Instant>,
) -> String {
    let mut lines = vec![format!(
        "shared_elapsed_ms={}",
        origin.get().map_or(0, |t| t.elapsed().as_millis())
    )];
    for (index, (binding, snapshot)) in bindings.iter().zip(snapshots).enumerate() {
        // Release the backend lock before replaying any public proof. This
        // diagnostic neither calls the provider nor acquires a driver lock.
        let snapshot = snapshot.lock().unwrap().clone();
        let state = &snapshot.state;
        let mut schedule = binding.initial_schedule().clone();
        for c in &state.history {
            match verify_commit_certificate_token_v3(&schedule, c) {
                Ok(token) => schedule = token.next_height_schedule().clone(),
                Err(reason) => {
                    lines.push(format!("node{index} schedule_replay_error={reason}"));
                    break;
                }
            }
        }
        lines.push(format!("node{index} height={} proposers_r0_to_r3={:?} protected_time_floor_ms={} pending_apply={:?} install={:?}",schedule.height(),(0..4).map(|r|schedule.expected_proposer_for_round(r)).collect::<Vec<_>>(),snapshot.protected_time_floor_ms,state.pending_driver_apply,state.membership_install.as_ref().map(|m|(&m.pending,m.receipts.iter().map(|r|(&r.body.local_signing_public_key,r.body.source_height,r.body.confirmation_height,r.body.activation_height,r.body.installed_revision)).collect::<Vec<_>>()))));
        if let Some(journal) = state.journal.as_ref() {
            lines.push(format!(
                "node{index} journal_height={} height_started_at={:?} local_key={}",
                journal.height,
                journal.height_started_at_ms,
                journal.local_signing_public_key()
            ));
            for event in &journal.events {
                use oasis7_consensus::tendermint_v3::BftSigningJournalEventV3 as E;
                lines.push(match event {
                    E::Proposal {proposal,local,now_ms}=>format!("node{index} proposal h={} r={} proposer={} value={} local={local} now_ms={now_ms}",proposal.header.height,proposal.header.round,proposal.header.proposer_id,proposal.header.block_id),
                    E::Vote {vote,local,now_ms}=>format!("node{index} vote h={} r={} validator={} phase={:?} value={:?} key={} local={local} now_ms={now_ms}",vote.height,vote.round,vote.validator_id,vote.phase,vote.value,vote.public_key_hex),
                    E::Timeout {now_ms}=>format!("node{index} timeout now_ms={now_ms}"),
                    E::ProposalEquivocation {now_ms,..}=>format!("node{index} proposal_equivocation now_ms={now_ms}"),
                });
            }
        }
    }
    lines.join("\n")
}

pub(super) fn wait_until(
    deadline: Instant,
    nodes: &[NodeRuntime],
    what: &str,
    diagnostic: &impl Fn() -> String,
    mut ready: impl FnMut() -> bool,
) {
    eprintln!("live rotation stage: {what}");
    loop {
        for n in nodes {
            let status = n.bft_v3_status().unwrap();
            assert!(status.running, "{what}: {status:?}\n{}", diagnostic());
        }
        if ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{what}: {:?}\n{}",
            nodes
                .iter()
                .map(NodeRuntime::bft_v3_status)
                .collect::<Vec<_>>(),
            diagnostic()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) struct FourLiveRotationNodes {
    pub nodes: Vec<NodeRuntime>,
    pub actual: Vec<Arc<Mutex<FormalBftHeaderNodeDriverV3>>>,
    pub bindings: Vec<BftSignerBindingV3>,
    pub snapshots: Vec<Arc<Mutex<BftSignerBackendSnapshotV3>>>,
    pub origin: Arc<std::sync::OnceLock<Instant>>,
    pub healed: Arc<AtomicBool>,
    pub old_calls: Arc<AtomicUsize>,
    pub new_calls: Arc<AtomicUsize>,
    pub servers: Vec<thread::JoinHandle<Result<(), String>>>,
    pub stage_budget: Duration,
    pub _roots: Vec<Scratch>,
    pub _stores: Vec<LocalCasStore>,
    pub _networks: Vec<Arc<oasis7_net::Libp2pNetwork>>,
}
impl FourLiveRotationNodes {
    pub fn new(
        h: &QualifiedH<BftProfileTargetV2>,
        decision_height: u64,
        activation_height: u64,
        player_id: Option<&str>,
    ) -> Self {
        // The independent read issuer explicitly grants K124 the candidate role.
        // Finality possession/activation alone never authorizes private reads.
        let gates = crate::node_lifecycle::network_gates_with_candidate_readers(
            h.trust.initial_trust.clone(),
            4,
            &[124],
        );
        let roots = (0..4).map(|_| Scratch::new()).collect::<Vec<_>>();
        let stores = roots.iter().map(|r| cas(r.path())).collect::<Vec<_>>();
        let actual = roots
            .iter()
            .zip(&stores)
            .map(|(root, store)| open_actual(h, root, store))
            .collect::<Vec<_>>();
        let bindings = actual
            .iter()
            .enumerate()
            .map(|(index, driver)| {
                let mut b = binding_for(driver, index as u8);
                // Reuse the established actual-World membership windows: private
                // acquisition alone may consume 60 seconds in a debug build.
                b.timeouts = oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
                    propose_ms: 90_000,
                    prevote_ms: 30_000,
                    precommit_ms: 30_000,
                    round_delta_ms: 10_000,
                };
                if index == 3 {
                    b.provisioned_signing_keys.push(BftSignerKeyIdentityV3 {
                        validator_id: validator_id(3),
                        public_key_hex: public(124),
                    });
                    b.provisioned_signing_keys
                        .sort_by(|a, b| a.public_key_hex.cmp(&b.public_key_hex));
                }
                b.validate().unwrap();
                driver
                    .lock()
                    .unwrap()
                    .configure_bft_v3_execution(&b)
                    .unwrap();
                b
            })
            .collect::<Vec<_>>();
        let networks = (0..4)
            .map(|_| {
                Arc::new(oasis7_net::Libp2pNetwork::new(
                    oasis7_net::Libp2pNetworkConfig {
                        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
                        request_response_timeout: Duration::from_secs(2),
                        ..Default::default()
                    },
                ))
            })
            .collect::<Vec<_>>();
        network_wait("bootstrap live Noise listeners", || {
            networks.iter().all(|n| !n.listening_addrs().is_empty())
        });
        for i in 0..4 {
            for j in i + 1..4 {
                networks[i]
                    .dial(networks[j].listening_addrs()[0].clone())
                    .unwrap();
            }
        }
        network_wait("bootstrap live full Noise mesh", || {
            networks.iter().all(|n| n.connected_peers().len() == 3)
        });
        let origin = Arc::new(std::sync::OnceLock::<Instant>::new());
        let healed = Arc::new(AtomicBool::new(false));
        let old_calls = Arc::new(AtomicUsize::new(0));
        let new_calls = Arc::new(AtomicUsize::new(0));
        let mut nodes = Vec::new();
        let mut snapshots = Vec::new();
        let mut servers = Vec::new();
        for index in 0..4 {
            let binding = &bindings[index];
            let local = index as u8;
            let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
                successor_archive_transfer: None,
                state: BftSignerStateV3::from_handoff(binding).unwrap(),
                lease_generation: 1,
                lease_token_digest: String::new(),
                protected_time_floor_ms: 100,
            }));
            let backend = membership_clock::elapsed_backend(
                MemoryTestSignerBackend {
                    binding_digest: binding.digest().unwrap(),
                    snapshot: snapshot.clone(),
                    fail_confirm_after_commit: None,
                },
                origin.clone(),
            );
            let keys: Vec<Box<dyn BftSigningKeyHandleV3>> = if index == 3 {
                vec![
                    Box::new(CountedKey {
                        key: fixtures::key(123),
                        calls: old_calls.clone(),
                    }),
                    Box::new(CountedKey {
                        key: fixtures::key(124),
                        calls: new_calls.clone(),
                    }),
                ]
            } else {
                vec![Box::new(MemoryTestSignerKey(fixtures::key(120 + local)))]
            };
            let service = Arc::new(
                BftSignerServiceV3::open_with_candidate_read_proofs(
                    binding.clone(),
                    Box::new(MemoryTestSignerKey(fixtures::key(200 + local))),
                    keys,
                    backend,
                    BftCandidateReadPolicyV3::STANDARD,
                )
                .unwrap(),
            );
            let (address, server) = provider_sessions(service, 1);
            servers.push(server);
            snapshots.push(snapshot);
            let provider = Arc::new(
                RemoteBftSignerProviderV3::new(
                    address,
                    Duration::from_secs(10),
                    binding.clone(),
                    fixtures::key(210 + local),
                    BftSignerTrustV3 {
                        schema_version: 3,
                        provider_public_key: public(200 + local),
                        binding_digest: binding.digest().unwrap(),
                    },
                )
                .unwrap(),
            );
            let routes = networks
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, n)| n.peer_id().to_string())
                .collect::<Vec<_>>();
            let public = Arc::new(
                NodeBftLibp2pPeerTransportV3::new(
                    networks[index].clone(),
                    oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(
                        binding.handoff(),
                    ),
                    routes.clone(),
                )
                .unwrap(),
            );
            let execution = Arc::new(NodeBftPeerExecutionStateV3::default());
            let hook: NodeBftSharedExecutionHookV3 =
                Arc::new(Mutex::new(Box::new(ActualHook(actual[index].clone()))));
            let exchange = Arc::new(
                NodeBftLibp2pPrivateCandidateExchangeV3::new(
                    networks[index].clone(),
                    NodeBftPrivateExchangeConfigV3 {
                        binding: binding.clone(),
                        candidate_policy_digest: gates.policy_digest.clone(),
                        candidate_grant_generation: 1,
                        peers: routes,
                        operation_timeout: Duration::from_secs(60),
                    },
                    execution.clone(),
                    hook.clone(),
                    gates.gates[index].clone(),
                    Some(Arc::new(Credentials {
                        grant: gates.committed_grants[index].clone(),
                        key: fixtures::key(130 + local),
                    })),
                )
                .unwrap(),
            );
            let config = NodeConfig::new(
                validator_id(local),
                binding.scope.world_id.clone(),
                NodeRole::Sequencer,
            )
            .unwrap()
            .with_tick_interval(Duration::from_millis(200))
            .unwrap()
            .with_replication(
                NodeReplicationConfig::new(roots[index].path().join("replication")).unwrap(),
            );
            let config = if let Some(player_id) = player_id {
                config.with_player_id(player_id).unwrap()
            } else {
                config
            };
            nodes.push(
                NodeRuntime::new(config)
                    .with_replication_network_consensus_enabled(false)
                    .try_with_shared_execution_hook(hook)
                    .unwrap()
                    .with_tendermint_v3_membership_network(
                        binding.clone(),
                        provider,
                        Arc::new(RotationGate {
                            inner: public,
                            local: index,
                            healed: healed.clone(),
                            decision_height,
                            activation_height,
                        }),
                        exchange,
                        execution,
                    )
                    .unwrap(),
            );
        }
        let t = bindings[0].timeouts;
        // Each stage gets its own bound for two actual rounds and one private
        // acquisition, derived from this binding's phase policy and exchange limit.
        let stage_ms = (0..2u64)
            .try_fold(60_000u64, |sum, round| {
                sum.checked_add(t.propose_ms)?
                    .checked_add(t.prevote_ms)?
                    .checked_add(t.precommit_ms)?
                    .checked_add(t.round_delta_ms.checked_mul(round)?.checked_mul(3)?)
            })
            .unwrap();
        let stage_budget = Duration::from_millis(stage_ms);
        Self {
            nodes,
            actual,
            bindings,
            snapshots,
            origin,
            healed,
            old_calls,
            new_calls,
            servers,
            stage_budget,
            _roots: roots,
            _stores: stores,
            _networks: networks,
        }
    }
    pub fn start(&mut self) {
        self.origin.set(Instant::now()).unwrap();
        for node in &mut self.nodes {
            node.start().unwrap();
        }
    }
}
