//! Actual multi-Node/World/private ledger with Noise peers and native signer TCP.
//! The signed issuer history and protected backend/clock are explicit test doubles.
//! This proves software composition, not physical domains or production custody.
use super::*;
use oasis7_node::{
    NodeBftCommittedReadCredentialsV3, NodeBftLibp2pPeerTransportV3,
    NodeBftLibp2pPrivateCandidateExchangeV3, NodeBftPeerExecutionStateV3,
    NodeBftPrivateExchangeConfigV3, NodeBftSharedExecutionHookV3,
};
struct Credentials {
    grant: oasis7_client_api::world_service_full_sync::FormalNodeReadGrantV1,
    key: SigningKey,
}
impl NodeBftCommittedReadCredentialsV3 for Credentials {
    fn grant(&self) -> oasis7_client_api::world_service_full_sync::FormalNodeReadGrantV1 {
        self.grant.clone()
    }
    fn sign_request(&self, bytes: &[u8]) -> Result<String, String> {
        Ok(hex::encode(self.key.sign(bytes).to_bytes()))
    }
}
fn binding_for(actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>, index: u8) -> BftSignerBindingV3 {
    let d = actual.lock().unwrap();
    BftSignerBindingV3::new(
        d.handoff().clone(),
        d.initial_schedule().clone(),
        d.material_identity().into(),
        validator_id(index),
        vec![BftSignerKeyIdentityV3 {
            validator_id: validator_id(index),
            public_key_hex: hex::encode(fixtures::key(120 + index).verifying_key().to_bytes()),
        }],
        oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 2000,
            prevote_ms: 2000,
            precommit_ms: 2000,
            round_delta_ms: 100,
        },
        format!("actual-network-{index}"),
        hex::encode(fixtures::key(210 + index).verifying_key().to_bytes()),
    )
    .unwrap()
}
fn network_wait(what: &str, mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(180);
    while !ready() {
        assert!(Instant::now() < end, "timeout {what}");
        thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn wire3_formal_read_three_actual_nodes_commit_nonempty_over_noise_and_private_grants() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(network_case)
        .unwrap()
        .join()
        .unwrap();
}
fn network_case() {
    network_case_with_fetch(false);
}
fn network_case_with_fetch(check_fetch: bool) {
    network_case_mode(check_fetch, false);
}
fn network_case_mode(check_fetch: bool, check_echo: bool) {
    network_case_options(check_fetch, check_echo, false);
}
fn network_case_options(check_fetch: bool, check_echo: bool, check_fairness: bool) {
    network_case_pending(check_fetch, check_echo, check_fairness, false);
}
fn network_case_pending(
    check_fetch: bool,
    check_echo: bool,
    check_fairness: bool,
    check_pending: bool,
) {
    network_case_membership(
        check_fetch,
        check_echo,
        check_fairness,
        check_pending,
        false,
    );
}
fn network_case_membership(
    check_fetch: bool,
    check_echo: bool,
    check_fairness: bool,
    check_pending: bool,
    check_membership: bool,
) {
    let count = if check_membership { 4 } else { 3 };
    let h = if check_membership {
        membership_cases::handoff()
    } else if check_pending {
        pending_cases::weighted_handoff()
    } else {
        qualified_handoff_v3_with_count_and_setup(3, |_| {})
    };
    let gates = if check_membership {
        crate::node_lifecycle::network_gates_for_count(h.trust.initial_trust.clone(), count as u8)
    } else {
        crate::node_lifecycle::network_gates(h.trust.initial_trust.clone())
    };
    let roots = (0..count).map(|_| Scratch::new()).collect::<Vec<_>>();
    let stores = roots.iter().map(|r| cas(r.path())).collect::<Vec<_>>();
    let mut actual = roots
        .iter()
        .zip(&stores)
        .map(|(r, s)| {
            Arc::new(Mutex::new(
                FormalBftHeaderNodeDriverV3::open_from_handoff_with_peer_network(
                    &r.path().join("ledger"),
                    s.clone(),
                    h.anchor.clone(),
                    h.target.clone(),
                    h.material.clone(),
                    h.reader.clone(),
                )
                .unwrap(),
            ))
        })
        .collect::<Vec<_>>();
    let bindings = actual
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let mut binding = binding_for(d, i as u8);
            if check_membership {
                // Actual debug World replay and native protected-journal checks
                // exceed the 2-second model clock used by fixed-set fixtures.
                // Preserve the 180-second acceptance deadline and allow real
                // elapsed-time rounds to contain those actual consumers.
                binding.timeouts = oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
                    propose_ms: 30_000,
                    prevote_ms: 30_000,
                    precommit_ms: 30_000,
                    round_delta_ms: 5_000,
                };
            }
            binding
        })
        .collect::<Vec<_>>();
    for (d, b) in actual.iter().zip(&bindings) {
        let mut driver = d.lock().unwrap();
        if check_membership {
            driver.enable_bft_v3_membership_execution().unwrap();
        }
        driver.configure_bft_v3_execution(b).unwrap();
    }
    let networks = (0..count)
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
    network_wait("Noise listeners", || {
        networks.iter().all(|n| !n.listening_addrs().is_empty())
    });
    for i in 0..count {
        for j in i + 1..count {
            networks[i]
                .dial(networks[j].listening_addrs()[0].clone())
                .unwrap();
        }
    }
    network_wait("full Noise mesh", || {
        networks
            .iter()
            .all(|n| n.connected_peers().len() == count - 1)
    });
    let public_delivery = Arc::new(std::sync::atomic::AtomicBool::new(!check_echo));
    let membership_isolation = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let barriers = gates
        .gates
        .iter()
        .map(|gate| Arc::new(fetch_cases::BarrierGate::new(gate.clone())))
        .collect::<Vec<_>>();
    let mut exchanges = Vec::new();
    let mut public_transports = Vec::new();
    let drains = (0..count)
        .map(|_| Arc::new(std::sync::atomic::AtomicU64::new(0)))
        .collect::<Vec<_>>();
    let received = (0..count)
        .map(|_| Arc::new(std::sync::atomic::AtomicU64::new(0)))
        .collect::<Vec<_>>();
    let mut execution_views = Vec::new();
    let first_proposer = bindings[0]
        .initial_schedule()
        .expected_proposer_for_round(0)
        .unwrap();
    let fair_reader = bindings
        .iter()
        .position(|b| b.local_validator_id == first_proposer)
        .unwrap();
    let fair_server = (fair_reader + 1) % 3;
    if check_pending {
        pending_cases::retain_original(&actual, &bindings);
    }
    let mut snapshots = Vec::new();
    let mut servers = Vec::new();
    let mut nodes = Vec::new();
    // Membership fault timers begin together after the fault-free transition prefix.
    // This one-shot test clock is never reset for stop/start recovery.
    let membership_clock_start = Arc::new(std::sync::OnceLock::<Instant>::new());
    for index in 0..count {
        let binding = &bindings[index];
        let local = index as u8;
        let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
            state: BftSignerStateV3::from_handoff(binding).unwrap(),
            lease_generation: 1,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 100,
        }));
        let backend = MemoryTestSignerBackend {
            binding_digest: binding.digest().unwrap(),
            fail_confirm_after_commit: None,
            snapshot: snapshot.clone(),
        };
        let backend: Box<dyn BftSignerBackendV3> = if check_membership {
            membership_clock::elapsed_backend(backend, membership_clock_start.clone())
        } else {
            Box::new(backend)
        };
        let service = Arc::new(
            BftSignerServiceV3::open_with_candidate_read_proofs(
                binding.clone(),
                Box::new(MemoryTestSignerKey(fixtures::key(200 + local))),
                vec![pending_cases::initial_key(local, check_pending)],
                backend,
                BftCandidateReadPolicyV3::STANDARD,
            )
            .unwrap(),
        );
        let (address, server) = if check_membership {
            membership_cases::two_session_provider(service)
        } else {
            tcp(service)
        };
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
                    provider_public_key: hex::encode(
                        fixtures::key(200 + local).verifying_key().to_bytes(),
                    ),
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
        let context = oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(
            actual[index].lock().unwrap().handoff(),
        );
        let public = Arc::new(
            NodeBftLibp2pPeerTransportV3::new(networks[index].clone(), context, routes.clone())
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
                barriers[index].clone(),
                Some(Arc::new(Credentials {
                    grant: gates.committed_grants[index].clone(),
                    key: fixtures::key(130 + local),
                })),
            )
            .unwrap(),
        );
        if check_fairness && index == fair_server {
            exchange.restrict_nonce_budget_for_testing(2).unwrap();
        }
        exchanges.push(exchange.clone());
        execution_views.push(execution.clone());
        public_transports.push(public.clone());
        let public = Arc::new(fetch_cases::PartitionedPublic {
            inner: public,
            enabled: public_delivery.clone(),
            held: Mutex::new(Vec::new()),
            drains: drains[index].clone(),
            received: received[index].clone(),
        });
        let public: Arc<dyn oasis7_node::NodeBftPeerTransportV3> = if check_membership && index == 0
        {
            Arc::new(membership_cases::HeightPartition {
                inner: public,
                isolated: membership_isolation.clone(),
            })
        } else {
            public
        };
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
        let config = if check_membership {
            config.with_player_id("membership-test-player").unwrap()
        } else {
            config
        };
        let runtime = NodeRuntime::new(config)
            .with_replication_network_consensus_enabled(false)
            .try_with_shared_execution_hook(hook)
            .unwrap();
        nodes.push(
            if check_membership {
                runtime.with_tendermint_v3_membership_network(
                    binding.clone(),
                    provider,
                    public,
                    exchange,
                    execution,
                )
            } else {
                runtime.with_tendermint_v3_network(
                    binding.clone(),
                    provider,
                    public,
                    exchange,
                    execution,
                )
            }
            .unwrap(),
        );
    }
    let proposer = bindings[0]
        .initial_schedule()
        .expected_proposer_for_round(0)
        .unwrap();
    let proposer = bindings
        .iter()
        .position(|b| b.local_validator_id == proposer)
        .unwrap();
    if check_membership {
        membership_cases::submit_admission(&nodes[proposer], &actual[proposer]);
    } else {
        nodes[proposer]
            .submit_consensus_action_payload(9500, payload("actual-noise-network-agent"))
            .unwrap();
    }
    for node in &mut nodes {
        node.start().unwrap();
    }
    if check_membership {
        membership_cases::exercise(
            &mut nodes,
            &actual,
            &snapshots,
            &execution_views,
            &membership_isolation,
            &membership_clock_start,
        );
        drop(nodes);
        drop(exchanges);
        drop(public_transports);
        for server in servers {
            server.join().unwrap().unwrap();
        }
        return;
    }
    if check_pending {
        pending_cases::recover(
            &mut nodes,
            &mut servers,
            &mut actual,
            &mut public_transports,
            &mut exchanges,
            &mut execution_views,
            &networks,
            &bindings,
            &snapshots,
            &roots,
            &stores,
            &h,
            &gates,
            &barriers,
            &public_delivery,
            &drains,
            &received,
        );
        return;
    }
    if check_echo {
        echo_cases::exercise(
            &nodes,
            &snapshots,
            &public_transports,
            &networks,
            proposer,
            &public_delivery,
            &drains,
            &received,
        );
    }
    let mut completed = [None, None, None];
    network_wait("three actual committed receipts", || {
        for (i, node) in nodes.iter().enumerate() {
            for batch in node.drain_committed_action_batches() {
                if batch.actions.iter().any(|a| a.action_id == 9500) {
                    assert!(completed[i].is_none(), "duplicate receipt");
                    completed[i] = Some(batch);
                }
            }
            let status = node.bft_v3_status().unwrap();
            assert!(status.running, "network Node blocked: {status:?}");
        }
        completed.iter().all(Option::is_some)
    });
    if check_fairness {
        fairness_cases::exercise(
            &bindings,
            &snapshots,
            &actual,
            &networks,
            &exchanges,
            &public_delivery,
            fair_reader,
            fair_server,
            completed[0].as_ref().unwrap(),
        );
    }
    if check_fetch {
        fetch_cases::exercise(
            &nodes,
            &bindings,
            &snapshots,
            &actual,
            &networks,
            &exchanges,
            &execution_views,
            &barriers,
            &public_delivery,
            &gates,
            completed[0].as_ref().unwrap(),
        );
    }
    for node in &mut nodes {
        node.stop().unwrap();
    }
    drop(nodes);
    for server in servers {
        server.join().unwrap().unwrap();
    }
    let first = completed[0].as_ref().unwrap();
    for (index, done) in completed.iter().enumerate() {
        let done = done.as_ref().unwrap();
        assert_eq!(done.height, first.height);
        assert_eq!(done.block_hash, first.block_hash);
        assert_eq!(done.actions, first.actions);
        assert!(
            actual[index]
                .lock()
                .unwrap()
                .world()
                .state()
                .agents
                .contains_key("actual-noise-network-agent")
        );
        let protected = snapshots[index].lock().unwrap();
        assert!(protected.state.pending_driver_apply.is_none());
        assert_eq!(
            protected.state.history.len(),
            protected.state.applied_installs.len()
        );
        assert!(
            protected
                .state
                .history
                .iter()
                .any(|c| c.header.block_id().unwrap() == done.block_hash)
        );
    }
    assert!(networks.iter().all(|n| n.published().is_empty()));
}

#[path = "driver_bft_v3_node_fetch_tests.rs"]
mod fetch_cases;

#[path = "driver_bft_v3_node_echo_tests.rs"]
mod echo_cases;

#[path = "driver_bft_v3_node_fairness_tests.rs"]
mod fairness_cases;

#[path = "driver_bft_v3_node_pending_tests.rs"]
mod pending_cases;

#[path = "driver_bft_v3_node_membership_tests.rs"]
mod membership_cases;
#[path = "driver_bft_v3_node_membership_clock.rs"]
mod membership_clock;
